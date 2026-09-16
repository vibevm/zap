"""Nonblocking trusted assessment-provider profile and stop-policy tests."""
from __future__ import annotations

import copy
from dataclasses import replace
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest

from test_control import action, activate, base_state
from test_service import make_store
from test_runtime_support import RuntimeFixture, tick_until
from zaplib.cli_runtime import build_automatic_coordinator
from zaplib.common import Refusal, packed, sha
from zaplib.control import active_policy, assess_action
from zaplib.control_trust import Principal
from zaplib.engine import build_engine
from zaplib.runtime import AutomaticCoordinator, RuntimeConfig, WorkerProfile, runtime_state
from zaplib.runtime_packets import assessment_for
from zaplib.runtime_assessment import (
    JsonProcessAssessmentAdapter, build_assessment_request,
    validate_assessment_response,
)


PROVIDER = '''import json, pathlib, sys, time
request = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
time.sleep(0.15)
mode = request["trusted_observations"][0]["content"].strip()
value = True if mode == "pause" else False if mode == "clear" else None
response = {
    "schema": "zap-runtime/assessment-response/1",
    "request_id": request["request_id"], "request_sha256": request["request_sha256"],
    "basis_sha256": "0" * 64 if mode == "stale" else request["basis_sha256"],
    "campaign_id": request["campaign_id"], "base_sha256": request["base_sha256"],
    "action_id": request["action"]["action_id"],
    "action_payload_sha256": request["action"]["payload_sha256"],
    "values": {field: value for field in request["required_fields"]},
}
print(json.dumps(response, sort_keys=True, separators=(",", ":")))
'''


class RuntimeAssessmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-assessment-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        provider = self.root / "provider.py"
        provider.write_text(PROVIDER, encoding="utf-8")
        self.mode = self.root / "assessment-mode.txt"
        self.mode.write_text("unknown", encoding="utf-8")
        parent = self.root / "fixture"
        parent.mkdir()
        store = make_store(parent)
        principal = Principal("assessment-host", "coordinator", "campaign-1")
        engine = build_engine(store, host_principal=principal)
        profile = {
            "schema": "zap-runtime-profile/1",
            "transport": {"root": str(self.root / "worker"), "allowed_workspace_roots": [str(self.root)],
                          "inherited_environment": [], "environment_allowlist": [], "allow_process_termination": False},
            "semantic_transport": {"root": str(self.root / "semantic"), "allowed_workspace_roots": [str(self.root)],
                                   "inherited_environment": [], "environment_allowlist": [], "allow_process_termination": False},
            "semantic": {"kind": "json-process", "argv": [sys.executable, "{packet_file}"],
                         "cwd": str(self.root), "launcher": None},
            "assessment_transport": {"root": str(self.root / "assessment"), "allowed_workspace_roots": [str(self.root)],
                                     "inherited_environment": [], "environment_allowlist": [], "allow_process_termination": False},
            "assessment": {"kind": "json-process", "argv": [sys.executable, "-B", str(provider), "{packet_file}"],
                           "cwd": str(self.root), "observation_paths": [str(self.mode)]},
            "worker": {"argv": [sys.executable, "{packet_file}"], "cwd": str(self.root), "standing_rule_paths": [],
                       "environment": {}, "resource_capacities": {}, "review_capacity": 1, "integration_capacity": 1,
                       "branch_for_work": {}, "stop_mode": "cooperative", "terminate_after_seconds": None},
            "verifications": {}, "runtime": {"transient_backoff_ns": 0, "idle_poll_seconds": 0},
        }
        path = self.root / "profile.json"
        path.write_text(json.dumps(profile), encoding="utf-8")
        coordinator = build_automatic_coordinator(engine, path)
        self.adapter = coordinator.config.assessment_provider
        self.assertIsInstance(self.adapter, JsonProcessAssessmentAdapter)
        self.state = activate(base_state())
        runtime = runtime_state(self.state)
        runtime["jobs"]["run-1"] = {"job_id": "run-1", "run_id": "run-1", "state": "running"}
        self.state.setdefault("extensions", {})["runtime"] = runtime

    def assessed(self, mode):
        self.mode.write_text(mode, encoding="utf-8")
        payload = {"mode": mode}
        candidate = action(action_id=f"action-{mode}", payload_sha256=sha(packed(payload)))
        command = {"event_id": f"event-{mode}", "base_revision": self.state["revision"],
                   "kind": "runtime.fixture-action", "reason": {"summary": "assessment fixture"},
                   "payload": payload}
        config = RuntimeConfig(WorkerProfile((sys.executable, "{packet_file}"), str(self.root)),
                               assessment_provider=self.adapter)
        for _ in range(100):
            try:
                return candidate, assessment_for(self.state, candidate, config, f"assessment-{mode}",
                                                 command_context=command)
            except Refusal as exc:
                self.assertEqual(exc.code, "ASSESSMENT_PENDING")
                time.sleep(0.02)
        self.fail("assessment provider did not complete")

    def test_false_admits_true_pauses_and_drain_targets_are_derived(self):
        clear_action, clear = self.assessed("clear")
        self.assertEqual(clear["drain_targets"], ["run-1"])
        clear_result = assess_action(self.state, clear_action, clear)
        self.assertEqual(clear_result["policy_result"], "clear")
        self.assertTrue(clear_result["eligible_for_admission"])
        status = self.adapter.status()
        observed = next(row for row in status["requests"] if row["action"]["action_id"] == "action-clear")
        self.assertEqual(observed["result"]["values"], {
            "changes_public_format": False, "migration_affects_user_data": False,
        })
        self.assertEqual(observed["policy"]["policy_id"], "policy-1")
        self.assertIn("receipt_sha256", observed["transport"])
        self.assertNotIn(str(self.mode), repr(observed))
        self.assertNotIn("clear", repr(observed["trusted_observations"]))
        pause_action, pause = self.assessed("pause")
        result = assess_action(self.state, pause_action, pause)
        self.assertEqual(result["policy_result"], "pause")
        self.assertFalse(result["eligible_for_admission"])
        self.assertEqual(result["delivery"]["required"], ["run-1"])

    def test_unknown_blocks_and_unaffected_action_does_not_spawn_provider(self):
        unknown_action, unknown = self.assessed("unknown")
        unknown_result = assess_action(self.state, unknown_action, unknown)
        self.assertEqual(unknown_result["policy_result"], "needs_evidence")
        self.assertFalse(unknown_result["eligible_for_admission"])
        before = len(list(self.adapter.transport.jobs.iterdir()))
        payload = {"unaffected": True}
        unaffected = action("work.accept", action_id="unaffected", payload_sha256=sha(packed(payload)))
        command = {"event_id": "unaffected", "base_revision": self.state["revision"], "kind": "runtime.unaffected",
                   "reason": {"summary": "unaffected"}, "payload": payload}
        supplied = self.adapter.assess(self.state, unaffected, command)
        self.assertEqual(supplied, {"values": {}, "drain_targets": ["run-1"]})
        self.assertEqual(len(list(self.adapter.transport.jobs.iterdir())), before)

    def test_stale_response_is_never_rebound(self):
        self.mode.write_text("stale", encoding="utf-8")
        payload = {"mode": "stale"}
        stale = action(action_id="action-stale", payload_sha256=sha(packed(payload)))
        command = {"event_id": "event-stale", "base_revision": self.state["revision"], "kind": "runtime.fixture-action",
                   "reason": {"summary": "stale"}, "payload": payload}
        self.assertIsNone(self.adapter.assess(self.state, stale, command))
        for _ in range(100):
            try:
                result = self.adapter.assess(self.state, stale, command)
            except Refusal as exc:
                self.assertEqual(exc.code, "ASSESSMENT_STALE")
                return
            self.assertIsNone(result)
            time.sleep(0.02)
        self.fail("stale provider response was not observed")

    def test_unrelated_append_reuses_basis_but_command_or_source_change_does_not(self):
        self.mode.write_text("clear", encoding="utf-8")
        payload = {"mode": "clear"}
        bound = action(action_id="stable-action", payload_sha256=sha(packed(payload)))
        command = {"event_id": "stable-event", "base_revision": self.state["revision"],
                   "kind": "runtime.fixture-action", "reason": {"summary": "stable"}, "payload": payload}
        first = build_assessment_request(self.state, bound, command, observation_paths=[self.mode])
        unrelated = copy.deepcopy(self.state)
        unrelated["revision"] += 1
        unrelated.setdefault("facts", {})["unrelated"] = {"id": "unrelated", "statement": "not referenced",
                                                           "status": "observed", "node_refs": [],
                                                           "evidence_refs": [], "source_refs": []}
        rebound = build_assessment_request(unrelated, bound, {**command, "base_revision": unrelated["revision"]},
                                             observation_paths=[self.mode])
        self.assertEqual(first, rebound)
        changed_payload = {"mode": "clear", "scope": "changed"}
        changed_action = action(action_id="stable-action", payload_sha256=sha(packed(changed_payload)))
        changed_command = {**command, "payload": changed_payload}
        changed = build_assessment_request(self.state, changed_action, changed_command,
                                           observation_paths=[self.mode])
        self.assertNotEqual(first["basis_sha256"], changed["basis_sha256"])
        old_response = {"schema": "zap-runtime/assessment-response/1", "request_id": first["request_id"],
                        "request_sha256": first["request_sha256"], "basis_sha256": first["basis_sha256"],
                        "campaign_id": first["campaign_id"], "base_sha256": first["base_sha256"],
                        "action_id": first["action"]["action_id"],
                        "action_payload_sha256": first["action"]["payload_sha256"],
                        "values": {field: False for field in first["required_fields"]}}
        with self.assertRaisesRegex(Refusal, "differs"):
            validate_assessment_response(changed, old_response)

        sourced = copy.deepcopy(self.state)
        sourced.setdefault("extensions", {}).setdefault("knowledge", {})["sources"] = {
            "S": {"content_sha256": "a" * 64, "capture_status": "current"},
        }
        source_action = action(action_id="source-action", sources=[{"source_id": "S", "sha256": "a" * 64}],
                               payload_sha256=sha(packed(payload)))
        source_first = build_assessment_request(sourced, source_action, command,
                                                observation_paths=[self.mode])
        sourced["extensions"]["knowledge"]["sources"]["S"]["content_sha256"] = "b" * 64
        source_changed = build_assessment_request(sourced, source_action, command,
                                                  observation_paths=[self.mode])
        self.assertNotEqual(source_first["basis_sha256"], source_changed["basis_sha256"])

    def test_actual_loop_false_dispatches_true_pauses_and_unknown_never_dispatches(self):
        for mode in ("clear", "pause", "unknown"):
            with self.subTest(mode=mode):
                self.mode.write_text(mode, encoding="utf-8")
                root = self.root / f"loop-{mode}"
                root.mkdir()
                fixture = RuntimeFixture(root, unknown_dispatch=True)
                config = replace(fixture.config, assessment_provider=self.adapter)
                coordinator = AutomaticCoordinator(
                    fixture.store, fixture.handlers, fixture.service,
                    fixture.transport, fixture.semantic, config,
                )
                if mode == "clear":
                    tick_until(coordinator, lambda _: bool(runtime_state(fixture.state())["jobs"]), limit=80)
                    self.assertTrue(runtime_state(fixture.state())["jobs"])
                    self.assertFalse(active_policy(fixture.state())["pauses"])
                elif mode == "pause":
                    tick_until(coordinator, lambda _: bool(active_policy(fixture.state())["pauses"]), limit=80)
                    self.assertFalse(runtime_state(fixture.state())["jobs"])
                    self.assertEqual(active_policy(fixture.state())["pauses"][0]["scope"], "campaign")
                else:
                    for _ in range(30):
                        coordinator.tick()
                        time.sleep(0.02)
                    self.assertFalse(runtime_state(fixture.state())["jobs"])
                    self.assertFalse(active_policy(fixture.state())["pauses"])


if __name__ == "__main__":
    unittest.main()
