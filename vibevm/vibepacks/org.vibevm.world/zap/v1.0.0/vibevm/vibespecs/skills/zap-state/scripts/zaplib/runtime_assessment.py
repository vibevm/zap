"""Nonblocking exact-action stop assessment through a trusted JSON process."""
from __future__ import annotations

import base64
import copy
import math
from pathlib import Path
import threading
from typing import Any, Sequence

from .common import Refusal, exact, identity, need, packed, parse, sha
from .control import active_policy, validate_action
from .runtime_packets import active_job_ids
from .transport import ProcessTransport

ASSESSMENT_REQUEST_SCHEMA = "zap-runtime/assessment-request/1"
ASSESSMENT_RESPONSE_SCHEMA = "zap-runtime/assessment-response/1"
MAX_CONTEXT_STRING = 4096
MAX_CONTEXT_ITEMS = 100
MAX_OBSERVATION_BYTES = 65536
REDACTED_KEYS = {"credential", "credentials", "packet", "private_prompt", "response", "secret", "token"}


def _fields(expression: Any) -> set[str]:
    if not isinstance(expression, dict) or len(expression) != 1:
        return set()
    operator, value = next(iter(expression.items()))
    if operator in {"all", "any"} and isinstance(value, list):
        return {field for item in value for field in _fields(item)}
    if operator == "not":
        return _fields(value)
    if operator == "eq" and isinstance(value, dict) and isinstance(value.get("field"), str):
        return {value["field"]}
    return set()


def _bounded(value: Any, depth: int = 0) -> Any:
    if depth >= 5:
        return {"truncated": True, "sha256": sha(packed(value))}
    if isinstance(value, str):
        if len(value) <= MAX_CONTEXT_STRING:
            return value
        return {"truncated": True, "sha256": sha(value.encode("utf-8")),
                "prefix": value[:MAX_CONTEXT_STRING], "original_chars": len(value)}
    if isinstance(value, list):
        rows = [_bounded(item, depth + 1) for item in value[:MAX_CONTEXT_ITEMS]]
        if len(value) <= MAX_CONTEXT_ITEMS:
            return rows
        return {"items": rows, "truncated": True, "total_items": len(value),
                "sha256": sha(packed(value))}
    if isinstance(value, dict):
        result = {}
        for key in sorted(value)[:MAX_CONTEXT_ITEMS]:
            item = value[key]
            result[key] = ({"redacted": True, "sha256": sha(packed(item))}
                           if key.casefold() in REDACTED_KEYS else _bounded(item, depth + 1))
        if len(value) > MAX_CONTEXT_ITEMS:
            result["$truncated"] = {"total_fields": len(value), "sha256": sha(packed(value))}
        return result
    return copy.deepcopy(value)


def _observations(paths: Sequence[str | Path]) -> list[dict[str, Any]]:
    result = []
    for item in paths:
        path = Path(item).resolve()
        try:
            raw = path.read_bytes()
        except OSError as exc:
            result.append({"path": str(path), "sha256": None, "bytes": None,
                           "encoding": None, "content": None,
                           "content_complete": False, "status": "unavailable",
                           "error": type(exc).__name__})
            continue
        part = raw[:MAX_OBSERVATION_BYTES]
        try:
            content, encoding = part.decode("utf-8"), "utf-8"
        except UnicodeDecodeError:
            content, encoding = base64.b64encode(part).decode("ascii"), "base64"
        result.append({"path": str(path), "sha256": sha(raw), "bytes": len(raw),
                       "encoding": encoding, "content": content,
                       "content_complete": len(part) == len(raw), "status": "current",
                       "error": None})
    return result


def _relevant_context(state: dict[str, Any], action: dict[str, Any]) -> dict[str, Any]:
    source_ids = {row["source_id"] for row in action["source_captures"]}
    subject = action.get("problem_id")
    knowledge = state.get("extensions", {}).get("knowledge", {})
    sources = knowledge.get("sources", {})
    source_rows = []
    for capture in action["source_captures"]:
        current = sources.get(capture["source_id"], {})
        source_rows.append({"source_id": capture["source_id"], "expected_sha256": capture["sha256"],
                            "current_sha256": current.get("content_sha256"),
                            "capture_status": current.get("capture_status"),
                            "applicability": _bounded(knowledge.get("applicability", {}).get(capture["source_id"]))})
    facts = [_bounded(row) for row in state.get("facts", {}).values()
             if source_ids.intersection(row.get("source_refs", [])) or subject in row.get("node_refs", [])]
    evidence = [_bounded(row) for row in state.get("evidence", {}).values()
                if subject in row.get("node_refs", [])]
    domain = state.get("extensions", {}).get("domain", {})
    return {"sources": source_rows, "facts": facts[:MAX_CONTEXT_ITEMS],
            "evidence": evidence[:MAX_CONTEXT_ITEMS],
            "active_intent_id": domain.get("active_intent_id"),
            "active_outcome_id": domain.get("active_outcome_id"),
            "validation_generation": domain.get("validation_generations", {}).get(subject, 0) if subject else None}


def build_assessment_request(
    state: dict[str, Any],
    action: Any,
    command_context: Any,
    *,
    observation_paths: Sequence[str | Path] = (),
) -> dict[str, Any]:
    checked = validate_action(action)
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "assessment provider requires an active charter")
    need(checked["campaign_id"] == policy["campaign_id"] and checked["base_sha256"] == policy["base_sha256"],
         "STALE_POLICY", "assessment action campaign/base differs")
    need(checked["charter_revision"] == policy["revision"], "STALE_POLICY", "assessment action charter differs")
    exact(command_context, {"event_id", "base_revision", "kind", "reason", "payload"})
    identity(command_context["event_id"]); identity(command_context["kind"])
    need(sha(packed(command_context["payload"])) == checked["payload_sha256"],
         "ASSESSMENT_STALE", "assessment command payload differs from action binding")
    rules = [copy.deepcopy(row) for row in policy["stop_policy"]["rules"]
             if checked["action_class"] in row["applies_to_actions"]]
    required_fields = sorted({field for rule in rules for field in _fields(rule["when"])})
    logical_command = {key: command_context[key] for key in ("event_id", "kind", "reason", "payload")}
    basis = {
        "campaign_id": policy["campaign_id"], "base_sha256": policy["base_sha256"],
        "action": checked,
        "command": {"event_id": command_context["event_id"], "kind": command_context["kind"],
                    "logical_sha256": sha(packed(logical_command)),
                    "payload_sha256": checked["payload_sha256"],
                    "payload_context": _bounded(command_context["payload"])},
        "policy": {"policy_id": policy["stop_policy"]["policy_id"],
                   "policy_revision": policy["stop_policy"]["revision"], "rules": rules},
        "required_fields": required_fields,
        "relevant_context": _relevant_context(state, checked),
        "trusted_observations": _observations(observation_paths),
    }
    basis_sha256 = sha(packed(basis))
    request = {"schema": ASSESSMENT_REQUEST_SCHEMA,
               "request_id": f"assessment:{checked['action_id']}:{basis_sha256}",
               "basis_sha256": basis_sha256, **basis}
    request["request_sha256"] = sha(packed(request))
    return request


def validate_assessment_response(request: dict[str, Any], value: Any) -> dict[str, Any]:
    exact(value, {
        "schema", "request_id", "request_sha256", "basis_sha256", "campaign_id",
        "base_sha256", "action_id", "action_payload_sha256", "values",
    })
    need(value["schema"] == ASSESSMENT_RESPONSE_SCHEMA, "ASSESSMENT_PROVIDER", "unknown assessment response schema")
    for key in ("request_id", "request_sha256", "basis_sha256", "campaign_id", "base_sha256"):
        need(value[key] == request[key], "ASSESSMENT_STALE", f"assessment response {key} differs")
    action = request["action"]
    need(value["action_id"] == action["action_id"] and value["action_payload_sha256"] == action["payload_sha256"],
         "ASSESSMENT_STALE", "assessment response action differs")
    need(isinstance(value["values"], dict) and set(value["values"]) == set(request["required_fields"]),
         "ASSESSMENT_PROVIDER", "assessment response fields differ from active policy")
    for key, item in value["values"].items():
        identity(key)
        need(item is None or type(item) in {str, bool, int, float}, "ASSESSMENT_PROVIDER", f"invalid assessment scalar {key}")
        need(not isinstance(item, float) or math.isfinite(item), "ASSESSMENT_PROVIDER", f"nonfinite assessment scalar {key}")
    return copy.deepcopy(value)


class JsonProcessAssessmentAdapter:
    """Durable provider cache keyed only by exact relevant assessment basis."""

    def __init__(self, transport: ProcessTransport, *, argv: Sequence[str], cwd: str | Path,
                 observation_paths: Sequence[str | Path] = ()):
        need(isinstance(transport, ProcessTransport), "ASSESSMENT_PROVIDER", "assessment adapter requires ProcessTransport")
        need(sum(item == "{packet_file}" for item in argv) == 1, "ARGV", "assessment argv needs one packet placeholder")
        self.transport = transport; self.argv = tuple(argv); self.cwd = str(cwd)
        self.observation_paths = tuple(str(Path(item).resolve()) for item in observation_paths)
        self._lock = threading.RLock()
        self._requests: dict[str, dict[str, Any]] = {}
        self._results: dict[str, dict[str, Any]] = {}

    def assess(self, state: dict[str, Any], action: dict[str, Any],
               command_context: dict[str, Any]) -> dict[str, Any] | None:
        request = build_assessment_request(
            state, action, command_context, observation_paths=self.observation_paths,
        )
        drains = active_job_ids(state)
        if not request["required_fields"]:
            return {"values": {}, "drain_targets": drains}
        job_id = request["request_id"]
        self.transport.submit(job_id, argv=self.argv, cwd=self.cwd,
                              packet=packed(request).decode("ascii"), environment={})
        with self._lock:
            self._requests[job_id] = copy.deepcopy(request)
        result = self.transport.collect(job_id)
        if not result["ready"]:
            return None
        if result["state"] != "succeeded":
            supplied = {"values": {field: None for field in request["required_fields"]},
                        "drain_targets": drains}
            with self._lock:
                self._results[job_id] = {"outcome": "provider_failure", **copy.deepcopy(supplied),
                                         "receipt_sha256": sha(packed(result))}
            return supplied
        stdout = result["stdout"]
        try:
            raw = Path(stdout["path"]).read_bytes()
            need(len(raw) == stdout["bytes"] and sha(raw) == stdout["sha256"],
                 "ASSESSMENT_PROVIDER", "assessment output artifact differs")
            value = parse(raw)
        except (OSError, ValueError) as exc:
            raise Refusal("ASSESSMENT_PROVIDER", "assessment output is unavailable or invalid") from exc
        response = validate_assessment_response(request, value)
        supplied = {"values": response["values"], "drain_targets": drains}
        with self._lock:
            self._results[job_id] = {"outcome": "observed", **copy.deepcopy(supplied),
                                     "response_sha256": sha(packed(response)),
                                     "receipt_sha256": sha(packed(result))}
        return supplied

    def status(self, request_id: str | None = None) -> dict[str, Any]:
        """Return non-secret request/transport/value provenance for a viewer."""
        with self._lock:
            ids = [request_id] if request_id is not None else sorted(self._requests)
            rows = []
            for key in ids:
                request = self._requests.get(key)
                need(request is not None, "REFERENCE", "unknown assessment request")
                receipt = self.transport.reconcile(key)
                observations = [{item_key: item[item_key] for item_key in ("sha256", "bytes", "content_complete", "status", "error")}
                                for item in request["trusted_observations"]]
                rows.append({
                    "request_id": key, "request_sha256": request["request_sha256"],
                    "basis_sha256": request["basis_sha256"],
                    "campaign_id": request["campaign_id"], "base_sha256": request["base_sha256"],
                    "action": copy.deepcopy(request["action"]),
                    "command": {item_key: request["command"][item_key]
                                for item_key in ("event_id", "kind", "logical_sha256", "payload_sha256")},
                    "policy": copy.deepcopy(request["policy"]),
                    "required_fields": list(request["required_fields"]),
                    "source_bindings": copy.deepcopy(request["relevant_context"]["sources"]),
                    "trusted_observations": observations,
                    "transport": {"state": receipt.get("state"),
                                  "result_available": receipt.get("result_available"),
                                  "receipt_sha256": sha(packed(receipt))},
                    "result": copy.deepcopy(self._results.get(key)),
                })
            return {"schema": "zap-runtime/assessment-status/1", "requests": rows}


__all__ = (
    "ASSESSMENT_REQUEST_SCHEMA", "ASSESSMENT_RESPONSE_SCHEMA",
    "JsonProcessAssessmentAdapter", "build_assessment_request",
    "validate_assessment_response",
)
