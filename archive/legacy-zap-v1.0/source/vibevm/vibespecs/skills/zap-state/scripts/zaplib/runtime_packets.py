"""Bound worker/verification profiles, packets, and control action envelopes."""
from __future__ import annotations

import copy
from dataclasses import dataclass, field
import os
from typing import Any, Callable, Mapping, Sequence
from pathlib import Path
import sys

from .common import identity, need, packed, sha
from .control import active_policy
from .domain import domain_state
from .runtime_model import ACTIVE_JOB_STATES, runtime_state

WORKER_PROVIDER_PROFILE_SCHEMA = "zap-worker-provider-profile/2"


@dataclass(frozen=True)
class WorkerProfile:
    argv: tuple[str, ...]
    cwd: str
    standing_rule_paths: tuple[str, ...] = ()
    environment: Mapping[str, str] = field(default_factory=dict, repr=False)
    resource_capacities: Mapping[str, int] = field(default_factory=dict)
    review_capacity: int = 1
    integration_capacity: int = 1
    branch_for_work: Mapping[str, str] = field(default_factory=dict)
    stop_mode: str = "cooperative"
    terminate_after_seconds: float | None = None
    required_inherited_environment: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        need(self.argv and sum(item == "{packet_file}" for item in self.argv) == 1 and all(isinstance(item, str) for item in self.argv), "ARGV", "worker argv needs one whole packet placeholder")
        need(isinstance(self.cwd, str) and self.cwd, "PATH", "worker cwd is required")
        need(self.review_capacity > 0 and self.integration_capacity > 0, "RUNTIME_VALUE", "worker capacities must be positive")
        need(self.stop_mode in {"cooperative", "terminate"}, "RUNTIME_VALUE", "unknown worker stop mode")
        need((self.stop_mode == "cooperative" and self.terminate_after_seconds is None) or
             (self.stop_mode == "terminate" and isinstance(self.terminate_after_seconds, (int, float)) and self.terminate_after_seconds > 0),
             "RUNTIME_VALUE", "terminate mode requires an explicit positive delay")
        for capacity in self.resource_capacities.values():
            need(type(capacity) is int and capacity > 0, "RUNTIME_VALUE", "resource capacities must be positive")


def codex_sol_xhigh_worker_profile(
    *, cwd: str, standing_rule_paths: Sequence[str] = (), launcher: str | os.PathLike[str] | None = None,
    resource_capacities: Mapping[str, int] | None = None, review_capacity: int = 1,
    integration_capacity: int = 1, branch_for_work: Mapping[str, str] | None = None,
    sandbox: str = "workspace-write", approval_policy: str = "auto-review",
) -> WorkerProfile:
    """Ready bounded coding-worker profile for the verified Sol/xhigh lane."""
    from .coordinator_adapter import codexrunner_path
    bridge = Path(__file__).with_name("worker_provider_bridge.py")
    inherited = (("SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "PATH", "TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA")
                 if os.name == "nt" else ("PATH", "HOME", "LANG", "LC_ALL", "TMPDIR"))
    need(sandbox in {"read-only", "workspace-write", "danger-full-access"} and approval_policy in {"auto-review", "never"}
         and (approval_policy != "auto-review" or sandbox == "workspace-write"),
         "RUNTIME_VALUE", "unsupported Codex worker execution policy")
    argv = (sys.executable, "-B", str(bridge), "--launcher", codexrunner_path(launcher),
            "--sandbox", sandbox, "--approval-policy", approval_policy, "{packet_file}")
    return WorkerProfile(argv=argv, cwd=cwd, standing_rule_paths=tuple(standing_rule_paths),
                         environment={"CODEXRUNNER_SANDBOXED": "1"},
                         resource_capacities=dict(resource_capacities or {}), review_capacity=review_capacity,
                         integration_capacity=integration_capacity, branch_for_work=dict(branch_for_work or {}),
                         required_inherited_environment=inherited)


@dataclass(frozen=True)
class VerificationSpec:
    check_id: str
    argv: tuple[str, ...]
    cwd: str
    target: str
    toolchain: str
    environment_label: str
    subjects: tuple[str, ...]
    cases: tuple[str, ...]
    source_refs: tuple[str, ...]
    environment: Mapping[str, str] = field(default_factory=dict, repr=False)

    def __post_init__(self) -> None:
        identity(self.check_id)
        need(self.argv and sum(item == "{packet_file}" for item in self.argv) == 1 and all(isinstance(item, str) for item in self.argv), "ARGV", "verification argv needs one whole packet placeholder")
        need(bool(self.cwd and self.target and self.toolchain and self.environment_label and self.subjects and self.cases and self.source_refs), "RUNTIME_VALUE", "verification binding is incomplete")

    def public_plan(self) -> dict[str, Any]:
        return {"check_id": self.check_id, "argv": list(self.argv), "cwd": self.cwd, "target": self.target,
                "toolchain": self.toolchain, "environment": self.environment_label, "subjects": list(self.subjects),
                "cases": list(self.cases), "source_refs": list(self.source_refs)}


@dataclass(frozen=True)
class RuntimeConfig:
    worker: WorkerProfile
    verifications: Mapping[str, tuple[VerificationSpec, ...]] = field(default_factory=dict)
    assessment_provider: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None
    safe_state_verifier: Callable[[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]], dict[str, Any] | None] | None = None
    artifact_capture: Callable[..., dict[str, Any]] | None = None
    artifact_reader: Callable[..., dict[str, Any]] | None = None
    artifact_allowed_root: str | None = None
    transient_backoff_ns: int = 30_000_000_000
    idle_poll_seconds: float = 0.1

    def __post_init__(self) -> None:
        need(type(self.transient_backoff_ns) is int and self.transient_backoff_ns >= 0, "RUNTIME_VALUE", "backoff must be nonnegative")
        need(self.idle_poll_seconds >= 0, "RUNTIME_VALUE", "poll interval must be nonnegative")
        need((self.artifact_capture is None) == (self.artifact_reader is None) == (self.artifact_allowed_root is None), "RUNTIME_VALUE",
             "artifact capture, reader, and allowed root must be configured together")


def active_contract(state: dict[str, Any], work_id: str) -> tuple[int, dict[str, Any], str]:
    domain = domain_state(state)
    history = domain["task_contracts"].get(work_id)
    need(history is not None, "RUNTIME_CONTRACT", "work has no task contract")
    version = history["active_version"]
    row = next((item for item in history["versions"] if item["version"] == version), None)
    need(row is not None and row["schema"] == "zap-task-contract/1", "RUNTIME_CONTRACT", "work needs a versioned ZAP task contract")
    return version, copy.deepcopy(row["contract"]), row["sha256"]


def source_captures(state: dict[str, Any], source_ids: Sequence[str]) -> list[dict[str, str]]:
    sources = state.get("extensions", {}).get("knowledge", {}).get("sources", {})
    result = []
    for source_id in source_ids:
        identity(source_id); source = sources.get(source_id)
        need(source is not None and source.get("capture_status") == "current", "RUNTIME_STALE", f"source unavailable for packet: {source_id}")
        result.append({"source_id": source_id, "sha256": source["content_sha256"]})
    need(len(result) == len({row["source_id"] for row in result}), "DUPLICATE", "duplicate packet source")
    return sorted(result, key=lambda row: row["source_id"])


def compile_worker_packet(
    state: dict[str, Any], work_id: str, job_id: str, attempt_id: str, profile: WorkerProfile,
    *, semantic_request_id: str, semantic_response_sha256: str,
) -> tuple[str, dict[str, Any]]:
    version, contract, contract_hash = active_contract(state, work_id)
    captures = source_captures(state, contract["source_handles"])
    domain = domain_state(state)
    packet = {
        "schema": "zap-worker-packet/1", "job_id": identity(job_id), "attempt_id": identity(attempt_id), "work_id": identity(work_id),
        "campaign_id": state["plan"]["plan_id"], "base_sha256": state["base_sha256"], "state_revision": state["revision"],
        "domain_revision": domain["revision"], "outcome_id": domain["active_outcome_id"],
        "contract_version": version, "contract_sha256": contract_hash, "contract": contract,
        "read_subjects": contract["read_subjects"], "write_subjects": contract["write_subjects"], "resources": contract["resources"],
        "integration_owner": contract["integration_owner"], "standing_rule_paths": list(profile.standing_rule_paths),
        "verification_bindings": contract["checks"], "safe_boundary": contract["safe_stop"], "source_captures": captures,
        "producer_result": "candidate_only",
    }
    raw = packed(packet).decode("ascii")
    reservation = {"read_subjects": contract["read_subjects"], "write_subjects": contract["write_subjects"], "resources": contract["resources"],
                   "integration_owner": contract["integration_owner"], "branch_id": profile.branch_for_work.get(work_id),
                   "resource_capacities": dict(profile.resource_capacities), "review_capacity": profile.review_capacity,
                   "integration_capacity": profile.integration_capacity}
    claim = {"schema": "zap-runtime/job-claimed/1", "job_id": job_id, "attempt_id": attempt_id, "work_id": work_id,
             "contract_version": version, "contract_sha256": contract_hash, "packet": raw, "packet_sha256": sha(raw.encode("utf-8")),
             "reservation": reservation, "source_captures": captures, "semantic_request_id": semantic_request_id,
             "semantic_response_sha256": semantic_response_sha256}
    return raw, claim


def compile_verification_packet(state: dict[str, Any], work_job: dict[str, Any], spec: VerificationSpec) -> str:
    packet = {"schema": "zap-verification-packet/1", "verification_id": f"verify:{work_job['attempt_id']}:{spec.check_id}",
              "work_id": work_job["work_id"], "work_job_id": work_job["job_id"], "attempt_id": work_job["attempt_id"],
              "check": spec.public_plan(), "source_captures": source_captures(state, spec.source_refs),
              "worker_result": copy.deepcopy(work_job["result"]), "result_contract": "observation_only"}
    return packed(packet).decode("ascii")


def active_job_ids(state: dict[str, Any]) -> list[str]:
    return sorted(job_id for job_id, row in runtime_state(state)["jobs"].items() if row["state"] in ACTIVE_JOB_STATES)


def worker_profile_sha256(profile: WorkerProfile) -> str:
    files = {}
    for item in profile.argv:
        path = Path(item)
        if path.suffix.casefold() in {".py", ".ps1", ".sh"} and path.is_file():
            files[str(path.resolve())] = sha(path.read_bytes())
    return sha(packed({"schema": WORKER_PROVIDER_PROFILE_SCHEMA, "argv": profile.argv, "cwd": profile.cwd,
                       "environment": dict(profile.environment), "stop_mode": profile.stop_mode,
                       "terminate_after_seconds": profile.terminate_after_seconds, "executable_files": files}))


def action_for(
    state: dict[str, Any], *, action_id: str, action_class: str, payload: dict[str, Any], source_rows: list[dict[str, str]],
    branch_id: str | None = None, run_id: str | None = None, problem_id: str | None = None,
) -> dict[str, Any]:
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "runtime action requires an active charter")
    return {"schema": "zap-action/1", "action_id": identity(action_id), "action_class": action_class,
            "campaign_id": policy["campaign_id"], "base_sha256": policy["base_sha256"], "charter_revision": policy["revision"],
            "payload_sha256": sha(packed(payload)), "source_captures": copy.deepcopy(source_rows),
            "branch_id": branch_id, "run_id": run_id, "problem_id": problem_id}


def assessment_for(state: dict[str, Any], action: dict[str, Any], config: RuntimeConfig, assessment_id: str,
                   command_context: dict[str, Any] | None = None) -> dict[str, Any]:
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "runtime assessment requires active policy")
    provider = config.assessment_provider
    supplied = (provider.assess(state, action, command_context) if provider is not None and callable(getattr(provider, "assess", None))
                else provider(state, action) if provider is not None else {"values": {}, "drain_targets": active_job_ids(state)})
    need(supplied is not None, "ASSESSMENT_PENDING", "trusted action assessment is still pending")
    need(isinstance(supplied, dict) and set(supplied) == {"values", "drain_targets"}, "ASSESSMENT", "assessment provider must return values and drain_targets")
    return {"schema": "zap-assessment/1", "assessment_id": identity(assessment_id), "policy_id": policy["stop_policy"]["policy_id"],
            "policy_revision": policy["stop_policy"]["revision"], "phase": "before_action", "values": supplied["values"],
            "drain_targets": sorted(supplied["drain_targets"])}
