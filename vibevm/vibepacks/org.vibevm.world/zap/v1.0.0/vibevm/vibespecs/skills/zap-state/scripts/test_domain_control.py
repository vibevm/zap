"""One focused integration path through the real control application service."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.control import (
    ACTION_CLASSES, CONTROL_HANDLERS, COORDINATOR_EVENT_KINDS, OWNER_EVENT_KINDS,
    active_policy,
)
from zaplib.domain import (
    DOMAIN_ACTION_KINDS, DOMAIN_DATA_KINDS, DOMAIN_HANDLERS, domain_state,
    intent_fingerprint,
)
from zaplib.knowledge import knowledge_snapshot
from zaplib.records import CORE_HANDLERS, compose_handlers
from zaplib.service import ApplicationService, CredentialAuthority
from zaplib.storage import import_mup, load_store

PLAN = '''schema = 1
plan_id = "fixture"
revision = 0
root_node = "R"
current_node = "X"
[[mandate]]
id = "OWNER"
text = "Preserve owner value."
disposition = "owned"
nodes = ["R", "X"]
[[node]]
id = "R"
parent = ""
title = "Campaign"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["OWNER"]
acceptance = ["Campaign result"]
evidence = []
[[node]]
id = "X"
parent = "R"
title = "Work"
kind = "atom"
state = "candidate"
order = 1
depends_on = []
mandates = ["OWNER"]
acceptance = ["Work result"]
evidence = []
'''


def task():
    return {"id": "X", "title": "Work", "goal": "Behavior", "read_paths": ["input"],
            "write_paths": ["output"], "steps": ["implement"], "positive_cases": ["works"],
            "negative_cases": ["refuses"], "checks": ["focused"], "acceptance": ["accepted"],
            "safe_stop": "candidate", "commit_subject": "feat: fixture", "notes": []}


class DomainControlIntegration(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zap-domain-control-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        plan = root / "plan.toml"; plan.write_text(PLAN, encoding="utf-8")
        tasks = root / "tasks"; tasks.mkdir()
        (tasks / "R.json").write_text(json.dumps({"id": "R", "tasks": [task()]}), encoding="utf-8")
        self.store = root / "store"
        import_mup(plan, tasks, self.store)
        self.handlers = compose_handlers(CORE_HANDLERS, CONTROL_HANDLERS, DOMAIN_HANDLERS)
        state = self.state()
        binding, self.token = CredentialAuthority.issue(
            "owner-cred", "owner", "owner", state["plan"]["plan_id"],
            control_kinds=OWNER_EVENT_KINDS | COORDINATOR_EVENT_KINDS,
            action_classes=ACTION_CLASSES,
        )
        self.service = ApplicationService(
            self.store, self.handlers, CredentialAuthority([binding]),
            action_kinds=DOMAIN_ACTION_KINDS, data_kinds=DOMAIN_DATA_KINDS,
        )
        self.intent = {"schema": "zap-domain/intent-proposed/1", "intent_id": "I1", "revision": 1,
            "previous_intent_id": None, "summary": "Useful verified result", "beneficiaries": ["owner"],
            "values": ["verified result"], "constraints": ["preserve mandate"], "source_refs": ["charter-1"]}
        self.service.submit_agent(self.command("domain.intent-proposed", self.intent, "intent-proposal"))
        state = self.state()
        mandate = state["plan"]["mandate"][0]
        self.charter = {
            "schema": "zap-charter/1", "charter_id": "charter-1", "campaign_id": "fixture",
            "base_sha256": state["base_sha256"], "revision": 1, "parent_sha256": None,
            "intent": "Useful verified result", "expected_outcome": {"outcome_id": "O1", "summary": "Working result"},
            "intent_binding": {"intent_id": "I1", "sha256": intent_fingerprint(self.intent)},
            "delegation": {"allowed_actions": sorted(ACTION_CLASSES), "adaptation": {
                "allow_target_revision": True, "mutable_obligations": [], "essential_obligations": [],
                "allowed_dispositions": ["excluded", "replaced", "retained", "unattainable"]}},
            "legacy_authority": [{"id": "OWNER", "disposition": "retained",
                                  "source_sha256": sha(packed(mandate)), "replacement_ref": None}],
            "stop_policy": {"schema": "zap-stop-policy/1", "policy_id": "policy-1", "revision": 1, "rules": []},
        }
        self.service.submit_agent(self.command("control.charter-drafted", {"charter": self.charter}, "draft-charter"))
        state = self.state()
        self.service.submit_control(self.command("control.charter-activated", {
            "charter_id": "charter-1", "charter_revision": 1, "charter_sha256": sha(packed(self.charter)),
            "campaign_id": "fixture", "base_sha256": state["base_sha256"],
        }, "activate-charter"), credential_id="owner-cred", credential=self.token)

    def state(self):
        return load_store(self.store, self.handlers if hasattr(self, "handlers") else CORE_HANDLERS)[0]

    def command(self, kind, payload, event_id):
        state = self.state()
        return {"event_id": event_id, "base_revision": state["revision"], "kind": kind,
                "reason": {"summary": "control/domain integration"}, "payload": payload}

    def authorized(self, command, action_class, suffix):
        state = self.state()
        policy = active_policy(state)
        action = {"schema": "zap-action/1", "action_id": f"action-{suffix}", "action_class": action_class,
            "campaign_id": "fixture", "base_sha256": state["base_sha256"], "charter_revision": policy["revision"],
            "payload_sha256": sha(packed(command["payload"])), "source_captures": [], "branch_id": None,
            "run_id": None, "problem_id": None}
        assessment = {"schema": "zap-assessment/1", "assessment_id": f"assessment-{suffix}",
            "policy_id": "policy-1", "policy_revision": 1, "phase": "before_action", "values": {}, "drain_targets": []}
        return self.service.apply_control_action(command, action, assessment,
            credential_id="owner-cred", credential=self.token)

    def test_agent_proposals_controlled_adoption_and_sticky_pause(self):
        adopt_intent = self.command("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1"},
                                    "intent-adopt")
        with self.assertRaises(Refusal):
            self.service.submit_agent(adopt_intent)
        self.authorized(adopt_intent, "outcome.adopt", "intent")

        outcome = {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O1", "revision": 1,
            "previous_outcome_id": None, "intent_id": "I1", "summary": "Working result",
            "benefits": ["owner value"], "guarantees": ["verified"], "tradeoffs": [], "obligations": []}
        self.service.submit_agent(self.command("domain.outcome-proposed", outcome, "outcome-proposal"))
        adopt = self.command("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1",
                             "outcome_id": "O1", "obligation_dispositions": []}, "outcome-adopt")
        with self.assertRaises(Refusal):
            self.service.submit_agent(adopt)
        self.authorized(adopt, "outcome.adopt", "outcome")
        self.assertEqual(domain_state(self.state())["active_outcome_id"], "O1")

        state = self.state()
        self.service.submit_control(self.command("control.owner-stop-requested", {
            "pause_id": "pause-1", "campaign_id": "fixture", "base_sha256": state["base_sha256"],
            "charter_revision": active_policy(state)["revision"], "reason": "Owner pauses integration test",
            "drain_targets": [],
        }, "owner-stop-1"), credential_id="owner-cred", credential=self.token)
        replace = self.command("domain.task-contract-replaced", {
            "schema": "zap-domain/task-contract-replaced/1", "work_id": "X", "expected_version": 0,
            "contract": {"schema": "zap-task-contract/1", "contract_id": "contract:X", "work_id": "X",
                "title": "Work", "goal": "Complete behavior", "read_subjects": ["input"], "write_subjects": ["output"],
                "resources": ["cpu"], "steps": ["implement"], "positive_cases": ["works"],
                "negative_cases": ["refuses"], "checks": ["focused"], "acceptance": ["proven"],
                "safe_stop": "candidate", "integration_owner": "X", "delivery_route": ["functional"],
                "required_stage": "functional", "source_handles": ["source:S"],
                "obligation_ids": sorted(key for key, row in domain_state(state)["obligations"].items()
                                         if row["status"] == "active" and "X" in row["owner_work_ids"])}}, "replace-paused")
        with self.assertRaisesRegex(Refusal, "pause|PAUSED"):
            self.authorized(replace, "task.update", "paused")
        self.assertEqual(domain_state(self.state())["task_contracts"]["X"]["active_version"], 0)

    def test_owner_charter_amendment_enables_exact_new_intent_binding(self):
        adopt_intent = self.command("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1"},
                                    "intent-adopt")
        self.authorized(adopt_intent, "outcome.adopt", "intent")
        outcome = {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O1", "revision": 1,
            "previous_outcome_id": None, "intent_id": "I1", "summary": "Working result", "benefits": ["owner value"],
            "guarantees": ["verified"], "tradeoffs": [], "obligations": []}
        self.service.submit_agent(self.command("domain.outcome-proposed", outcome, "outcome-proposal"))
        self.authorized(self.command("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1",
            "outcome_id": "O1", "obligation_dispositions": []}, "outcome-adopt"), "outcome.adopt", "outcome")

        intent2 = {"schema": "zap-domain/intent-proposed/1", "intent_id": "I2", "revision": 2,
            "previous_intent_id": "I1", "summary": "Revised owner intent", "beneficiaries": ["owner"],
            "values": ["new owner value"], "constraints": ["preserve mandate"], "source_refs": ["charter-2"]}
        self.service.submit_agent(self.command("domain.intent-proposed", intent2, "intent-2-proposal"))
        outcome2 = {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
            "previous_outcome_id": "O1", "intent_id": "I2", "summary": "Result for amended intent",
            "benefits": ["new owner value"], "guarantees": ["verified"], "tradeoffs": [], "obligations": []}
        self.service.submit_agent(self.command("domain.outcome-proposed", outcome2, "outcome-2-proposal"))

        def review(review_id, policy_revision):
            state = self.state(); domain = domain_state(state)
            knowledge = knowledge_snapshot(state, [])
            dispositions = [{"obligation_id": key, "disposition": "retained", "successor_ids": [],
                             "unmet_portion": "", "reason": "still required"}
                            for key, row in sorted(domain["obligations"].items()) if row["status"] == "active"]
            return {"schema": "zap-domain/review-proposed/1", "review_id": review_id, "previous_review_id": None,
                "signals": ["owner intent amendment"], "captures": {"base_sha256": state["base_sha256"],
                    "zap_revision": state["revision"], "domain_revision": domain["revision"], "intent_id": "I1",
                    "outcome_id": "O1", "policy_revision": policy_revision, "source_captures": [], "jobs": []},
                "knowledge": {"before": None,
                    "after": {"region_ids": [], **knowledge},
                    "new_region_ids": [], "affected_dependencies": [], "closure_complete": True},
                "alternatives": [{"id": "amended", "description": "Use amended intent", "value": "owner selected",
                    "feasibility": "feasible", "remaining_cost": "bounded", "risks": [], "unknowns": []}],
                "chosen": "amended", "decision": {"kind": "pivot_outcome", "rationale": "owner amended intent"},
                "transition": {"intent_id": "I2", "outcome_id": "O2", "obligation_dispositions": dispositions,
                    "ownership_changes": [], "work_changes": [], "preserved_evidence_ids": [], "job_reconciliation": [],
                    "tradeoffs": [], "preserved_benefits": ["verified value"]}, "next_trigger": "integration boundary"}

        review1 = review("REV1", 1)
        self.service.submit_agent(self.command("domain.review-proposed", review1, "review-1"))
        with self.assertRaisesRegex(Refusal, "owner binding"):
            self.authorized(self.command("domain.review-applied", {"schema": "zap-domain/review-applied/1",
                "review_id": "REV1", "expected_domain_revision": domain_state(self.state())["revision"]},
                "review-1-apply"), "adaptive.apply", "review-before-amendment")

        policy = active_policy(self.state())
        charter2 = copy.deepcopy(self.charter)
        charter2.update({"revision": 2, "parent_sha256": policy["charter_sha256"], "intent": "Revised owner intent",
                         "expected_outcome": {"outcome_id": "O2", "summary": "Result for amended intent"},
                         "intent_binding": {"intent_id": "I2", "sha256": intent_fingerprint(intent2)}})
        self.service.submit_control(self.command("control.charter-amended", {"charter": charter2}, "charter-amend"),
                                    credential_id="owner-cred", credential=self.token)
        review2 = review("REV2", 2)
        self.service.submit_agent(self.command("domain.review-proposed", review2, "review-2"))
        self.authorized(self.command("domain.review-applied", {"schema": "zap-domain/review-applied/1",
            "review_id": "REV2", "expected_domain_revision": domain_state(self.state())["revision"]},
            "review-2-apply"), "adaptive.apply", "review-after-amendment")
        self.assertEqual((domain_state(self.state())["active_intent_id"], domain_state(self.state())["active_outcome_id"]),
                         ("I2", "O2"))


if __name__ == "__main__":
    unittest.main()
