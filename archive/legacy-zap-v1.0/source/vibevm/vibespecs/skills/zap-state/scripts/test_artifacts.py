from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from zaplib.artifacts import (
    PrivateArtifactStore, capture_source_blob, read_registered_source,
)
from zaplib.common import Refusal
from zaplib.sources import observe_source


class ArtifactStoreTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.allowed = self.root / "allowed"
        self.allowed.mkdir()
        self.artifacts = PrivateArtifactStore(self.root / "private", create=True)

    def tearDown(self):
        self.temp.cleanup()

    def test_content_is_addressed_by_registered_handle_and_exact_hash(self):
        first = self.artifacts.register_bytes("artifact-1", b"abcdef", kind="artifact")
        second = self.artifacts.register_bytes("artifact-1", b"uvwxyz", kind="artifact")
        page = self.artifacts.read("artifact-1", first["sha256"], offset=1, limit=3)
        self.assertEqual(page["content"], "bcd")
        self.assertFalse(page["complete"])
        self.assertEqual(page["next_offset"], 4)
        self.assertEqual(len(self.artifacts.descriptors()[0]["versions"]), 2)
        with self.assertRaises(Refusal):
            self.artifacts.read("artifact-1", "0" * 64)
        with self.assertRaises(Refusal):
            self.artifacts.read("unregistered", second["sha256"])

    def test_source_capture_retains_immutable_bytes_when_live_file_changes(self):
        source = self.allowed / "source.txt"
        source.write_text("captured", encoding="utf-8")
        captured = capture_source_blob(
            self.artifacts, source, self.allowed, source_id="source-1",
        )
        descriptor = captured["source"]
        source.write_text("changed", encoding="utf-8")
        self.assertEqual(observe_source(descriptor)["observed"]["status"], "changed")
        state = {
            "extensions": {
                "knowledge": {
                    "sources": {
                        "source-1": {
                            **descriptor,
                            "versions": [descriptor["content_sha256"]],
                        },
                    },
                },
            },
        }
        content = read_registered_source(
            state, self.artifacts, "source-1", descriptor["content_sha256"],
        )
        self.assertEqual(content["content"], "captured")
        self.assertNotIn(str(self.allowed), repr(self.artifacts.descriptors()))

    def test_arbitrary_task_path_is_not_a_content_endpoint(self):
        secret = self.allowed / "not-registered.txt"
        secret.write_text("private", encoding="utf-8")
        with self.assertRaises(Refusal):
            self.artifacts.read("not-registered", "0" * 64)
        outside = self.root / "outside.txt"
        outside.write_text("outside", encoding="utf-8")
        with self.assertRaises((Refusal, ValueError)):
            self.artifacts.capture_file("outside", outside, self.allowed)

    def test_corrupted_private_blob_is_refused(self):
        descriptor = self.artifacts.register_bytes("artifact-corrupt", b"original", kind="artifact")
        catalog = self.artifacts._load()
        blob = self.artifacts.root / catalog["entries"]["artifact-corrupt"]["versions"][0]["blob"]
        blob.write_bytes(b"tampered")
        with self.assertRaisesRegex(Refusal, "blob bytes"):
            self.artifacts.read("artifact-corrupt", descriptor["sha256"])

    def test_opening_an_uninitialized_store_for_read_does_not_create_it(self):
        empty = self.root / "empty-private"
        empty.mkdir()
        with self.assertRaisesRegex(Refusal, "blob directory"):
            PrivateArtifactStore(empty)
        self.assertEqual(list(empty.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
