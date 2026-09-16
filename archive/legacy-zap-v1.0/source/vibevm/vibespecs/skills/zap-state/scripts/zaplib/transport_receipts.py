"""Exact descriptor-bound validation for private subprocess receipts."""
from __future__ import annotations

from pathlib import Path
from typing import Any

from .common import Refusal, identity, need
from .transport_io import artifact, atomic_json, atomic_write, now_ns, read_json, verify_artifact


def owned(record: Any, descriptor: dict[str, Any], schema: str) -> bool:
    return (
        isinstance(record, dict)
        and record.get("schema") == schema
        and record.get("job_id") == descriptor["job_id"]
        and record.get("nonce") == descriptor["nonce"]
        and record.get("descriptor_sha256") == descriptor["descriptor_sha256"]
    )


def base_record(descriptor: dict[str, Any], schema: str, **extra: Any) -> dict[str, Any]:
    return {
        "schema": schema, "job_id": descriptor["job_id"], "nonce": descriptor["nonce"],
        "descriptor_sha256": descriptor["descriptor_sha256"], **extra,
    }


def complete_spawn_failure(job_dir: Path, descriptor: dict[str, Any]) -> None:
    stdout, stderr = job_dir / "stdout.bin", job_dir / "stderr.bin"
    atomic_write(stdout, b""); atomic_write(stderr, b"")
    atomic_json(job_dir / "completed.json", base_record(
        descriptor, "zap-process-completed/1", state="failed", exit_code=None,
        exit_observed_at_ns=None, completed_at_ns=now_ns(), stdout=artifact(stdout),
        stderr=artifact(stderr), diagnostic={"classification": "transport_spawn_error"},
        termination_sent=False, delivered_stop_request_ids=[], safe_state_verified=False,
        effects_reconciled=False,
    ))


def prepare_known_pre_effect(job_dir: Path, descriptor: dict[str, Any]) -> bool:
    allowed = {"descriptor.json", "packet.txt", "prepared.json"}
    if {path.name for path in job_dir.iterdir()} - allowed:
        return False
    prepared = job_dir / "prepared.json"
    if prepared.exists():
        need(owned(read_json(prepared), descriptor, "zap-process-prepared/1"),
             "TRANSPORT_CORRUPT", "invalid prepared receipt")
    else:
        atomic_json(prepared, base_record(descriptor, "zap-process-prepared/1", prepared_at_ns=now_ns()))
    atomic_json(job_dir / "launch_intent.json", base_record(
        descriptor, "zap-process-launch-intent/1", intended_at_ns=now_ns(),
    ))
    return True


def stop_policy(mode: Any, terminate_after_seconds: Any) -> tuple[str, float | None]:
    need(mode in {"cooperative", "terminate"}, "STOP", "stop mode must be cooperative or terminate")
    if mode == "cooperative":
        need(terminate_after_seconds is None, "STOP", "cooperative stop has no termination deadline")
        return mode, None
    need(type(terminate_after_seconds) in {int, float} and terminate_after_seconds > 0,
         "STOP", "terminate mode requires a positive delay")
    return mode, float(terminate_after_seconds)


def request_policy(record: dict[str, Any]) -> tuple[str, float | None] | None:
    try:
        return stop_policy(record.get("mode"), record.get("terminate_after_seconds"))
    except Refusal:
        return None


def valid_stop_request(value: Any, descriptor: dict[str, Any]) -> bool:
    expected = {
        "schema", "job_id", "nonce", "descriptor_sha256", "request_id", "mode",
        "terminate_after_seconds", "requested_at_ns",
    }
    if not (isinstance(value, dict) and set(value) == expected
            and owned(value, descriptor, "zap-process-stop-request/1")
            and isinstance(value.get("request_id"), str)
            and type(value.get("requested_at_ns")) is int):
        return False
    try:
        identity(value["request_id"])
    except Refusal:
        return False
    return request_policy(value) is not None


def stop_requests(job_dir: Path, descriptor: dict[str, Any]) -> list[dict[str, Any]]:
    request_dir = job_dir / "stop_requests"
    if not request_dir.is_dir():
        return []
    result = []
    for path in sorted(request_dir.glob("*.json")):
        try:
            value = read_json(path)
            if valid_stop_request(value, descriptor):
                result.append(value)
        except Refusal:
            continue
    return result


def _valid_delivery(value: Any, descriptor: dict[str, Any], request: dict[str, Any]) -> bool:
    expected = {
        "schema", "job_id", "nonce", "descriptor_sha256", "request_id",
        "delivered", "mode", "method", "delivered_at_ns",
    }
    return (
        isinstance(value, dict) and set(value) == expected
        and owned(value, descriptor, "zap-process-stop-delivery/1")
        and value.get("request_id") == request["request_id"]
        and value.get("delivered") is True
        and value.get("mode") == request["mode"]
        and value.get("method") == "cooperative_stop_file"
        and type(value.get("delivered_at_ns")) is int
    )


def _valid_termination(value: Any, descriptor: dict[str, Any], request_ids: set[str]) -> bool:
    expected = {
        "schema", "job_id", "nonce", "descriptor_sha256", "pid", "process_token",
        "request_id", "mode", "ownership_verified", "signal_sent", "sent_at_ns",
    }
    return (
        isinstance(value, dict) and set(value) == expected
        and owned(value, descriptor, "zap-process-termination/1")
        and value.get("signal_sent") is True and value.get("mode") == "terminate"
        and value.get("ownership_verified") is True
        and type(value.get("pid")) is int and isinstance(value.get("process_token"), str)
        and type(value.get("sent_at_ns")) is int and value.get("request_id") in request_ids
    )


def stop_summary(job_dir: Path, descriptor: dict[str, Any], request_key) -> dict[str, Any]:
    requests = stop_requests(job_dir, descriptor)
    delivered_ids = []
    delivery_dir = job_dir / "stop_deliveries"
    for request in requests:
        path = delivery_dir / f"{request_key(request['request_id'])}.json"
        if path.exists():
            try:
                if _valid_delivery(read_json(path), descriptor, request):
                    delivered_ids.append(request["request_id"])
            except Refusal:
                pass
    termination_sent = False
    termination = job_dir / "termination.json"
    if termination.exists():
        try:
            termination_sent = _valid_termination(
                read_json(termination), descriptor, {row["request_id"] for row in requests},
            )
        except Refusal:
            pass
    return {
        "requested": bool(requests), "request_count": len(requests),
        "delivered": bool(requests) and len(delivered_ids) == len(requests),
        "delivery_count": len(delivered_ids), "termination_sent": termination_sent,
        "request_ids": [row["request_id"] for row in requests],
        "delivered_request_ids": delivered_ids,
        "termination_requested": any(row["mode"] == "terminate" for row in requests),
    }


def started_record(job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any] | None:
    path = job_dir / "started.json"
    if not path.exists():
        return None
    value = read_json(path)
    expected = {"schema", "job_id", "nonce", "descriptor_sha256", "pid", "process_token", "started_at_ns"}
    if not (isinstance(value, dict) and set(value) == expected
            and owned(value, descriptor, "zap-process-started/1")
            and type(value.get("pid")) is int and value["pid"] > 0
            and (isinstance(value.get("process_token"), str) or value.get("process_token") is None)
            and type(value.get("started_at_ns")) is int):
        return None
    return value


def host_started_record(job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any] | None:
    path = job_dir / "host_started.json"
    if not path.exists():
        return None
    value = read_json(path)
    expected = {
        "schema", "job_id", "nonce", "descriptor_sha256", "host_pid",
        "host_process_token", "started_at_ns",
    }
    if not (isinstance(value, dict) and set(value) == expected
            and owned(value, descriptor, "zap-process-host/1")
            and type(value.get("host_pid")) is int and value["host_pid"] > 0
            and (isinstance(value.get("host_process_token"), str) or value.get("host_process_token") is None)
            and type(value.get("started_at_ns")) is int):
        return None
    return value


def completed_record(
    job_dir: Path, descriptor: dict[str, Any], root: Path, terminal_states: set[str], request_key,
) -> dict[str, Any] | None:
    path = job_dir / "completed.json"
    if not path.exists():
        return None
    value = read_json(path)
    expected = {
        "schema", "job_id", "nonce", "descriptor_sha256", "state", "exit_code",
        "exit_observed_at_ns", "completed_at_ns", "stdout", "stderr", "diagnostic",
        "termination_sent", "delivered_stop_request_ids", "safe_state_verified", "effects_reconciled",
    }
    if not (isinstance(value, dict) and set(value) == expected
            and owned(value, descriptor, "zap-process-completed/1")
            and value.get("state") in terminal_states
            and (type(value.get("exit_code")) is int or value.get("exit_code") is None)):
        return None
    if not (verify_artifact(value.get("stdout"), root, expected_path=job_dir / "stdout.bin")
            and verify_artifact(value.get("stderr"), root, expected_path=job_dir / "stderr.bin")):
        return None
    diagnostic = value.get("diagnostic")
    delivered = value.get("delivered_stop_request_ids")
    if not (isinstance(diagnostic, dict) and set(diagnostic) == {"classification"}
            and isinstance(diagnostic["classification"], str)
            and isinstance(delivered, list) and all(isinstance(key, str) for key in delivered)
            and delivered == sorted(set(delivered)) and type(value.get("termination_sent")) is bool
            and value.get("safe_state_verified") is False and value.get("effects_reconciled") is False
            and type(value.get("completed_at_ns")) is int):
        return None
    exit_code, state = value["exit_code"], value["state"]
    if exit_code is None:
        if not (state == "failed" and value.get("exit_observed_at_ns") is None
                and diagnostic["classification"] == "transport_spawn_error"):
            return None
    elif (started_record(job_dir, descriptor) is None or host_started_record(job_dir, descriptor) is None
          or type(value.get("exit_observed_at_ns")) is not int):
        return None
    if state == "succeeded" and (exit_code != 0 or delivered or value["termination_sent"]):
        return None
    if state == "failed" and (exit_code == 0 or delivered or value["termination_sent"]):
        return None
    if state == "stopped" and (not delivered or value["termination_sent"]):
        return None
    if state == "interrupted" and (not delivered or value["termination_sent"] is not True):
        return None
    required_diagnostic = {"succeeded": "success", "stopped": "stop_completed", "interrupted": "process_terminated"}.get(state)
    if required_diagnostic is not None and diagnostic["classification"] != required_diagnostic:
        return None
    stop = stop_summary(job_dir, descriptor, request_key)
    if not set(delivered) <= set(stop["delivered_request_ids"]):
        return None
    if value["termination_sent"] is True and not stop["termination_sent"]:
        return None
    return value


__all__ = (
    "base_record", "completed_record", "complete_spawn_failure", "host_started_record", "owned",
    "prepare_known_pre_effect", "request_policy", "started_record", "stop_policy", "stop_requests",
    "stop_summary", "valid_stop_request",
)
