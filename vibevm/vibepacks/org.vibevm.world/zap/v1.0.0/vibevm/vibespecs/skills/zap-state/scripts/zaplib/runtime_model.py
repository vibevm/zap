"""Persistent pure reducers for coordinator jobs, attempts, waits, and reviews."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any

from .common import exact, identity, need, packed, sha, string, strings
from .domain import DOMAIN_HANDLERS, domain_state
from .records import CORE_HANDLERS, HandlerSpec
from .sources import compare_source_captures

RUNTIME_SCHEMA = "zap-runtime/1"
ACTIVE_JOB_STATES = {"claimed", "dispatched", "starting", "running", "stop_requested", "stopping", "unknown_effect"}
TERMINAL_TRANSPORT_STATES = {"succeeded", "failed", "stopped", "interrupted"}
WAIT_CLASSIFICATIONS = {"provider_quota", "provider_auth", "provider_unavailable", "rate_limit", "transport_spawn_error", "configuration_error", "model_response_invalid"}


def _empty() -> dict[str, Any]:
    return {"schema": RUNTIME_SCHEMA, "revision": 0, "jobs": {}, "attempts": {}, "reservations": {},
            "verification_jobs": {}, "review_requests": {}, "semantic_requests": {}, "resource_waits": {},
            "prepared_claims": {}, "observations": {}, "closure_status": {"status": "open", "reason": "not_evaluated"}}


def _runtime(state: dict[str, Any]) -> dict[str, Any]:
    runtime = state.setdefault("extensions", {}).setdefault("runtime", _empty())
    need(runtime.get("schema") == RUNTIME_SCHEMA, "RUNTIME_STATE", "unsupported runtime projection")
    for key, value in _empty().items():
        runtime.setdefault(key, copy.deepcopy(value))
    return runtime


def runtime_state(state: dict[str, Any]) -> dict[str, Any]:
    existing = state.get("extensions", {}).get("runtime")
    if existing is None:
        return _empty()
    need(existing.get("schema") == RUNTIME_SCHEMA, "RUNTIME_STATE", "unsupported runtime projection")
    return copy.deepcopy(existing)


def runtime_frontier(state: dict[str, Any]) -> list[str]:
    """Return domain frontier work with no unresolved runtime attempt."""
    from .domain import domain_frontier
    runtime = runtime_state(state)
    occupied = {row["work_id"] for row in runtime["jobs"].values() if row["state"] not in {"retry_released", "accepted"}}
    return [work_id for work_id in domain_frontier(state) if work_id not in occupied]


def _strict(schema: str, required: set[str]):
    def validate(value: Any) -> dict[str, Any]:
        exact(value, {"schema", *required})
        need(value["schema"] == schema, "RUNTIME_SCHEMA", f"expected {schema}")
        return copy.deepcopy(value)
    return validate


def _digest(value: Any, label: str) -> str:
    need(isinstance(value, str) and len(value) == 64 and set(value) <= set("0123456789abcdef"), "RUNTIME_VALUE", f"invalid {label}")
    return value


def _identities(value: Any, label: str) -> list[str]:
    values = strings(value, label)
    for item in values:
        identity(item)
    need(len(values) == len(set(values)), "DUPLICATE", f"duplicate {label}")
    return values


def _unique_strings(value: Any, label: str) -> list[str]:
    values = strings(value, label)
    need(len(values) == len(set(values)), "DUPLICATE", f"duplicate {label}")
    return values


def _contract_row(state: dict[str, Any], work_id: str, version: int) -> dict[str, Any]:
    history = domain_state(state)["task_contracts"].get(work_id)
    need(history is not None and history["active_version"] == version, "RUNTIME_STALE", "task contract version differs")
    row = next((item for item in history["versions"] if item["version"] == version), None)
    need(row is not None and row["schema"] == "zap-task-contract/1", "RUNTIME_CONTRACT", "current ZAP task contract is required")
    return row


def _reservation(value: Any) -> dict[str, Any]:
    exact(value, {"read_subjects", "write_subjects", "resources", "integration_owner", "branch_id", "resource_capacities", "review_capacity", "integration_capacity"})
    result = {"read_subjects": _unique_strings(value["read_subjects"], "read subjects"),
              "write_subjects": _unique_strings(value["write_subjects"], "write subjects"),
              "resources": _unique_strings(value["resources"], "resources"),
              "integration_owner": identity(value["integration_owner"]),
              "branch_id": None if value["branch_id"] is None else identity(value["branch_id"])}
    need(isinstance(value["resource_capacities"], dict), "RUNTIME_VALUE", "resource capacities must be a mapping")
    capacities = {}
    for key, capacity in value["resource_capacities"].items():
        string(key, "resource capacity key"); need(type(capacity) is int and capacity > 0, "RUNTIME_VALUE", "resource capacity must be positive")
        capacities[key] = capacity
    for key in result["resources"]:
        capacities.setdefault(key, 1)
    need(type(value["review_capacity"]) is int and value["review_capacity"] > 0, "RUNTIME_VALUE", "review capacity must be positive")
    need(type(value["integration_capacity"]) is int and value["integration_capacity"] > 0, "RUNTIME_VALUE", "integration capacity must be positive")
    return {**result, "resource_capacities": capacities, "review_capacity": value["review_capacity"], "integration_capacity": value["integration_capacity"]}


def _claim_payload(value: Any) -> dict[str, Any]:
    value = _strict("zap-runtime/job-claimed/1", {"job_id", "attempt_id", "work_id", "contract_version", "contract_sha256", "packet", "packet_sha256", "reservation", "source_captures", "semantic_request_id", "semantic_response_sha256"})(value)
    for key in ("job_id", "attempt_id", "work_id"):
        identity(value[key])
    need(type(value["contract_version"]) is int and value["contract_version"] >= 0, "RUNTIME_VALUE", "invalid contract version")
    string(value["packet"], "worker packet"); need(sha(value["packet"].encode("utf-8")) == value["packet_sha256"], "RUNTIME_STALE", "worker packet hash differs")
    _digest(value["contract_sha256"], "contract hash"); _digest(value["packet_sha256"], "packet hash")
    identity(value["semantic_request_id"]); _digest(value["semantic_response_sha256"], "semantic response hash")
    value["reservation"] = _reservation(value["reservation"])
    need(isinstance(value["source_captures"], list), "RUNTIME_VALUE", "source captures must be a list")
    return value


def _conflicts(runtime: dict[str, Any], reservation: dict[str, Any]) -> None:
    active = [runtime["reservations"][job_id] for job_id, job in runtime["jobs"].items()
              if job["state"] in ACTIVE_JOB_STATES and job_id in runtime["reservations"]]
    reads, writes = set(reservation["read_subjects"]), set(reservation["write_subjects"])
    for other in active:
        other_reads, other_writes = set(other["read_subjects"]), set(other["write_subjects"])
        need(not (writes & (other_reads | other_writes) or other_writes & reads), "RUNTIME_CONFLICT", "read/write subject reservation conflicts")
    for resource in reservation["resources"]:
        peers = [row for row in active if resource in row["resources"]]
        for row in peers:
            need(row["resource_capacities"].get(resource, 1) == reservation["resource_capacities"][resource], "RUNTIME_CONFLICT", "resource capacity policy differs")
        need(len(peers) < reservation["resource_capacities"][resource], "RUNTIME_CONFLICT", f"resource capacity exhausted: {resource}")
    owner_peers = sum(row["integration_owner"] == reservation["integration_owner"] for row in active)
    need(owner_peers < reservation["integration_capacity"], "RUNTIME_CONFLICT", "integration owner capacity exhausted")
    pending_review = sum(job["state"] in {"result_ready", "candidate", "verification", "review_pending"} for job in runtime["jobs"].values())
    need(pending_review < reservation["review_capacity"], "RUNTIME_CONFLICT", "review capacity exhausted")


def _job_claimed(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state)
    need(payload["job_id"] not in runtime["jobs"] and payload["attempt_id"] not in runtime["attempts"], "DUPLICATE", "job or attempt identity exists")
    row = _contract_row(state, payload["work_id"], payload["contract_version"])
    need(row["sha256"] == payload["contract_sha256"], "RUNTIME_STALE", "task contract hash differs")
    contract = row["contract"]
    reservation = payload["reservation"]
    for field in ("read_subjects", "write_subjects", "resources"):
        need(reservation[field] == contract[field], "RUNTIME_CONTRACT", f"reservation {field} differs from contract")
    need(reservation["integration_owner"] == contract["integration_owner"], "RUNTIME_CONTRACT", "integration owner differs from contract")
    captures = compare_source_captures(state, payload["source_captures"])
    need(captures["status"] == "current", "RUNTIME_STALE", "dispatch source captures are stale or unknown")
    _conflicts(runtime, reservation)
    DOMAIN_HANDLERS["domain.work-dispatched"].apply(state, {"schema": "zap-domain/work-dispatched/1", "work_id": payload["work_id"], "from_state": "ready", "job_id": payload["job_id"]}, event_id)
    runtime["jobs"][payload["job_id"]] = {**copy.deepcopy(payload), "state": "claimed", "claim_event_id": event_id,
                                               "transport": None, "result": None, "stop": None, "review_request_id": None}
    runtime["attempts"][payload["attempt_id"]] = {"attempt_id": payload["attempt_id"], "job_id": payload["job_id"], "work_id": payload["work_id"],
                                                     "state": "claimed", "classification": "work", "history": []}
    runtime["reservations"][payload["job_id"]] = {**reservation, "active": True}


def _claim_prepared(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); request = runtime["semantic_requests"].get(payload["request_id"])
    need(request is not None and request["state"] in {"requested", "submitted"} and request["request_kind"] == "selection",
         "RUNTIME_STALE", "selection request is not pending for claim preparation")
    _digest(payload["response_sha256"], "prepared response hash"); claim = _claim_payload(payload["claim"])
    need(claim["semantic_request_id"] == payload["request_id"] and claim["semantic_response_sha256"] == payload["response_sha256"],
         "RUNTIME_STALE", "prepared claim semantic binding differs")
    need(payload["request_id"] not in runtime["prepared_claims"] and claim["job_id"] not in runtime["jobs"],
         "DUPLICATE", "selection claim was already prepared or committed")
    runtime["prepared_claims"][payload["request_id"]] = {"claim": claim, "response_sha256": payload["response_sha256"], "event_id": event_id}


def _transport_payload(schema: str, key: str):
    def validate(value: Any) -> dict[str, Any]:
        value = _strict(schema, {key, "receipt"})(value)
        identity(value[key]); need(isinstance(value["receipt"], dict), "RUNTIME_VALUE", "transport receipt must be a mapping")
        need(value["receipt"].get("job_id") == value[key], "RUNTIME_VALUE", "transport receipt job identity differs")
        return value
    return validate


def _job_submitted(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None and job["state"] == "claimed", "RUNTIME_STALE", "job is not awaiting submission")
    receipt = payload["receipt"]
    need(receipt.get("accepted") is True and isinstance(receipt.get("descriptor_sha256"), str), "RUNTIME_VALUE", "transport did not accept descriptor")
    job["transport"] = copy.deepcopy(receipt); job["descriptor_sha256"] = receipt["descriptor_sha256"]
    job["state"] = receipt.get("state") if receipt.get("state") in ACTIVE_JOB_STATES else "dispatched"
    runtime["attempts"][job["attempt_id"]]["state"] = job["state"]


def _job_status(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None and job.get("descriptor_sha256") == payload["receipt"].get("descriptor_sha256"), "RUNTIME_STALE", "status descriptor differs")
    observed = payload["receipt"].get("state")
    need(observed in ACTIVE_JOB_STATES | TERMINAL_TRANSPORT_STATES, "RUNTIME_VALUE", "unknown transport state")
    job["state"] = observed; job.setdefault("status_history", []).append({"event_id": event_id, "receipt": copy.deepcopy(payload["receipt"])})
    job["last_status_sha256"] = sha(packed(payload["receipt"]))
    job["last_status_sha256"] = sha(packed(payload["receipt"]))
    runtime["attempts"][job["attempt_id"]]["state"] = observed


def _result_payload(value: Any) -> dict[str, Any]:
    value = _strict("zap-runtime/job-result-observed/1", {"job_id", "receipt"})(value)
    identity(value["job_id"]); need(isinstance(value["receipt"], dict) and value["receipt"].get("job_id") == value["job_id"] and value["receipt"].get("ready") is True, "RUNTIME_VALUE", "result receipt is not ready")
    need(value["receipt"].get("state") in TERMINAL_TRANSPORT_STATES, "RUNTIME_VALUE", "result receipt is not terminal")
    return value


def _job_result(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None and job.get("descriptor_sha256") == payload["receipt"].get("descriptor_sha256"), "RUNTIME_STALE", "result descriptor differs")
    job["result"] = copy.deepcopy(payload["receipt"]); job["result_sha256"] = sha(packed(payload["receipt"])); job["state"] = "result_ready"
    job["result_sha256"] = sha(packed(payload["receipt"]))
    runtime["attempts"][job["attempt_id"]]["state"] = "result_ready"
    runtime["attempts"][job["attempt_id"]]["transport_classification"] = payload["receipt"].get("diagnostic", {}).get("classification")


def _candidate(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None and job["state"] == "result_ready" and job["work_id"] == payload["work_id"], "RUNTIME_STALE", "job result is not candidate-ready")
    need(job.get("result", {}).get("state") == "succeeded" and job["result"].get("diagnostic", {}).get("classification") == "success",
         "RUNTIME_VALUE", "failed, blocked, invalid or interrupted worker output cannot become a candidate")
    need(sha(packed(job["result"])) == payload["result_sha256"], "RUNTIME_STALE", "job result receipt differs")
    DOMAIN_HANDLERS["domain.work-transitioned"].apply(state, {"schema": "zap-domain/work-transitioned/1", "work_id": payload["work_id"],
        "from_state": "active", "to_state": "candidate", "successor_ids": []}, event_id)
    job["state"] = "candidate"; runtime["attempts"][job["attempt_id"]]["state"] = "candidate"
    runtime["reservations"][payload["job_id"]]["active"] = False


def _verification_claim(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    from .control import require_action
    require_action(state, "verification.run")
    runtime = _runtime(state); job = runtime["jobs"].get(payload["work_job_id"])
    need(job is not None and job["state"] == "candidate", "RUNTIME_STALE", "verification requires a candidate job")
    verification_id = identity(payload["verification_id"]); identity(payload["evidence_id"])
    need(verification_id not in runtime["verification_jobs"] and payload["evidence_id"] not in state.get("evidence", {}), "DUPLICATE", "verification or evidence identity exists")
    need(isinstance(payload["plan"], dict), "RUNTIME_VALUE", "verification plan must be a mapping")
    string(payload["packet"], "verification packet"); _digest(payload["packet_sha256"], "verification packet hash")
    need(sha(payload["packet"].encode("utf-8")) == payload["packet_sha256"], "RUNTIME_STALE", "verification packet hash differs")
    runtime["verification_jobs"][verification_id] = {**copy.deepcopy(payload), "state": "claimed", "claim_event_id": event_id, "transport": None, "result": None}
    job["state"] = "verification"


def _verification_submit(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["verification_jobs"].get(payload["verification_id"])
    need(row is not None and row["state"] == "claimed", "RUNTIME_STALE", "verification is not awaiting submission")
    receipt = payload["receipt"]; need(receipt.get("accepted") is True, "RUNTIME_VALUE", "verification transport was not accepted")
    row["transport"] = copy.deepcopy(receipt); row["descriptor_sha256"] = receipt["descriptor_sha256"]; row["state"] = receipt.get("state", "dispatched")


def _verification_result_payload(value: Any) -> dict[str, Any]:
    value = _strict("zap-runtime/verification-result-observed/1", {"verification_id", "receipt", "evidence"})(value)
    identity(value["verification_id"]); need(isinstance(value["receipt"], dict) and value["receipt"].get("ready") is True, "RUNTIME_VALUE", "verification result is not ready")
    need(isinstance(value["evidence"], dict), "RUNTIME_VALUE", "verification evidence must be a mapping")
    return value


def _verification_result(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["verification_jobs"].get(payload["verification_id"])
    need(row is not None and row.get("descriptor_sha256") == payload["receipt"].get("descriptor_sha256") and
         payload["receipt"].get("job_id") == payload["verification_id"], "RUNTIME_STALE", "verification descriptor differs")
    evidence = payload["evidence"]
    expected_result = "observed_pass" if payload["receipt"].get("state") == "succeeded" else "observed_fail"
    need(evidence.get("id") == row["evidence_id"] and evidence.get("result") == expected_result, "RUNTIME_VALUE", "verification evidence result differs from transport")
    CORE_HANDLERS["evidence.recorded"].apply(state, evidence, event_id)
    row["result"] = copy.deepcopy(payload["receipt"]); row["state"] = "observed"
    runtime["jobs"][row["work_job_id"]]["state"] = "review_pending"


def _verification_artifact(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["verification_jobs"].get(payload["verification_id"])
    need(row is not None and row["state"] == "observed" and row.get("artifact_source") is None,
         "RUNTIME_STALE", "verification is not awaiting an artifact capture")
    source = state.get("extensions", {}).get("knowledge", {}).get("sources", {}).get(payload["source_id"])
    need(source is not None and source.get("capture_status") == "current" and source.get("content_sha256") == payload["source_sha256"],
         "RUNTIME_STALE", "captured verification artifact source differs")
    need(isinstance(payload["blob"], dict) and payload["blob"].get("sha256") == payload["source_sha256"],
         "RUNTIME_VALUE", "verification artifact blob identity differs")
    row["artifact_source"] = {"source_id": payload["source_id"], "sha256": payload["source_sha256"],
                              "blob": copy.deepcopy(payload["blob"]), "event_id": event_id}


def _review_request(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); request_id = identity(payload["review_request_id"])
    need(request_id not in runtime["review_requests"], "DUPLICATE", "review request exists")
    work_ids = _identities(payload["work_ids"], "review work ids"); need(work_ids, "RUNTIME_VALUE", "review request needs work")
    _identities(payload["trigger_ids"], "review trigger ids"); string(payload["scope"], "review scope"); string(payload["summary"], "review summary")
    runtime["review_requests"][request_id] = {**copy.deepcopy(payload), "event_id": event_id, "state": "pending", "semantic_request_id": None}


def _review_reopened(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["review_requests"].get(payload["review_request_id"])
    need(row is not None and row["state"] == "awaiting_evidence" and row.get("semantic_request_id") == payload["prior_semantic_request_id"],
         "RUNTIME_STALE", "review is not awaiting new evidence for that semantic request")
    _digest(payload["prior_scope_sha256"], "prior review scope"); _digest(payload["current_scope_sha256"], "current review scope")
    need(payload["prior_scope_sha256"] != payload["current_scope_sha256"], "RUNTIME_STALE", "review scope did not change")
    row["state"] = "pending"; row["semantic_request_id"] = None; row["reopened_event_id"] = event_id


def _semantic_request(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); request_id = identity(payload["request_id"])
    need(request_id not in runtime["semantic_requests"], "DUPLICATE", "semantic request exists")
    need(payload["request_kind"] in {"selection", "review", "reassessment", "acceptance", "closure"}, "RUNTIME_VALUE", "unknown semantic request kind")
    need(type(payload["state_revision"]) is int and payload["state_revision"] == state["revision"] + 1, "RUNTIME_STALE", "semantic request revision differs")
    _digest(payload["request_sha256"], "semantic request hash"); need(isinstance(payload["request"], dict), "RUNTIME_VALUE", "semantic request must be a mapping")
    runtime["semantic_requests"][request_id] = {**copy.deepcopy(payload), "event_id": event_id, "state": "requested", "submit_receipt": None, "response": None}
    body = payload["request"].get("request", {})
    for review in body.get("review_requests", []):
        review_id = review.get("review_request_id")
        need(review_id in runtime["review_requests"] and runtime["review_requests"][review_id]["state"] == "pending", "RUNTIME_STALE", "semantic review request differs")
        runtime["review_requests"][review_id]["state"] = "in_progress"
        runtime["review_requests"][review_id]["semantic_request_id"] = request_id


def _semantic_submit(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["semantic_requests"].get(payload["request_id"])
    need(row is not None and row["state"] == "requested", "RUNTIME_STALE", "semantic request is not awaiting submission")
    need(isinstance(payload["receipt"], dict), "RUNTIME_VALUE", "semantic submit receipt must be a mapping")
    row["submit_receipt"] = copy.deepcopy(payload["receipt"]); row["state"] = "submitted"


def _semantic_result(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["semantic_requests"].get(payload["request_id"])
    need(row is not None and row["state"] in {"requested", "submitted"}, "RUNTIME_STALE", "semantic request is not pending")
    need(isinstance(payload["response"], dict), "RUNTIME_VALUE", "semantic response must be a mapping")
    _digest(payload["response_sha256"], "semantic response hash"); need(sha(packed(payload["response"])) == payload["response_sha256"], "RUNTIME_STALE", "semantic response hash differs")
    need(payload["outcome"] in {"applied", "stale", "no_action", "rejected"}, "RUNTIME_VALUE", "unknown semantic result outcome")
    row["response"] = copy.deepcopy(payload["response"]); row["state"] = payload["outcome"]; row["result_event_id"] = event_id
    body = row["request"].get("request", {})
    for review in body.get("review_requests", []):
        review_row = runtime["review_requests"].get(review.get("review_request_id"))
        if review_row and review_row.get("semantic_request_id") == payload["request_id"]:
            review_row["state"] = "pending" if payload["outcome"] == "stale" else "resolved" if payload["outcome"] == "applied" else "awaiting_evidence"


def _semantic_wait(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["semantic_requests"].get(payload["request_id"]); wait_id = identity(payload["wait_id"])
    need(row is not None and row["state"] in {"requested", "submitted"} and wait_id not in runtime["resource_waits"], "RUNTIME_STALE", "semantic wait request differs")
    need(payload["classification"] in WAIT_CLASSIFICATIONS, "RUNTIME_VALUE", "unknown semantic wait classification")
    next_retry = payload["next_retry_ns"]
    need(type(payload["observed_at_ns"]) is int and ((payload["classification"] in {"configuration_error", "model_response_invalid"} and next_retry is None)
         or (type(next_retry) is int and next_retry >= payload["observed_at_ns"])), "RUNTIME_VALUE", "invalid semantic retry time")
    need(isinstance(payload["diagnostic"], dict), "RUNTIME_VALUE", "semantic wait diagnostic must be a mapping")
    runtime["resource_waits"][wait_id] = {**copy.deepcopy(payload), "operation_kind": "semantic", "operation_id": payload["request_id"], "job_id": None,
                                                "event_id": event_id, "state": "waiting"}
    row["state"] = "waiting"; row["wait_id"] = wait_id


def _semantic_wait_release(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); wait = runtime["resource_waits"].get(payload["wait_id"]); row = runtime["semantic_requests"].get(payload["request_id"])
    need(wait is not None and wait["state"] == "waiting" and wait.get("operation_id") == payload["request_id"] and row is not None and row["state"] == "waiting", "RUNTIME_STALE", "semantic wait release differs")
    need(type(payload["observed_now_ns"]) is int and (wait["next_retry_ns"] is None or payload["observed_now_ns"] >= wait["next_retry_ns"]),
         "RUNTIME_STALE", "semantic retry boundary not reached")
    wait["state"] = "released"; wait["released_event_id"] = event_id; row["state"] = "failed"
    body = row["request"].get("request", {})
    for review in body.get("review_requests", []):
        review_row = runtime["review_requests"].get(review.get("review_request_id"))
        if review_row and review_row.get("semantic_request_id") == payload["request_id"]:
            review_row["state"] = "pending"; review_row["semantic_request_id"] = None


def _semantic_rebound(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); row = runtime["semantic_requests"].get(payload["request_id"])
    need(row is not None and row["state"] in {"requested", "submitted"}, "RUNTIME_STALE", "semantic request is not pending for rebind")
    need(payload["from_revision"] == row["state_revision"] and payload["to_revision"] == state["revision"] and payload["to_revision"] >= payload["from_revision"], "RUNTIME_STALE", "semantic rebind revision differs")
    _digest(payload["response_sha256"], "semantic response hash"); _digest(payload["basis_sha256"], "semantic rebind basis hash")
    row.setdefault("rebindings", []).append({**copy.deepcopy(payload), "event_id": event_id})


def _native_facts_observed(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); identity(payload["source_id"]); _digest(payload["previous_sha256"], "previous native source hash")
    need(isinstance(payload["capture"], dict) and payload["capture"].get("source", {}).get("id") == payload["source_id"], "RUNTIME_VALUE", "native fact capture binding differs")
    runtime["observations"][event_id] = {"kind": "native_facts_refresh", **copy.deepcopy(payload), "event_id": event_id,
                                          "adjudication": "unassessed"}


def _job_reset(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None and job["work_id"] == payload["work_id"] and job.get("result", {}).get("state") in {"stopped", "interrupted"}, "RUNTIME_STALE", "only a stopped/interrupted job can be reset")
    string(payload["reason"], "job reset reason")
    DOMAIN_HANDLERS["domain.work-transitioned"].apply(state, {"schema": "zap-domain/work-transitioned/1", "work_id": payload["work_id"], "from_state": "active", "to_state": "blocked", "successor_ids": []}, event_id)
    DOMAIN_HANDLERS["domain.work-transitioned"].apply(state, {"schema": "zap-domain/work-transitioned/1", "work_id": payload["work_id"], "from_state": "blocked", "to_state": "ready", "successor_ids": []}, event_id)
    job["state"] = "retry_released"; runtime["reservations"][payload["job_id"]]["active"] = False


def _acceptance_observed(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"]); domain = domain_state(state)
    acceptance = domain["acceptances"].get(payload["acceptance_id"])
    need(job is not None and acceptance is not None and acceptance["work_id"] == job["work_id"], "RUNTIME_STALE", "work acceptance receipt differs")
    need(type(payload["domain_revision"]) is int and payload["domain_revision"] == domain["revision"], "RUNTIME_STALE", "acceptance domain revision differs")
    job["state"] = "accepted"; job["acceptance_id"] = payload["acceptance_id"]; job["acceptance_event_id"] = event_id


def _wait(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); wait_id = identity(payload["wait_id"]); job = runtime["jobs"].get(payload["job_id"])
    need(wait_id not in runtime["resource_waits"] and job is not None, "RUNTIME_STALE", "wait identity or job differs")
    string(payload["capability"], "wait capability"); need(payload["classification"] in WAIT_CLASSIFICATIONS, "RUNTIME_VALUE", "unknown resource wait classification")
    need(type(payload["observed_at_ns"]) is int and payload["observed_at_ns"] >= 0, "RUNTIME_VALUE", "invalid observation time")
    need(payload["next_retry_ns"] is None or type(payload["next_retry_ns"]) is int and payload["next_retry_ns"] >= payload["observed_at_ns"], "RUNTIME_VALUE", "invalid retry time")
    _identities(payload["affected_operations"], "affected operations")
    runtime["resource_waits"][wait_id] = {**copy.deepcopy(payload), "event_id": event_id, "state": "waiting"}
    job["state"] = "waiting"; runtime["attempts"][job["attempt_id"]]["state"] = "waiting"
    runtime["attempts"][job["attempt_id"]]["classification"] = payload["classification"]
    runtime["reservations"][payload["job_id"]]["active"] = False


def _retry_release(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); wait = runtime["resource_waits"].get(payload["wait_id"]); job = runtime["jobs"].get(payload["job_id"])
    need(wait is not None and wait["state"] == "waiting" and job is not None and job["work_id"] == payload["work_id"], "RUNTIME_STALE", "retry wait/job differs")
    need(type(payload["observed_now_ns"]) is int and (wait["next_retry_ns"] is None or payload["observed_now_ns"] >= wait["next_retry_ns"]), "RUNTIME_STALE", "retry boundary not reached")
    DOMAIN_HANDLERS["domain.work-transitioned"].apply(state, {"schema": "zap-domain/work-transitioned/1", "work_id": payload["work_id"], "from_state": "active", "to_state": "blocked", "successor_ids": []}, event_id)
    DOMAIN_HANDLERS["domain.work-transitioned"].apply(state, {"schema": "zap-domain/work-transitioned/1", "work_id": payload["work_id"], "from_state": "blocked", "to_state": "ready", "successor_ids": []}, event_id)
    wait["state"] = "released"; wait["released_event_id"] = event_id; job["state"] = "retry_released"


def _stop_observed(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
    runtime = _runtime(state); job = runtime["jobs"].get(payload["job_id"])
    need(job is not None, "REFERENCE", "stop job missing"); identity(payload["pause_id"]); identity(payload["request_id"])
    need(isinstance(payload["receipt"], dict), "RUNTIME_VALUE", "stop receipt must be a mapping")
    job["stop"] = {"pause_id": payload["pause_id"], "request_id": payload["request_id"], "receipt": copy.deepcopy(payload["receipt"]), "event_id": event_id}
    observed = payload["receipt"].get("state")
    if job.get("result") is not None and job["state"] not in {"candidate", "verification", "review_pending", "accepted"}:
        job["state"] = "result_ready"
    elif observed in ACTIVE_JOB_STATES | TERMINAL_TRANSPORT_STATES:
        job["state"] = observed


def _handler(kind: str, validator, apply):
    def wrapped(state, payload, event_id):
        runtime = _runtime(state); apply(state, payload, event_id); runtime["revision"] += 1
    return HandlerSpec(kind, validator, wrapped)


RUNTIME_HANDLERS = MappingProxyType({spec.kind: spec for spec in (
    _handler("runtime.job-claim-prepared", _strict("zap-runtime/job-claim-prepared/1", {"request_id", "response_sha256", "claim"}), _claim_prepared),
    _handler("runtime.job-claimed", _claim_payload, _job_claimed),
    _handler("runtime.job-submitted", _transport_payload("zap-runtime/job-submitted/1", "job_id"), _job_submitted),
    _handler("runtime.job-status-observed", _transport_payload("zap-runtime/job-status-observed/1", "job_id"), _job_status),
    _handler("runtime.job-result-observed", _result_payload, _job_result),
    _handler("runtime.job-candidate-recorded", _strict("zap-runtime/job-candidate-recorded/1", {"job_id", "work_id", "result_sha256"}), _candidate),
    _handler("runtime.verification-claimed", _strict("zap-runtime/verification-claimed/1", {"verification_id", "work_job_id", "evidence_id", "plan", "packet", "packet_sha256"}), _verification_claim),
    _handler("runtime.verification-submitted", _transport_payload("zap-runtime/verification-submitted/1", "verification_id"), _verification_submit),
    _handler("runtime.verification-result-observed", _verification_result_payload, _verification_result),
    _handler("runtime.verification-artifact-captured", _strict("zap-runtime/verification-artifact-captured/1", {"verification_id", "source_id", "source_sha256", "blob"}), _verification_artifact),
    _handler("runtime.review-requested", _strict("zap-runtime/review-requested/1", {"review_request_id", "trigger_ids", "work_ids", "scope", "summary"}), _review_request),
    _handler("runtime.review-reopened", _strict("zap-runtime/review-reopened/1", {"review_request_id", "prior_semantic_request_id", "prior_scope_sha256", "current_scope_sha256"}), _review_reopened),
    _handler("runtime.semantic-requested", _strict("zap-runtime/semantic-requested/1", {"request_id", "request_kind", "state_revision", "request_sha256", "request"}), _semantic_request),
    _handler("runtime.semantic-submitted", _strict("zap-runtime/semantic-submitted/1", {"request_id", "receipt"}), _semantic_submit),
    _handler("runtime.semantic-result-recorded", _strict("zap-runtime/semantic-result-recorded/1", {"request_id", "response", "response_sha256", "outcome"}), _semantic_result),
    _handler("runtime.semantic-wait-recorded", _strict("zap-runtime/semantic-wait-recorded/1", {"wait_id", "request_id", "classification", "observed_at_ns", "next_retry_ns", "diagnostic"}), _semantic_wait),
    _handler("runtime.semantic-wait-released", _strict("zap-runtime/semantic-wait-released/1", {"wait_id", "request_id", "observed_now_ns"}), _semantic_wait_release),
    _handler("runtime.semantic-rebound", _strict("zap-runtime/semantic-rebound/1", {"request_id", "response_sha256", "from_revision", "to_revision", "basis_sha256"}), _semantic_rebound),
    _handler("runtime.native-facts-observed", _strict("zap-runtime/native-facts-observed/1", {"source_id", "previous_sha256", "capture"}), _native_facts_observed),
    _handler("runtime.resource-wait-recorded", _strict("zap-runtime/resource-wait-recorded/1", {"wait_id", "job_id", "capability", "classification", "observed_at_ns", "affected_operations", "next_retry_ns"}), _wait),
    _handler("runtime.retry-released", _strict("zap-runtime/retry-released/1", {"wait_id", "job_id", "work_id", "observed_now_ns"}), _retry_release),
    _handler("runtime.job-reset-ready", _strict("zap-runtime/job-reset-ready/1", {"job_id", "work_id", "reason"}), _job_reset),
    _handler("runtime.job-acceptance-observed", _strict("zap-runtime/job-acceptance-observed/1", {"job_id", "acceptance_id", "domain_revision"}), _acceptance_observed),
    _handler("runtime.stop-observed", _strict("zap-runtime/stop-observed/1", {"job_id", "pause_id", "request_id", "receipt"}), _stop_observed),
)})

_RUNTIME_EVENT_FIELDS = {
    "runtime.job-claim-prepared": {"schema": "zap-runtime/job-claim-prepared/1", "required": ["request_id", "response_sha256", "claim"]},
    "runtime.job-claimed": {"schema": "zap-runtime/job-claimed/1", "required": ["job_id", "attempt_id", "work_id", "contract_version", "contract_sha256", "packet", "packet_sha256", "reservation", "source_captures", "semantic_request_id", "semantic_response_sha256"]},
    "runtime.job-submitted": {"schema": "zap-runtime/job-submitted/1", "required": ["job_id", "receipt"]},
    "runtime.job-status-observed": {"schema": "zap-runtime/job-status-observed/1", "required": ["job_id", "receipt"]},
    "runtime.job-result-observed": {"schema": "zap-runtime/job-result-observed/1", "required": ["job_id", "receipt"]},
    "runtime.job-candidate-recorded": {"schema": "zap-runtime/job-candidate-recorded/1", "required": ["job_id", "work_id", "result_sha256"]},
    "runtime.verification-claimed": {"schema": "zap-runtime/verification-claimed/1", "required": ["verification_id", "work_job_id", "evidence_id", "plan", "packet", "packet_sha256"]},
    "runtime.verification-submitted": {"schema": "zap-runtime/verification-submitted/1", "required": ["verification_id", "receipt"]},
    "runtime.verification-result-observed": {"schema": "zap-runtime/verification-result-observed/1", "required": ["verification_id", "receipt", "evidence"]},
    "runtime.verification-artifact-captured": {"schema": "zap-runtime/verification-artifact-captured/1", "required": ["verification_id", "source_id", "source_sha256", "blob"]},
    "runtime.review-requested": {"schema": "zap-runtime/review-requested/1", "required": ["review_request_id", "trigger_ids", "work_ids", "scope", "summary"]},
    "runtime.review-reopened": {"schema": "zap-runtime/review-reopened/1", "required": ["review_request_id", "prior_semantic_request_id", "prior_scope_sha256", "current_scope_sha256"]},
    "runtime.semantic-requested": {"schema": "zap-runtime/semantic-requested/1", "required": ["request_id", "request_kind", "state_revision", "request_sha256", "request"]},
    "runtime.semantic-submitted": {"schema": "zap-runtime/semantic-submitted/1", "required": ["request_id", "receipt"]},
    "runtime.semantic-result-recorded": {"schema": "zap-runtime/semantic-result-recorded/1", "required": ["request_id", "response", "response_sha256", "outcome"]},
    "runtime.semantic-wait-recorded": {"schema": "zap-runtime/semantic-wait-recorded/1", "required": ["wait_id", "request_id", "classification", "observed_at_ns", "next_retry_ns", "diagnostic"]},
    "runtime.semantic-wait-released": {"schema": "zap-runtime/semantic-wait-released/1", "required": ["wait_id", "request_id", "observed_now_ns"]},
    "runtime.semantic-rebound": {"schema": "zap-runtime/semantic-rebound/1", "required": ["request_id", "response_sha256", "from_revision", "to_revision", "basis_sha256"]},
    "runtime.native-facts-observed": {"schema": "zap-runtime/native-facts-observed/1", "required": ["source_id", "previous_sha256", "capture"]},
    "runtime.resource-wait-recorded": {"schema": "zap-runtime/resource-wait-recorded/1", "required": ["wait_id", "job_id", "capability", "classification", "observed_at_ns", "affected_operations", "next_retry_ns"]},
    "runtime.retry-released": {"schema": "zap-runtime/retry-released/1", "required": ["wait_id", "job_id", "work_id", "observed_now_ns"]},
    "runtime.job-reset-ready": {"schema": "zap-runtime/job-reset-ready/1", "required": ["job_id", "work_id", "reason"]},
    "runtime.job-acceptance-observed": {"schema": "zap-runtime/job-acceptance-observed/1", "required": ["job_id", "acceptance_id", "domain_revision"]},
    "runtime.stop-observed": {"schema": "zap-runtime/stop-observed/1", "required": ["job_id", "pause_id", "request_id", "receipt"]},
}
RUNTIME_EVENT_SCHEMAS = MappingProxyType({kind: {**row, "required": ["schema", *row["required"]], "additional_properties": False}
                                          for kind, row in _RUNTIME_EVENT_FIELDS.items()})
if set(RUNTIME_EVENT_SCHEMAS) != set(RUNTIME_HANDLERS):
    raise RuntimeError("runtime event schema/handler mismatch")
RUNTIME_DATA_KINDS = frozenset()
RUNTIME_ACTION_KINDS = MappingProxyType({"runtime.job-claimed": "work.dispatch", "runtime.job-candidate-recorded": "plan.lower",
                                         "runtime.verification-claimed": "verification.run", "runtime.retry-released": "plan.lower",
                                         "runtime.job-reset-ready": "plan.lower"})
RUNTIME_OBSERVATION_KINDS = frozenset(set(RUNTIME_HANDLERS) - RUNTIME_DATA_KINDS - set(RUNTIME_ACTION_KINDS))
RUNTIME_EVENT_ROUTES = MappingProxyType({
    kind: ({"route": "data", "action": None} if kind in RUNTIME_DATA_KINDS else
           {"route": "action", "action": RUNTIME_ACTION_KINDS[kind]} if kind in RUNTIME_ACTION_KINDS else
           {"route": "trusted_observation", "action": None})
    for kind in RUNTIME_HANDLERS
})
RUNTIME_CAPABILITIES = MappingProxyType({"schema": RUNTIME_SCHEMA, "version": 1, "events": tuple(sorted(RUNTIME_HANDLERS)),
                                         "data_kinds": tuple(sorted(RUNTIME_DATA_KINDS)), "action_kinds": dict(RUNTIME_ACTION_KINDS),
                                         "observation_kinds": tuple(sorted(RUNTIME_OBSERVATION_KINDS))})
