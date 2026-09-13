"""Exact-prefix projection cache, service recorder, and drift tests."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from types import MappingProxyType
import tempfile
import unittest
from unittest.mock import patch

from test_service import make_store
from zaplib.common import Refusal, packed, parse
from zaplib.engine import build_engine
from zaplib.projection_cache import ProjectionCache
from zaplib.records import CORE_HANDLERS, HandlerSpec
from zaplib.storage import record


def command(revision, event_id):
    return {"event_id": event_id, "base_revision": revision, "kind": "node.classified",
            "reason": {"summary": "cache fixture"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"}}


class ProjectionCacheTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-cache-")
        self.addCleanup(self.temp.cleanup)
        parent = Path(self.temp.name) / "fixture"
        parent.mkdir()
        self.store = make_store(parent)
        self.cache = ProjectionCache()

    def test_warm_and_suffix_loads_are_detached_and_service_record_is_idempotent(self):
        state, events, pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertIsNone(pending)
        state["plan"]["node"][0]["title"] = "poison"
        events.clear()
        clean, clean_events, _pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertNotEqual(clean["plan"]["node"][0]["title"], "poison")
        self.assertEqual(len(clean_events), 1)
        appended = self.cache.record(self.store, command(clean["revision"], "cache-event"), CORE_HANDLERS)
        self.assertFalse(appended["idempotent"])
        retry = self.cache.record(self.store, command(clean["revision"], "cache-event"), CORE_HANDLERS)
        self.assertTrue(retry["idempotent"])
        loaded, loaded_events, _pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertEqual(loaded["revision"], 1)
        self.assertEqual(len(loaded_events), 2)
        stats = self.cache.stats()
        self.assertEqual(stats["cold_loads"], 1)
        self.assertGreaterEqual(stats["warm_hits"], 2)
        self.assertEqual(stats["suffix_loads"], 1)

        with (self.store / "events.jsonl").open("ab") as stream:
            stream.write(b'{"partial"')
        _state, _events, pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertIsNotNone(pending)
        with self.assertRaisesRegex(Refusal, "incomplete final"):
            self.cache.record(self.store, command(1, "blocked"), CORE_HANDLERS)

    def test_middle_edit_requires_explicit_invalidation_and_reducer_change_is_cold(self):
        self.cache.load(self.store, CORE_HANDLERS)
        journal = self.store / "events.jsonl"
        receipt = parse(journal.read_bytes(), tagged=True)
        receipt["event_id"] = "replacement-import"
        journal.write_bytes(packed(receipt) + b"\n")
        with self.assertRaisesRegex(Refusal, "history differs"):
            self.cache.load(self.store, CORE_HANDLERS)
        self.cache.invalidate(self.store)
        state, events, _pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertEqual(state["revision"], 0)
        self.assertEqual(events[0]["event_id"], "replacement-import")

        changed = dict(CORE_HANDLERS)
        original = changed["node.classified"]
        changed["node.classified"] = HandlerSpec(original.kind, original.validate_payload, original.apply)
        self.cache.load(self.store, MappingProxyType(changed))
        self.assertEqual(self.cache.stats()["cold_loads"], 3)

    def test_cold_load_accepts_a_valid_suffix_appended_while_replay_is_running(self):
        original = __import__("zaplib.projection_cache", fromlist=["load_store"]).load_store
        appended = False

        def growing(store, handlers):
            nonlocal appended
            state, events, pending = original(store, handlers)
            if not appended:
                appended = True
                record(store, command(state["revision"], "grew-during-cold"), handlers)
            return state, events, pending

        with patch("zaplib.projection_cache.load_store", growing):
            state, events, pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertTrue(appended)
        self.assertIsNone(pending)
        self.assertEqual(state["revision"], 1)
        self.assertEqual(events[-1]["event_id"], "grew-during-cold")
        self.assertEqual(self.cache.stats()["cold_loads"], 1)

    def test_cold_growth_cannot_hide_a_duplicate_import_receipt(self):
        original = __import__("zaplib.projection_cache", fromlist=["load_store"]).load_store
        appended = False

        def malformed_growth(store, handlers):
            nonlocal appended
            state, events, pending = original(store, handlers)
            if not appended:
                appended = True
                with (Path(store) / "events.jsonl").open("ab") as stream:
                    stream.write(packed(events[0]) + b"\n")
            return state, events, pending

        with patch("zaplib.projection_cache.load_store", malformed_growth):
            with self.assertRaisesRegex(Refusal, "expected fields"):
                self.cache.load(self.store, CORE_HANDLERS)
        self.assertEqual(self.cache.stats()["cold_loads"], 0)

    def test_engine_service_backend_share_one_cache(self):
        engine = build_engine(self.store, projection_cache=self.cache)
        engine.load()
        engine.service.load_projection()
        engine.capabilities()
        self.assertEqual(self.cache.stats()["cold_loads"], 1)
        self.assertGreaterEqual(self.cache.stats()["warm_hits"], 2)

    def test_parallel_readers_receive_independent_projections(self):
        self.cache.load(self.store, CORE_HANDLERS)

        def load_and_mutate(index):
            state, events, pending = self.cache.load(self.store, CORE_HANDLERS)
            state["plan"]["node"][0]["title"] = f"thread-{index}"
            events.clear()
            return pending

        with ThreadPoolExecutor(max_workers=8) as pool:
            self.assertEqual(list(pool.map(load_and_mutate, range(24))), [None] * 24)
        state, events, _pending = self.cache.load(self.store, CORE_HANDLERS)
        self.assertNotIn("thread-", state["plan"]["node"][0]["title"])
        self.assertEqual(len(events), 1)


if __name__ == "__main__":
    unittest.main()
