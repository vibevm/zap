"""Snapshot identity and tail-replay tests."""
from __future__ import annotations

from pathlib import Path
import shutil
import tempfile
import unittest

from zaplib.common import Refusal, packed, parse, sha
from zaplib.snapshots import create_snapshot, load_snapshot_tail
from zaplib.sources import capture_source
from zaplib.storage import load_store
from test_knowledge_support import HANDLERS, append, create_store


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-snapshot-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = create_store(self.root)
        source = self.root / "source.txt"
        source.write_text("captured", encoding="utf-8")
        append(self.store, "knowledge.source-recorded", {"source": capture_source(source, self.root, source_id="S")}, "source-1")
        self.snapshot = self.root / "snapshot.json"
        create_snapshot(self.store, self.snapshot, HANDLERS)

    def test_snapshot_plus_tail_matches_full_replay(self):
        append(self.store, "node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"}, "tail-1")
        append(self.store, "knowledge.region-recorded", {"id": "K", "question": "New fog?", "node_refs": ["T"]}, "tail-2")
        full = load_store(self.store, HANDLERS)[0]
        resumed = load_snapshot_tail(self.store, self.snapshot, HANDLERS)
        self.assertEqual(packed(resumed["state"]), packed(full))
        self.assertEqual((resumed["snapshot_revision"], resumed["cursor"], len(resumed["tail_events"])), (1, 3, 2))

    def test_capture_lock_rejects_a_concurrent_normal_append(self):
        errors = []

        def concurrent_append():
            try:
                append(self.store, "node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"}, "concurrent")
            except Refusal as exc:
                errors.append(exc.code)

        second = self.root / "locked-snapshot.json"
        create_snapshot(self.store, second, HANDLERS, capture_hook=concurrent_append)
        self.assertEqual(errors, ["BUSY"])
        self.assertEqual(load_store(self.store, HANDLERS)[0]["revision"], 1)

    def test_forged_rehashed_state_misses_trusted_cache_and_refuses_derivation(self):
        cache = set()
        cold = load_snapshot_tail(self.store, self.snapshot, HANDLERS, verification_cache=cache)
        warm = load_snapshot_tail(self.store, self.snapshot, HANDLERS, verification_cache=cache)
        self.assertEqual((cold["snapshot_verification"], warm["snapshot_verification"], len(cache)), ("cold", "warm", 1))
        snapshot = parse(self.snapshot.read_bytes(), tagged=True)
        snapshot["state"]["execution_mode"] = "forged-active"
        snapshot["state_sha256"] = sha(packed(snapshot["state"]))
        self.snapshot.write_bytes(packed(snapshot) + b"\n")
        with self.assertRaisesRegex(Refusal, "not derived"):
            load_snapshot_tail(self.store, self.snapshot, HANDLERS, verification_cache=cache)
        self.assertEqual(len(cache), 1)

    def test_changed_base_prefix_and_reducer_version_refuse(self):
        base_changed = self.root / "base-changed"
        shutil.copytree(self.store, base_changed)
        base = base_changed / "base.json"
        base.write_bytes(base.read_bytes() + b" ")
        with self.assertRaisesRegex(Refusal, "base identity"):
            load_snapshot_tail(base_changed, self.snapshot, HANDLERS)

        prefix_changed = self.root / "prefix-changed"
        shutil.copytree(self.store, prefix_changed)
        journal = prefix_changed / "events.jsonl"
        raw = journal.read_bytes()
        journal.write_bytes(bytes([raw[0] ^ 1]) + raw[1:])
        with self.assertRaisesRegex(Refusal, "prefix differs"):
            load_snapshot_tail(prefix_changed, self.snapshot, HANDLERS)

        with self.assertRaisesRegex(Refusal, "reducer identity"):
            load_snapshot_tail(self.store, self.snapshot, HANDLERS, reducer_version="zap-reducer/2")


if __name__ == "__main__":
    unittest.main()
