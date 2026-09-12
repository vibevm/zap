from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, exact, packed, sha
from zaplib.control import (
    ACTION_CLASSES,
    CONTROL_HANDLERS,
    COORDINATOR_EVENT_KINDS,
    OWNER_EVENT_KINDS,
    active_policy,
    control_state,
    require_action,
)
from zaplib.records import CORE_HANDLERS, HandlerSpec, compose_handlers
from zaplib.service import ApplicationService, CredentialAuthority
from zaplib.storage import import_mup, load_store, record


def _validate_product(value):
    exact(value, {"value"})
    if not isinstance(value["value"], int):
        raise Refusal("PRODUCT", "value must be an integer")
    return copy.deepcopy(value)


def _apply_product(state, payload, event_id):
    require_action(state, "work.dispatch")
    state["extensions"].setdefault("test_product", {})[event_id] = payload["value"]


def _validate_source(value):
    exact(value, {"source_id", "sha256", "status"})
    if value["status"] not in {"current", "invalidated"}:
        raise Refusal("SOURCE", "invalid source status")
    return copy.deepcopy(value)


def _apply_source(state, payload, _event_id):
    state["extensions"].setdefault("knowledge", {}).setdefault("sources", {})[payload["source_id"]] = {
        "sha256": payload["sha256"],
        "status": payload["status"],
    }


def _validate_note(value):
    exact(value, {"id"})
    return copy.deepcopy(value)


def _apply_note(state, payload, event_id):
    state["extensions"].setdefault("test_notes", {})[payload["id"]] = event_id


def _validate_job_observation(value):
    exact(value, {"job_id", "state"})
    if value["state"] not in {"running", "completed", "actual_stopped", "unknown_effect"}:
        raise Refusal("OBSERVATION", "invalid observed job state")
    return copy.deepcopy(value)


def _apply_job_observation(state, payload, event_id):
    state["extensions"].setdefault("runtime", {}).setdefault("jobs", {})[payload["job_id"]] = {
        "job_id": payload["job_id"],
        "state": payload["state"],
        "observation_event_id": event_id,
    }


TEST_HANDLERS = compose_handlers(
    CORE_HANDLERS,
    CONTROL_HANDLERS,
    HandlerSpec("domain.product", _validate_product, _apply_product),
    HandlerSpec("test.source-set", _validate_source, _apply_source),
    HandlerSpec("test.note", _validate_note, _apply_note),
    HandlerSpec("runtime.job-observed", _validate_job_observation, _apply_job_observation),
)


class CrashBeforeProduct:
    def __init__(self):
        self.crashed = False

    def __call__(self, store, command, handlers):
        if command.get("kind") == "domain.product" and not self.crashed:
            self.crashed = True
            raise OSError("simulated process loss before product append")
        return record(store, command, handlers)


def make_store(parent: Path) -> Path:
    plan = parent / "plan.toml"
    tasks = parent / "tasks"
    store = parent / "store"
    tasks.mkdir()
    plan.write_text(
        """
schema = 1
plan_id = "campaign-1"
revision = 0
root_node = "root"
current_node = "work"

[[mandate]]
id = "legacy-1"
text = "Do not start without authority"
disposition = "active"
nodes = ["root", "work"]

[[node]]
id = "root"
parent = ""
title = "Root"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["legacy-1"]
acceptance = ["accepted"]
evidence = []

[[node]]
id = "work"
parent = "root"
title = "Work"
kind = "atom"
state = "planned"
order = 1
depends_on = []
mandates = ["legacy-1"]
acceptance = ["works"]
evidence = []
""".strip() + "\n",
        encoding="utf-8",
    )
    task = {
        "id": "work",
        "title": "Work",
        "goal": "Do the work",
        "read_paths": [],
        "write_paths": [],
        "steps": ["work"],
        "positive_cases": ["works"],
        "negative_cases": ["does not self-authorize"],
        "checks": ["focused test"],
        "acceptance": ["accepted"],
        "safe_stop": "before effect",
        "commit_subject": "feat: work",
        "notes": [],
    }
    (tasks / "root.json").write_text(json.dumps({"id": "root", "tasks": [task]}), encoding="utf-8")
    import_mup(plan, tasks, store)
    return store


def event(state, kind, payload, event_id):
    return {
        "event_id": event_id,
        "base_revision": state["revision"],
        "kind": kind,
        "reason": {"summary": f"test {kind}"},
        "payload": payload,
    }


def make_charter(state):
    mandate = state["plan"]["mandate"][0]
    return {
        "schema": "zap-charter/1",
        "charter_id": "charter-1",
        "campaign_id": "campaign-1",
        "base_sha256": state["base_sha256"],
        "revision": 1,
        "parent_sha256": None,
        "intent": "Deliver useful work",
        "intent_binding": {"intent_id": "intent-1", "sha256": "b" * 64},
        "expected_outcome": {"outcome_id": "outcome-1", "summary": "Useful result"},
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
            "disposition": "retained",
            "source_sha256": sha(packed(mandate)),
            "replacement_ref": None,
        }],
        "stop_policy": {
            "schema": "zap-stop-policy/1",
            "policy_id": "policy-1",
            "revision": 1,
            "rules": [{
                "id": "two-failed-approaches",
                "applies_to_actions": ["work.dispatch"],
                "scope": "campaign",
                "timing": "before_next_action",
                "when": {"failed_approaches": {"gte": 2}},
            }],
        },
    }


class ServiceFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.store = make_store(Path(self.temp.name))
        owner_binding, self.owner_token = CredentialAuthority.issue(
            "owner-credential", "owner-principal", "owner", "campaign-1",
            control_kinds=OWNER_EVENT_KINDS | COORDINATOR_EVENT_KINDS,
            action_classes=["work.dispatch"],
        )
        coordinator_binding, self.coordinator_token = CredentialAuthority.issue(
            "coordinator-credential", "coordinator-principal", "coordinator", "campaign-1",
            control_kinds=COORDINATOR_EVENT_KINDS,
            action_classes=["work.dispatch"],
        )
        reader_binding, self.reader_token = CredentialAuthority.issue(
            "reader-credential", "reader-principal", "reader", "campaign-1",
        )
        foreign_binding, self.foreign_token = CredentialAuthority.issue(
            "foreign-credential", "foreign-principal", "owner", "other-campaign",
            control_kinds=OWNER_EVENT_KINDS,
        )
        self.trust = CredentialAuthority([owner_binding, coordinator_binding, reader_binding, foreign_binding])
        self.service = ApplicationService(
            self.store,
            TEST_HANDLERS,
            self.trust,
            action_kinds={"domain.product": "work.dispatch"},
            data_kinds={"test.note"},
            observation_kinds={"test.source-set", "runtime.job-observed"},
        )
        self.activate()

    def tearDown(self):
        self.temp.cleanup()

    def state(self):
        return load_store(self.store, TEST_HANDLERS)[0]

    def activate(self):
        state = self.state()
        body = make_charter(state)
        self.service.submit_agent(event(state, "control.charter-drafted", {"charter": body}, "draft-service"))
        state = self.state()
        self.service.submit_control(event(state, "control.charter-activated", {
            "charter_id": "charter-1",
            "charter_revision": 1,
            "charter_sha256": sha(packed(body)),
            "campaign_id": "campaign-1",
            "base_sha256": state["base_sha256"],
        }, "activate-service"), credential_id="owner-credential", credential=self.owner_token)

    def product_inputs(self, event_id="product-1", assessment_id="assessment-1", sources=None):
        state = self.state()
        payload = {"value": 1}
        command = event(state, "domain.product", payload, event_id)
        action = {
            "schema": "zap-action/1",
            "action_id": "action-1",
            "action_class": "work.dispatch",
            "campaign_id": "campaign-1",
            "base_sha256": state["base_sha256"],
            "charter_revision": active_policy(state)["revision"],
            "payload_sha256": sha(packed(payload)),
            "source_captures": [] if sources is None else sources,
            "branch_id": "branch-1",
            "run_id": "run-1",
            "problem_id": "problem-1",
        }
        assessment = {
            "schema": "zap-assessment/1",
            "assessment_id": assessment_id,
            "policy_id": "policy-1",
            "policy_revision": 1,
            "phase": "before_action",
            "values": {},
            "drain_targets": [],
        }
        return command, action, assessment


class AuthorizationTests(ServiceFixture):
    def test_trusted_source_capture_is_allowed_before_charter_activation(self):
        parent = Path(self.temp.name) / "bootstrap"
        parent.mkdir()
        store = make_store(parent)
        service = ApplicationService(
            store, TEST_HANDLERS, self.trust,
            action_kinds={"domain.product": "work.dispatch"},
            data_kinds={"test.note"},
            observation_kinds={"test.source-set", "runtime.job-observed"},
        )
        state = load_store(store, TEST_HANDLERS)[0]
        service.submit_observation(event(state, "test.source-set", {
            "source_id": "bootstrap-source", "sha256": "b" * 64, "status": "current",
        }, "bootstrap-source"), credential_id="coordinator-credential", credential=self.coordinator_token)
        state = load_store(store, TEST_HANDLERS)[0]
        self.assertIsNone(active_policy(state))
        self.assertEqual(state["extensions"]["knowledge"]["sources"]["bootstrap-source"]["sha256"], "b" * 64)

    def test_trusted_transport_observation_is_allowed_while_paused_but_agent_and_reader_fail(self):
        state = self.state()
        self.service.submit_observation(event(state, "runtime.job-observed", {
            "job_id": "job-1", "state": "running",
        }, "job-running"), credential_id="coordinator-credential", credential=self.coordinator_token)
        state = self.state()
        self.service.submit_control(event(state, "control.owner-stop-requested", {
            "pause_id": "pause-drain", "campaign_id": "campaign-1",
            "base_sha256": state["base_sha256"], "charter_revision": 1,
            "reason": "drain existing job", "drain_targets": ["job-1"],
        }, "pause-drain"), credential_id="owner-credential", credential=self.owner_token)
        state = self.state()
        self.service.submit_observation(event(state, "runtime.job-observed", {
            "job_id": "job-1", "state": "completed",
        }, "job-completed"), credential_id="coordinator-credential", credential=self.coordinator_token)
        self.assertEqual(self.state()["extensions"]["runtime"]["jobs"]["job-1"]["state"], "completed")

        forged = event(self.state(), "runtime.job-observed", {
            "job_id": "job-forged", "state": "completed",
        }, "job-forged")
        with self.assertRaisesRegex(Refusal, "trusted transport"):
            self.service.submit_agent(forged)
        with self.assertRaisesRegex(Refusal, "coordinator"):
            self.service.submit_observation(
                forged, credential_id="reader-credential", credential=self.reader_token,
            )

    def test_unclassified_extension_handler_is_refused_at_service_construction(self):
        with self.assertRaisesRegex(Refusal, "unclassified extension"):
            ApplicationService(
                self.store,
                TEST_HANDLERS,
                self.trust,
                data_kinds={"test.note"},
                observation_kinds={"test.source-set", "runtime.job-observed"},
            )

    def test_active_plan_refinement_requires_plan_lower_action_route(self):
        state = self.state()
        with self.assertRaisesRegex(Refusal, "privileged product"):
            self.service.submit_agent(event(state, "plan.refined", {}, "unsafe-refinement"))

    def test_false_owner_label_and_agent_product_route_fail(self):
        state = self.state()
        fake = event(state, "control.pause-resumed", {
            "pause_id": "fake", "pause_sha256": "a" * 64, "decision": "I am owner",
        }, "fake-owner")
        fake["actor"] = "owner"
        with self.assertRaises(Refusal):
            self.service.submit_agent(fake)
        command, _action, _assessment = self.product_inputs()
        command["payload"]["owner"] = True
        with self.assertRaisesRegex(Refusal, "privileged product"):
            self.service.submit_agent(command)

    def test_wrong_credential_campaign_hash_and_revision_fail_without_product(self):
        command, action, assessment = self.product_inputs()
        before = self.state()["revision"]
        with self.assertRaisesRegex(Refusal, "credential"):
            self.service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential="wrong",
            )
        with self.assertRaises(Refusal):
            self.service.submit_control(
                event(self.state(), "control.owner-stop-requested", {
                    "pause_id": "foreign-stop", "campaign_id": "campaign-1",
                    "base_sha256": self.state()["base_sha256"], "charter_revision": 1,
                    "reason": "foreign", "drain_targets": [],
                }, "foreign-stop"),
                credential_id="foreign-credential", credential=self.foreign_token,
            )
        bad_hash = copy.deepcopy(action)
        bad_hash["payload_sha256"] = "b" * 64
        with self.assertRaisesRegex(Refusal, "payload hash"):
            self.service.apply_control_action(
                command, bad_hash, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        bad_campaign = copy.deepcopy(action)
        bad_campaign["campaign_id"] = "other-campaign"
        with self.assertRaises(Refusal):
            self.service.apply_control_action(
                command, bad_campaign, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        bad_revision = copy.deepcopy(action)
        bad_revision["charter_revision"] = 2
        with self.assertRaisesRegex(Refusal, "charter revision"):
            self.service.apply_control_action(
                command, bad_revision, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        stale = copy.deepcopy(command)
        stale["base_revision"] -= 1
        with self.assertRaisesRegex(Refusal, "base revision"):
            self.service.apply_control_action(
                stale, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        self.assertEqual(self.state()["revision"], before)

    def test_reader_credential_is_campaign_scoped_and_has_no_command_power(self):
        principal = self.service.authorize_read(credential_id="reader-credential", credential=self.reader_token)
        self.assertEqual(principal.role, "reader")
        descriptors = json.dumps(self.service.route_descriptors(), sort_keys=True)
        self.assertIn('"domain.product": "work.dispatch"', descriptors)
        self.assertNotIn(self.reader_token, descriptors)
        state = self.state()
        with self.assertRaises(Refusal):
            self.service.submit_control(
                event(state, "control.owner-stop-requested", {
                    "pause_id": "reader-stop", "campaign_id": "campaign-1",
                    "base_sha256": state["base_sha256"], "charter_revision": 1,
                    "reason": "reader cannot stop", "drain_targets": [],
                }, "reader-stop"),
                credential_id="reader-credential", credential=self.reader_token,
            )

    def test_control_exact_retry_precedes_current_cas_but_changed_request_fails(self):
        state = self.state()
        request = event(state, "control.owner-stop-requested", {
            "pause_id": "retry-stop", "campaign_id": "campaign-1",
            "base_sha256": state["base_sha256"], "charter_revision": 1,
            "reason": "retry proof", "drain_targets": [],
        }, "retry-stop-event")
        first = self.service.submit_control(
            request, credential_id="owner-credential", credential=self.owner_token,
        )
        second = self.service.submit_control(
            request, credential_id="owner-credential", credential=self.owner_token,
        )
        self.assertFalse(first["idempotent"])
        self.assertTrue(second["idempotent"])
        changed = copy.deepcopy(request)
        changed["payload"]["reason"] = "changed"
        with self.assertRaisesRegex(Refusal, "another command"):
            self.service.submit_control(
                changed, credential_id="owner-credential", credential=self.owner_token,
            )

    def test_successful_action_never_persists_credential(self):
        command, action, assessment = self.product_inputs()
        result = self.service.apply_control_action(
            command, action, assessment,
            credential_id="coordinator-credential", credential=self.coordinator_token,
        )
        self.assertTrue(result["ok"])
        self.assertEqual(self.state()["extensions"]["test_product"]["product-1"], 1)
        journal = (self.store / "events.jsonl").read_text(encoding="utf-8")
        self.assertNotIn(self.coordinator_token, journal)
        repeated = self.service.apply_control_action(
            command, action, assessment,
            credential_id="coordinator-credential", credential=self.coordinator_token,
        )
        self.assertTrue(repeated["idempotent"])

    def test_after_action_assessment_never_authorizes_replay_of_old_effect(self):
        command, action, assessment = self.product_inputs(event_id="product-after", assessment_id="assessment-after")
        assessment["phase"] = "after_action"
        with self.assertRaisesRegex(Refusal, "not eligible"):
            self.service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        self.assertNotIn("product-after", self.state()["extensions"].get("test_product", {}))


class ReservationTests(ServiceFixture):
    def service_with_crash(self):
        crash = CrashBeforeProduct()
        return ApplicationService(
            self.store, TEST_HANDLERS, self.trust,
            action_kinds={"domain.product": "work.dispatch"},
            data_kinds={"test.note"},
            observation_kinds={"test.source-set", "runtime.job-observed"},
            recorder=crash,
        ), crash

    def test_crash_and_unrelated_append_rebind_exact_reservation(self):
        service, crash = self.service_with_crash()
        command, action, assessment = self.product_inputs()
        with self.assertRaisesRegex(OSError, "simulated"):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        self.assertTrue(crash.crashed)
        reserved = control_state(self.state())["admissions"]
        self.assertEqual(len(reserved), 1)
        state = self.state()
        self.service.submit_agent(event(state, "test.note", {"id": "unrelated"}, "unrelated"))
        retry = service.apply_control_action(
            command, action, assessment,
            credential_id="coordinator-credential", credential=self.coordinator_token,
        )
        self.assertTrue(retry["ok"])
        self.assertEqual(self.state()["extensions"]["test_product"]["product-1"], 1)
        self.assertTrue(any("rebound_event_id" in row for row in [control_state(self.state())["pending_grant"]]))

    def test_new_failed_approaches_after_crash_invalidate_clear_reservation(self):
        service, _crash = self.service_with_crash()
        command, action, assessment = self.product_inputs(event_id="product-failure", assessment_id="assessment-failure")
        with self.assertRaises(OSError):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        state = self.state()
        self.service.submit_agent(event(state, "evidence.recorded", {
            "id": "failure-evidence", "claim": "failed", "subject": "approach",
            "result": "observed_fail", "artifact_refs": [], "node_refs": ["root"],
        }, "evidence-failure"))
        for index in (1, 2):
            state = self.state()
            self.service.submit_control(event(state, "control.approach-outcome-recorded", {
                "record_id": f"failure-{index}", "problem_id": "root",
                "approach_id": f"approach-{index}", "strategy_sha256": sha(f"strategy-{index}".encode()),
                "outcome": "failed", "evidence_refs": ["failure-evidence"],
            }, f"failure-event-{index}"), credential_id="coordinator-credential", credential=self.coordinator_token)
        with self.assertRaisesRegex(Refusal, "sticky pause"):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        self.assertNotIn("product-failure", self.state()["extensions"].get("test_product", {}))

    def test_source_invalidation_after_crash_blocks_rebind(self):
        source_hash = "b" * 64
        state = self.state()
        self.service.submit_observation(event(state, "test.source-set", {
            "source_id": "source-1", "sha256": source_hash, "status": "current",
        }, "source-current"), credential_id="coordinator-credential", credential=self.coordinator_token)
        service, _crash = self.service_with_crash()
        command, action, assessment = self.product_inputs(
            event_id="product-source",
            assessment_id="assessment-source",
            sources=[{"source_id": "source-1", "sha256": source_hash}],
        )
        with self.assertRaises(OSError):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        state = self.state()
        self.service.submit_observation(event(state, "test.source-set", {
            "source_id": "source-1", "sha256": source_hash, "status": "invalidated",
        }, "source-invalidated"), credential_id="coordinator-credential", credential=self.coordinator_token)
        with self.assertRaisesRegex(Refusal, "new evidence"):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
            )
        self.assertNotIn("product-source", self.state()["extensions"].get("test_product", {}))

    def test_one_shot_exception_survives_crash_for_same_action_and_cannot_replay(self):
        state = self.state()
        self.service.submit_control(event(state, "control.owner-stop-requested", {
            "pause_id": "pause-exception", "campaign_id": "campaign-1",
            "base_sha256": state["base_sha256"], "charter_revision": 1,
            "reason": "owner pause", "drain_targets": [],
        }, "pause-exception"), credential_id="owner-credential", credential=self.owner_token)
        command, action, assessment = self.product_inputs(event_id="product-exception", assessment_id="assessment-exception")
        pause = control_state(self.state())["pauses"]["pause-exception"]
        self.service.submit_control(event(self.state(), "control.action-exception-granted", {
            "exception_id": "exception-1", "pause_id": "pause-exception",
            "pause_sha256": pause["pause_sha256"], "action_id": action["action_id"],
            "action_class": action["action_class"], "payload_sha256": action["payload_sha256"],
            "source_captures_sha256": sha(packed(action["source_captures"])),
            "charter_revision": 1, "reason": "one exact action",
        }, "grant-exception"), credential_id="owner-credential", credential=self.owner_token)
        service, _crash = self.service_with_crash()
        command["base_revision"] = self.state()["revision"]
        with self.assertRaises(OSError):
            service.apply_control_action(
                command, action, assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
                exception_id="exception-1",
            )
        state = self.state()
        self.service.submit_agent(event(state, "test.note", {"id": "between"}, "between"))
        service.apply_control_action(
            command, action, assessment,
            credential_id="coordinator-credential", credential=self.coordinator_token,
            exception_id="exception-1",
        )
        self.assertEqual(self.state()["extensions"]["test_product"]["product-exception"], 1)
        second_command, second_action, second_assessment = self.product_inputs(
            event_id="product-exception-2", assessment_id="assessment-exception-2",
        )
        second_command["base_revision"] = self.state()["revision"]
        with self.assertRaises(Refusal):
            service.apply_control_action(
                second_command, second_action, second_assessment,
                credential_id="coordinator-credential", credential=self.coordinator_token,
                exception_id="exception-1",
            )
        self.assertNotIn("product-exception-2", self.state()["extensions"].get("test_product", {}))


if __name__ == "__main__":
    unittest.main()
