"""Focused integration of domain proof with the real source applicability API."""
from __future__ import annotations

import copy
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from zaplib import domain_adaptive, domain_proof
from zaplib.common import Refusal
from zaplib.domain import DOMAIN_HANDLERS, domain_state
from zaplib.knowledge import KNOWLEDGE_HANDLERS, knowledge_snapshot
from zaplib.records import CORE_HANDLERS, apply_command, compose_handlers, initial_state
from zaplib.sources import capture_source, current_applicability, observe_source

HANDLERS = compose_handlers(CORE_HANDLERS, KNOWLEDGE_HANDLERS, DOMAIN_HANDLERS)


def fixture_state():
    plan = {"schema": 1, "plan_id": "fixture", "revision": 0, "root_node": "X", "current_node": "X",
        "mandate": [{"id": "OWNER", "text": "Keep value", "disposition": "owned", "nodes": ["X"]}],
        "node": [{"id": "X", "parent": "", "title": "Work", "kind": "campaign", "state": "candidate",
                  "order": 0, "depends_on": [], "mandates": ["OWNER"], "acceptance": ["Result"], "evidence": []}]}
    return initial_state({"plan": plan, "task_contracts": {}}, "a" * 64)


class DomainSourcesIntegration(unittest.TestCase):
    def test_inapplicable_adjudication_retains_a_stale_source_identity(self):
        state = fixture_state()

        def apply(kind, payload, event_id):
            nonlocal state
            state = apply_command(state, {"event_id": event_id, "base_revision": state["revision"], "kind": kind,
                "reason": {"summary": "stale evidence classification"}, "payload": payload}, HANDLERS)

        apply("evidence.recorded", {"id": "E", "claim": "Old observation", "subject": "X",
              "result": "observed_fail", "artifact_refs": ["artifact:old"], "node_refs": ["X"]}, "evidence")
        with tempfile.TemporaryDirectory(prefix="zap-domain-stale-") as temporary:
            root = Path(temporary); path = root / "input.txt"; path.write_text("old", encoding="utf-8")
            descriptor = capture_source(path, root, source_id="S")
            apply("knowledge.source-recorded", {"source": descriptor}, "source")
            path.write_text("changed", encoding="utf-8")
            apply("knowledge.source-observed", observe_source(descriptor), "source-changed")
        domain = domain_state(state)
        domain["active_outcome_id"] = "O1"; domain["original_outcome_id"] = "O1"
        domain["outcome_revisions"]["O1"] = {"outcome_id": "O1", "status": "active", "revision": 1}
        domain["obligations"]["OWNER"]["current_outcome_ids"] = ["O1"]
        state["extensions"]["domain"] = domain
        policy = {"campaign_id": "fixture", "base_sha256": state["base_sha256"], "revision": 1,
                  "allowed_actions": ["evidence.adjudicate"], "adaptation": {"allow_target_revision": True,
                  "mutable_obligations": [], "essential_obligations": [], "allowed_dispositions": ["retained"]}}
        with mock.patch.object(domain_proof, "require_action", return_value=policy):
            apply("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1", "evidence_id": "E",
                "expected_revision": -1, "disposition": "inapplicable", "applies_to": {"outcome_id": "O1",
                    "obligation_ids": ["OWNER"], "work_ids": ["X"], "stage": "functional", "scope": "old input"},
                "source_refs": ["S"], "method": {"argv": ["verify"], "target": "X", "toolchain": "fixture",
                    "environment": "test", "subjects": ["X"], "cases": ["changed input"]},
                "limitations": ["source changed"]}, "inapplicable")
        row = domain_state(state)["evidence_adjudications"]["E"]
        self.assertEqual("stale", row["applicability_at_adjudication"]["status"])
        self.assertEqual([{"source_id": "S", "sha256": descriptor["content_sha256"]}],
                         row["source_captures_at_adjudication"])

    def test_real_applicability_is_required_by_domain_adjudication(self):
        state = fixture_state()

        def apply(kind, payload, event_id):
            nonlocal state
            state = apply_command(state, {"event_id": event_id, "base_revision": state["revision"], "kind": kind,
                "reason": {"summary": "domain/source integration"}, "payload": payload}, HANDLERS)

        apply("evidence.recorded", {"id": "E", "claim": "Observed pass", "subject": "X",
              "result": "observed_pass", "artifact_refs": ["artifact:trace"], "node_refs": ["X"]}, "evidence")
        with tempfile.TemporaryDirectory(prefix="zap-domain-source-") as temporary:
            root = Path(temporary)
            source_path = root / "input.txt"
            source_path.write_text("stable input", encoding="utf-8")
            descriptor = capture_source(source_path, root, source_id="S", source_kind="file", applicability_scope=None)
        apply("knowledge.source-recorded", {"source": descriptor}, "source")
        apply("knowledge.applicability-assessed", {"source_id": "S", "status": "applicable",
              "scope": {"kind": "subjects", "subjects": [{"kind": "node", "id": "X"}]},
              "evidence_refs": ["E"], "basis": "exact captured input"}, "applicability")
        apply("knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S"}, "status": "complete",
              "boundary": [{"kind": "source", "id": "S"}], "missing": [], "evidence_refs": ["E"],
              "basis": "all declared inputs captured"}, "closure")
        self.assertEqual(current_applicability(state, ["S"])["status"], "applicable")

        domain = domain_state(state)
        domain["active_outcome_id"] = "O1"
        domain["original_outcome_id"] = "O1"
        domain["outcome_revisions"]["O1"] = {"outcome_id": "O1", "status": "active", "revision": 1}
        domain["obligations"]["OWNER"]["current_outcome_ids"] = ["O1"]
        state["extensions"]["domain"] = domain
        policy = {"campaign_id": "fixture", "base_sha256": state["base_sha256"], "revision": 1,
                  "allowed_actions": ["evidence.adjudicate"], "adaptation": {"allow_target_revision": True,
                  "mutable_obligations": [], "essential_obligations": [], "allowed_dispositions": ["retained"]}}
        with mock.patch.object(domain_proof, "require_action", return_value=policy):
            apply("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1", "evidence_id": "E",
                "expected_revision": -1, "disposition": "accepted", "applies_to": {"outcome_id": "O1",
                    "obligation_ids": ["OWNER"], "work_ids": ["X"], "stage": "functional", "scope": "fixture"},
                "source_refs": ["S"], "method": {"argv": ["verify"], "target": "domain", "toolchain": "fixture",
                    "environment": "test", "subjects": ["X"], "cases": ["positive", "negative"]},
                "limitations": []}, "domain-adjudication")
        self.assertEqual(domain_state(state)["evidence_adjudications"]["E"]["disposition"], "accepted")

        domain = domain_state(state)
        domain["active_intent_id"] = "I1"
        domain["intents"]["I1"] = {"intent_id": "I1", "status": "active", "revision": 1}
        state["extensions"]["domain"] = domain
        review = {"schema": "zap-domain/review-proposed/1", "review_id": "REV1", "previous_review_id": None,
            "signals": ["source may have changed"], "captures": {"base_sha256": state["base_sha256"],
                "zap_revision": state["revision"], "domain_revision": domain["revision"], "intent_id": "I1",
                "outcome_id": "O1", "policy_revision": 1,
                "source_captures": [{"source_id": "S", "sha256": "d" * 64}], "jobs": []},
            "knowledge": {"before": None,
                "after": {"region_ids": [], **knowledge_snapshot(state, [])},
                "new_region_ids": [], "affected_dependencies": [], "closure_complete": False},
            "alternatives": [{"id": "keep", "description": "Keep route", "value": "known",
                "feasibility": "feasible", "remaining_cost": "bounded", "risks": [], "unknowns": ["source freshness"]}],
            "chosen": "keep", "decision": {"kind": "keep_route", "rationale": "verify capture first"},
            "transition": {"intent_id": None, "outcome_id": None, "obligation_dispositions": [],
                "ownership_changes": [], "work_changes": [], "preserved_evidence_ids": [],
                "preserved_stage_acceptance_ids": [], "preserved_work_acceptance_ids": [],
                "preserved_integration_acceptance_ids": [], "job_reconciliation": [],
                "tradeoffs": [], "preserved_benefits": ["owner value"]}, "next_trigger": "fresh capture"}
        apply("domain.review-proposed", review, "review")
        with mock.patch.object(domain_adaptive, "require_action", return_value=policy), self.assertRaisesRegex(
                Refusal, "source changed"):
            apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                             "expected_domain_revision": domain_state(state)["revision"]}, "review-apply")

        apply("knowledge.region-recorded", {"id": "K", "question": "Which integration path is viable?",
                                             "node_refs": ["X"]}, "region")
        apply("knowledge.region-transitioned", {"region_id": "K", "from": "unexamined", "to": "evidenced",
              "evidence_refs": ["E"], "reason": "initial path was measured"}, "region-evidenced")
        apply("knowledge.region-transitioned", {"region_id": "K", "from": "evidenced", "to": "unexamined",
              "evidence_refs": [], "reason": "new constraint reopens the question"}, "region-reopened")
        apply("knowledge.region-split", {"region_id": "K", "children": [
            {"id": "K.API", "question": "Is the API path viable?", "node_refs": ["X"], "relevance": "relevant"},
            {"id": "K.SVC", "question": "Is the service path required?", "node_refs": ["X"], "relevance": "unknown"}],
            "reason": "the reopened question contains two independent unknowns"}, "region-split")
        captured = knowledge_snapshot(state, ["K", "K.API", "K.SVC"])

        def fog_review(review_id, after):
            current = domain_state(state)
            return {"schema": "zap-domain/review-proposed/1", "review_id": review_id, "previous_review_id": None,
                "signals": ["fog expanded after reopening"], "captures": {"base_sha256": state["base_sha256"],
                    "zap_revision": state["revision"], "domain_revision": current["revision"], "intent_id": "I1",
                    "outcome_id": "O1", "policy_revision": 1, "source_captures": [], "jobs": []},
                "knowledge": {"before": None, "after": after, "new_region_ids": ["K.API", "K.SVC"],
                    "affected_dependencies": [], "closure_complete": False},
                "alternatives": [{"id": "probe", "description": "Probe the API path first", "value": "high information",
                    "feasibility": "unknown", "remaining_cost": "small", "risks": ["service may still be required"],
                    "unknowns": ["API support"]}], "chosen": "probe",
                "decision": {"kind": "reorder", "rationale": "resolve the cheapest decision-changing question first"},
                "transition": {"intent_id": None, "outcome_id": None, "obligation_dispositions": [],
                    "ownership_changes": [], "work_changes": [{"work_id": "X", "operation": "reprioritize", "order": 5,
                        "successor_ids": [], "reason": "new information priority"}], "preserved_evidence_ids": [],
                    "preserved_stage_acceptance_ids": [], "preserved_work_acceptance_ids": [],
                    "preserved_integration_acceptance_ids": [],
                    "job_reconciliation": [], "tradeoffs": [], "preserved_benefits": ["owner value"]},
                "next_trigger": "API probe result"}

        with self.assertRaises(Refusal):
            apply("domain.review-proposed", fog_review("FABRICATED", {"region_ids": ["MISSING"],
                "revision": 0, "sha256": "0" * 64, "regions": {"MISSING": {}}}), "fabricated-review")
        before_review = copy.deepcopy(state)
        after = {"region_ids": ["K", "K.API", "K.SVC"], **captured}
        apply("domain.review-proposed", fog_review("REV2", after), "review-2")
        apply("knowledge.region-relevance-set", {"region_id": "K.API", "relevance": "irrelevant",
              "reason": "changed after review capture"}, "region-drift")
        with mock.patch.object(domain_adaptive, "require_action", return_value=policy), self.assertRaisesRegex(
                Refusal, "knowledge regions changed"):
            apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV2",
                                             "expected_domain_revision": domain_state(state)["revision"]}, "review-2-stale")
        state = before_review
        apply("domain.review-proposed", fog_review("REV2", after), "review-2")
        apply("knowledge.region-recorded", {"id": "OTHER", "question": "Unrelated future question",
                                             "node_refs": ["X"]}, "unrelated-region")
        with mock.patch.object(domain_adaptive, "require_action", return_value=policy):
            apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV2",
                                             "expected_domain_revision": domain_state(state)["revision"]}, "review-2-apply")
        self.assertEqual(domain_state(state)["work_updates"]["X"]["order"], 5)


if __name__ == "__main__":
    unittest.main()
