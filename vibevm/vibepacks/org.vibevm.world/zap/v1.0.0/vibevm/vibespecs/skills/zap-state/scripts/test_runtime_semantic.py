from pathlib import Path
import tempfile
import unittest

from zaplib.domain import domain_state
from zaplib.runtime import AutomaticCoordinator
from zaplib.runtime_semantic import completion_ready, materialize_model_payload, scoped_semantic_context, semantic_scope_sha256
from zaplib.sources import capture_vibevm_facts
from test_runtime_support import RuntimeFixture


class RuntimeSemanticTests(unittest.TestCase):
    def test_completion_uses_only_current_selective_pivot_carryover(self):
        from test_domain import DomainTests
        preserved = DomainTests(); preserved.setUp(); preserved.accept_single_work(); preserved.propose_o2(); preserved.apply_sparse_pivot()
        self.assertTrue(completion_ready(preserved.state))
        stale = DomainTests(); stale.setUp(); stale.accept_single_work(); stale.propose_o2(); stale.apply_sparse_pivot(stale.sparse_request(preserve=False))
        self.assertFalse(completion_ready(stale.state))

    def test_selection_and_review_contexts_are_bounded_to_relevant_rows(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-context-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport,
                                               fixture.semantic, fixture.config)
            coordinator.tick()
            request = next(iter(fixture.semantic.requests.values()))
            frontier_row = request["request"]["frontier"][0]
            self.assertEqual(frontier_row["source_captures"][0]["source_id"], "S")
            self.assertNotIn("steps", frontier_row["contract"])
            self.assertNotIn("acceptance", frontier_row["contract"])

            state = fixture.state()
            context = scoped_semantic_context(state, [{"work_ids": ["T"]}], [], [])
            self.assertEqual(set(context["domain"]["contracts"]), {"T"})
            self.assertEqual(context["source_refs"], ["S"])
            self.assertNotIn("task_contracts", context["domain"])
            self.assertNotIn("reviews", context["domain"])
            self.assertEqual(context["knowledge"]["snapshot"]["region_ids"], [])

    def test_relevant_fog_is_materialized_and_scoped_while_unrelated_fog_rebinds(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-fog-") as temporary:
            root = Path(temporary); fixture = RuntimeFixture(root)
            fixture._data("knowledge.region-recorded", {"id": "K", "question": "Does T need another check?", "node_refs": ["T"]}, "Relevant fog")
            fixture._data("knowledge.region-relevance-set", {"region_id": "K", "relevance": "unknown", "reason": "Expose relevant fog"}, "Materialize relevant fog")
            fixture._data("knowledge.region-recorded", {"id": "OTHER", "question": "Unrelated root concern?", "node_refs": ["R"]}, "Unrelated fog")
            state = fixture.state(); source = state["extensions"]["knowledge"]["sources"]["S"]
            capture = [{"source_id": "S", "sha256": source["content_sha256"]}]
            fixture._apply("knowledge.dependency-recorded", {"id": "S-to-T", "prerequisite": {"kind": "source", "id": "S"},
                           "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "evidence.adjudicate", "Bind relevant source", capture)
            state = fixture.state(); review = {"review_request_id": "review:K", "trigger_ids": ["K"], "work_ids": ["T"],
                                               "scope": "fog", "summary": "Review K", "state": "pending"}
            context = scoped_semantic_context(state, [review], [], [])
            self.assertEqual(context["knowledge"]["snapshot"]["region_ids"], ["K"])
            self.assertIn("S-to-T", context["knowledge"]["dependencies"])
            self.assertIn("S", context["knowledge"]["applicability"])
            self.assertNotIn("OTHER", context["knowledge"]["snapshot"]["regions"])
            body = {"review_requests": [review], "jobs": [], "verification_jobs": [], **context}
            before = semantic_scope_sha256(body)
            fixture._data("knowledge.region-recorded", {"id": "OTHER-2", "question": "Another unrelated concern?", "node_refs": ["R"]}, "More unrelated fog")
            state = fixture.state(); after_unrelated = {"review_requests": [review], "jobs": [], "verification_jobs": [],
                                                        **scoped_semantic_context(state, [review], [], [])}
            self.assertEqual(before, semantic_scope_sha256(after_unrelated))
            fixture._data("knowledge.region-relevance-set", {"region_id": "K", "relevance": "relevant", "reason": "T depends on it"}, "Relevant fog changed")
            changed = {"review_requests": [review], "jobs": [], "verification_jobs": [],
                       **scoped_semantic_context(fixture.state(), [review], [], [])}
            self.assertNotEqual(before, semantic_scope_sha256(changed))

    def test_review_materialization_injects_exact_relevant_snapshot_and_refuses_fabricated_region(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-review-materialize-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); fixture._data("knowledge.region-recorded", {"id": "K", "question": "Bound T?", "node_refs": ["T"]}, "Fog")
            fixture._data("knowledge.region-relevance-set", {"region_id": "K", "relevance": "unknown", "reason": "Expose fog"}, "Materialize fog")
            state = fixture.state(); domain = domain_state(state); review_row = {"review_request_id": "review:K", "trigger_ids": ["K"], "work_ids": ["T"], "scope": "fog", "summary": "Review K"}
            context = scoped_semantic_context(state, [review_row], [], [])
            expected = {"base_sha256": state["base_sha256"], "zap_revision": state["revision"], "domain_revision": domain["revision"],
                        "intent_id": domain["active_intent_id"], "outcome_id": domain["active_outcome_id"], "policy_revision": 1,
                        "source_captures": context["source_captures"]}
            transition = {"intent_id": None, "outcome_id": None, "obligation_dispositions": [], "ownership_changes": [], "work_changes": [],
                          "preserved_evidence_ids": [], "preserved_stage_acceptance_ids": [], "preserved_work_acceptance_ids": [],
                          "preserved_integration_acceptance_ids": [], "job_reconciliation": [], "tradeoffs": [], "preserved_benefits": ["proof"]}
            payload = {"schema": "zap-domain/review-proposed/1", "review_id": "REV-K", "previous_review_id": None, "signals": ["fog"],
                       "captures": {**expected, "jobs": []}, "knowledge": {"before": None,
                           "after": {"region_ids": ["K"], "revision": 0, "sha256": "0" * 64, "regions": {}},
                           "new_region_ids": [], "affected_dependencies": ["S-to-T"], "closure_complete": False},
                       "alternatives": [{"id": "keep", "description": "Keep route", "value": "bounded", "feasibility": "feasible",
                                         "remaining_cost": "small", "risks": [], "unknowns": ["K"]}], "chosen": "keep",
                       "decision": {"kind": "keep_route", "rationale": "Keep work while fog is explicit"}, "transition": transition,
                       "next_trigger": "K evidence"}
            materialized = materialize_model_payload(state, "domain.review-proposed", payload, expected_review_capture=expected,
                                                     request_knowledge=context["knowledge"])
            self.assertNotEqual(materialized["knowledge"]["after"]["sha256"], "0" * 64)
            fixture._data("domain.review-proposed", materialized, "Materialized semantic review")
            fixture._apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV-K",
                           "expected_domain_revision": domain_state(fixture.state())["revision"]}, "adaptive.apply", "Apply semantic review")
            self.assertEqual(domain_state(fixture.state())["last_applied_review_id"], "REV-K")
            forged = {**payload, "review_id": "FORGED", "knowledge": {**payload["knowledge"], "after": {**payload["knowledge"]["after"], "region_ids": ["MISSING"]},
                                                                       "new_region_ids": ["MISSING"]}}
            with self.assertRaises(Exception):
                materialize_model_payload(state, "domain.review-proposed", forged, expected_review_capture=expected,
                                          request_knowledge=context["knowledge"])

    def test_native_fact_content_is_included_for_reassessment(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-native-context-") as temporary:
            root = Path(temporary); fixture = RuntimeFixture(root); xml = root / "native.xml"
            xml.write_text('<spec xmlns="https://vibevm.org/spec/1"><p><FACT fact="true" status="spec/plan">Native decision fact</FACT></p></spec>', encoding="utf-8")
            captured = capture_vibevm_facts(xml, root, source_id="N")
            fixture._observe("knowledge.native-facts-recorded", {"capture": captured}, "Capture native fact")
            rows = [{"source_id": "N", "sha256": captured["source"]["content_sha256"]}]
            fixture._apply("knowledge.dependency-recorded", {"id": "N-to-T", "prerequisite": {"kind": "source", "id": "N"},
                           "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "evidence.adjudicate", "Bind native fact", rows)
            context = scoped_semantic_context(fixture.state(), [{"work_ids": ["T"]}], [], [])
            self.assertEqual(next(iter(context["knowledge"]["native_facts"].values()))["text"], "Native decision fact")

    def test_sparse_review_transition_is_expanded_before_event_submission(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-sparse-review-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            fixture._data("domain.outcome-proposed", {
                "schema": "zap-domain/outcome-proposed/1", "outcome_id": "OUTCOME-2", "revision": 2,
                "previous_outcome_id": "OUTCOME", "intent_id": "INTENT", "summary": "Refined verified tasks",
                "benefits": ["proof"], "guarantees": ["checked"], "tradeoffs": ["revised ordering"], "obligations": [],
            }, "Propose revised fixture outcome")
            state = fixture.state()
            sparse = {
                "schema": "zap-domain/sparse-review-transition/1", "intent_id": None, "outcome_id": "OUTCOME-2",
                "changed_dispositions": [], "ownership_changes": [], "work_changes": [],
                "preserved_evidence_ids": [], "preserved_stage_acceptance_ids": [],
                "preserved_work_acceptance_ids": [], "preserved_integration_acceptance_ids": [],
                "job_reconciliation": [], "tradeoffs": ["revised ordering"], "preserved_benefits": ["proof"],
            }
            original = {"schema": "zap-domain/review-proposed/1", "transition": sparse}
            materialized = materialize_model_payload(state, "domain.review-proposed", original)
            active = {key for key, row in domain_state(state)["obligations"].items() if row["status"] == "active"}
            self.assertNotIn("schema", materialized["transition"])
            self.assertEqual({row["obligation_id"] for row in materialized["transition"]["obligation_dispositions"]}, active)
            self.assertEqual(original["transition"], sparse)


if __name__ == "__main__":
    unittest.main()
