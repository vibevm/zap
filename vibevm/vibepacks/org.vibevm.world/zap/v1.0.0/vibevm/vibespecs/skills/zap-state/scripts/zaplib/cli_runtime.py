"""Strict runtime-profile loading and AutomaticCoordinator construction."""
from __future__ import annotations

from pathlib import Path
import copy
from functools import partial
from typing import Any

from .artifacts import PrivateArtifactStore, capture_source_blob
from .common import exact, need, parse
from .engine import Engine
from .runtime import (
    AutomaticCoordinator, JsonProcessCoordinatorAdapter, RuntimeConfig,
    VerificationSpec, WorkerProfile, codex_sol_xhigh_adapter,
    codex_sol_xhigh_worker_profile,
)
from .runtime_assessment import JsonProcessAssessmentAdapter
from .transport import ProcessTransport

PROFILE_SCHEMA = "zap-runtime-profile/1"


def _strings(value: Any, label: str, *, nonempty: bool = False) -> list[str]:
    need(isinstance(value, list) and all(isinstance(item, str) and item for item in value), "PROFILE", f"{label} must be a string list")
    need(not nonempty or bool(value), "PROFILE", f"{label} must not be empty")
    return value


def _string_map(value: Any, label: str, *, nonempty_values: bool = False) -> dict[str, str]:
    need(isinstance(value, dict) and all(isinstance(key, str) and key and isinstance(item, str) and (item or not nonempty_values) for key, item in value.items()), "PROFILE", f"{label} must map strings to strings")
    return value


def _transport(
    value: Any,
    *,
    required_inherited: tuple[str, ...] = (),
    required_environment: tuple[str, ...] = (),
) -> ProcessTransport:
    exact(value, {"root", "allowed_workspace_roots", "inherited_environment", "environment_allowlist", "allow_process_termination"})
    need(isinstance(value["root"], str) and value["root"], "PROFILE", "transport root is required")
    _strings(value["allowed_workspace_roots"], "transport roots", nonempty=True)
    need(value["inherited_environment"] is None or isinstance(value["inherited_environment"], list), "PROFILE", "inherited environment must be null or a list")
    if value["inherited_environment"] is not None:
        _strings(value["inherited_environment"], "inherited environment")
        need(set(required_inherited) <= set(value["inherited_environment"]), "PROFILE", "transport omits environment required by worker profile")
    configured_allowlist = _strings(value["environment_allowlist"], "environment allowlist")
    environment_allowlist = sorted(set(configured_allowlist) | set(required_environment))
    return ProcessTransport(
        value["root"], value["allowed_workspace_roots"],
        inherited_environment=required_inherited if value["inherited_environment"] is None and required_inherited else value["inherited_environment"],
        environment_allowlist=environment_allowlist,
        allow_process_termination=value["allow_process_termination"],
    )


def _worker(value: Any) -> WorkerProfile:
    exact(value, {
        "argv", "cwd", "standing_rule_paths", "environment",
        "resource_capacities", "review_capacity", "integration_capacity",
        "branch_for_work", "stop_mode", "terminate_after_seconds",
    }, {"kind", "launcher"})
    kind = value.get("kind", "argv")
    need(kind in {"argv", "codex-sol-xhigh"}, "PROFILE", "unknown worker profile kind")
    argv = _strings(value["argv"], "worker argv", nonempty=kind == "argv")
    paths = _strings(value["standing_rule_paths"], "standing rule paths")
    environment = _string_map(value["environment"], "worker environment")
    need(isinstance(value["resource_capacities"], dict) and all(isinstance(key, str) and key for key in value["resource_capacities"]), "PROFILE", "resource capacities must be a mapping")
    need(type(value["review_capacity"]) is int and value["review_capacity"] > 0
         and type(value["integration_capacity"]) is int and value["integration_capacity"] > 0,
         "PROFILE", "worker review/integration capacities must be positive integers")
    branch_for_work = _string_map(value["branch_for_work"], "branch bindings", nonempty_values=True)
    if kind == "codex-sol-xhigh":
        need(not argv and not environment, "PROFILE", "codex-sol-xhigh worker uses its fixed bridge argv/environment")
        need(value["stop_mode"] == "cooperative" and value["terminate_after_seconds"] is None, "PROFILE", "codex-sol-xhigh worker profile uses cooperative stop")
        launcher = value.get("launcher")
        need(launcher is None or isinstance(launcher, str) and launcher, "PROFILE", "worker launcher must be null or a nonempty path")
        return codex_sol_xhigh_worker_profile(
            cwd=value["cwd"], standing_rule_paths=paths, launcher=launcher,
            resource_capacities=value["resource_capacities"], review_capacity=value["review_capacity"],
            integration_capacity=value["integration_capacity"], branch_for_work=branch_for_work,
        )
    need(value.get("launcher") is None, "PROFILE", "argv worker does not accept a launcher override")
    return WorkerProfile(
        argv=tuple(argv),
        cwd=value["cwd"],
        standing_rule_paths=tuple(paths),
        environment=environment,
        resource_capacities=dict(value["resource_capacities"]),
        review_capacity=value["review_capacity"],
        integration_capacity=value["integration_capacity"],
        branch_for_work=branch_for_work,
        stop_mode=value["stop_mode"],
        terminate_after_seconds=value["terminate_after_seconds"],
    )


def _verification(value: Any) -> VerificationSpec:
    exact(value, {
        "check_id", "argv", "cwd", "target", "toolchain",
        "environment_label", "subjects", "cases", "source_refs", "environment",
    })
    argv = _strings(value["argv"], "verification argv", nonempty=True)
    subjects = _strings(value["subjects"], "verification subjects", nonempty=True)
    cases = _strings(value["cases"], "verification cases", nonempty=True)
    source_refs = _strings(value["source_refs"], "verification source refs", nonempty=True)
    environment = _string_map(value["environment"], "verification environment")
    need(all(isinstance(value[key], str) and value[key] for key in ("check_id", "cwd", "target", "toolchain", "environment_label")),
         "PROFILE", "verification scalar fields must be nonempty strings")
    return VerificationSpec(
        check_id=value["check_id"],
        argv=tuple(argv),
        cwd=value["cwd"],
        target=value["target"],
        toolchain=value["toolchain"],
        environment_label=value["environment_label"],
        subjects=tuple(subjects),
        cases=tuple(cases),
        source_refs=tuple(source_refs),
        environment=environment,
    )


def load_runtime_profile(path: str | Path) -> dict[str, Any]:
    value = parse(Path(path).read_bytes())
    exact(value, {"schema", "transport", "semantic_transport", "semantic", "worker", "verifications", "runtime"},
          {"artifact_capture", "assessment_transport", "assessment"})
    need(value["schema"] == PROFILE_SCHEMA, "PROFILE", "unknown runtime profile schema")
    exact(value["semantic"], {"kind", "argv", "cwd", "launcher"})
    need(value["semantic"]["kind"] in {"json-process", "codex-sol-xhigh"}, "PROFILE", "unknown semantic adapter kind")
    need(isinstance(value["semantic"]["cwd"], str) and value["semantic"]["cwd"], "PROFILE", "semantic cwd is required")
    need(isinstance(value["verifications"], dict), "PROFILE", "verifications must be a mapping")
    need(all(isinstance(key, str) and key and isinstance(rows, list) for key, rows in value["verifications"].items()), "PROFILE", "verification bindings must map work ids to lists")
    exact(value["runtime"], {"transient_backoff_ns", "idle_poll_seconds"})
    need(type(value["runtime"]["transient_backoff_ns"]) is int and value["runtime"]["transient_backoff_ns"] >= 0,
         "PROFILE", "runtime backoff must be a nonnegative integer")
    need(type(value["runtime"]["idle_poll_seconds"]) in {int, float} and value["runtime"]["idle_poll_seconds"] >= 0,
         "PROFILE", "runtime poll interval must be a nonnegative number")
    if "artifact_capture" in value:
        exact(value["artifact_capture"], {"allowed_root"})
        need(isinstance(value["artifact_capture"]["allowed_root"], str) and value["artifact_capture"]["allowed_root"], "PROFILE", "artifact capture allowed root is required")
    need(("assessment" in value) == ("assessment_transport" in value), "PROFILE", "assessment profile and transport must be configured together")
    if "assessment" in value:
        exact(value["assessment"], {"kind", "argv", "cwd"}, {"observation_paths"})
        need(value["assessment"]["kind"] == "json-process", "PROFILE", "unknown trusted assessment adapter kind")
        _strings(value["assessment"]["argv"], "assessment argv", nonempty=True)
        _strings(value["assessment"].get("observation_paths", []), "assessment observation paths")
        need(isinstance(value["assessment"]["cwd"], str) and value["assessment"]["cwd"], "PROFILE", "assessment cwd is required")
    return value


def _resolve_profile_paths(value: dict[str, Any], profile_path: str | Path) -> dict[str, Any]:
    result = copy.deepcopy(value)
    base = Path(profile_path).resolve().parent

    def resolve(raw: str) -> str:
        path = Path(raw)
        return str(path if path.is_absolute() else base / path)

    for key in ("transport", "semantic_transport", "assessment_transport"):
        if key not in result:
            continue
        result[key]["root"] = resolve(result[key]["root"])
        result[key]["allowed_workspace_roots"] = [resolve(item) for item in result[key]["allowed_workspace_roots"]]
    if "artifact_capture" in result:
        result["artifact_capture"]["allowed_root"] = resolve(result["artifact_capture"]["allowed_root"])
    result["worker"]["cwd"] = resolve(result["worker"]["cwd"])
    result["worker"]["standing_rule_paths"] = [resolve(item) for item in result["worker"]["standing_rule_paths"]]
    result["semantic"]["cwd"] = resolve(result["semantic"]["cwd"])
    if "assessment" in result:
        result["assessment"]["cwd"] = resolve(result["assessment"]["cwd"])
        result["assessment"]["observation_paths"] = [
            resolve(item) for item in result["assessment"].get("observation_paths", [])
        ]
    if result["semantic"]["launcher"]:
        result["semantic"]["launcher"] = resolve(result["semantic"]["launcher"])
    if result["worker"].get("launcher"):
        result["worker"]["launcher"] = resolve(result["worker"]["launcher"])
    for rows in result["verifications"].values():
        for row in rows:
            row["cwd"] = resolve(row["cwd"])
    return result


def build_automatic_coordinator(
    engine: Engine,
    profile_path: str | Path,
    *,
    artifacts: PrivateArtifactStore | None = None,
) -> AutomaticCoordinator:
    need(engine.host_principal is not None and engine.host_principal.role in {"owner", "coordinator"}, "PRINCIPAL", "runtime needs a configured coordinator host principal")
    value = _resolve_profile_paths(load_runtime_profile(profile_path), profile_path)
    worker = _worker(value["worker"])
    transport = _transport(
        value["transport"], required_inherited=worker.required_inherited_environment,
        required_environment=tuple(worker.environment),
    )
    semantic_config = value["semantic"]
    if semantic_config["kind"] == "codex-sol-xhigh":
        need(semantic_config["launcher"] is None or isinstance(semantic_config["launcher"], str) and semantic_config["launcher"], "PROFILE", "codex-sol-xhigh launcher must be null or a nonempty path")
        semantic_transport_config = value["semantic_transport"]
        need(
            semantic_transport_config["inherited_environment"] is None
            and semantic_transport_config["environment_allowlist"] == []
            and semantic_transport_config["allow_process_termination"] is False,
            "PROFILE",
            "codex-sol-xhigh semantic transport uses its fixed credential-preserving environment and cooperative stop boundary",
        )
        semantic = codex_sol_xhigh_adapter(
            semantic_transport_config["root"],
            semantic_transport_config["allowed_workspace_roots"],
            cwd=semantic_config["cwd"],
            launcher=semantic_config["launcher"],
        )
    else:
        semantic_argv = _strings(semantic_config["argv"], "semantic argv", nonempty=True)
        need(semantic_config["launcher"] is None, "PROFILE", "json-process semantic adapter does not accept a launcher override")
        semantic_transport = _transport(value["semantic_transport"])
        semantic = JsonProcessCoordinatorAdapter(
            semantic_transport,
            argv=tuple(semantic_argv),
            cwd=semantic_config["cwd"],
        )
    verifications = {
        work_id: tuple(_verification(row) for row in rows)
        for work_id, rows in value["verifications"].items()
    }
    artifact_config = value.get("artifact_capture")
    need(not verifications or artifact_config is not None, "PROFILE", "configured verifications require guarded artifact capture")
    need(artifact_config is None or artifacts is not None, "PROFILE", "artifact capture profile requires a private artifact store")
    assessment_provider = None
    if "assessment" in value:
        need(value["assessment_transport"]["allow_process_termination"] is False,
             "PROFILE", "assessment transport cannot enable process termination")
        assessment_transport = _transport(value["assessment_transport"])
        assessment_provider = JsonProcessAssessmentAdapter(
            assessment_transport, argv=tuple(value["assessment"]["argv"]),
            cwd=value["assessment"]["cwd"],
            observation_paths=value["assessment"].get("observation_paths", ()),
        )
    config = RuntimeConfig(
        worker=worker,
        verifications=verifications,
        assessment_provider=assessment_provider,
        artifact_capture=None if artifact_config is None else partial(capture_source_blob, artifacts),
        artifact_reader=None if artifact_config is None else artifacts.read,
        artifact_allowed_root=None if artifact_config is None else artifact_config["allowed_root"],
        transient_backoff_ns=value["runtime"]["transient_backoff_ns"],
        idle_poll_seconds=value["runtime"]["idle_poll_seconds"],
    )
    need(config.worker.stop_mode != "terminate" or value["transport"]["allow_process_termination"] is True, "PROFILE", "terminate worker mode requires transport termination capability")
    return AutomaticCoordinator(
        engine.store, engine.handlers, engine.service, transport, semantic, config,
    )
