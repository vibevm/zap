"""Prepare, and only on --execute run, an isolated real Sol/xhigh ZAP proof."""
from __future__ import annotations

import argparse
from functools import partial
import json
from pathlib import Path
import sys
import time

from .artifacts import PrivateArtifactStore, capture_source_blob
from .common import need, packed, sha
from .control import ACTION_CLASS_SET, CONTROL_HANDLERS, active_policy, control_state
from .control_trust import CredentialAuthority, Principal
from .coordinator_adapter import codex_sol_xhigh_adapter
from .domain import DOMAIN_ACTION_KINDS, DOMAIN_DATA_KINDS, DOMAIN_HANDLERS, domain_state, intent_fingerprint
from .knowledge import KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_HANDLERS
from .records import CORE_HANDLERS, compose_handlers
from .runtime_loop import AutomaticCoordinator
from .runtime_model import RUNTIME_ACTION_KINDS, RUNTIME_DATA_KINDS, RUNTIME_HANDLERS, RUNTIME_OBSERVATION_KINDS
from .runtime_packets import RuntimeConfig, VerificationSpec, action_for, active_job_ids, codex_sol_xhigh_worker_profile
from .runtime_reconciliation_model import (RECONCILIATION_ACTION_KINDS, RECONCILIATION_DATA_KINDS,
    RECONCILIATION_HANDLERS, RECONCILIATION_OBSERVATION_KINDS)
from .service import ApplicationService
from .sources import capture_source
from .storage import import_mup, load_store
from .transport import ProcessTransport

PLAN = '''schema = 1
plan_id = "zap-live-probe"
revision = 1
root_node = "R"
current_node = "T"
[[mandate]]
id = "M"
text = "Produce the exact bounded live-probe artifact."
disposition = "owned"
nodes = ["T"]
[[node]]
id = "R"
parent = ""
title = "Live probe"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = []
acceptance = []
evidence = []
[[node]]
id = "T"
parent = "R"
title = "Write bounded artifact"
kind = "atom"
state = "planned"
order = 1
depends_on = []
mandates = ["M"]
acceptance = ["artifact bytes independently verified"]
evidence = []
'''

VERIFY = '''import hashlib, json, pathlib, sys
packet = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
target = pathlib.Path(packet["check"]["target"])
raw = target.read_bytes() if target.is_file() else b""
passed = raw == b"ZAP live probe\\n"
print(json.dumps({"schema":"zap-verification-output/1", "result":"pass" if passed else "fail",
                  "artifacts":[{"path":str(target.resolve()), "sha256":hashlib.sha256(raw).hexdigest(), "bytes":len(raw)}],
                  "summary":"exact live-probe bytes verified"}))
raise SystemExit(0 if passed else 3)
'''


def _write_exact(path: Path, text: str):
    if path.exists():
        need(path.is_file() and path.read_text(encoding="utf-8") == text, "PROBE", f"unexpected existing probe file: {path.name}")
    else:
        path.parent.mkdir(parents=True, exist_ok=True); path.write_text(text, encoding="utf-8")


class _Probe:
    def __init__(self, root: Path):
        self.root = root; self.store = root / "store"; self.counter = 0
        handlers = compose_handlers(CORE_HANDLERS, CONTROL_HANDLERS, DOMAIN_HANDLERS, KNOWLEDGE_HANDLERS,
                                    RUNTIME_HANDLERS, RECONCILIATION_HANDLERS)
        knowledge_data = {kind for kind, route in KNOWLEDGE_EVENT_ROUTES.items() if route["route"] == "agent_data"}
        knowledge_observations = {kind for kind, route in KNOWLEDGE_EVENT_ROUTES.items() if route["route"] == "effect_adapter"}
        knowledge_actions = {kind: route["action"] for kind, route in KNOWLEDGE_EVENT_ROUTES.items() if route["route"] == "trusted_service"}
        principal = Principal("host", "owner", "zap-live-probe", frozenset(CONTROL_HANDLERS), frozenset(ACTION_CLASS_SET))
        self.handlers = handlers
        self.service = ApplicationService(self.store, handlers, CredentialAuthority(), principal,
            action_kinds={**DOMAIN_ACTION_KINDS, **knowledge_actions, **RUNTIME_ACTION_KINDS, **RECONCILIATION_ACTION_KINDS},
            data_kinds=set(DOMAIN_DATA_KINDS) | knowledge_data | set(RUNTIME_DATA_KINDS) | set(RECONCILIATION_DATA_KINDS),
            observation_kinds=knowledge_observations | set(RUNTIME_OBSERVATION_KINDS) | set(RECONCILIATION_OBSERVATION_KINDS))

    def state(self):
        return load_store(self.store, self.handlers)[0]

    def command(self, kind, payload, reason):
        self.counter += 1
        return {"event_id": f"probe:{self.counter}:{kind}", "base_revision": self.state()["revision"],
                "kind": kind, "reason": {"summary": reason}, "payload": payload}

    def data(self, kind, payload, reason):
        return self.service.submit_agent(self.command(kind, payload, reason))

    def observe(self, kind, payload, reason):
        return self.service.submit_host_observation(self.command(kind, payload, reason))

    def control(self, kind, payload, reason):
        return self.service.submit_host(self.command(kind, payload, reason))

    def action(self, kind, payload, action_class, reason, sources=()):
        state = self.state(); command = self.command(kind, payload, reason); policy = active_policy(state)
        action = action_for(state, action_id=f"probe-action:{self.counter}", action_class=action_class, payload=payload, source_rows=list(sources), problem_id="T")
        assessment = {"schema": "zap-assessment/1", "assessment_id": f"probe-assessment:{self.counter}",
                      "policy_id": policy["stop_policy"]["policy_id"], "policy_revision": policy["stop_policy"]["revision"],
                      "phase": "before_action", "values": {}, "drain_targets": active_job_ids(state)}
        return self.service.apply_host_action(command, action, assessment)

    def initialize(self):
        if domain_state(self.state())["active_outcome_id"] is not None:
            return
        source = capture_source(self.root / "input.txt", self.root, source_id="INPUT")
        self.observe("knowledge.source-recorded", {"source": source}, "Capture live-probe input")
        intent = {"schema": "zap-domain/intent-proposed/1", "intent_id": "INTENT", "revision": 1, "previous_intent_id": None,
                  "summary": "Produce one independently verified artifact", "beneficiaries": ["ZAP maintainer"],
                  "values": ["truthful automatic execution"], "constraints": ["isolated root", "candidate is not acceptance"], "source_refs": ["INPUT"]}
        self.data("domain.intent-proposed", intent, "Propose probe intent")
        state = self.state(); mandate = state["plan"]["mandate"][0]
        charter = {"schema": "zap-charter/1", "charter_id": "CHARTER", "campaign_id": "zap-live-probe", "base_sha256": state["base_sha256"],
                   "revision": 1, "parent_sha256": None, "intent": intent["summary"],
                   "intent_binding": {"intent_id": "INTENT", "sha256": intent_fingerprint(intent)},
                   "expected_outcome": {"outcome_id": "OUTCOME", "summary": "Exact artifact verified and accepted"},
                   "delegation": {"allowed_actions": sorted(ACTION_CLASS_SET), "adaptation": {"allow_target_revision": True,
                       "mutable_obligations": [], "essential_obligations": [], "allowed_dispositions": ["excluded", "replaced", "retained", "unattainable"]}},
                   "legacy_authority": [{"id": "M", "disposition": "retained", "source_sha256": sha(packed(mandate)), "replacement_ref": None}],
                   "stop_policy": {"schema": "zap-stop-policy/1", "policy_id": "POLICY", "revision": 1, "rules": []}}
        self.control("control.charter-drafted", {"charter": charter}, "Draft isolated probe charter")
        entry = control_state(self.state())["charters"]["CHARTER"][0]
        self.control("control.charter-activated", {"charter_id": "CHARTER", "charter_revision": 1, "charter_sha256": entry["sha256"],
                     "campaign_id": "zap-live-probe", "base_sha256": state["base_sha256"]}, "Activate isolated probe charter")
        self.action("domain.intent-adopted", {"schema": "zap-domain/intent-adopted/1", "intent_id": "INTENT"}, "outcome.adopt", "Adopt probe intent")
        self.data("domain.outcome-proposed", {"schema": "zap-domain/outcome-proposed/1", "outcome_id": "OUTCOME", "revision": 1,
                  "previous_outcome_id": None, "intent_id": "INTENT", "summary": "Exact artifact verified and accepted",
                  "benefits": ["real end-to-end proof"], "guarantees": ["exact bytes checked"], "tradeoffs": [], "obligations": []}, "Propose probe outcome")
        self.action("domain.outcome-adopted", {"schema": "zap-domain/outcome-adopted/1", "outcome_id": "OUTCOME", "obligation_dispositions": []},
                    "outcome.adopt", "Adopt probe outcome")
        capture = [{"source_id": "INPUT", "sha256": source["content_sha256"]}]
        self.data("evidence.recorded", {"id": "INPUT-E", "claim": "Probe input bytes captured", "subject": "source:INPUT",
                  "result": "observed_pass", "artifact_refs": [f"sha256:{source['content_sha256']}"], "node_refs": ["T"]}, "Record input capture")
        scope = {"kind": "subjects", "subjects": [{"kind": "task", "id": "T"}]}
        self.action("knowledge.applicability-assessed", {"source_id": "INPUT", "status": "applicable", "scope": scope,
                    "evidence_refs": ["INPUT-E"], "basis": "Exact isolated probe input"}, "evidence.adjudicate", "Assess probe input", capture)
        self.action("knowledge.closure-assessed", {"subject": {"kind": "source", "id": "INPUT"}, "status": "complete",
                    "boundary": scope["subjects"], "missing": [], "evidence_refs": ["INPUT-E"], "basis": "Single explicit task consumes this input"},
                    "evidence.adjudicate", "Close probe input boundary", capture)
        domain = domain_state(self.state()); obligations = sorted(row["obligation_id"] for row in domain["ownership"] if row["work_id"] == "T")
        target = str((self.root / "result.txt").resolve())
        contract = {"schema": "zap-task-contract/1", "contract_id": "contract:T", "work_id": "T", "title": "Write exact probe artifact",
                    "goal": "Write exact UTF-8 bytes `ZAP live probe\\n` to result.txt", "read_subjects": [str((self.root / "input.txt").resolve())],
                    "write_subjects": [target], "resources": ["codex-worker"],
                    "steps": ["Read the bounded packet", "Write only result.txt with the exact required bytes", "Return candidate report"],
                    "positive_cases": ["result.txt bytes equal the required text"], "negative_cases": ["extra or changed bytes fail verification"],
                    "checks": ["probe-exact-bytes"], "acceptance": ["independent verifier and central acceptance"],
                    "safe_stop": "Leave result.txt absent or atomically complete, then report stopped", "integration_owner": "T",
                    "delivery_route": ["direct"], "required_stage": "functional", "source_handles": ["INPUT"], "obligation_ids": obligations}
        self.action("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T", "expected_version": 0,
                    "contract": contract}, "task.update", "Version probe contract", capture)


def prepare_probe(root: str | Path) -> dict:
    root = Path(root).absolute(); root.mkdir(parents=True, exist_ok=True)
    _write_exact(root / "plan.toml", PLAN); _write_exact(root / "input.txt", "Write the exact requested live-probe artifact.\n")
    _write_exact(root / "standing-rules.txt", "Only write result.txt inside this isolated probe root. Never self-accept.\n")
    _write_exact(root / "verify.py", VERIFY)
    task = {"id": "T", "title": "Write bounded artifact", "goal": "live probe", "read_paths": ["input.txt"], "write_paths": ["result.txt"],
            "steps": ["write exact bytes"], "positive_cases": ["exact"], "negative_cases": ["changed"], "checks": ["probe-exact-bytes"],
            "acceptance": ["independently verified"], "safe_stop": "atomic file", "commit_subject": "test: live probe", "notes": []}
    tasks = root / "tasks"; tasks.mkdir(exist_ok=True); _write_exact(tasks / "R.json", json.dumps({"id": "R", "tasks": [task]}, sort_keys=True))
    if not (root / "store" / "base.json").is_file():
        import_mup(root / "plan.toml", tasks, root / "store")
    probe = _Probe(root); probe.initialize()
    return {"schema": "zap-live-probe/prepared/1", "root": str(root), "store": str(probe.store),
            "base_sha256": probe.state()["base_sha256"], "revision": probe.state()["revision"], "execution_mode": probe.state()["execution_mode"]}


def execute_probe(root: str | Path, *, launcher=None, timeout_seconds=900.0, worker_sandbox="workspace-write") -> dict:
    prepared = prepare_probe(root); root = Path(prepared["root"]); probe = _Probe(root)
    worker = codex_sol_xhigh_worker_profile(cwd=str(root), launcher=launcher, standing_rule_paths=[str(root / "standing-rules.txt")],
                                            resource_capacities={"codex-worker": 1}, sandbox=worker_sandbox, approval_policy="never")
    transport = ProcessTransport(root / "worker-transport", [root], inherited_environment=worker.required_inherited_environment,
                                 environment_allowlist=worker.environment)
    semantic = codex_sol_xhigh_adapter(root / "semantic-transport", [root], cwd=root, launcher=launcher)
    verifier = VerificationSpec("probe-exact-bytes", (sys.executable, "-B", str(root / "verify.py"), "{packet_file}"), str(root),
                                str(root / "result.txt"), "python-3.11", "isolated-live-probe", (str(root / "result.txt"),),
                                ("exact bytes", "changed bytes"), ("INPUT",))
    artifacts = PrivateArtifactStore(root / "private-artifacts", create=True)
    config = RuntimeConfig(worker, {"T": (verifier,)}, artifact_capture=partial(capture_source_blob, artifacts), artifact_reader=artifacts.read,
                           artifact_allowed_root=str(root), idle_poll_seconds=.1)
    coordinator = AutomaticCoordinator(probe.store, probe.handlers, probe.service, transport, semantic, config)
    deadline = time.monotonic() + timeout_seconds; result = coordinator.tick()
    while not result["campaign_finished"] and time.monotonic() < deadline:
        time.sleep(config.idle_poll_seconds); result = coordinator.tick()
    need(result["campaign_finished"], "PROBE_TIMEOUT", "live probe did not reach a truthful closure before timeout")
    return {"schema": "zap-live-probe/result/1", **result, "root": str(root), "artifact": str(root / "result.txt")}


def main(argv=None):
    parser = argparse.ArgumentParser(); parser.add_argument("--root", required=True); parser.add_argument("--execute", action="store_true")
    parser.add_argument("--launcher"); parser.add_argument("--timeout-seconds", type=float, default=900.0)
    parser.add_argument("--worker-sandbox", choices=("workspace-write", "danger-full-access"), default="workspace-write"); args = parser.parse_args(argv)
    result = execute_probe(args.root, launcher=args.launcher, timeout_seconds=args.timeout_seconds,
                           worker_sandbox=args.worker_sandbox) if args.execute else prepare_probe(args.root)
    print(json.dumps(result, sort_keys=True)); return 0


if __name__ == "__main__":
    raise SystemExit(main())
