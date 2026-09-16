"""Asynchronous semantic coordinator protocol and durable JSON process adapter."""
from __future__ import annotations

import copy
from dataclasses import dataclass
import os
from pathlib import Path
import shutil
import sys
from typing import Any, Iterable, Mapping, Protocol, Sequence

from .common import Refusal, exact, identity, need, packed, parse, sha, string
from .domain import DOMAIN_EVENT_SCHEMAS, SPARSE_REVIEW_TRANSITION_SCHEMA
from .knowledge import KNOWLEDGE_EVENT_SCHEMAS
from .transport import ProcessTransport

COORDINATOR_REQUEST_SCHEMA = "zap-coordinator-request/1"
COORDINATOR_RESPONSE_SCHEMA = "zap-coordinator-response/1"
PROVIDER_PROFILE_SCHEMA = "zap-codex-provider-profile/2"
COMMANDS_BY_REQUEST = {
    "selection": set(),
    "review": {"domain.review-proposed", "domain.outcome-proposed", "domain.task-contract-replaced", "domain.deferral-created",
               "domain.deferral-transferred", "domain.deferral-closed", "domain.deferral-inapplicable",
               "knowledge.region-transitioned", "knowledge.region-relevance-set", "knowledge.region-split", "knowledge.region-merged"},
    "reassessment": {"domain.review-proposed", "domain.outcome-proposed", "domain.plan-lowered", "domain.task-contract-replaced",
                     "domain.deferral-created", "domain.deferral-transferred", "domain.deferral-closed", "domain.deferral-inapplicable",
                     "knowledge.region-transitioned", "knowledge.region-relevance-set", "knowledge.region-split", "knowledge.region-merged"},
    "acceptance": {"domain.evidence-adjudicated", "domain.stage-accepted", "domain.integration-accepted", "domain.work-accepted"},
    "closure": {"domain.campaign-closed"},
}

def command_contracts_for(request_kind: str) -> dict[str, Any]:
    """Return only the authoritative payload contracts allowed for one request."""
    need(request_kind in COMMANDS_BY_REQUEST, "COORDINATOR", "unknown coordinator request kind")
    result = {}
    for kind in sorted(COMMANDS_BY_REQUEST[request_kind]):
        schema = DOMAIN_EVENT_SCHEMAS.get(kind) or KNOWLEDGE_EVENT_SCHEMAS.get(kind)
        need(schema is not None, "COORDINATOR", f"missing event contract for {kind}")
        result[kind] = {"payload_schema": copy.deepcopy(schema)}
        if kind == "domain.review-proposed":
            result[kind]["sparse_transition_schema"] = copy.deepcopy(SPARSE_REVIEW_TRANSITION_SCHEMA)
    return result

COORDINATOR_RESPONSE_JSON_SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "required": ["schema", "request_id", "request_kind", "state_revision", "request_sha256", "disposition", "rationale", "selection", "commands"],
    "properties": {
        "schema": {"type": "string", "const": COORDINATOR_RESPONSE_SCHEMA},
        "request_id": {"type": "string"},
        "request_kind": {"type": "string", "enum": sorted(COMMANDS_BY_REQUEST)},
        "state_revision": {"type": "integer", "minimum": 0},
        "request_sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
        "disposition": {"type": "string", "enum": ["select", "commands", "needs_evidence", "wait", "no_change"]},
        "rationale": {"type": "string", "minLength": 1},
        "selection": {
            "anyOf": [
                {"type": "null"},
                {"type": "object", "additionalProperties": False, "required": ["work_id"],
                 "properties": {"work_id": {"type": "string"}}},
            ],
        },
        "commands": {
            "type": "array",
            "items": {
                "type": "object", "additionalProperties": False,
                "required": ["kind", "payload", "reason"],
                "properties": {
                    "kind": {"type": "string"}, "payload": {"type": "object"},
                    "reason": {"type": "string", "minLength": 1},
                },
            },
        },
    },
}


def provider_response_json_schema(request: dict[str, Any]) -> dict[str, Any]:
    """Return a fully closed strict-output schema for one bound provider call."""
    request = validate_request(request)
    allowed = sorted(COMMANDS_BY_REQUEST[request["request_kind"]]) or ["none"]
    dispositions = ["select", "needs_evidence", "wait", "no_change"] if request["request_kind"] == "selection" else ["commands", "needs_evidence", "wait", "no_change"]
    selection = ({"anyOf": [
        {"type": "null"},
        {"type": "object", "additionalProperties": False, "required": ["work_id"],
         "properties": {"work_id": {"type": "string"}}},
    ]} if request["request_kind"] == "selection" else {"type": "null"})
    commands = {"type": "array", "items": {
        "type": "object", "additionalProperties": False,
        "required": ["kind", "payload_json", "reason"],
        "properties": {"kind": {"type": "string", "enum": allowed}, "payload_json": {"type": "string"}, "reason": {"type": "string"}},
    }}
    if request["request_kind"] == "selection":
        commands["maxItems"] = 0
    return {
        "type": "object", "additionalProperties": False,
        "required": ["schema", "disposition", "rationale", "selection", "commands"],
        "properties": {
            "schema": {"type": "string", "const": "zap-provider-coordinator-output/1"},
            "disposition": {"type": "string", "enum": dispositions},
            "rationale": {"type": "string"},
            "selection": selection,
            "commands": commands,
        },
    }


def bind_provider_response(request: dict[str, Any], value: Any) -> dict[str, Any]:
    """Decode strict provider strings and inject immutable request bindings."""
    request = validate_request(request)
    exact(value, {"schema", "disposition", "rationale", "selection", "commands"})
    need(value["schema"] == "zap-provider-coordinator-output/1" and isinstance(value["commands"], list), "COORDINATOR", "invalid provider response")
    commands = []
    for command in value["commands"]:
        exact(command, {"kind", "payload_json", "reason"}); payload = parse(command["payload_json"])
        need(isinstance(payload, dict), "COORDINATOR", "provider command payload_json must decode to an object")
        commands.append({"kind": command["kind"], "payload": payload, "reason": command["reason"]})
    response = {"schema": COORDINATOR_RESPONSE_SCHEMA, "request_id": request["request_id"], "request_kind": request["request_kind"],
                "state_revision": request["state_revision"], "request_sha256": sha(packed(request)), "disposition": value["disposition"],
                "rationale": value["rationale"], "selection": value["selection"], "commands": commands}
    return validate_response(request, response)


class SemanticCoordinatorAdapter(Protocol):
    def submit(self, request_id: str, request: dict[str, Any]) -> dict[str, Any]: ...
    def poll(self, request_id: str, request: dict[str, Any]) -> dict[str, Any]: ...
    def request_stop(self, request_id: str, stop_id: str) -> dict[str, Any]: ...


def validate_request(value: Any) -> dict[str, Any]:
    exact(value, {"schema", "request_id", "request_kind", "state_revision", "base_sha256", "request"})
    need(value["schema"] == COORDINATOR_REQUEST_SCHEMA and value["request_kind"] in COMMANDS_BY_REQUEST, "COORDINATOR", "unknown coordinator request")
    identity(value["request_id"])
    need(type(value["state_revision"]) is int and value["state_revision"] >= 0, "COORDINATOR", "invalid coordinator state revision")
    need(isinstance(value["base_sha256"], str) and len(value["base_sha256"]) == 64, "COORDINATOR", "invalid coordinator base hash")
    need(isinstance(value["request"], dict), "COORDINATOR", "coordinator request body must be a mapping")
    return value


def validate_response(request: dict[str, Any], value: Any) -> dict[str, Any]:
    request = validate_request(request)
    exact(value, {"schema", "request_id", "request_kind", "state_revision", "request_sha256", "disposition", "rationale", "selection", "commands"})
    need(value["schema"] == COORDINATOR_RESPONSE_SCHEMA and value["request_id"] == request["request_id"] and value["request_kind"] == request["request_kind"], "COORDINATOR", "coordinator response binding differs")
    need(value["state_revision"] == request["state_revision"] and value["request_sha256"] == sha(packed(request)), "COORDINATOR_STALE", "coordinator response state/request binding differs")
    need(value["disposition"] in {"select", "commands", "needs_evidence", "wait", "no_change"}, "COORDINATOR", "unknown coordinator disposition")
    string(value["rationale"], "coordinator rationale")
    commands = value["commands"]
    need(isinstance(commands, list), "COORDINATOR", "coordinator commands must be a list")
    for command in commands:
        exact(command, {"kind", "payload", "reason"}); identity(command["kind"]); string(command["reason"], "coordinator command reason")
        need(isinstance(command["payload"], dict) and command["kind"] in COMMANDS_BY_REQUEST[request["request_kind"]], "COORDINATOR", "coordinator command is outside request scope")
    if request["request_kind"] == "closure" and commands:
        need(len(commands) == 1 and commands[0]["kind"] == "domain.campaign-closed" and
             commands[0]["payload"].get("classification") in {"original", "revised"},
             "COORDINATOR", "automatic closure permits evidence-backed original/revised success only")
    selection = value["selection"]
    if value["disposition"] == "select":
        exact(selection, {"work_id"}); identity(selection["work_id"]); need(not commands and request["request_kind"] == "selection", "COORDINATOR", "selection response shape differs")
    elif value["disposition"] == "commands":
        need(selection is None and commands and request["request_kind"] != "selection", "COORDINATOR", "command response shape differs")
    else:
        need(selection is None and not commands, "COORDINATOR", "non-action response must not carry effects")
    return value


class JsonProcessCoordinatorAdapter:
    """Run one JSON semantic request per durable ProcessTransport job."""

    def __init__(self, transport: ProcessTransport, *, argv: Sequence[str], cwd: str | os.PathLike[str], environment: Mapping[str, str] | None = None):
        need(isinstance(transport, ProcessTransport), "COORDINATOR", "JSON coordinator needs ProcessTransport")
        need(sum(item == "{packet_file}" for item in argv) == 1, "ARGV", "coordinator argv needs one packet placeholder")
        self.transport = transport; self.argv = tuple(argv); self.cwd = str(cwd); self.environment = dict(environment or {})

    def submit(self, request_id: str, request: dict[str, Any]) -> dict[str, Any]:
        validate_request(request); need(request_id == request["request_id"], "COORDINATOR", "request identity differs")
        return self.transport.submit(request_id, argv=self.argv, cwd=self.cwd, packet=packed(request).decode("ascii"), environment=self.environment)

    def profile_sha256(self) -> str:
        files = {}
        for item in self.argv:
            path = Path(item)
            if path.suffix.casefold() in {".py", ".ps1", ".sh"} and path.is_file():
                files[str(path.resolve())] = sha(path.read_bytes())
        return sha(packed({"schema": PROVIDER_PROFILE_SCHEMA, "argv": self.argv, "cwd": self.cwd,
                           "environment": self.environment, "executable_files": files}))

    @staticmethod
    def _failure_diagnostic(result: dict[str, Any]) -> dict[str, Any]:
        diagnostic = dict(result.get("diagnostic") or {})
        stderr = result.get("stderr")
        if not isinstance(stderr, dict):
            return diagnostic
        raw = Path(stderr["path"]).read_bytes()
        need(len(raw) == stderr["bytes"] and sha(raw) == stderr["sha256"], "COORDINATOR", "coordinator stderr artifact differs")
        lowered = raw.decode("utf-8", errors="replace").casefold()
        if "auth.json" in lowered or "not logged in" in lowered or "authentication" in lowered:
            diagnostic["classification"] = "provider_auth"
        elif "rate limit" in lowered or "quota" in lowered:
            diagnostic["classification"] = "provider_quota"
        elif any(marker in lowered for marker in ("invalid_json_schema", "not inside a trusted directory", "psargumentexception",
                                                   "unrecognized option", "unknown option", "strict config")):
            diagnostic["classification"] = "configuration_error"
        elif "model_response_invalid" in lowered or "command response shape differs" in lowered or "coordinator response" in lowered or "zaplib.common.refusal" in lowered:
            diagnostic["classification"] = "model_response_invalid"
        diagnostic["stderr_sha256"] = stderr["sha256"]
        stdout = result.get("stdout")
        if isinstance(stdout, dict):
            raw_out = Path(stdout["path"]).read_bytes()
            need(len(raw_out) == stdout["bytes"] and sha(raw_out) == stdout["sha256"], "COORDINATOR", "coordinator failure output differs")
            try:
                failure = parse(raw_out)
            except (Refusal, ValueError, UnicodeError):
                failure = None
            if isinstance(failure, dict) and failure.get("schema") == "zap-provider-validation-failure/1":
                diagnostic["classification"] = "model_response_invalid"
                diagnostic["validator_feedback"] = copy.deepcopy(failure.get("diagnostic"))
                diagnostic["provider_response_sha256"] = failure.get("provider_response_sha256")
        return diagnostic

    def poll(self, request_id: str, request: dict[str, Any]) -> dict[str, Any]:
        validate_request(request); result = self.transport.collect(request_id)
        if not result["ready"]:
            return {"ready": False, "state": result["state"], "diagnostic": result.get("diagnostic")}
        if result["state"] != "succeeded":
            diagnostic = self._failure_diagnostic(result); diagnostic["profile_sha256"] = self.profile_sha256()
            return {"ready": True, "ok": False, "state": result["state"], "diagnostic": diagnostic}
        stdout = result["stdout"]; raw = Path(stdout["path"]).read_bytes()
        need(len(raw) == stdout["bytes"] and sha(raw) == stdout["sha256"], "COORDINATOR", "coordinator output artifact differs")
        try:
            response = validate_response(request, parse(raw))
        except (Refusal, ValueError, UnicodeError):
            return {"ready": True, "ok": False, "state": "invalid_response", "diagnostic": {
                "classification": "model_response_invalid", "stdout_sha256": stdout["sha256"],
                "profile_sha256": self.profile_sha256()}}
        return {"ready": True, "ok": True, "state": "succeeded", "response": response, "response_sha256": sha(packed(response)), "transport": result}

    def request_stop(self, request_id: str, stop_id: str) -> dict[str, Any]:
        return self.transport.request_stop(request_id, stop_id)


@dataclass(frozen=True)
class CoordinatorProcessProfile:
    argv: tuple[str, ...]
    cwd: str
    inherited_environment: tuple[str, ...]
    environment: Mapping[str, str]


def codexrunner_path(launcher: str | os.PathLike[str] | None = None) -> str:
    resolved = str(launcher) if launcher is not None else (shutil.which("codexrunner") or shutil.which("codexrunner.ps1"))
    need(resolved is not None and bool(resolved.strip()), "COORDINATOR", "codexrunner launcher was not found on PATH; pass launcher explicitly")
    return resolved


def codex_sol_xhigh_profile(
    *, cwd: str | os.PathLike[str], launcher: str | os.PathLike[str] | None = None,
) -> CoordinatorProcessProfile:
    """Ready ProcessTransport profile for the verified Codex Sol/xhigh lane."""
    resolved = codexrunner_path(launcher)
    bridge = Path(__file__).with_name("provider_bridge.py")
    argv = (sys.executable, "-B", str(bridge), "--launcher", resolved, "{packet_file}")
    inherited = (("SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "PATH", "TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA")
                 if os.name == "nt" else ("PATH", "HOME", "LANG", "LC_ALL", "TMPDIR"))
    return CoordinatorProcessProfile(argv=argv, cwd=str(cwd), inherited_environment=inherited,
                                     environment={"CODEXRUNNER_SANDBOXED": "1"})


def codex_sol_xhigh_adapter(
    transport_root: str | os.PathLike[str], allowed_workspace_roots: Iterable[str | os.PathLike[str]], *,
    cwd: str | os.PathLike[str], launcher: str | os.PathLike[str] | None = None,
) -> JsonProcessCoordinatorAdapter:
    """Construct the ready durable adapter while preserving provider auth homes."""
    profile = codex_sol_xhigh_profile(cwd=cwd, launcher=launcher)
    transport = ProcessTransport(transport_root, allowed_workspace_roots, inherited_environment=profile.inherited_environment,
                                 environment_allowlist=profile.environment)
    return JsonProcessCoordinatorAdapter(transport, argv=profile.argv, cwd=profile.cwd, environment=profile.environment)
