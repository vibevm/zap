"""Auditable pending-tail repair and crash-boundary tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, parse, sha
from zaplib.recovery import repair_pending_tail
from zaplib.sources import capture_source
from zaplib.storage import load_store
from test_knowledge_support import HANDLERS, append, create_store


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-recovery-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def damaged(self, name: str, tail: bytes = b'{"partial":') -> tuple[Path, bytes, str]:
        case = self.root / name
        case.mkdir()
        store = create_store(case)
        journal = store / "events.jsonl"
        committed = journal.read_bytes()
        journal.write_bytes(committed + tail)
        return store, committed, sha(tail)

    def test_repair_quarantines_original_before_switch_and_resumes(self):
        store, committed, tail_hash = self.damaged("prepared")
        before = (store / "events.jsonl").read_bytes()

        def crash(point):
            if point == "after_quarantine":
                raise RuntimeError(point)

        with self.assertRaises(RuntimeError):
            repair_pending_tail(store, repair_id="repair-1", expected_tail_sha256=tail_hash, fault=crash)
        self.assertEqual((store / "events.jsonl").read_bytes(), before)
        quarantine = store / "recovery" / "repair-1"
        self.assertEqual((quarantine / "original-events.jsonl").read_bytes(), before)
        self.assertEqual((quarantine / "pending-tail.bin").read_bytes(), before[len(committed):])
        receipt = parse((quarantine / "repair-receipt.json").read_bytes(), tagged=True)
        self.assertEqual(receipt["pending_tail"]["sha256"], tail_hash)
        result = repair_pending_tail(store, repair_id="repair-1", expected_tail_sha256=tail_hash)
        self.assertTrue(result["switched"])
        self.assertEqual((store / "events.jsonl").read_bytes(), committed)
        self.assertTrue((quarantine / "switch-completed.json").is_file())
        self.assertIsNone(load_store(store)[2])

    def test_crash_after_switch_is_detectable_and_exact_retry_completes_receipt(self):
        store, committed, tail_hash = self.damaged("switched")

        def crash(point):
            if point == "after_switch":
                raise RuntimeError(point)

        with self.assertRaises(RuntimeError):
            repair_pending_tail(store, repair_id="repair-2", expected_tail_sha256=tail_hash, fault=crash)
        quarantine = store / "recovery" / "repair-2"
        self.assertEqual((store / "events.jsonl").read_bytes(), committed)
        self.assertTrue((quarantine / "repair-receipt.json").is_file())
        self.assertFalse((quarantine / "switch-completed.json").exists())
        result = repair_pending_tail(store, repair_id="repair-2", expected_tail_sha256=tail_hash)
        self.assertFalse(result["switched"])
        self.assertTrue((quarantine / "switch-completed.json").is_file())

    def test_middle_corruption_and_existing_writer_lock_are_never_overwritten(self):
        store, committed, tail_hash = self.damaged("middle", b'{"seq":1}\n{"tail"')
        journal = store / "events.jsonl"
        before = journal.read_bytes()
        with self.assertRaises((Refusal, ValueError, KeyError)):
            repair_pending_tail(store, repair_id="repair-middle", expected_tail_sha256=sha(b'{"tail"'))
        self.assertEqual(journal.read_bytes(), before)
        self.assertFalse((store / "recovery" / "repair-middle").exists())

        journal.write_bytes(committed + b'{"partial":')
        lock = store / "writer.lock"
        lock.write_bytes(b"live-or-stale-owner-must-be-inspected")
        with self.assertRaisesRegex(Refusal, "never steal"):
            repair_pending_tail(store, repair_id="repair-lock", expected_tail_sha256=sha(b'{"partial":'))
        self.assertEqual(lock.read_bytes(), b"live-or-stale-owner-must-be-inspected")

    def test_repair_replays_extension_events_with_the_supplied_registry(self):
        case = self.root / "extensions"
        case.mkdir()
        store = create_store(case)
        source_path = case / "source.txt"
        source_path.write_text("captured", encoding="utf-8")
        append(store, "knowledge.source-recorded", {"source": capture_source(source_path, case, source_id="S")}, "source")
        journal = store / "events.jsonl"
        journal.write_bytes(journal.read_bytes() + b'{"partial":')
        result = repair_pending_tail(store, repair_id="repair-extension", expected_tail_sha256=sha(b'{"partial":'), handlers=HANDLERS)
        self.assertTrue(result["ok"])
        self.assertIn("S", load_store(store, HANDLERS)[0]["extensions"]["knowledge"]["sources"])


if __name__ == "__main__":
    unittest.main()
