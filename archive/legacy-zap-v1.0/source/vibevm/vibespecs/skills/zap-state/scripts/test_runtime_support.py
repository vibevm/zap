"""Activated isolated campaign fixture for automatic-runtime tests."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import time
from typing import Any

from zaplib.common import packed, sha
from zaplib.control import ACTION_CLASS_SET, CONTROL_HANDLERS, active_policy, control_state
from zaplib.control_trust import CredentialAuthority, Principal
from zaplib.domain import DOMAIN_ACTION_KINDS, DOMAIN_DATA_KINDS, DOMAIN_HANDLERS, domain_state, intent_fingerprint
from zaplib.knowledge import KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_HANDLERS
from zaplib.records import CORE_HANDLERS, compose_handlers
from zaplib.runtime import RUNTIME_ACTION_KINDS, RUNTIME_DATA_KINDS, RUNTIME_HANDLERS, RUNTIME_OBSERVATION_KINDS, RuntimeConfig, VerificationSpec, WorkerProfile, runtime_state
from zaplib.runtime_packets import action_for, active_job_ids
from zaplib.service import ApplicationService
from zaplib.sources import capture_source
from zaplib.storage import import_mup, load_store
from zaplib.transport import ProcessTransport

PLAN = '''schema = 1
plan_id = "runtime-fixture"
revision = 1
root_node = "R"
current_node = "T"
[[mandate]]
id = "M"
text = "Deliver the fixture"
disposition = "owned"
nodes = ["R", "T", "U"]
[[node]]
id = "R"
parent = ""
title = "Runtime fixture"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["M"]
acceptance = ["Campaign integrated"]
evidence = []
[[node]]
id = "T"
parent = "R"
title = "Task T"
kind = "atom"
state = "planned"
order = 1
depends_on = []
mandates = ["M"]
acceptance = ["T verified"]
evidence = []
[[node]]
id = "U"
parent = "R"
title = "Task U"
kind = "atom"
state = "planned"
order = 2
depends_on = []
mandates = ["M"]
acceptance = ["U verified"]
evidence = []
'''


def legacy_task(work_id):
    return {"id": work_id, "title": work_id, "goal": "fixture", "read_paths": ["source"], "write_paths": ["candidate"],
            "steps": ["work"], "positive_cases": ["pass"], "negative_cases": ["fail"], "checks": [f"check-{work_id}"],
            "acceptance": ["verified"], "safe_stop": "write safe marker", "commit_subject": "feat: fixture", "notes": []}


WORKER_SCRIPT = '''import json, os, pathlib, sys, time
packet = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
target = pathlib.Path(packet["write_subjects"][0])
stop = pathlib.Path(os.environ["ZAP_STOP_FILE"])
deadline = time.monotonic() + 3.0
while time.monotonic() < deadline:
    if stop.exists():
        pathlib.Path(str(target) + ".safe").write_text("safe", encoding="utf-8")
        print(json.dumps({"candidate": False, "safe": True}))
        raise SystemExit(0)
    time.sleep(0.02)
target.write_text("candidate", encoding="utf-8")
print(json.dumps({"candidate": True, "work_id": packet["work_id"]}))
'''

VERIFY_SCRIPT = '''import json, pathlib, sys
packet = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
target = pathlib.Path(packet["check"]["target"])
print(json.dumps({"exists": target.is_file(), "target": str(target)}))
raise SystemExit(0 if target.is_file() and target.read_text(encoding="utf-8") == "candidate" else 3)
'''


class ScriptedSemanticAdapter:
    def __init__(self):
        self.requests = {}

    def submit(self, request_id, request):
        self.requests[request_id] = request
        return {"accepted": True, "idempotent": request_id in self.requests}

    def poll(self, request_id, request):
        body = request["request"]
        if request["request_kind"] == "selection":
            disposition, selection, commands = "select", {"work_id": body["frontier"][0]["work_id"]}, []
        elif request["request_kind"] == "acceptance":
            commands = []
            domain = body["domain"]
            for job in body["jobs"]:
                work_id = job["work_id"]
                verifications = [row for row in body["verification_jobs"] if row["work_job_id"] == job["job_id"] and row["state"] == "observed"]
                evidence_ids = [row["evidence_id"] for row in verifications]
                obligations = sorted({row["obligation_id"] for row in domain["ownership"] if row["work_id"] == work_id and domain["obligations"][row["obligation_id"]]["status"] == "active"})
                for verification in verifications:
                    evidence_id = verification["evidence_id"]
                    commands.append({"kind": "domain.evidence-adjudicated", "reason": f"Adjudicate actual check for {work_id}", "payload": {
                        "schema": "zap-domain/evidence-adjudicated/1", "evidence_id": evidence_id, "expected_revision": -1,
                        "disposition": "accepted", "applies_to": {"outcome_id": domain["active_outcome_id"], "obligation_ids": obligations,
                        "work_ids": [work_id], "stage": "functional", "scope": "isolated fixture"},
                        "source_refs": verification["plan"]["source_refs"], "method": {key: verification["plan"][key] for key in ("argv", "target", "toolchain", "environment", "subjects", "cases")},
                        "limitations": []}})
                stage_id = f"stage:{work_id}:functional"
                commands.append({"kind": "domain.stage-accepted", "reason": f"Accept functional stage for {work_id}", "payload": {
                    "schema": "zap-domain/stage-accepted/1", "stage_acceptance_id": stage_id, "work_id": work_id, "stage": "functional",
                    "outcome_id": domain["active_outcome_id"], "evidence_ids": evidence_ids, "obligation_ids": obligations,
                    "scope": "isolated fixture", "summary": "Configured checks passed"}})
                commands.append({"kind": "domain.work-accepted", "reason": f"Centrally accept {work_id}", "payload": {
                    "schema": "zap-domain/work-accepted/1", "acceptance_id": f"acceptance:{work_id}:runtime", "work_id": work_id,
                    "outcome_id": domain["active_outcome_id"], "stage_acceptance_id": stage_id, "evidence_ids": evidence_ids,
                    "obligation_ids": obligations, "integration_acceptance_ids": [], "summary": "Independent configured verification accepted"}})
            disposition, selection = "commands", None
        else:
            disposition, selection, commands = "no_change", None, []
        response = {"schema": "zap-coordinator-response/1", "request_id": request_id, "request_kind": request["request_kind"],
                    "state_revision": request["state_revision"], "request_sha256": sha(packed(request)), "disposition": disposition,
                    "rationale": "Fixture semantic decision", "selection": selection, "commands": commands}
        return {"ready": True, "ok": True, "response": response, "response_sha256": sha(packed(response))}

    def request_stop(self, request_id, stop_id):
        return {"requested": True, "request_id": request_id, "stop_id": stop_id}


class RuntimeFixture:
    def __init__(self, root: Path, *, conflict=False, unknown_dispatch=False, safe_verifier=False, activated=True, scoped_stop=False,
                 worker_seconds=3.0, root_acceptance=True):
        self.root = root; self.counter = 0
        plan_text = PLAN if root_acceptance else PLAN.replace('acceptance = ["Campaign integrated"]', "acceptance = []", 1)
        plan = root / "plan.toml"; plan.write_text(plan_text, encoding="utf-8")
        tasks = root / "tasks"; tasks.mkdir(); (tasks / "R.json").write_text(json.dumps({"id": "R", "tasks": [legacy_task("T"), legacy_task("U")]}), encoding="utf-8")
        self.store = root / "store"; import_mup(plan, tasks, self.store)
        self.worker_script = root / "worker.py"; self.worker_script.write_text(WORKER_SCRIPT.replace("+ 3.0", f"+ {worker_seconds}"), encoding="utf-8")
        self.verify_script = root / "verify.py"; self.verify_script.write_text(VERIFY_SCRIPT, encoding="utf-8")
        self.source_path = root / "source.txt"; self.source_path.write_text("source", encoding="utf-8")
        self.targets = {"T": root / "T.candidate", "U": root / ("T.candidate" if conflict else "U.candidate")}
        handlers = compose_handlers(CORE_HANDLERS, CONTROL_HANDLERS, DOMAIN_HANDLERS, KNOWLEDGE_HANDLERS, RUNTIME_HANDLERS)
        knowledge_data = {kind for kind, route in KNOWLEDGE_EVENT_ROUTES.items() if route["route"] == "agent_data"}
        knowledge_observations = {"knowledge.source-recorded", "knowledge.native-facts-recorded", "knowledge.source-recaptured"}
        knowledge_actions = {kind: route["action"] for kind, route in KNOWLEDGE_EVENT_ROUTES.items()
                             if route["route"] == "trusted_service" and kind not in knowledge_observations}
        action_kinds = {**DOMAIN_ACTION_KINDS, **knowledge_actions, **RUNTIME_ACTION_KINDS}
        data_kinds = set(DOMAIN_DATA_KINDS) | knowledge_data | set(RUNTIME_DATA_KINDS)
        observations = knowledge_observations | set(RUNTIME_OBSERVATION_KINDS)
        principal = Principal("host", "owner", "runtime-fixture", frozenset(CONTROL_HANDLERS), frozenset(ACTION_CLASS_SET))
        self.service = ApplicationService(self.store, handlers, CredentialAuthority(), principal, action_kinds=action_kinds,
                                          data_kinds=data_kinds, observation_kinds=observations)
        self.handlers = handlers
        self._observe("knowledge.source-recorded", {"source": capture_source(self.source_path, root, source_id="S")}, "capture source")
        if activated:
            self._activate(unknown_dispatch, scoped_stop)
            self._domain_setup(conflict)
        self.transport = ProcessTransport(root / "transport", [root])
        profile = WorkerProfile(argv=(sys.executable, "-B", str(self.worker_script), "{packet_file}"), cwd=str(root), review_capacity=4,
                                integration_capacity=1, resource_capacities={"worker": 2})
        verifications = {}
        for work_id in ("T", "U"):
            verifications[work_id] = (VerificationSpec(f"check-{work_id}", (sys.executable, "-B", str(self.verify_script), "{packet_file}"), str(root),
                str(self.targets[work_id]), "python-3.11", "isolated", (str(self.targets[work_id]),), ("positive", "negative"), ("S",)),)
        assessment_provider = (lambda state, action: {"values": {}, "drain_targets": active_job_ids(state)}) if unknown_dispatch else None
        verifier = self._safe_verifier if safe_verifier else None
        self.config = RuntimeConfig(profile, verifications, assessment_provider=assessment_provider, safe_state_verifier=verifier,
                                    transient_backoff_ns=10, idle_poll_seconds=0.01)
        self.semantic = ScriptedSemanticAdapter()

    def state(self):
        return load_store(self.store, self.handlers)[0]

    def _id(self, prefix):
        self.counter += 1; return f"{prefix}:{self.counter}"

    def _command(self, kind, payload, reason):
        return {"event_id": self._id(kind), "base_revision": self.state()["revision"], "kind": kind, "reason": {"summary": reason}, "payload": payload}

    def _data(self, kind, payload, reason):
        return self.service.submit_agent(self._command(kind, payload, reason))

    def _observe(self, kind, payload, reason):
        return self.service.submit_host_observation(self._command(kind, payload, reason))

    def _control(self, kind, payload, reason):
        return self.service.submit_host(self._command(kind, payload, reason))

    def _apply(self, kind, payload, action_class, reason, source_rows=()):
        state = self.state(); command = self._command(kind, payload, reason)
        action = action_for(state, action_id=self._id("action"), action_class=action_class, payload=payload, source_rows=list(source_rows))
        policy = active_policy(state); assessment = {"schema": "zap-assessment/1", "assessment_id": self._id("assessment"),
            "policy_id": policy["stop_policy"]["policy_id"], "policy_revision": policy["stop_policy"]["revision"],
            "phase": "before_action", "values": {}, "drain_targets": active_job_ids(state)}
        return self.service.apply_host_action(command, action, assessment)

    def _activate(self, unknown_dispatch, scoped_stop=False):
        intent = {"schema": "zap-domain/intent-proposed/1", "intent_id": "INTENT", "revision": 1, "previous_intent_id": None,
                  "summary": "Run isolated tasks", "beneficiaries": ["tester"], "values": ["truth"], "constraints": ["no implicit acceptance"], "source_refs": ["S"]}
        self._data("domain.intent-proposed", intent, "Propose fixture intent")
        state = self.state(); mandate = state["plan"]["mandate"][0]
        rules = []
        if unknown_dispatch:
            rules = [{"id": "dispatch-known", "applies_to_actions": ["work.dispatch"], "scope": "campaign",
                      "timing": "before_action", "when": {"eq": {"field": "dispatch_allowed", "value": True}}}]
        elif scoped_stop:
            rules = [{"id": "scoped-stop", "applies_to_actions": ["verification.run"], "scope": "run",
                      "timing": "before_action", "when": {"eq": {"field": "stop_requested", "value": True}}}]
        charter = {"schema": "zap-charter/1", "charter_id": "CHARTER", "campaign_id": "runtime-fixture", "base_sha256": state["base_sha256"],
                   "revision": 1, "parent_sha256": None, "intent": "Run isolated tasks", "intent_binding": {"intent_id": "INTENT", "sha256": intent_fingerprint(intent)},
                   "expected_outcome": {"outcome_id": "OUTCOME", "summary": "Verified tasks"},
                   "delegation": {"allowed_actions": sorted(ACTION_CLASS_SET), "adaptation": {"allow_target_revision": True,
                       "mutable_obligations": [], "essential_obligations": [], "allowed_dispositions": ["excluded", "replaced", "retained", "unattainable"]}},
                   "legacy_authority": [{"id": "M", "disposition": "retained", "source_sha256": sha(packed(mandate)), "replacement_ref": None}],
                   "stop_policy": {"schema": "zap-stop-policy/1", "policy_id": "POLICY", "revision": 1, "rules": rules}}
        self._control("control.charter-drafted", {"charter": charter}, "Draft fixture charter")
        entry = control_state(self.state())["charters"]["CHARTER"][0]
        self._control("control.charter-activated", {"charter_id": "CHARTER", "charter_revision": 1, "charter_sha256": entry["sha256"],
                      "campaign_id": "runtime-fixture", "base_sha256": state["base_sha256"]}, "Activate fixture charter")

    def _domain_setup(self, conflict):
        self._apply("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "INTENT"}, "outcome.adopt", "Adopt fixture intent")
        outcome = {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "OUTCOME", "revision": 1, "previous_outcome_id": None,
                   "intent_id": "INTENT", "summary": "Verified tasks", "benefits": ["proof"], "guarantees": ["checked"], "tradeoffs": [], "obligations": []}
        self._data("domain.outcome-proposed", outcome, "Propose fixture outcome")
        self._apply("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "OUTCOME", "obligation_dispositions": []}, "outcome.adopt", "Adopt fixture outcome")
        source_hash = self.state()["extensions"]["knowledge"]["sources"]["S"]["content_sha256"]
        captures = [{"source_id": "S", "sha256": source_hash}]
        self._data("evidence.recorded", {"id": "SOURCE-E", "claim": "Source captured", "subject": "source:S", "result": "observed_pass",
                   "artifact_refs": [f"sha256:{source_hash}"], "node_refs": ["T", "U"]}, "Record source observation")
        scope = {"kind": "subjects", "subjects": [{"kind": "node", "id": "T"}, {"kind": "node", "id": "U"}]}
        self._apply("knowledge.applicability-assessed", {"source_id": "S", "status": "applicable", "scope": scope,
                    "evidence_refs": ["SOURCE-E"], "basis": "Fixture source bytes checked"}, "evidence.adjudicate", "Adjudicate source", captures)
        self._apply("knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S"}, "status": "complete", "boundary": scope["subjects"],
                    "missing": [], "evidence_refs": ["SOURCE-E"], "basis": "Fixture inputs bounded"}, "evidence.adjudicate", "Bound source closure", captures)
        domain = domain_state(self.state())
        for work_id in ("T", "U"):
            obligations = sorted({row["obligation_id"] for row in domain["ownership"] if row["work_id"] == work_id and domain["obligations"][row["obligation_id"]]["status"] == "active"})
            target = self.targets[work_id]
            contract = {"schema": "zap-task-contract/1", "contract_id": f"contract:{work_id}", "work_id": work_id, "title": f"Task {work_id}",
                        "goal": "Produce candidate", "read_subjects": [str(self.source_path)], "write_subjects": [str(target)], "resources": ["worker"],
                        "steps": ["Read packet", "Write candidate"], "positive_cases": ["candidate"], "negative_cases": ["stop"],
                        "checks": [f"check-{work_id}"], "acceptance": ["configured check"], "safe_stop": "write .safe marker and exit",
                        "integration_owner": work_id, "delivery_route": ["direct"], "required_stage": "functional", "source_handles": ["S"], "obligation_ids": obligations}
            self._apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": work_id,
                        "expected_version": 0, "contract": contract}, "task.update", f"Version contract {work_id}", captures)

    def _safe_verifier(self, state, job, pause, receipt):
        marker = Path(job["reservation"]["write_subjects"][0] + ".safe")
        if marker.is_file():
            raw = marker.read_bytes(); return {"state": "safe", "receipt": {"path": str(marker), "sha256": sha(raw), "bytes": len(raw)}}
        target = Path(job["reservation"]["write_subjects"][0])
        if receipt.get("state") in {"succeeded", "failed"} and target.is_file():
            raw = target.read_bytes(); return {"state": "completed", "receipt": {"path": str(target), "sha256": sha(raw), "bytes": len(raw)}}
        return None

    def owner_stop(self, pause_id="OWNER-STOP"):
        state = self.state(); policy = active_policy(state); drains = active_job_ids(state)
        return self._control("control.owner-stop-requested", {"pause_id": pause_id, "campaign_id": "runtime-fixture", "base_sha256": state["base_sha256"],
                             "charter_revision": policy["revision"], "reason": "Fixture owner stop", "drain_targets": drains}, "Stop fixture")

    def owner_resume(self, pause_id="OWNER-STOP"):
        pause = next(row for row in active_policy(self.state())["pauses"] if row["pause_id"] == pause_id)
        return self._control("control.pause-resumed", {"pause_id": pause_id, "pause_sha256": pause["pause_sha256"],
                             "decision": "Resume after verified safe boundary"}, "Resume fixture")

    def scoped_stop(self, job_id):
        state = self.state(); policy = active_policy(state); job = runtime_state(state)["jobs"][job_id]
        action = action_for(state, action_id=f"scoped-action:{job_id}", action_class="verification.run", payload={"job_id": job_id},
                            source_rows=job["source_captures"], run_id=job_id, problem_id=job["work_id"])
        assessment = {"schema": "zap-assessment/1", "assessment_id": f"scoped-assessment:{job_id}", "policy_id": policy["stop_policy"]["policy_id"],
                      "policy_revision": policy["stop_policy"]["revision"], "phase": "before_action", "values": {"stop_requested": True},
                      "drain_targets": [job_id]}
        return self._control("control.action-assessed", {"action": action, "assessment": assessment}, "Create scoped fixture pause")


def tick_until(coordinator, predicate, *, limit=200):
    history = []
    for _ in range(limit):
        result = coordinator.tick(); history.append(result)
        if predicate(result): return result, history
        time.sleep(0.02)
    raise AssertionError(json.dumps({"runtime": runtime_state(coordinator._load()[0]), "recent": history[-5:]}, default=str))
