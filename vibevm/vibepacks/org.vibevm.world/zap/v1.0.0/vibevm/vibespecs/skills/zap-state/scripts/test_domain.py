"""Focused positive and negative tests for the pure ZAP domain reducers."""
from __future__ import annotations

import copy
from contextlib import ExitStack
import unittest
from unittest import mock

from zaplib.common import Refusal
from zaplib.domain import (
    DOMAIN_EVENT_SCHEMAS, DOMAIN_HANDLERS, DOMAIN_OPERATIONS,
    SPARSE_REVIEW_TRANSITION_SCHEMA, build_sparse_review_transition,
    current_acceptance_coverage, domain_frontier, domain_state, intent_fingerprint,
)
from zaplib import domain_adaptive, domain_deferrals, domain_graph, domain_proof, domain_reuse, domain_work
from zaplib.knowledge import knowledge_snapshot
from zaplib.domain_model import work_is_accepted
from zaplib.records import CORE_HANDLERS, apply_command, compose_handlers, initial_state

HANDLERS = compose_handlers(CORE_HANDLERS, DOMAIN_HANDLERS)


def task(work_id):
    return {
        "id": work_id, "title": work_id, "goal": "Deliver behavior", "read_paths": ["input"],
        "write_paths": ["output"], "steps": ["Implement"], "positive_cases": ["works"],
        "negative_cases": ["refuses"], "checks": ["focused check"], "acceptance": ["accepted"],
        "safe_stop": "candidate", "commit_subject": "feat: fixture", "notes": [],
    }


def make_state(single=False):
    ids = ["X"] if single else ["R", "T", "U", "V"]
    root = ids[0]
    nodes = [{
        "id": root, "parent": "", "title": "Campaign", "kind": "campaign",
        "state": "candidate" if single else "planned", "order": 0, "depends_on": [],
        "mandates": ["OWNER"], "acceptance": ["Campaign result"], "evidence": [],
    }]
    if not single:
        nodes += [
            {"id": "T", "parent": "R", "title": "Producer", "kind": "atom", "state": "candidate",
             "order": 1, "depends_on": [], "mandates": ["OWNER"], "acceptance": ["Producer behavior"], "evidence": []},
            {"id": "U", "parent": "R", "title": "Legacy accepted", "kind": "atom", "state": "accepted",
             "order": 2, "depends_on": [], "mandates": ["OWNER"], "acceptance": ["Legacy behavior"], "evidence": ["legacy:proof"]},
            {"id": "V", "parent": "R", "title": "Consumer", "kind": "atom", "state": "planned",
             "order": 3, "depends_on": ["T"], "mandates": ["OWNER"], "acceptance": ["Consumer behavior"], "evidence": []},
        ]
    plan = {
        "schema": 1, "plan_id": "fixture", "revision": 7, "root_node": root,
        "current_node": "X" if single else "T",
        "mandate": [{"id": "OWNER", "text": "Keep owner value", "disposition": "owned", "nodes": ids}],
        "node": nodes,
    }
    contracts = {key: task(key) for key in ids if key != "U"}
    return initial_state({"plan": plan, "task_contracts": contracts}, "a" * 64)


class DomainTests(unittest.TestCase):
    def setUp(self):
        self.state = make_state()
        self.actions = []
        self.denied = set()
        self.mutable = None
        self.essential = set()
        self.intent_binding_id = None
        self.policy_revision = 1
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        for module in (domain_graph, domain_work, domain_deferrals, domain_proof, domain_adaptive):
            self.stack.enter_context(mock.patch.object(module, "require_action", side_effect=self.require))
        self.stack.enter_context(mock.patch.object(domain_proof, "_applicability", side_effect=self.applicability))
        self.stack.enter_context(mock.patch.object(
            domain_proof, "_source_captures",
            side_effect=lambda state, refs, **kwargs: [{"source_id": key, "sha256": "b" * 64} for key in refs],
        ))
        self.stack.enter_context(mock.patch.object(domain_reuse, "_sources_current", side_effect=self.reuse_sources))
        self.stack.enter_context(mock.patch.object(domain_reuse, "_captures_current", side_effect=self.reuse_captures))
        self.stack.enter_context(mock.patch.object(domain_adaptive, "_check_sources", return_value=None))
        self.stack.enter_context(mock.patch.object(domain_adaptive, "_builder_policy",
                                                   side_effect=lambda state: self.require(state, "adaptive.apply")))
        self.sources_applicable = True

    def require(self, state, action):
        self.actions.append(action)
        if action in self.denied:
            raise Refusal("AUTH", f"denied {action}")
        domain = domain_state(state)
        mutable = sorted(domain["obligations"]) if self.mutable is None else sorted(self.mutable)
        binding_id = self.intent_binding_id or domain["active_intent_id"]
        if binding_id is None and domain["intents"]:
            binding_id = max(domain["intents"], key=lambda key: domain["intents"][key]["revision"])
        binding = None
        if binding_id is not None:
            required = DOMAIN_EVENT_SCHEMAS["domain.intent-proposed"]["required"]
            proposal = {key: domain["intents"][binding_id][key] for key in required}
            binding = {"intent_id": binding_id, "sha256": intent_fingerprint(proposal)}
        return {
            "campaign_id": "fixture", "base_sha256": state["base_sha256"], "revision": self.policy_revision,
            "charter_id": "charter-1", "charter_sha256": ("c" if self.policy_revision == 1 else "d") * 64,
            "allowed_actions": [action],
            "intent_binding": binding,
            "adaptation": {"allow_target_revision": True, "mutable_obligations": mutable,
                           "essential_obligations": sorted(self.essential),
                           "allowed_dispositions": ["retained", "replaced", "excluded", "unattainable"]},
        }

    def applicability(self, state, refs):
        return {"status": "applicable" if self.sources_are_applicable else "stale",
                "applicable": self.sources_applicable, "refs": list(refs),
                "stale_refs": [] if self.sources_applicable else list(refs), "unknown_refs": [],
                "incomplete_closure": False}

    def reuse_sources(self, state, refs):
        if not self.sources_are_applicable:
            raise Refusal("DOMAIN_EVIDENCE", "accepted evidence is no longer applicable")

    def reuse_captures(self, state, captures):
        if not self.sources_are_applicable:
            raise Refusal("DOMAIN_EVIDENCE", "preserved proof source content changed")

    @property
    def sources_applicable(self):
        return self.sources_are_applicable

    @sources_applicable.setter
    def sources_applicable(self, value):
        self.sources_are_applicable = value

    def apply(self, kind, payload):
        command = {"event_id": f"event-{self.state['revision'] + 1}", "base_revision": self.state["revision"],
                   "kind": kind, "reason": {"summary": "focused domain test"}, "payload": payload}
        self.state = apply_command(self.state, command, HANDLERS)
        return self.state

    def intent(self):
        self.apply("domain.intent-proposed", {
            "schema": "zap-domain/intent-proposed/1", "intent_id": "I1", "revision": 1,
            "previous_intent_id": None, "summary": "Useful campaign", "beneficiaries": ["owner"],
            "values": ["verified value"], "constraints": ["preserve guarantees"], "source_refs": ["owner:charter"],
        })
        self.apply("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1"})

    def activate(self, *, single=False):
        if single:
            self.state = make_state(single=True)
        self.intent()
        owner = "X" if single else "T"
        self.apply("domain.outcome-proposed", {
            "schema": "zap-domain/outcome-proposed/1", "outcome_id": "O1", "revision": 1,
            "previous_outcome_id": None, "intent_id": "I1", "summary": "Working result",
            "benefits": ["owner value"], "guarantees": ["verified"], "tradeoffs": [],
            "obligations": [{"id": "NEW", "statement": "New behavior", "essential": False,
                             "source_refs": ["owner:charter"], "owners": [{"work_id": owner, "role": "implementation"}]}],
        })
        self.apply("domain.outcome-adopted", {
            "schema": "zap-domain/outcome-adopted/1", "outcome_id": "O1", "obligation_dispositions": [],
        })

    def contract(self, work_id, stage="functional", obligations=None):
        obligations = self.obligations(work_id) if obligations is None else list(obligations)
        return {
            "schema": "zap-task-contract/1", "contract_id": f"contract:{work_id}", "work_id": work_id,
            "title": "Bounded work", "goal": "Deliver the full behavior", "read_subjects": ["subject:input"],
            "write_subjects": ["subject:output"], "resources": ["cpu"], "steps": ["implement"],
            "positive_cases": ["works"], "negative_cases": ["refuses invalid input"],
            "checks": ["focused test"], "acceptance": ["full behavior proven"], "safe_stop": "candidate",
            "integration_owner": work_id, "delivery_route": [stage], "required_stage": stage,
            "source_handles": ["source:S"], "obligation_ids": sorted(obligations),
        }

    def obligations(self, work_id):
        domain = domain_state(self.state)
        return sorted({row["obligation_id"] for row in domain["ownership"]
                       if row["work_id"] == work_id and domain["obligations"][row["obligation_id"]]["status"] == "active"})

    def evidence(self, work_id, obligations, stage="functional", evidence_id="E"):
        self.apply("evidence.recorded", {"id": evidence_id, "claim": "Focused behavior passes", "subject": work_id,
                                         "result": "observed_pass", "artifact_refs": ["artifact:trace"], "node_refs": [work_id]})
        self.apply("domain.evidence-adjudicated", {
            "schema": "zap-domain/evidence-adjudicated/1", "evidence_id": evidence_id, "expected_revision": -1,
            "disposition": "accepted", "applies_to": {"outcome_id": domain_state(self.state)["active_outcome_id"],
                "obligation_ids": obligations, "work_ids": [work_id], "stage": stage, "scope": "exact fixture"},
            "source_refs": ["S"], "method": {"argv": ["python", "-m", "unittest"], "target": "domain",
                "toolchain": "python-3.11", "environment": "fixture", "subjects": [work_id], "cases": ["positive", "negative"]},
            "limitations": [],
        })

    def accept_single_work(self):
        self.activate(single=True)
        obligations = self.obligations("X")
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "X",
                                                      "expected_version": 0, "contract": self.contract("X")})
        self.evidence("X", obligations)
        self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
            "work_id": "X", "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E"],
            "obligation_ids": obligations, "scope": "complete fixture", "summary": "stage accepted"})
        self.apply("domain.work-accepted", {"schema": "zap-domain/work-accepted/1", "acceptance_id": "A1", "work_id": "X",
            "outcome_id": "O1", "stage_acceptance_id": "S1", "evidence_ids": ["E"], "obligation_ids": obligations,
            "integration_acceptance_ids": [], "summary": "complete work accepted"})
        return obligations

    def test_legacy_projection_is_lazy_lossless_and_sourced(self):
        before = copy.deepcopy(self.state)
        domain = domain_state(self.state)
        self.assertEqual(self.state, before)
        self.assertEqual(domain["schema"], "zap-domain/1")
        self.assertIn("OWNER", domain["obligations"])
        self.assertIn("acceptance:T:0", domain["obligations"])
        self.assertEqual(domain["legacy_acceptance"]["U"]["adjudication"], "legacy_assertion")
        self.assertEqual(domain["task_contracts"]["T"]["active_version"], 0)

    def test_machine_schemas_cover_exact_handler_registry(self):
        self.assertEqual(set(DOMAIN_EVENT_SCHEMAS), set(DOMAIN_HANDLERS))
        self.assertTrue(all(not row["additionalProperties"] and "schema" in row["required"]
                            for row in DOMAIN_EVENT_SCHEMAS.values()))
        self.assertEqual(DOMAIN_OPERATIONS["domain.materialize-review-transition"]["input_schema"],
                         SPARSE_REVIEW_TRANSITION_SCHEMA)

    def test_proposals_are_data_only_but_adoption_requires_policy(self):
        self.denied.add("outcome.adopt")
        self.apply("domain.intent-proposed", {
            "schema": "zap-domain/intent-proposed/1", "intent_id": "I1", "revision": 1,
            "previous_intent_id": None, "summary": "Useful", "beneficiaries": ["owner"], "values": ["value"],
            "constraints": [], "source_refs": [],
        })
        with self.assertRaisesRegex(Refusal, "denied"):
            self.apply("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1"})
        with self.assertRaises(Refusal):
            self.apply("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I1", "actor": "owner"})

    def test_outcome_revision_requires_complete_authorized_dispositions(self):
        self.activate()
        self.apply("domain.outcome-proposed", {
            "schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
            "previous_outcome_id": "O1", "intent_id": "I1", "summary": "Revised result",
            "benefits": ["owner value"], "guarantees": ["verified"], "tradeoffs": ["replace implementation"],
            "obligations": [{"id": "NEW2", "statement": "Replacement behavior", "essential": False,
                "source_refs": ["owner:charter"], "owners": [{"work_id": "T", "role": "implementation"}]}],
        })
        active = sorted(key for key, row in domain_state(self.state)["obligations"].items() if row["status"] == "active")
        dispositions = [{"obligation_id": key, "disposition": "retained", "successor_ids": [],
                         "unmet_portion": "", "reason": "still required"} for key in active]
        next(row for row in dispositions if row["obligation_id"] == "NEW").update(
            disposition="replaced", successor_ids=["NEW2"], unmet_portion="Old form replaced", reason="delegated tradeoff")
        self.essential = {"NEW"}
        with self.assertRaisesRegex(Refusal, "essential"):
            self.apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "O2",
                                                  "obligation_dispositions": dispositions})
        self.essential = set()
        with self.assertRaisesRegex(Refusal, "every active"):
            self.apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "O2",
                                                  "obligation_dispositions": dispositions[:-1]})
        self.apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "O2",
                                              "obligation_dispositions": dispositions})
        domain = domain_state(self.state)
        self.assertEqual(domain["active_outcome_id"], "O2")
        self.assertEqual(domain["obligations"]["NEW"]["status"], "replaced")
        self.assertEqual(domain["obligations"]["NEW"]["dispositions"][-1]["successor_ids"], ["NEW2"])
        self.assertEqual(domain["original_outcome_id"], "O1")

    def test_lowering_requires_current_coverage_and_leaf_contracts(self):
        self.activate()
        obligations = self.obligations("V")
        child = {"id": "V.1", "parent": "V", "title": "Implement consumer", "kind": "atom", "state": "planned",
                 "order": 1, "depends_on": [], "acceptance": ["consumer proven"], "required_stage": "functional"}
        coverage = [{"obligation_id": key, "assignments": [{"work_id": "V.1", "role": "implementation"}]}
                    for key in obligations]
        payload = {"schema": "zap-domain/plan-lowered/1", "parent_id": "V", "nodes": [child], "edges": [],
                   "coverage": coverage,
                   "contracts": [self.contract("V.1", obligations=obligations + ["acceptance:V.1:0"])],
                   "integration_owner": "V.1"}
        with self.assertRaisesRegex(Refusal, "not covered"):
            self.apply("domain.plan-lowered", {**payload, "coverage": coverage[:-1]})
        with self.assertRaisesRegex(Refusal, "task contract"):
            self.apply("domain.plan-lowered", {**payload, "contracts": []})
        self.apply("domain.plan-lowered", payload)
        domain = domain_state(self.state)
        self.assertIn("V.1", domain["work_nodes"])
        self.assertEqual(domain["task_contracts"]["V.1"]["active_version"], 1)
        self.assertNotIn("V.1", domain_frontier(self.state), "inherited T dependency must still block the child")
        nested_obligations = self.obligations("V.1")
        nested = {"id": "V.1.1", "parent": "V.1", "title": "Nested proof", "kind": "atom", "state": "planned",
                  "order": 1, "depends_on": [], "acceptance": ["nested behavior"], "required_stage": "functional"}
        self.apply("domain.plan-lowered", {"schema": "zap-domain/plan-lowered/1", "parent_id": "V.1",
            "nodes": [nested], "edges": [],
            "coverage": [{"obligation_id": key, "assignments": [{"work_id": "V.1.1", "role": "verification"}]}
                         for key in nested_obligations],
            "contracts": [self.contract("V.1.1", obligations=nested_obligations + ["acceptance:V.1.1:0"])],
            "integration_owner": "V.1.1"})
        self.assertIn("V.1.1", domain_state(self.state)["work_nodes"])

    def test_contract_rename_and_transitions_use_exact_action_classes(self):
        self.activate()
        contract = self.contract("T")
        with self.assertRaisesRegex(Refusal, "every current work obligation"):
            self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                "expected_version": 0, "contract": {**contract, "obligation_ids": contract["obligation_ids"][:-1]}})
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                                                      "expected_version": 0, "contract": contract})
        self.apply("domain.work-renamed", {"schema": "zap-domain/work-renamed/1", "work_id": "V",
                                           "expected_title": "Consumer", "new_title": "Revised consumer"})
        self.apply("domain.work-transitioned", {"schema": "zap-domain/work-transitioned/1", "work_id": "T",
                                                "from_state": "candidate", "to_state": "ready", "successor_ids": []})
        self.apply("domain.work-dispatched", {"schema": "zap-domain/work-dispatched/1", "work_id": "T",
                                              "from_state": "ready", "job_id": "JOB1"})
        self.assertIn("task.update", self.actions)
        self.assertIn("plan.lower", self.actions)
        self.assertIn("work.dispatch", self.actions)
        with self.assertRaises(Refusal):
            self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                                                          "expected_version": 0, "contract": self.contract("T")})

    def test_producer_pass_and_declared_maturity_cannot_accept_work(self):
        self.activate()
        obligations = self.obligations("T")
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                                                      "expected_version": 0, "contract": self.contract("T")})
        self.apply("evidence.recorded", {"id": "E", "claim": "producer says pass", "subject": "T",
                                         "result": "observed_pass", "artifact_refs": ["artifact:trace"], "node_refs": ["T"]})
        with self.assertRaisesRegex(Refusal, "centrally accepted"):
            self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
                "work_id": "T", "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E"],
                "obligation_ids": obligations, "scope": "fixture", "summary": "stage done"})
        self.state["classifications"]["T"] = {"work_type": "change", "maturity": "productized", "assertion_status": "declared"}
        with self.assertRaisesRegex(Refusal, "centrally accepted"):
            self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
                "work_id": "T", "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E"],
                "obligation_ids": obligations, "scope": "fixture", "summary": "stage done"})

    def test_applicable_evidence_stage_and_central_acceptance(self):
        self.activate()
        obligations = self.obligations("T")
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                                                      "expected_version": 0, "contract": self.contract("T")})
        self.evidence("T", obligations)
        self.sources_applicable = False
        with self.assertRaisesRegex(Refusal, "no longer applicable"):
            self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
                "work_id": "T", "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E"],
                "obligation_ids": obligations, "scope": "fixture", "summary": "stage done"})
        self.sources_applicable = True
        self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
            "work_id": "T", "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E"],
            "obligation_ids": obligations, "scope": "fixture", "summary": "stage done"})
        self.apply("domain.work-accepted", {"schema": "zap-domain/work-accepted/1", "acceptance_id": "A1",
            "work_id": "T", "outcome_id": "O1", "stage_acceptance_id": "S1", "evidence_ids": ["E"],
            "obligation_ids": obligations, "integration_acceptance_ids": [], "summary": "centrally accepted"})
        self.assertIn("A1", domain_state(self.state)["acceptances"])
        self.assertIn("V", domain_frontier(self.state))

    def test_scoped_checks_collectively_cover_stage_and_work_obligations(self):
        self.activate(single=True)
        obligations = self.obligations("X")
        self.assertGreaterEqual(len(obligations), 2)
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "X",
                                                      "expected_version": 0, "contract": self.contract("X")})
        split = ([obligations[0]], obligations[1:])
        self.evidence("X", split[0], evidence_id="E1")
        self.evidence("X", split[1], evidence_id="E2")
        stage = {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1", "work_id": "X",
            "stage": "functional", "outcome_id": "O1", "evidence_ids": ["E1"],
            "obligation_ids": obligations, "scope": "complementary checks", "summary": "stage accepted"}
        with self.assertRaisesRegex(Refusal, "collectively cover"):
            self.apply("domain.stage-accepted", stage)
        stage["evidence_ids"] = ["E1", "E2"]
        self.apply("domain.stage-accepted", stage)
        acceptance = {"schema": "zap-domain/work-accepted/1", "acceptance_id": "A1", "work_id": "X",
            "outcome_id": "O1", "stage_acceptance_id": "S1", "evidence_ids": ["E1"],
            "obligation_ids": obligations, "integration_acceptance_ids": [], "summary": "work accepted"}
        with self.assertRaisesRegex(Refusal, "collectively cover"):
            self.apply("domain.work-accepted", acceptance)
        acceptance["evidence_ids"] = ["E1", "E2"]
        self.apply("domain.work-accepted", acceptance)
        self.assertTrue(work_is_accepted(self.state, domain_state(self.state), "X"))

    def test_adjudication_rejects_wrong_subject_and_missing_artifact(self):
        self.activate()
        obligations = self.obligations("T")
        for evidence_id, node_refs, artifacts, message in (
            ("E1", ["V"], ["artifact:trace"], "work subject"),
            ("E2", ["T"], [], "artifact reference"),
        ):
            self.apply("evidence.recorded", {"id": evidence_id, "claim": "claimed pass", "subject": node_refs[0],
                "result": "observed_pass", "artifact_refs": artifacts, "node_refs": node_refs})
            with self.assertRaisesRegex(Refusal, message):
                self.apply("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1",
                    "evidence_id": evidence_id, "expected_revision": -1, "disposition": "accepted",
                    "applies_to": {"outcome_id": "O1", "obligation_ids": obligations, "work_ids": ["T"],
                                   "stage": "functional", "scope": "fixture"},
                    "source_refs": ["S"], "method": {"argv": ["verify"], "target": "domain",
                        "toolchain": "fixture", "environment": "test", "subjects": ["T"], "cases": ["negative"]},
                    "limitations": []})

    def test_deferral_covers_transfers_and_closes_with_applicable_evidence(self):
        self.activate()
        obligations = self.obligations("V")
        self.apply("domain.deferral-created", {"schema": "zap-domain/deferral-created/1", "deferral_id": "D1",
            "outcome_id": "O1", "obligation_ids": obligations, "work_ids": ["V"], "scope": "consumer shell",
            "reason": "stage boundary", "current_guarantees": ["no consumer yet"], "responsible_party": "team-a",
            "closure_requirement": "focused proof"})
        self.apply("domain.deferral-transferred", {"schema": "zap-domain/deferral-transferred/1", "deferral_id": "D1",
            "from_responsible_party": "team-a", "to_responsible_party": "team-b", "to_work_ids": ["V"], "reason": "ownership change"})
        self.evidence("V", obligations, stage="prototype")
        self.apply("domain.deferral-closed", {"schema": "zap-domain/deferral-closed/1", "deferral_id": "D1",
                                               "evidence_ids": ["E"], "reason": "closure requirement proven"})
        self.assertEqual(domain_state(self.state)["deferrals"]["D1"]["status"], "closed")

    def review_payload(self, *, pivot=False, dispositions=None):
        domain = domain_state(self.state)
        knowledge = knowledge_snapshot(self.state, [])
        transition = {"intent_id": None, "outcome_id": "O2" if pivot else None, "obligation_dispositions": dispositions or [],
                      "ownership_changes": [], "work_changes": [], "preserved_evidence_ids": [],
                      "preserved_stage_acceptance_ids": [], "preserved_work_acceptance_ids": [],
                      "preserved_integration_acceptance_ids": [], "job_reconciliation": [],
                      "tradeoffs": ["bounded change"] if pivot else [], "preserved_benefits": ["owner value"]}
        return {"schema": "zap-domain/review-proposed/1", "review_id": "REV1", "previous_review_id": None,
            "signals": ["new information"], "captures": {"base_sha256": self.state["base_sha256"],
                "zap_revision": self.state["revision"], "domain_revision": domain["revision"],
                "intent_id": domain["active_intent_id"], "outcome_id": domain["active_outcome_id"],
                "policy_revision": 1, "source_captures": [], "jobs": []},
            "knowledge": {"before": None,
                "after": {"region_ids": [], **knowledge},
                "new_region_ids": [], "affected_dependencies": [], "closure_complete": False},
            "alternatives": [{"id": "keep", "description": "Keep route", "value": "known",
                "feasibility": "feasible", "remaining_cost": "bounded", "risks": [], "unknowns": ["future cost"]},
                {"id": "pivot", "description": "Revise route", "value": "higher", "feasibility": "feasible",
                 "remaining_cost": "lower", "risks": ["switching"], "unknowns": []}],
            "chosen": "pivot" if pivot else "keep", "decision": {"kind": "pivot_outcome" if pivot else "keep_route",
                "rationale": "best current value"}, "transition": transition, "next_trigger": "next integration boundary"}

    def sparse_request(self, *, changed=None, preserve=True):
        return {"schema": "zap-domain/sparse-review-transition/1", "intent_id": None, "outcome_id": "O2",
            "changed_dispositions": changed or [], "ownership_changes": [], "work_changes": [],
            "preserved_evidence_ids": ["E"] if preserve else [],
            "preserved_stage_acceptance_ids": ["S1"] if preserve else [],
            "preserved_work_acceptance_ids": ["A1"] if preserve else [],
            "preserved_integration_acceptance_ids": [], "job_reconciliation": [], "tradeoffs": [],
            "preserved_benefits": ["owner value"]}

    def propose_o2(self, *, guarantees=None, obligations=None):
        self.apply("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2",
            "revision": 2, "previous_outcome_id": "O1", "intent_id": "I1", "summary": "Locally revised result",
            "benefits": ["owner value"], "guarantees": guarantees or ["verified"], "tradeoffs": ["local adjustment"],
            "obligations": obligations or []})

    def apply_sparse_pivot(self, request=None):
        transition = build_sparse_review_transition(self.state, request or self.sparse_request())
        payload = self.review_payload(pivot=True)
        payload["transition"] = transition
        if transition["preserved_evidence_ids"]:
            payload["captures"]["source_captures"] = [{"source_id": "S", "sha256": "b" * 64}]
        self.apply("domain.review-proposed", payload)
        self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                               "expected_domain_revision": domain_state(self.state)["revision"]})

    def test_sparse_builder_expands_large_denominator_deterministically(self):
        self.state = make_state(single=True)
        domain = domain_state(self.state)
        template = copy.deepcopy(domain["obligations"]["OWNER"])
        domain["obligations"] = {}
        for index in range(1292):
            key = f"O{index:04d}"
            domain["obligations"][key] = {**copy.deepcopy(template), "id": key, "statement": f"obligation {index}"}
        domain["active_outcome_id"] = "O1"
        domain["outcome_revisions"] = {
            "O1": {"outcome_id": "O1", "status": "active", "revision": 1, "guarantees": ["verified"]},
            "O2": {"outcome_id": "O2", "status": "proposed", "revision": 2, "previous_outcome_id": "O1",
                   "guarantees": ["verified"], "obligations": []},
        }
        self.state["extensions"]["domain"] = domain
        changed = [{"obligation_id": "O1291", "disposition": "excluded", "successor_ids": [],
                    "unmet_portion": "explicitly removed", "reason": "authorized sparse change"}]
        transition = build_sparse_review_transition(self.state, self.sparse_request(changed=changed, preserve=False))
        self.assertEqual(1292, len(transition["obligation_dispositions"]))
        self.assertEqual("O0000", transition["obligation_dispositions"][0]["obligation_id"])
        self.assertEqual("excluded", transition["obligation_dispositions"][-1]["disposition"])
        self.assertEqual(1291, sum(row["disposition"] == "retained" for row in transition["obligation_dispositions"]))

    def test_pivot_selectively_reuses_proof_without_rewriting_history(self):
        obligations = self.accept_single_work()
        before = copy.deepcopy(domain_state(self.state))
        self.propose_o2()
        self.apply_sparse_pivot()
        domain = domain_state(self.state)
        self.assertEqual("O1", domain["evidence_adjudications"]["E"]["applies_to"]["outcome_id"])
        self.assertEqual("O1", domain["stages"]["S1"]["outcome_id"])
        self.assertEqual("O1", domain["acceptances"]["A1"]["outcome_id"])
        self.assertEqual(before["evidence_adjudications"]["E"], domain["evidence_adjudications"]["E"])
        self.assertEqual("O2", domain["reuse_witnesses"]["evidence"]["E"][0]["to_outcome_id"])
        self.assertEqual("O2", domain["reuse_witnesses"]["stages"]["S1"][0]["to_outcome_id"])
        self.assertEqual("O2", domain["reuse_witnesses"]["acceptances"]["A1"][0]["to_outcome_id"])
        self.assertTrue(work_is_accepted(self.state, domain, "X"))
        coverage = current_acceptance_coverage(self.state)
        self.assertEqual((["X"], ["A1"]), (coverage["work_ids"], coverage["acceptance_ids"]))
        self.apply("domain.campaign-closed", {"schema": "zap-domain/campaign-closed/1", "closure_id": "C1",
            "classification": "revised", "active_outcome_id": "O2", "actual_benefit": "owner value delivered",
            "obligation_results": [{"obligation_id": key, "result": "accepted", "unmet_portion": "",
                "successor_ids": [], "evidence_ids": ["E"]} for key in sorted(obligations)],
            "acceptance_ids": ["A1"], "integration_acceptance_ids": [], "deferral_ids": [],
            "promotion_ids": [], "final_gate_evidence_ids": ["E"], "summary": "reused proof remains current"})
        self.assertEqual("closed", domain_state(self.state)["closure"]["status"])

    def test_pivot_without_explicit_reuse_does_not_make_old_acceptance_current(self):
        obligations = self.accept_single_work()
        self.propose_o2()
        self.apply_sparse_pivot(self.sparse_request(preserve=False))
        domain = domain_state(self.state)
        self.assertFalse(work_is_accepted(self.state, domain, "X"))
        self.assertEqual([], current_acceptance_coverage(self.state)["work_ids"])
        with self.assertRaisesRegex(Refusal, "central work acceptance"):
            self.apply("domain.campaign-closed", {"schema": "zap-domain/campaign-closed/1", "closure_id": "C1",
                "classification": "revised", "active_outcome_id": "O2", "actual_benefit": "claimed",
                "obligation_results": [{"obligation_id": key, "result": "accepted", "unmet_portion": "",
                    "successor_ids": [], "evidence_ids": ["E"]} for key in sorted(obligations)],
                "acceptance_ids": ["A1"], "integration_acceptance_ids": [], "deferral_ids": [],
                "promotion_ids": [], "final_gate_evidence_ids": ["E"], "summary": "must refuse false carryover"})

    def test_sparse_reuse_refuses_changed_guarantee_new_obligation_and_disposition(self):
        obligations = self.accept_single_work()
        self.propose_o2(guarantees=["different guarantee"])
        with self.assertRaisesRegex(Refusal, "guarantee"):
            build_sparse_review_transition(self.state, self.sparse_request())

        self.state = make_state(single=True)
        obligations = self.accept_single_work()
        self.propose_o2(obligations=[{"id": "O2-NEW", "statement": "New uncovered work", "essential": False,
            "source_refs": ["owner:charter"], "owners": [{"work_id": "X", "role": "implementation"}]}])
        with self.assertRaisesRegex(Refusal, "new work obligation"):
            build_sparse_review_transition(self.state, self.sparse_request())

        self.state = make_state(single=True)
        obligations = self.accept_single_work()
        self.propose_o2()
        changed = [{"obligation_id": obligations[0], "disposition": "excluded", "successor_ids": [],
                    "unmet_portion": "removed", "reason": "authorized local change"}]
        with self.assertRaisesRegex(Refusal, "obligation was changed"):
            build_sparse_review_transition(self.state, self.sparse_request(changed=changed))

    def test_sparse_reuse_refuses_changed_contract_and_stale_source(self):
        self.accept_single_work()
        revised = self.contract("X")
        revised["goal"] = "Changed work subject"
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "X",
                                                       "expected_version": 1, "contract": revised})
        self.propose_o2()
        with self.assertRaisesRegex(Refusal, "revalidation|contract"):
            build_sparse_review_transition(self.state, self.sparse_request())

        self.state = make_state(single=True)
        self.accept_single_work()
        self.propose_o2()
        self.sources_applicable = False
        with self.assertRaisesRegex(Refusal, "no longer applicable"):
            build_sparse_review_transition(self.state, self.sparse_request())

    def test_adaptive_pivot_is_atomic_and_cannot_lose_obligations(self):
        self.activate()
        self.apply("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
            "previous_outcome_id": "O1", "intent_id": "I1", "summary": "Better result", "benefits": ["owner value"],
            "guarantees": ["verified"], "tradeoffs": [], "obligations": []})
        active = sorted(key for key, row in domain_state(self.state)["obligations"].items() if row["status"] == "active")
        dispositions = [{"obligation_id": key, "disposition": "retained", "successor_ids": [],
                         "unmet_portion": "", "reason": "still required"} for key in active]
        before_review = copy.deepcopy(self.state)
        self.apply("domain.review-proposed", self.review_payload(pivot=True, dispositions=dispositions[:-1]))
        expected = domain_state(self.state)["revision"]
        with self.assertRaisesRegex(Refusal, "every active"):
            self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                                  "expected_domain_revision": expected})
        self.state = before_review
        self.apply("domain.review-proposed", self.review_payload(pivot=True, dispositions=dispositions))
        expected = domain_state(self.state)["revision"]
        self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                              "expected_domain_revision": expected})
        domain = domain_state(self.state)
        self.assertEqual(domain["active_outcome_id"], "O2")
        self.assertEqual(domain["reviews"]["REV1"]["job_effect_status"], "planned_not_performed_by_reducer")

    def test_intent_revision_requires_atomic_adaptive_outcome_revision(self):
        self.activate()
        self.apply("domain.intent-proposed", {"schema": "zap-domain/intent-proposed/1", "intent_id": "I2", "revision": 2,
            "previous_intent_id": "I1", "summary": "Revised owner intent", "beneficiaries": ["owner"],
            "values": ["greater value"], "constraints": ["preserve guarantees"], "source_refs": ["owner:amendment"]})
        with self.assertRaisesRegex(Refusal, "atomic adaptive"):
            self.apply("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "I2"})
        self.apply("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
            "previous_outcome_id": "O1", "intent_id": "I2", "summary": "Result for revised intent",
            "benefits": ["greater value"], "guarantees": ["verified"], "tradeoffs": [], "obligations": []})
        active = sorted(key for key, row in domain_state(self.state)["obligations"].items() if row["status"] == "active")
        dispositions = [{"obligation_id": key, "disposition": "retained", "successor_ids": [],
                         "unmet_portion": "", "reason": "still required"} for key in active]
        payload = self.review_payload(pivot=True, dispositions=dispositions)
        payload["transition"]["intent_id"] = "I2"
        before_review = copy.deepcopy(self.state)
        self.apply("domain.review-proposed", payload)
        expected = domain_state(self.state)["revision"]
        with self.assertRaisesRegex(Refusal, "owner binding"):
            self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                                  "expected_domain_revision": expected})
        self.state = before_review
        self.intent_binding_id = "I2"
        self.policy_revision = 2
        payload = self.review_payload(pivot=True, dispositions=dispositions)
        payload["transition"]["intent_id"] = "I2"
        payload["captures"]["policy_revision"] = 2
        self.apply("domain.review-proposed", payload)
        expected = domain_state(self.state)["revision"]
        self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                              "expected_domain_revision": expected})
        domain = domain_state(self.state)
        self.assertEqual((domain["active_intent_id"], domain["active_outcome_id"]), ("I2", "O2"))

    def test_keep_route_review_can_reconcile_jobs_without_performing_effects(self):
        self.activate()
        self.state["extensions"]["runtime"] = {"jobs": {"J1": {"status": "running", "attempt_id": "ATT1"}}}
        payload = self.review_payload()
        payload["captures"]["jobs"] = [{"job_id": "J1", "status": "running", "attempt_id": "ATT1"}]
        payload["transition"]["job_reconciliation"] = [
            {"job_id": "J1", "action": "finish_compatible", "safe_boundary": "candidate", "reason": "still useful"}
        ]
        before_jobs = copy.deepcopy(self.state["extensions"]["runtime"]["jobs"])
        self.apply("domain.review-proposed", payload)
        expected = domain_state(self.state)["revision"]
        self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                              "expected_domain_revision": expected})
        self.assertEqual(self.state["extensions"]["runtime"]["jobs"], before_jobs)
        self.assertEqual(domain_state(self.state)["reviews"]["REV1"]["decision"]["kind"], "keep_route")

    def test_explicit_ownership_transfer_and_drop_controls_dependency_readiness(self):
        self.activate()
        self.assertNotIn("V", domain_frontier(self.state))
        domain = domain_state(self.state)
        relations = [row for row in domain["ownership"] if row["work_id"] == "T"
                     and domain["obligations"][row["obligation_id"]]["status"] == "active"]
        payload = self.review_payload()
        payload["transition"]["ownership_changes"] = [
            {"obligation_id": row["obligation_id"], "from_work_id": "T",
             "assignments": [{"work_id": "R", "role": row["role"]}], "reason": "replacement owns the obligation"}
            for row in relations
        ]
        payload["transition"]["work_changes"] = [
            {"work_id": "T", "operation": "drop", "order": None, "successor_ids": [], "reason": "authorized route change"}
        ]
        self.apply("domain.review-proposed", payload)
        expected = domain_state(self.state)["revision"]
        self.apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": "REV1",
                                              "expected_domain_revision": expected})
        self.assertEqual(domain_state(self.state)["work_updates"]["T"]["state"], "dropped")
        self.assertIn("V", domain_frontier(self.state))

    def test_imported_dropped_prerequisite_does_not_unlock_consumer(self):
        self.state = make_state()
        next(row for row in self.state["plan"]["node"] if row["id"] == "T")["state"] = "dropped"
        self.assertNotIn("V", domain_frontier(self.state))

    def test_deferral_can_become_inapplicable_only_after_authorized_obligation_disposition(self):
        self.activate()
        self.apply("domain.deferral-created", {"schema": "zap-domain/deferral-created/1", "deferral_id": "D1",
            "outcome_id": "O1", "obligation_ids": ["NEW"], "work_ids": ["T"], "scope": "optional behavior",
            "reason": "stage boundary", "current_guarantees": ["current behavior preserved"], "responsible_party": "team-a",
            "closure_requirement": "resolve outcome applicability"})
        with self.assertRaisesRegex(Refusal, "active obligations"):
            self.apply("domain.deferral-inapplicable", {"schema": "zap-domain/deferral-inapplicable/1", "deferral_id": "D1",
                                                         "outcome_id": "O1", "reason": "claimed obsolete"})
        self.apply("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
            "previous_outcome_id": "O1", "intent_id": "I1", "summary": "Authorized narrower result",
            "benefits": ["owner value"], "guarantees": ["verified"], "tradeoffs": ["omit optional behavior"], "obligations": []})
        active = sorted(key for key, row in domain_state(self.state)["obligations"].items() if row["status"] == "active")
        dispositions = [{"obligation_id": key, "disposition": "retained", "successor_ids": [], "unmet_portion": "",
                         "reason": "still required"} for key in active]
        next(row for row in dispositions if row["obligation_id"] == "NEW").update(
            disposition="excluded", unmet_portion="optional behavior omitted", reason="delegated tradeoff")
        self.apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "O2",
                                              "obligation_dispositions": dispositions})
        self.apply("domain.deferral-inapplicable", {"schema": "zap-domain/deferral-inapplicable/1", "deferral_id": "D1",
                                                     "outcome_id": "O2", "reason": "obligation explicitly excluded"})
        self.assertEqual(domain_state(self.state)["deferrals"]["D1"]["status"], "inapplicable")

    def test_integration_acceptance_marks_legacy_inputs_as_assertions(self):
        self.state = make_state()
        for node in self.state["plan"]["node"]:
            if node["id"] in {"T", "V"}:
                node["state"] = "accepted"; node["evidence"] = [f"legacy:{node['id']}"]
        self.activate()
        obligations = self.obligations("R")
        self.evidence("R", obligations)
        payload = {"schema": "zap-domain/integration-accepted/1", "integration_id": "INT1", "work_id": "R",
            "child_work_ids": ["T", "U", "V"], "legacy_child_ids": ["T", "U", "V"], "outcome_id": "O1",
            "evidence_ids": ["E"], "obligation_ids": obligations, "summary": "integrated legacy inputs checked"}
        with self.assertRaisesRegex(Refusal, "current central acceptance"):
            self.apply("domain.integration-accepted", {**payload, "legacy_child_ids": ["U", "V"]})
        self.apply("domain.integration-accepted", payload)
        self.assertTrue(domain_state(self.state)["integration_acceptances"]["INT1"]["legacy_inputs_are_assertions"])

    def test_fact_promotion_records_external_adapter_metadata_only(self):
        self.activate()
        obligations = self.obligations("T")
        self.evidence("T", obligations)
        self.apply("fact.recorded", {"id": "F1", "statement": "accepted project fact", "status": "observed",
            "node_refs": ["T"], "evidence_refs": ["E"], "source_refs": ["S"]})
        self.apply("fact.recorded", {"id": "F2", "statement": "producer assertion", "status": "asserted",
            "node_refs": ["T"], "evidence_refs": ["E"], "source_refs": ["S"]})
        with self.assertRaisesRegex(Refusal, "observed fact"):
            self.apply("domain.fact-promotion-recorded", {"schema": "zap-domain/fact-promotion-recorded/1",
                "promotion_id": "P0", "fact_id": "F2", "target_handle": "spec://project/facts#f2",
                "content_sha256": "b" * 64, "evidence_ids": ["E"], "adapter_receipt": "receipt:external",
                "summary": "must not promote producer assertion"})
        self.apply("domain.fact-promotion-recorded", {"schema": "zap-domain/fact-promotion-recorded/1",
            "promotion_id": "P1", "fact_id": "F1", "target_handle": "spec://project/facts#f1",
            "content_sha256": "b" * 64, "evidence_ids": ["E"], "adapter_receipt": "receipt:external",
            "summary": "permanent project source written by adapter"})
        promotion = domain_state(self.state)["promotions"]["P1"]
        self.assertEqual(promotion["effect"], "confirmed_by_external_adapter_receipt")

    def _complete_single(self, revised=False):
        self.activate(single=True)
        if revised:
            self.apply("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "O2", "revision": 2,
                "previous_outcome_id": "O1", "intent_id": "I1", "summary": "Revised complete result",
                "benefits": ["owner value"], "guarantees": ["verified"], "tradeoffs": [], "obligations": []})
            active = sorted(key for key, row in domain_state(self.state)["obligations"].items() if row["status"] == "active")
            self.apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "O2",
                "obligation_dispositions": [{"obligation_id": key, "disposition": "retained", "successor_ids": [],
                    "unmet_portion": "", "reason": "still required"} for key in active]})
        outcome = domain_state(self.state)["active_outcome_id"]
        obligations = self.obligations("X")
        self.apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "X",
                                                      "expected_version": 0, "contract": self.contract("X")})
        self.evidence("X", obligations)
        self.apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": "S1",
            "work_id": "X", "stage": "functional", "outcome_id": outcome, "evidence_ids": ["E"],
            "obligation_ids": obligations, "scope": "complete fixture", "summary": "stage accepted"})
        self.apply("domain.work-accepted", {"schema": "zap-domain/work-accepted/1", "acceptance_id": "A1", "work_id": "X",
            "outcome_id": outcome, "stage_acceptance_id": "S1", "evidence_ids": ["E"], "obligation_ids": obligations,
            "integration_acceptance_ids": [], "summary": "complete work accepted"})
        results = [{"obligation_id": key, "result": "accepted", "unmet_portion": "", "successor_ids": [],
                    "evidence_ids": ["E"]} for key in sorted(domain_state(self.state)["obligations"])]
        self.apply("domain.campaign-closed", {"schema": "zap-domain/campaign-closed/1", "closure_id": "C1",
            "classification": "revised" if revised else "original", "active_outcome_id": outcome,
            "actual_benefit": "complete verified value", "obligation_results": results, "acceptance_ids": ["A1"],
            "integration_acceptance_ids": [], "deferral_ids": [], "promotion_ids": [], "final_gate_evidence_ids": ["E"],
            "summary": "truthful complete closure"})

    def test_original_and_revised_success_require_current_complete_acceptance(self):
        self._complete_single(revised=False)
        self.assertEqual(domain_state(self.state)["closure"]["classification"], "original")
        self.state = make_state(single=True); self.actions.clear()
        self._complete_single(revised=True)
        self.assertEqual(domain_state(self.state)["closure"]["classification"], "revised")

    def test_partial_closure_is_truthful_and_success_cannot_hide_unmet_work(self):
        self.activate(single=True)
        obligations = self.obligations("X")
        self.evidence("X", obligations)
        results = [{"obligation_id": key, "result": "retained_unmet", "unmet_portion": "still missing",
                    "successor_ids": [], "evidence_ids": []} for key in sorted(domain_state(self.state)["obligations"])]
        payload = {"schema": "zap-domain/campaign-closed/1", "closure_id": "C1", "classification": "partial",
            "active_outcome_id": "O1", "actual_benefit": "useful evidence", "obligation_results": results,
            "acceptance_ids": [], "integration_acceptance_ids": [], "deferral_ids": [], "promotion_ids": [],
            "final_gate_evidence_ids": ["E"], "summary": "partial value with explicit gaps"}
        lying = {**payload, "classification": "original"}
        with self.assertRaisesRegex(Refusal, "unmet active"):
            self.apply("domain.campaign-closed", lying)
        self.apply("domain.campaign-closed", payload)
        self.assertEqual(domain_state(self.state)["closure"]["classification"], "partial")
        self.state = make_state(single=True); self.actions.clear(); self.activate(single=True)
        obligations = self.obligations("X"); self.evidence("X", obligations)
        self.apply("evidence.recorded", {"id": "FAIL", "claim": "final path is unattainable", "subject": "X",
            "result": "observed_fail", "artifact_refs": ["artifact:failure"], "node_refs": ["X"]})
        self.apply("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1",
            "evidence_id": "FAIL", "expected_revision": -1, "disposition": "accepted",
            "applies_to": {"outcome_id": "O1", "obligation_ids": obligations, "work_ids": ["X"],
                           "stage": "functional", "scope": "captured unattainable conditions"},
            "source_refs": ["S"], "method": {"argv": ["verify", "unattainable"], "target": "domain",
                "toolchain": "fixture", "environment": "test", "subjects": ["X"], "cases": ["failure"]},
            "limitations": ["captured conditions only"]})
        unreachable = {**payload, "closure_id": "C2", "classification": "unreachable",
                       "obligation_results": [{"obligation_id": key, "result": "retained_unmet",
                           "unmet_portion": "unattainable in captured conditions", "successor_ids": [], "evidence_ids": ["FAIL"]}
                           for key in sorted(domain_state(self.state)["obligations"])],
                       "final_gate_evidence_ids": ["FAIL"], "summary": "unattainable with explicit conditions"}
        self.apply("domain.campaign-closed", unreachable)
        self.assertEqual(domain_state(self.state)["closure"]["classification"], "unreachable")


if __name__ == "__main__":
    unittest.main()
