"""Selective reuse through the real control and knowledge service routes."""
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.control import ACTION_CLASSES, CONTROL_HANDLERS, COORDINATOR_EVENT_KINDS, OWNER_EVENT_KINDS, active_policy
from zaplib.domain import (
    DOMAIN_ACTION_KINDS, DOMAIN_DATA_KINDS, DOMAIN_HANDLERS,
    build_sparse_review_transition, current_acceptance_coverage, domain_state, intent_fingerprint,
)
from zaplib.domain_model import work_is_accepted
from zaplib.knowledge import KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_HANDLERS, knowledge_snapshot
from zaplib.records import CORE_HANDLERS, compose_handlers
from zaplib.service import ApplicationService, CredentialAuthority
from zaplib.sources import capture_source
from zaplib.storage import import_mup, load_store


PLAN = '''schema = 1
plan_id = "reuse-fixture"
revision = 0
root_node = "P"
current_node = "X"
[[mandate]]
id = "OWNER"
text = "Preserve owner value."
disposition = "owned"
nodes = ["R", "X", "Y"]
[[node]]
id = "P"
parent = ""
title = "Program"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = []
acceptance = []
evidence = []
[[node]]
id = "R"
parent = "P"
title = "Campaign"
kind = "campaign"
state = "candidate"
order = 0
depends_on = []
mandates = ["OWNER"]
acceptance = ["Integrated result"]
evidence = []
[[node]]
id = "X"
parent = "R"
title = "Changing branch"
kind = "atom"
state = "candidate"
order = 1
depends_on = []
mandates = ["OWNER"]
acceptance = ["X result"]
evidence = []
[[node]]
id = "Y"
parent = "R"
title = "Stable branch"
kind = "atom"
state = "candidate"
order = 2
depends_on = []
mandates = ["OWNER"]
acceptance = ["Y result"]
evidence = []
'''


def imported_task(work_id):
    return {"id": work_id, "title": work_id, "goal": "Deliver", "read_paths": ["input"],
            "write_paths": ["output"], "steps": ["implement"], "positive_cases": ["works"],
            "negative_cases": ["refuses"], "checks": ["focused"], "acceptance": ["accepted"],
            "safe_stop": "candidate", "commit_subject": "feat: fixture", "notes": []}


class DomainReuseIntegration(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zap-domain-reuse-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        plan = self.root / "plan.toml"; plan.write_text(PLAN, encoding="utf-8")
        tasks = self.root / "tasks"; tasks.mkdir()
        (tasks / "P.json").write_text(json.dumps({"id": "P", "tasks": [imported_task("R")]}), encoding="utf-8")
        (tasks / "R.json").write_text(json.dumps({"id": "R", "tasks": [
            imported_task("X"), imported_task("Y"),
        ]}), encoding="utf-8")
        self.store = self.root / "store"
        import_mup(plan, tasks, self.store)
        self.handlers = compose_handlers(CORE_HANDLERS, CONTROL_HANDLERS, KNOWLEDGE_HANDLERS, DOMAIN_HANDLERS)
        action_kinds = dict(DOMAIN_ACTION_KINDS)
        data_kinds = set(DOMAIN_DATA_KINDS)
        observations = set()
        for kind, route in KNOWLEDGE_EVENT_ROUTES.items():
            if route["route"] == "agent_data":
                data_kinds.add(kind)
            elif route["route"] == "effect_adapter":
                observations.add(kind)
            else:
                action_kinds[kind] = route["action"]
        state = self.state()
        binding, self.token = CredentialAuthority.issue(
            "owner-cred", "owner", "owner", state["plan"]["plan_id"],
            control_kinds=OWNER_EVENT_KINDS | COORDINATOR_EVENT_KINDS, action_classes=ACTION_CLASSES,
        )
        self.service = ApplicationService(
            self.store, self.handlers, CredentialAuthority([binding]),
            action_kinds=action_kinds, data_kinds=data_kinds, observation_kinds=observations,
        )
        self.intent = {"schema": "zap-domain/intent-proposed/1", "intent_id": "I1", "revision": 1,
            "previous_intent_id": None, "summary": "Useful result", "beneficiaries": ["owner"],
            "values": ["verified value"], "constraints": ["preserve guarantees"], "source_refs": ["charter"]}
        self.service.submit_agent(self.command("domain.intent-proposed", self.intent, "intent-proposed"))
        state = self.state(); mandate = state["plan"]["mandate"][0]
        self.charter = {"schema": "zap-charter/1", "charter_id": "charter-1", "campaign_id": "reuse-fixture",
            "base_sha256": state["base_sha256"], "revision": 1, "parent_sha256": None, "intent": "Useful result",
            "expected_outcome": {"outcome_id": "O1", "summary": "Working result"},
            "intent_binding": {"intent_id": "I1", "sha256": intent_fingerprint(self.intent)},
            "delegation": {"allowed_actions": sorted(ACTION_CLASSES), "adaptation": {
                "allow_target_revision": True, "mutable_obligations": [], "essential_obligations": [],
                "allowed_dispositions": ["excluded", "replaced", "retained", "unattainable"]}},
            "legacy_authority": [{"id": "OWNER", "disposition": "retained",
                "source_sha256": sha(packed(mandate)), "replacement_ref": None}],
            "stop_policy": {"schema": "zap-stop-policy/1", "policy_id": "policy-1", "revision": 1, "rules": []}}
        self.service.submit_agent(self.command("control.charter-drafted", {"charter": self.charter}, "charter-draft"))
        self.service.submit_control(self.command("control.charter-activated", {"charter_id": "charter-1",
            "charter_revision": 1, "charter_sha256": sha(packed(self.charter)), "campaign_id": "reuse-fixture",
            "base_sha256": state["base_sha256"]}, "charter-activate"),
            credential_id="owner-cred", credential=self.token)
        self.authorized("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1"},
                        "outcome.adopt", "intent-adopt")
        self.service.submit_agent(self.command("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1",
            "outcome_id": "O1", "revision": 1, "previous_outcome_id": None, "intent_id": "I1",
            "summary": "Working result", "benefits": ["owner value"], "guarantees": ["verified"],
            "tradeoffs": [], "obligations": []}, "outcome-1-proposed"))
        self.authorized("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1",
            "outcome_id": "O1", "obligation_dispositions": []}, "outcome.adopt", "outcome-1-adopt")
        self.sources = {}
        for work_id in ("X", "Y"):
            self.accept_leaf(work_id)
        self.accept_parent()

    def state(self):
        return load_store(self.store, self.handlers if hasattr(self, "handlers") else CORE_HANDLERS)[0]

    def command(self, kind, payload, event_id):
        return {"event_id": event_id, "base_revision": self.state()["revision"], "kind": kind,
                "reason": {"summary": "selective reuse integration"}, "payload": payload}

    def authorized(self, kind, payload, action_class, event_id):
        command = self.command(kind, payload, event_id)
        state = self.state(); policy = active_policy(state)
        action = {"schema": "zap-action/1", "action_id": f"action-{event_id}", "action_class": action_class,
            "campaign_id": "reuse-fixture", "base_sha256": state["base_sha256"],
            "charter_revision": policy["revision"], "payload_sha256": sha(packed(payload)),
            "source_captures": [], "branch_id": None, "run_id": None, "problem_id": None}
        assessment = {"schema": "zap-assessment/1", "assessment_id": f"assessment-{event_id}",
            "policy_id": "policy-1", "policy_revision": 1, "phase": "before_action",
            "values": {}, "drain_targets": []}
        return self.service.apply_control_action(command, action, assessment,
            credential_id="owner-cred", credential=self.token)

    def obligations(self, work_id):
        domain = domain_state(self.state())
        return sorted(row["obligation_id"] for row in domain["ownership"] if row["work_id"] == work_id
                      and domain["obligations"][row["obligation_id"]]["status"] == "active")

    def contract(self, work_id):
        return {"schema": "zap-task-contract/1", "contract_id": f"contract:{work_id}", "work_id": work_id,
            "title": work_id, "goal": "Deliver unchanged subject", "read_subjects": [f"input:{work_id}"],
            "write_subjects": [f"output:{work_id}"], "resources": ["cpu"], "steps": ["implement"],
            "positive_cases": ["works"], "negative_cases": ["refuses"], "checks": ["focused"],
            "acceptance": ["proven"], "safe_stop": "candidate", "integration_owner": work_id,
            "delivery_route": ["functional"], "required_stage": "functional",
            "source_handles": [f"source:S-{work_id}"], "obligation_ids": self.obligations(work_id)}

    def proof(self, work_id):
        evidence_id = f"E-{work_id}"; source_id = f"S-{work_id}"
        self.service.submit_agent(self.command("evidence.recorded", {"id": evidence_id,
            "claim": f"{work_id} passes", "subject": work_id, "result": "observed_pass",
            "artifact_refs": [f"artifact:{work_id}"], "node_refs": [work_id]}, f"evidence-{work_id}"))
        path = self.root / f"{work_id}.txt"; path.write_text(f"stable {work_id}", encoding="utf-8")
        descriptor = capture_source(path, self.root, source_id=source_id)
        self.sources[work_id] = (path, descriptor)
        self.service.submit_observation(self.command("knowledge.source-recorded", {"source": descriptor},
            f"source-{work_id}"), credential_id="owner-cred", credential=self.token)
        scope = {"kind": "subjects", "subjects": [{"kind": "node", "id": work_id}]}
        self.authorized("knowledge.applicability-assessed", {"source_id": source_id, "status": "applicable",
            "scope": scope, "evidence_refs": [evidence_id], "basis": "captured exact input"},
            "evidence.adjudicate", f"applicability-{work_id}")
        self.authorized("knowledge.closure-assessed", {"subject": {"kind": "source", "id": source_id},
            "status": "complete", "boundary": [{"kind": "source", "id": source_id}], "missing": [],
            "evidence_refs": [evidence_id], "basis": "input closure complete"},
            "evidence.adjudicate", f"closure-{work_id}")
        obligations = self.obligations(work_id)
        self.authorized("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1",
            "evidence_id": evidence_id, "expected_revision": -1, "disposition": "accepted",
            "applies_to": {"outcome_id": "O1", "obligation_ids": obligations, "work_ids": [work_id],
                "stage": "functional", "scope": "exact work"}, "source_refs": [source_id],
            "method": {"argv": ["verify", work_id], "target": work_id, "toolchain": "fixture",
                "environment": "test", "subjects": [work_id], "cases": ["positive", "negative"]},
            "limitations": []}, "evidence.adjudicate", f"adjudicate-{work_id}")
        return evidence_id, obligations

    def accept_leaf(self, work_id):
        self.authorized("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1",
            "work_id": work_id, "expected_version": 0, "contract": self.contract(work_id)},
            "task.update", f"contract-{work_id}")
        evidence_id, obligations = self.proof(work_id)
        self.authorized("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1",
            "stage_acceptance_id": f"ST-{work_id}", "work_id": work_id, "stage": "functional",
            "outcome_id": "O1", "evidence_ids": [evidence_id], "obligation_ids": obligations,
            "scope": "complete work", "summary": "stage accepted"}, "stage.accept", f"stage-{work_id}")
        self.authorized("domain.work-accepted", {"schema": "zap-domain/work-accepted/1",
            "acceptance_id": f"A-{work_id}", "work_id": work_id, "outcome_id": "O1",
            "stage_acceptance_id": f"ST-{work_id}", "evidence_ids": [evidence_id],
            "obligation_ids": obligations, "integration_acceptance_ids": [], "summary": "work accepted"},
            "work.accept", f"accept-{work_id}")

    def accept_parent(self):
        self.authorized("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1",
            "work_id": "R", "expected_version": 0, "contract": self.contract("R")}, "task.update", "contract-R")
        evidence_id, obligations = self.proof("R")
        self.authorized("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1",
            "stage_acceptance_id": "ST-R", "work_id": "R", "stage": "functional", "outcome_id": "O1",
            "evidence_ids": [evidence_id], "obligation_ids": obligations, "scope": "integrated result",
            "summary": "parent stage accepted"}, "stage.accept", "stage-R")
        self.authorized("domain.integration-accepted", {"schema": "zap-domain/integration-accepted/1",
            "integration_id": "INT-R", "work_id": "R", "child_work_ids": ["X", "Y"], "legacy_child_ids": [],
            "outcome_id": "O1", "evidence_ids": [evidence_id], "obligation_ids": obligations,
            "summary": "both branches integrated"}, "work.accept", "integration-R")
        self.authorized("domain.work-accepted", {"schema": "zap-domain/work-accepted/1", "acceptance_id": "A-R",
            "work_id": "R", "outcome_id": "O1", "stage_acceptance_id": "ST-R",
            "evidence_ids": [evidence_id], "obligation_ids": obligations,
            "integration_acceptance_ids": ["INT-R"], "summary": "campaign work accepted"},
            "work.accept", "accept-R")

    def propose_pivot(self):
        self.service.submit_agent(self.command("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1",
            "outcome_id": "O2", "revision": 2, "previous_outcome_id": "O1", "intent_id": "I1",
            "summary": "Revise X only", "benefits": ["owner value"], "guarantees": ["verified"],
            "tradeoffs": ["revalidate X"], "obligations": []}, "outcome-2-proposed"))
        request = {"schema": "zap-domain/sparse-review-transition/1", "intent_id": None, "outcome_id": "O2",
            "changed_dispositions": [], "ownership_changes": [], "work_changes": [{"work_id": "X",
                "operation": "revalidate", "order": None, "successor_ids": [], "reason": "X input changed"}],
            "preserved_evidence_ids": ["E-Y"], "preserved_stage_acceptance_ids": ["ST-Y"],
            "preserved_work_acceptance_ids": ["A-Y"], "preserved_integration_acceptance_ids": [],
            "job_reconciliation": [], "tradeoffs": ["revalidate X"], "preserved_benefits": ["owner value"]}
        return build_sparse_review_transition(self.state(), request)

    def propose_review(self, transition, review_id="REV1"):
        state = self.state(); domain = domain_state(state); source = self.sources["Y"][1]
        payload = {"schema": "zap-domain/review-proposed/1", "review_id": review_id, "previous_review_id": None,
            "signals": ["X changed"], "captures": {"base_sha256": state["base_sha256"],
                "zap_revision": state["revision"], "domain_revision": domain["revision"], "intent_id": "I1",
                "outcome_id": "O1", "policy_revision": 1,
                "source_captures": [{"source_id": "S-Y", "sha256": source["content_sha256"]}], "jobs": []},
            "knowledge": {"before": None, "after": {"region_ids": [], **knowledge_snapshot(state, [])},
                "new_region_ids": [], "affected_dependencies": [], "closure_complete": True},
            "alternatives": [{"id": "local", "description": "Revalidate X only", "value": "preserves Y",
                "feasibility": "feasible", "remaining_cost": "bounded", "risks": [], "unknowns": []}],
            "chosen": "local", "decision": {"kind": "pivot_outcome", "rationale": "Y inputs remain exact"},
            "transition": transition, "next_trigger": "X revalidation complete"}
        self.service.submit_agent(self.command("domain.review-proposed", payload, f"{review_id}-proposed"))

    def test_local_pivot_reuses_stable_branch_and_invalidates_parent_integration(self):
        transition = self.propose_pivot()
        with self.assertRaisesRegex(Refusal, "work changed|revalidation"):
            build_sparse_review_transition(self.state(), {"schema": "zap-domain/sparse-review-transition/1",
                "intent_id": None, "outcome_id": "O2", "changed_dispositions": [], "ownership_changes": [],
                "work_changes": transition["work_changes"], "preserved_evidence_ids": ["E-X", "E-Y", "E-R"],
                "preserved_stage_acceptance_ids": ["ST-X", "ST-Y", "ST-R"],
                "preserved_work_acceptance_ids": ["A-X", "A-Y", "A-R"],
                "preserved_integration_acceptance_ids": ["INT-R"], "job_reconciliation": [],
                "tradeoffs": ["revalidate X"], "preserved_benefits": ["owner value"]})
        self.propose_review(transition)
        self.authorized("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
            "expected_domain_revision": domain_state(self.state())["revision"]}, "adaptive.apply", "review-apply")
        state = self.state(); domain = domain_state(state)
        self.assertTrue(work_is_accepted(state, domain, "Y"))
        self.assertFalse(work_is_accepted(state, domain, "X"))
        self.assertFalse(work_is_accepted(state, domain, "R"), "parent integration still depends on changed X")
        coverage = current_acceptance_coverage(state)
        self.assertEqual(["Y"], coverage["work_ids"])
        self.assertEqual([], coverage["integration_acceptance_ids"])
        self.assertEqual("O1", domain["acceptances"]["A-Y"]["outcome_id"])
        self.assertEqual("O2", domain["reuse_witnesses"]["acceptances"]["A-Y"][0]["to_outcome_id"])

    def test_source_change_after_review_capture_refuses_reuse(self):
        transition = self.propose_pivot()
        self.propose_review(transition)
        path, old = self.sources["Y"]
        path.write_text("changed Y", encoding="utf-8")
        changed = capture_source(path, self.root, source_id="S-Y")
        self.service.submit_observation(self.command("knowledge.source-recaptured", {
            "previous_sha256": old["content_sha256"], "source": changed}, "source-Y-recaptured"),
            credential_id="owner-cred", credential=self.token)
        with self.assertRaisesRegex(Refusal, "source changed|source content changed"):
            self.authorized("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                "expected_domain_revision": domain_state(self.state())["revision"]}, "adaptive.apply", "review-stale")
        self.assertFalse(domain_state(self.state())["reuse_witnesses"]["evidence"])

    def test_complementary_final_gate_proofs_cover_the_full_closure_union(self):
        domain = domain_state(self.state())
        evidence_by_obligation = {}
        for evidence_id, row in domain["evidence_adjudications"].items():
            for obligation_id in row["applies_to"]["obligation_ids"]:
                evidence_by_obligation.setdefault(obligation_id, evidence_id)
        results = [{"obligation_id": key, "result": "accepted", "unmet_portion": "",
                    "successor_ids": [], "evidence_ids": [evidence_by_obligation[key]]}
                   for key, row in sorted(domain["obligations"].items()) if row["status"] == "active"]
        base = {"schema": "zap-domain/campaign-closed/1", "classification": "original",
            "active_outcome_id": "O1", "actual_benefit": "integrated owner value", "obligation_results": results,
            "acceptance_ids": ["A-R", "A-X", "A-Y"], "integration_acceptance_ids": ["INT-R"],
            "deferral_ids": [], "promotion_ids": [], "summary": "complementary scoped proof union"}
        with self.assertRaisesRegex(Refusal, "collectively cover"):
            self.authorized("domain.campaign-closed", {**base, "closure_id": "C-missing",
                "final_gate_evidence_ids": ["E-R", "E-X"]}, "campaign.close", "closure-missing")
        self.authorized("domain.campaign-closed", {**base, "closure_id": "C-complete",
            "final_gate_evidence_ids": ["E-R", "E-X", "E-Y"]}, "campaign.close", "closure-complete")
        self.assertEqual("closed", domain_state(self.state())["closure"]["status"])


if __name__ == "__main__":
    unittest.main()
