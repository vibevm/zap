from __future__ import annotations

import copy
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.control import (
    ACTION_CLASSES,
    CONTROL_EVENT_SCHEMAS,
    CONTROL_HANDLERS,
    active_policy,
    assess_action,
    control_state,
    convert_legacy_stop_policy,
    require_action,
)
from zaplib.records import CORE_HANDLERS, apply_command, compose_handlers, initial_state


H64 = "a" * 64
HANDLERS = compose_handlers(CORE_HANDLERS, CONTROL_HANDLERS)


def base_state() -> dict:
    mandate = {"id": "legacy-1", "text": "Do not start without authority", "disposition": "active", "nodes": ["root"]}
    plan = {
        "schema": 1,
        "plan_id": "campaign-1",
        "revision": 0,
        "root_node": "root",
        "current_node": "root",
        "node": [{
            "id": "root", "parent": "", "title": "Root", "kind": "campaign",
            "state": "planned", "order": 0, "depends_on": [],
            "mandates": ["legacy-1"], "acceptance": ["accepted"], "evidence": [],
        }],
        "mandate": [mandate],
    }
    return initial_state({"plan": plan, "task_contracts": {}}, H64)


def command(state: dict, kind: str, payload: dict, event_id: str) -> dict:
    return {
        "event_id": event_id,
        "base_revision": state["revision"],
        "kind": kind,
        "reason": {"summary": f"test {kind}"},
        "payload": payload,
    }


def charter(state: dict, *, revision: int = 1, parent_sha256: str | None = None, legacy_disposition: str = "retained") -> dict:
    mandate = state["plan"]["mandate"][0]
    replacement = "replacement-1" if legacy_disposition == "superseded" else None
    return {
        "schema": "zap-charter/1",
        "charter_id": "charter-1",
        "campaign_id": "campaign-1",
        "base_sha256": H64,
        "revision": revision,
        "parent_sha256": parent_sha256,
        "intent": "Deliver the accepted outcome",
        "intent_binding": {"intent_id": "intent-1", "sha256": "b" * 64},
        "expected_outcome": {"outcome_id": "outcome-1", "summary": "A useful result"},
        "delegation": {
            "allowed_actions": list(ACTION_CLASSES),
            "adaptation": {
                "allow_target_revision": True,
                "mutable_obligations": [],
                "essential_obligations": ["legacy-1"],
                "allowed_dispositions": ["excluded", "replaced", "retained", "unattainable"],
            },
        },
        "legacy_authority": [{
            "id": "legacy-1",
            "disposition": legacy_disposition,
            "source_sha256": sha(packed(mandate)),
            "replacement_ref": replacement,
        }],
        "stop_policy": {
            "schema": "zap-stop-policy/1",
            "policy_id": "policy-1",
            "revision": 1,
            "rules": [
                {
                    "id": "format-risk",
                    "applies_to_actions": ["work.dispatch"],
                    "scope": "run",
                    "timing": "before_action",
                    "when": {"all": [
                        {"eq": {"field": "changes_public_format", "value": True}},
                        {"eq": {"field": "migration_affects_user_data", "value": True}},
                    ]},
                },
                {
                    "id": "two-failed-approaches",
                    "applies_to_actions": ["work.dispatch"],
                    "scope": "campaign",
                    "timing": "before_next_action",
                    "when": {"failed_approaches": {"gte": 2}},
                },
            ],
        },
    }


def activate(state: dict) -> dict:
    body = charter(state)
    state = apply_command(state, command(state, "control.charter-drafted", {"charter": body}, "draft-1"), HANDLERS)
    return apply_command(state, command(state, "control.charter-activated", {
        "charter_id": "charter-1",
        "charter_revision": 1,
        "charter_sha256": sha(packed(body)),
        "campaign_id": "campaign-1",
        "base_sha256": H64,
    }, "activate-1"), HANDLERS)


def action(action_class: str = "work.dispatch", *, action_id: str = "action-1", payload_sha256: str = H64, sources: list | None = None) -> dict:
    return {
        "schema": "zap-action/1",
        "action_id": action_id,
        "action_class": action_class,
        "campaign_id": "campaign-1",
        "base_sha256": H64,
        "charter_revision": 1,
        "payload_sha256": payload_sha256,
        "source_captures": [] if sources is None else sources,
        "branch_id": None,
        "run_id": "run-1" if action_class == "work.dispatch" else None,
        "problem_id": "problem-1" if action_class == "work.dispatch" else None,
    }


def assessment(assessment_id: str, *, values: dict | None = None, phase: str = "before_action", drains: list | None = None) -> dict:
    return {
        "schema": "zap-assessment/1",
        "assessment_id": assessment_id,
        "policy_id": "policy-1",
        "policy_revision": 1,
        "phase": phase,
        "values": {} if values is None else values,
        "drain_targets": [] if drains is None else drains,
    }


class CharterTests(unittest.TestCase):
    def test_every_control_handler_has_one_machine_payload_descriptor(self):
        self.assertEqual(set(CONTROL_HANDLERS), set(CONTROL_EVENT_SCHEMAS))

    def test_draft_cannot_execute_and_unresolved_legacy_cannot_activate(self):
        state = base_state()
        body = charter(state, legacy_disposition="owner_decision_required")
        state = apply_command(state, command(state, "control.charter-drafted", {"charter": body}, "draft-unresolved"), HANDLERS)
        self.assertIsNone(active_policy(state))
        with self.assertRaisesRegex(Refusal, "no owner charter"):
            require_action(state, "work.dispatch")
        with self.assertRaisesRegex(Refusal, "requires owner decision"):
            apply_command(state, command(state, "control.charter-activated", {
                "charter_id": "charter-1", "charter_revision": 1,
                "charter_sha256": sha(packed(body)),
                "campaign_id": "campaign-1", "base_sha256": H64,
            }, "activate-unresolved"), HANDLERS)

    def test_active_policy_has_stable_domain_shape(self):
        state = activate(base_state())
        policy = active_policy(state)
        self.assertEqual(
            set(policy),
            {"campaign_id", "base_sha256", "revision", "allowed_actions", "adaptation", "charter_id", "charter_sha256", "intent_binding", "stop_policy", "pause", "pauses"},
        )
        self.assertEqual(policy["campaign_id"], "campaign-1")
        self.assertEqual(policy["revision"], 1)
        self.assertEqual(policy["intent_binding"], {"intent_id": "intent-1", "sha256": "b" * 64})
        self.assertIn("adaptive.apply", policy["allowed_actions"])
        self.assertEqual(policy["adaptation"]["essential_obligations"], ["legacy-1"])

    def test_wrong_activation_hash_campaign_or_revision_fails(self):
        state = base_state()
        body = charter(state)
        state = apply_command(state, command(state, "control.charter-drafted", {"charter": body}, "draft-wrong"), HANDLERS)
        for change in (
            {"charter_sha256": "b" * 64},
            {"campaign_id": "other"},
            {"charter_revision": 2},
        ):
            payload = {
                "charter_id": "charter-1", "charter_revision": 1,
                "charter_sha256": sha(packed(body)),
                "campaign_id": "campaign-1", "base_sha256": H64,
            }
            payload.update(change)
            with self.assertRaises(Refusal):
                apply_command(state, command(state, "control.charter-activated", payload, "activate-wrong"), HANDLERS)

    def test_legacy_classification_must_cover_every_imported_mandate(self):
        state = base_state()
        body = charter(state)
        body["legacy_authority"] = []
        with self.assertRaisesRegex(Refusal, "incomplete"):
            apply_command(state, command(state, "control.charter-drafted", {"charter": body}, "draft-incomplete"), HANDLERS)

    def test_legacy_policy_conversion_is_explicit_and_campaign_wide(self):
        converted = convert_legacy_stop_policy({
            "schema": "zap-stop/1",
            "status": "example_unapproved",
            "rules": [{
                "id": "legacy-rule", "scope": "run", "timing": "before_action",
                "when": {"eq": {"field": "risk", "value": True}},
            }],
        }, policy_id="policy-converted", revision=1, applies_to_actions=["work.dispatch"])
        self.assertEqual(converted["schema"], "zap-stop-policy/1")
        self.assertEqual(converted["rules"][0]["scope"], "campaign")

    def test_legacy_control_only_charter_may_omit_intent_binding(self):
        state = base_state()
        body = charter(state)
        body.pop("intent_binding")
        state = apply_command(state, command(state, "control.charter-drafted", {"charter": body}, "draft-legacy-charter"), HANDLERS)
        state = apply_command(state, command(state, "control.charter-activated", {
            "charter_id": "charter-1", "charter_revision": 1,
            "charter_sha256": sha(packed(body)), "campaign_id": "campaign-1",
            "base_sha256": H64,
        }, "activate-legacy-charter"), HANDLERS)
        self.assertIsNone(active_policy(state)["intent_binding"])


class AssessmentTests(unittest.TestCase):
    def test_unknown_blocks_only_affected_action_and_after_action_is_too_late(self):
        state = activate(base_state())
        unknown = assess_action(state, action(), assessment("unknown", values={
            "changes_public_format": True,
            "migration_affects_user_data": None,
        }))
        self.assertEqual(unknown["policy_result"], "needs_evidence")
        unaffected = assess_action(state, action("work.accept"), assessment("unaffected"))
        self.assertEqual(unaffected["policy_result"], "clear")
        late = assess_action(state, action(), assessment("late", phase="after_action", values={
            "changes_public_format": True,
            "migration_affects_user_data": True,
        }))
        self.assertEqual(late["policy_result"], "too_late")
        self.assertTrue(late["evaluated"]["action_already_occurred"])
        self.assertFalse(late["prevented_action"])
        self.assertFalse(late["prevention_performed"])
        before = assess_action(state, action(), assessment("before", values={
            "changes_public_format": True,
            "migration_affects_user_data": True,
        }))
        self.assertEqual(before["policy_result"], "pause")
        self.assertTrue(before["would_prevent"])
        self.assertFalse(before["prevention_performed"])
        self.assertFalse(before["action_admitted"])

    def test_source_capture_must_match_persisted_knowledge(self):
        state = activate(base_state())
        source_action = action("work.accept", sources=[{"source_id": "source-1", "sha256": "b" * 64}])
        missing = assess_action(state, source_action, assessment("source-missing"))
        self.assertEqual(missing["policy_result"], "needs_evidence")
        state["extensions"]["knowledge"] = {"sources": {"source-1": {"sha256": "b" * 64, "status": "current"}}}
        current = assess_action(state, source_action, assessment("source-current"))
        self.assertEqual(current["policy_result"], "clear")
        state["extensions"]["knowledge"]["sources"]["source-1"]["status"] = "invalidated"
        invalidated = assess_action(state, source_action, assessment("source-invalid"))
        self.assertEqual(invalidated["policy_result"], "needs_evidence")

    def test_drain_targets_are_derived_from_persisted_runtime(self):
        state = activate(base_state())
        state["extensions"]["runtime"] = {"jobs": {
            "run-1": {"job_id": "run-1", "run_id": "run-1", "state": "running"},
            "done": {"job_id": "done", "run_id": "done", "state": "completed"},
        }}
        risky = {"changes_public_format": True, "migration_affects_user_data": True}
        with self.assertRaisesRegex(Refusal, "drain"):
            assess_action(state, action(), assessment("omitted-run", values=risky))
        result = assess_action(state, action(), assessment("captured-run", values=risky, drains=["run-1"]))
        self.assertEqual(result["delivery"]["required"], ["run-1"])


class PauseAndApproachTests(unittest.TestCase):
    def test_semantic_approach_outcome_requires_known_problem_and_usable_evidence(self):
        state = activate(base_state())
        state["evidence"]["bad"] = {"result": "inconclusive"}
        payload = {
            "record_id": "bad-record", "problem_id": "root", "approach_id": "approach-bad",
            "strategy_sha256": sha(b"bad-strategy"), "outcome": "failed",
            "evidence_refs": ["bad"],
        }
        with self.assertRaisesRegex(Refusal, "usable observed evidence"):
            apply_command(state, command(state, "control.approach-outcome-recorded", payload, "bad-evidence"), HANDLERS)
        payload["problem_id"] = "unknown-problem"
        state["evidence"]["bad"]["result"] = "observed_fail"
        with self.assertRaisesRegex(Refusal, "known stable plan node"):
            apply_command(state, command(state, "control.approach-outcome-recorded", payload, "bad-problem"), HANDLERS)

    def test_rule_pause_is_sticky_across_a_domain_pivot(self):
        state = activate(base_state())
        state["extensions"]["runtime"] = {"jobs": {
            "run-1": {"job_id": "run-1", "run_id": "run-1", "state": "running"},
        }}
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": action(),
            "assessment": assessment("rule-pause", values={
                "changes_public_format": True,
                "migration_affects_user_data": True,
            }, drains=["run-1"]),
        }, "rule-pause-event"), HANDLERS)
        pause = control_state(state)["pauses"]["pause:rule-pause"]
        self.assertEqual(pause["scope"], "run")
        require_action(state, "outcome.adopt")
        require_action(state, "work.dispatch", run_id="run-2")
        with self.assertRaisesRegex(Refusal, "sticky pause"):
            require_action(state, "outcome.adopt", run_id="run-1")

    def test_multiple_scoped_pauses_preserve_unrelated_work_and_campaign_pause_overrides(self):
        state = activate(base_state())
        risky = {"changes_public_format": True, "migration_affects_user_data": True}
        first_action = action()
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": first_action, "assessment": assessment("run-one", values=risky),
        }, "pause-run-one"), HANDLERS)
        second_action = action(action_id="action-run-two")
        second_action["run_id"] = "run-2"
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": second_action, "assessment": assessment("run-two", values=risky),
        }, "pause-run-two"), HANDLERS)
        self.assertEqual(control_state(state)["active_pauses"], ["pause:run-one", "pause:run-two"])
        require_action(state, "work.dispatch", run_id="run-3")

        state["evidence"]["ev-1"] = {"result": "observed_fail"}
        for index in (1, 2):
            state = apply_command(state, command(state, "control.approach-outcome-recorded", {
                "record_id": f"override-record-{index}", "problem_id": "root",
                "approach_id": f"override-approach-{index}",
                "strategy_sha256": sha(f"override-strategy-{index}".encode()),
                "outcome": "failed", "evidence_refs": ["ev-1"],
            }, f"override-event-{index}"), HANDLERS)
        third_action = action(action_id="action-run-three")
        third_action["run_id"] = "run-3"
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": third_action,
            "assessment": assessment("campaign-override", values={
                "changes_public_format": False,
                "migration_affects_user_data": False,
            }),
        }, "campaign-override-event"), HANDLERS)
        control = control_state(state)
        self.assertIn("pause:campaign-override", control["active_pauses"])
        self.assertEqual(control["pauses"]["pause:campaign-override"]["scope"], "campaign")
        with self.assertRaisesRegex(Refusal, "sticky pause"):
            require_action(state, "work.accept")

    def test_owner_stop_drains_whole_campaign_and_exact_resume_is_one_time(self):
        state = activate(base_state())
        state["extensions"]["runtime"] = {"jobs": {
            "run-1": {"job_id": "run-1", "state": "running"},
            "run-2": {"job_id": "run-2", "state": "unknown_effect"},
        }}
        with self.assertRaises(Refusal):
            apply_command(state, command(state, "control.owner-stop-requested", {
                "pause_id": "pause-owner", "campaign_id": "campaign-1", "base_sha256": H64,
                "charter_revision": 1, "reason": "owner stop", "drain_targets": ["run-1"],
            }, "stop-omitted"), HANDLERS)
        state = apply_command(state, command(state, "control.owner-stop-requested", {
            "pause_id": "pause-owner", "campaign_id": "campaign-1", "base_sha256": H64,
            "charter_revision": 1, "reason": "owner stop", "drain_targets": ["run-1", "run-2"],
        }, "stop-owner"), HANDLERS)
        pause = control_state(state)["pauses"]["pause-owner"]
        self.assertEqual(pause["scope"], "campaign")
        self.assertEqual(pause["delivery"]["state"], "pending")
        self.assertEqual(pause["actual_safe_state"]["state"], "unknown")
        with self.assertRaises(Refusal):
            require_action(state, "work.accept")
        for run_id in ("run-1", "run-2"):
            pause = control_state(state)["pauses"]["pause-owner"]
            state = apply_command(state, command(state, "control.pause-delivery-acknowledged", {
                "pause_id": "pause-owner", "pause_sha256": pause["pause_sha256"],
                "subject_id": run_id, "state": "delivered", "receipt_sha256": sha(run_id.encode()),
            }, f"deliver-{run_id}"), HANDLERS)
            pause = control_state(state)["pauses"]["pause-owner"]
            state = apply_command(state, command(state, "control.pause-safe-state-acknowledged", {
                "pause_id": "pause-owner", "pause_sha256": pause["pause_sha256"],
                "run_id": run_id, "state": "safe", "receipt_sha256": sha((run_id + "-safe").encode()),
            }, f"safe-{run_id}"), HANDLERS)
        pause = control_state(state)["pauses"]["pause-owner"]
        state = apply_command(state, command(state, "control.pause-resumed", {
            "pause_id": "pause-owner", "pause_sha256": pause["pause_sha256"], "decision": "continue",
        }, "resume-owner"), HANDLERS)
        self.assertIsNone(control_state(state)["active_pause"])
        with self.assertRaises(Refusal):
            apply_command(state, command(state, "control.pause-resumed", {
                "pause_id": "pause-owner",
                "pause_sha256": control_state(state)["pauses"]["pause-owner"]["pause_sha256"],
                "decision": "again",
            }, "resume-again"), HANDLERS)

    def test_two_distinct_failures_pause_globally_provider_errors_do_not_and_epoch_keeps_history(self):
        state = activate(base_state())
        state["evidence"]["ev-1"] = {"result": "observed_fail"}
        for index, outcome in enumerate(("provider_error", "retry", "failed", "failed"), start=1):
            state = apply_command(state, command(state, "control.approach-outcome-recorded", {
                "record_id": f"record-{index}",
                "problem_id": "root",
                "approach_id": f"approach-{index}",
                "strategy_sha256": sha(f"strategy-{index}".encode()),
                "outcome": outcome,
                "evidence_refs": [] if outcome in {"provider_error", "retry"} else ["ev-1"],
            }, f"approach-event-{index}"), HANDLERS)
            if outcome in {"provider_error", "retry"}:
                interim = assess_action(state, action(), assessment(f"interim-{index}", values={
                    "changes_public_format": False,
                    "migration_affects_user_data": False,
                }))
                self.assertEqual(interim["policy_result"], "clear")
        result = assess_action(state, action(), assessment("two-failures", values={
            "changes_public_format": False,
            "migration_affects_user_data": False,
        }))
        self.assertEqual(result["policy_result"], "pause")
        self.assertEqual(result["effective_scope"], "campaign")
        after_observation = assess_action(state, action(), assessment("two-failures-after", phase="after_action", values={
            "changes_public_format": False,
            "migration_affects_user_data": False,
        }))
        self.assertEqual(after_observation["policy_result"], "pause")
        self.assertTrue(after_observation["would_prevent"])
        history_before = copy.deepcopy(control_state(state)["approach_history"])
        state = apply_command(state, command(state, "control.approach-epoch-advanced", {
            "problem_id": "root", "expected_epoch": 0, "new_epoch": 1, "reason": "owner selected a new counting epoch",
        }, "epoch-1"), HANDLERS)
        self.assertEqual(control_state(state)["approach_history"], history_before)
        result = assess_action(state, action(), assessment("new-epoch", values={
            "changes_public_format": False,
            "migration_affects_user_data": False,
        }))
        self.assertEqual(result["policy_result"], "clear")

    def test_same_strategy_hash_does_not_count_as_two_approaches(self):
        state = activate(base_state())
        state["evidence"]["ev-1"] = {"result": "observed_fail"}
        strategy = sha(b"one stable strategy")
        for index in (1, 2):
            state = apply_command(state, command(state, "control.approach-outcome-recorded", {
                "record_id": f"same-record-{index}",
                "problem_id": "root",
                "approach_id": f"same-approach-{index}",
                "strategy_sha256": strategy,
                "outcome": "failed",
                "evidence_refs": ["ev-1"],
            }, f"same-event-{index}"), HANDLERS)
        result = assess_action(state, action(), assessment("same-strategy", values={
            "changes_public_format": False,
            "migration_affects_user_data": False,
        }))
        self.assertEqual(result["policy_result"], "clear")

    def test_two_failure_assessment_materializes_a_global_sticky_pause(self):
        state = activate(base_state())
        state["evidence"]["ev-1"] = {"result": "observed_fail"}
        for index in (1, 2):
            state = apply_command(state, command(state, "control.approach-outcome-recorded", {
                "record_id": f"global-record-{index}",
                "problem_id": "root",
                "approach_id": f"global-approach-{index}",
                "strategy_sha256": sha(f"global-strategy-{index}".encode()),
                "outcome": "failed",
                "evidence_refs": ["ev-1"],
            }, f"global-event-{index}"), HANDLERS)
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": action(),
            "assessment": assessment("global-pause", values={
                "changes_public_format": False,
                "migration_affects_user_data": False,
            }),
        }, "global-pause-event"), HANDLERS)
        control = control_state(state)
        self.assertEqual(control["active_pause"], "pause:global-pause")
        self.assertEqual(control["pauses"]["pause:global-pause"]["scope"], "campaign")

    def test_unresolved_legacy_approach_requires_evidence(self):
        state = base_state()
        state["approaches"]["legacy-a"] = {
            "id": "legacy-a", "problem_id": "root", "strategy_key": "same strategy",
            "outcome": "unresolved", "verdicts": [],
        }
        state = activate(state)
        result = assess_action(state, action(), assessment("legacy-unknown", values={
            "changes_public_format": False,
            "migration_affects_user_data": False,
        }))
        self.assertEqual(result["policy_result"], "needs_evidence")
        self.assertTrue(result["rules"][1]["details"][0]["unresolved_legacy"])

    def test_one_shot_exception_and_exact_reservation_rebind(self):
        state = activate(base_state())
        state = apply_command(state, command(state, "control.owner-stop-requested", {
            "pause_id": "pause-ex", "campaign_id": "campaign-1", "base_sha256": H64,
            "charter_revision": 1, "reason": "inspect", "drain_targets": [],
        }, "stop-ex"), HANDLERS)
        exact_action = action("work.accept", action_id="exception-action")
        pause = control_state(state)["pauses"]["pause-ex"]
        state = apply_command(state, command(state, "control.action-exception-granted", {
            "exception_id": "exception-1", "pause_id": "pause-ex", "pause_sha256": pause["pause_sha256"],
            "action_id": exact_action["action_id"], "action_class": exact_action["action_class"],
            "payload_sha256": exact_action["payload_sha256"],
            "source_captures_sha256": sha(packed(exact_action["source_captures"])),
            "charter_revision": 1, "reason": "one exact action",
        }, "grant-ex"), HANDLERS)
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": exact_action, "assessment": assessment("assess-ex"),
        }, "assess-ex"), HANDLERS)
        logical_hash = sha(packed({
            "event_id": "product-1", "kind": "domain.product", "reason": {"summary": "product"}, "payload": {"value": 1},
        }))
        admission_payload = {
            "admission_id": "admission-1", "action": exact_action, "assessment_id": "assess-ex",
            "exception_id": "exception-1", "command_kind": "domain.product",
            "product_event_id": "product-1", "product_command_sha256": logical_hash,
        }
        state = apply_command(state, command(state, "control.action-admitted", admission_payload, "admit-ex"), HANDLERS)
        self.assertTrue(control_state(state)["exceptions"]["exception-1"]["consumed"])
        require_action(state, "work.accept")
        with self.assertRaises(Refusal):
            apply_command(state, command(state, "control.action-admitted", {
                **admission_payload, "admission_id": "admission-2",
            }, "admit-ex-again"), HANDLERS)
        state = apply_command(state, command(state, "knowledge.region-recorded", {
            "id": "unrelated", "question": "unrelated", "node_refs": ["root"],
        }, "unrelated-event"), HANDLERS)
        with self.assertRaises(Refusal):
            require_action(state, "work.accept")
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": exact_action, "assessment": assessment("assess-rebind"),
        }, "assess-rebind"), HANDLERS)
        state = apply_command(state, command(state, "control.action-reservation-rebound", {
            "admission_id": "admission-1", "assessment_id": "assess-rebind",
            "product_event_id": "product-1", "product_command_sha256": logical_hash,
        }, "rebind-1"), HANDLERS)
        require_action(state, "work.accept")

    def test_charter_amendment_invalidates_old_assessment(self):
        state = activate(base_state())
        old_action = action("work.accept")
        state = apply_command(state, command(state, "control.action-assessed", {
            "action": old_action, "assessment": assessment("old-assessment"),
        }, "assess-old"), HANDLERS)
        previous = active_policy(state)
        amended = charter(state, revision=2, parent_sha256=previous["charter_sha256"])
        amended["intent"] = "Revised owner intent"
        state = apply_command(state, command(state, "control.charter-amended", {"charter": amended}, "amend-1"), HANDLERS)
        with self.assertRaisesRegex(Refusal, "invalidated"):
            apply_command(state, command(state, "control.action-admitted", {
                "admission_id": "stale-admission", "action": old_action,
                "assessment_id": "old-assessment", "exception_id": None,
                "command_kind": "domain.product", "product_event_id": "product-stale",
                "product_command_sha256": H64,
            }, "admit-stale"), HANDLERS)


if __name__ == "__main__":
    unittest.main()
