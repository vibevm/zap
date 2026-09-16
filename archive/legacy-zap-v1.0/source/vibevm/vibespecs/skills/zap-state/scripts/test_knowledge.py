"""Knowledge dependency invalidation and fog-transition tests."""
from __future__ import annotations

import hashlib
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, packed
from zaplib.knowledge import KNOWLEDGE_HANDLERS, invalidation_closure, knowledge_snapshot
from zaplib.records import CORE_HANDLERS, apply_command, compose_handlers
from zaplib.sources import capture_source
from zaplib.storage import load_store
from test_knowledge_support import HANDLERS, append, command, create_store


class KnowledgeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-knowledge-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = create_store(self.root)

    def evidence(self, key: str, node: str = "T") -> None:
        append(self.store, "evidence.recorded", {"id": key, "claim": f"Observed {key}", "subject": f"fixture:{key}",
               "result": "observed_pass", "artifact_refs": [f"trace:{key}"], "node_refs": [node]}, f"event-{key}")

    def source(self, key: str, text: str) -> dict:
        path = self.root / f"{key}.txt"
        path.write_text(text, encoding="utf-8")
        descriptor = capture_source(path, self.root, source_id=key)
        append(self.store, "knowledge.source-recorded", {"source": descriptor}, f"source-{key}")
        return descriptor

    def closure(self, endpoint: dict[str, str], event_id: str) -> None:
        append(self.store, "knowledge.closure-assessed", {"subject": endpoint, "status": "complete", "boundary": [],
               "missing": [], "evidence_refs": ["E1"], "basis": "Bounded fixture dependencies"}, event_id)

    def test_changed_source_invalidates_known_fact_decision_task_chain_only(self):
        first = self.source("S1", "alpha")
        self.source("S2", "unrelated")
        self.evidence("E1")
        self.evidence("E2", "U")
        append(self.store, "fact.recorded", {"id": "F1", "statement": "Bound fact", "status": "observed", "node_refs": ["T"],
               "evidence_refs": ["E1"], "source_refs": ["S1"]}, "fact-1")
        append(self.store, "decision.recorded", {"id": "D1", "question": "Which route?", "alternatives": [{"id": "a", "description": "A"}, {"id": "b", "description": "B"}],
               "chosen": "a", "rationale": "Bound evidence", "authority_ref": "unverified:data", "consequences": ["Task T"],
               "node_refs": ["T"], "evidence_refs": ["E1"]}, "decision-1")
        edges = [
            ("edge-sf", {"kind": "source", "id": "S1"}, {"kind": "fact", "id": "F1"}),
            ("edge-fd", {"kind": "fact", "id": "F1"}, {"kind": "decision", "id": "D1"}),
            ("edge-dt", {"kind": "decision", "id": "D1"}, {"kind": "task", "id": "T"}),
            ("edge-se", {"kind": "source", "id": "S2"}, {"kind": "evidence", "id": "E2"}),
        ]
        for edge_id, prerequisite, dependent in edges:
            append(self.store, "knowledge.dependency-recorded", {"id": edge_id, "prerequisite": prerequisite,
                   "dependent": dependent, "relation": "supports"}, edge_id)
        for index, endpoint in enumerate([edges[0][1], edges[0][2], edges[1][2], edges[2][2]]):
            self.closure(endpoint, f"closure-{index}")
        before = load_store(self.store, HANDLERS)[0]
        unrelated = packed(before["evidence"]["E2"])
        changed_hash = hashlib.sha256(b"changed").hexdigest()
        self.assertNotEqual(changed_hash, first["content_sha256"])
        append(self.store, "knowledge.source-observed", {"source_id": "S1", "observed": {"status": "changed",
               "sha256": changed_hash, "bytes": 7, "detail": None}}, "changed-1")
        state = load_store(self.store, HANDLERS)[0]
        invalidation = state["extensions"]["knowledge"]["invalidations"]["changed-1"]
        affected = {(row["kind"], row["id"]) for row in invalidation["affected"]}
        self.assertEqual(affected, {("source", "S1"), ("fact", "F1"), ("decision", "D1"), ("task", "T")})
        self.assertFalse(invalidation["incomplete_closure"])
        self.assertEqual(packed(state["evidence"]["E2"]), unrelated)
        self.assertNotEqual(state["extensions"]["knowledge"]["endpoint_status"].get("evidence:E2"), "stale")

    def test_missing_relevant_closure_is_visible_and_domain_endpoints_are_typed(self):
        self.source("S1", "alpha")
        self.evidence("E1")
        append(self.store, "knowledge.dependency-recorded", {"id": "edge", "prerequisite": {"kind": "source", "id": "S1"},
               "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "edge")
        self.closure({"kind": "source", "id": "S1"}, "source-closure")
        state = load_store(self.store, HANDLERS)[0]
        result = invalidation_closure(state, [{"kind": "source", "id": "S1"}])
        self.assertEqual({(row["kind"], row["id"]) for row in result["affected"]}, {("source", "S1"), ("task", "T")})
        self.assertTrue(result["incomplete_closure"])
        state["extensions"]["domain"] = {"obligations": {"O": {}}, "outcome_revisions": {"OUT": {}}}
        handlers = compose_handlers(CORE_HANDLERS, KNOWLEDGE_HANDLERS)
        after = apply_command(state, {"event_id": "domain-edge", "base_revision": state["revision"], "kind": "knowledge.dependency-recorded",
             "reason": {"summary": "Typed domain dependency"}, "payload": {"id": "domain-edge", "prerequisite": {"kind": "obligation", "id": "O"},
             "dependent": {"kind": "outcome", "id": "OUT"}, "relation": "affects"}}, handlers)
        self.assertIn("domain-edge", after["extensions"]["knowledge"]["dependencies"])

    def test_direct_recapture_with_changed_bytes_invalidates_dependents(self):
        original = self.source("S1", "alpha")
        self.evidence("E1")
        append(self.store, "knowledge.dependency-recorded", {"id": "edge-recapture", "prerequisite": {"kind": "source", "id": "S1"},
               "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "edge-recapture")
        self.closure({"kind": "source", "id": "S1"}, "closure-source")
        self.closure({"kind": "task", "id": "T"}, "closure-task")
        path = self.root / "S1.txt"
        path.write_text("new bytes", encoding="utf-8")
        recaptured = capture_source(path, self.root, source_id="S1")
        append(self.store, "knowledge.source-recaptured", {"previous_sha256": original["content_sha256"], "source": recaptured}, "recapture")
        knowledge = load_store(self.store, HANDLERS)[0]["extensions"]["knowledge"]
        affected = {(row["kind"], row["id"]) for row in knowledge["invalidations"]["recapture"]["affected"]}
        self.assertEqual(affected, {("source", "S1"), ("task", "T")})
        self.assertEqual(knowledge["endpoint_status"]["source:S1"], "current")
        self.assertEqual(knowledge["endpoint_status"]["task:T"], "stale")
        self.assertNotIn("source:S1", knowledge["closures"])

    def test_region_reopen_relevance_split_and_merge_survive_replay(self):
        self.evidence("E1")
        append(self.store, "knowledge.region-recorded", {"id": "K", "question": "What is unknown?", "node_refs": ["T"]}, "region-core")
        append(self.store, "knowledge.region-transitioned", {"region_id": "K", "from": "unexamined", "to": "evidenced",
               "evidence_refs": ["E1"], "reason": "Observed"}, "region-evidenced")
        evidenced_snapshot = knowledge_snapshot(load_store(self.store, HANDLERS)[0], ["K"])
        append(self.store, "knowledge.region-transitioned", {"region_id": "K", "from": "evidenced", "to": "unexamined",
               "evidence_refs": [], "reason": "New signal reopened the question"}, "region-reopened")
        reopened_snapshot = knowledge_snapshot(load_store(self.store, HANDLERS)[0], ["K"])
        self.assertGreater(reopened_snapshot["revision"], evidenced_snapshot["revision"])
        self.assertNotEqual(reopened_snapshot["sha256"], evidenced_snapshot["sha256"])
        append(self.store, "knowledge.region-relevance-set", {"region_id": "K", "relevance": "irrelevant",
               "reason": "Outside the current local route"}, "region-relevance")
        selected_before_unrelated = knowledge_snapshot(load_store(self.store, HANDLERS)[0], ["K"])
        self.source("UNRELATED", "not part of K")
        selected_after_unrelated = knowledge_snapshot(load_store(self.store, HANDLERS)[0], ["K"])
        self.assertEqual(selected_after_unrelated, selected_before_unrelated)
        with self.assertRaises(Refusal):
            knowledge_snapshot(load_store(self.store, HANDLERS)[0], ["FABRICATED"])
        children = [
            {"id": "K1", "question": "First part?", "node_refs": ["T"], "relevance": "relevant"},
            {"id": "K2", "question": "Second part?", "node_refs": ["U"], "relevance": "unknown"},
        ]
        append(self.store, "knowledge.region-split", {"region_id": "K", "children": children, "reason": "Questions became distinct"}, "region-split")
        merged = {"id": "K3", "question": "Shared question?", "node_refs": ["T", "U"], "relevance": "relevant"}
        append(self.store, "knowledge.region-merged", {"region_ids": ["K1", "K2"], "merged": merged,
               "reason": "One experiment covers both"}, "region-merge")
        first = load_store(self.store, HANDLERS)[0]
        second = load_store(self.store, HANDLERS)[0]
        self.assertEqual(packed(first), packed(second))
        regions = first["extensions"]["knowledge"]["regions"]
        self.assertEqual(regions["K"]["state"], "invalidated")
        self.assertEqual(regions["K"]["relevance"], "irrelevant")
        self.assertEqual(regions["K"]["history"][1]["transition"], "reopened")
        self.assertEqual((regions["K1"]["state"], regions["K2"]["state"], regions["K3"]["state"]), ("invalidated", "invalidated", "unexamined"))
        expanded_snapshot = knowledge_snapshot(first, ["K", "K1", "K2", "K3"])
        self.assertEqual(set(expanded_snapshot["regions"]), {"K", "K1", "K2", "K3"})


if __name__ == "__main__":
    unittest.main()
