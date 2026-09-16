"""Bounded public projections that retain decisions without leaking effect inputs."""
from __future__ import annotations

import copy
import os
from typing import Any


def _text(value: Any, limit: int) -> tuple[str | None, bool]:
    if not isinstance(value, str):
        return None, False
    return value[:limit], len(value) > limit


def _command_summary(value: Any) -> dict[str, Any] | None:
    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        return None
    reason, truncated = _text(value.get("reason"), 1024)
    payload = value.get("payload") if isinstance(value.get("payload"), dict) else {}
    affected, provenance = {}, {}
    for key, item in payload.items():
        if key in {"source_refs", "evidence_refs", "evidence_ids", "node_refs"}:
            if isinstance(item, list) and all(isinstance(entry, str) for entry in item):
                provenance[key] = copy.deepcopy(item)
        elif key.endswith("_id") and isinstance(item, str):
            affected[key] = item
        elif key.endswith("_ids") and isinstance(item, list) and all(isinstance(entry, str) for entry in item):
            affected[key] = copy.deepcopy(item)
    return {
        "kind": value["kind"], "reason": reason, "reason_truncated": truncated,
        "affected_ids": affected, "provenance": provenance,
    }


def public_semantic_decision(value: Any, response_sha256: str) -> dict[str, Any]:
    """Project the validated coordinator decision fields, never its full payloads."""
    if not isinstance(value, dict) or value.get("schema") != "zap-coordinator-response/1":
        return {"redacted": True, "sha256": response_sha256}
    rationale, truncated = _text(value.get("rationale"), 4096)
    selection = value.get("selection")
    selected = {"work_id": selection["work_id"]} if isinstance(selection, dict) and isinstance(selection.get("work_id"), str) else None
    commands = []
    if isinstance(value.get("commands"), list):
        commands = [row for item in value["commands"] if (row := _command_summary(item)) is not None]
    return {
        "schema": "zap-public-semantic-decision/1",
        "request_id": value.get("request_id"), "request_kind": value.get("request_kind"),
        "state_revision": value.get("state_revision"), "request_sha256": value.get("request_sha256"),
        "disposition": value.get("disposition"), "selection": selected,
        "rationale": rationale, "rationale_truncated": truncated, "commands": commands,
        "response_sha256": response_sha256, "provider_material_redacted": True,
    }


def sanitize_event(event: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(event)
    payload = result.get("payload")
    if isinstance(payload, dict):
        if result.get("kind") == "runtime.job-claimed" and "packet" in payload:
            payload["packet"] = {"redacted": True, "sha256": payload.get("packet_sha256")}
        for key in ("source", "capture"):
            row = payload.get(key)
            if isinstance(row, dict):
                source = row.get("source") if key == "capture" else row
                if isinstance(source, dict):
                    source.pop("root", None)
    return sanitize_public_value(result)


def sanitize_public_value(value: Any) -> Any:
    if isinstance(value, list):
        return [sanitize_public_value(item) for item in value]
    if not isinstance(value, dict):
        return value
    result = {}
    artifact_descriptor = "sha256" in value and "bytes" in value
    source_descriptor = "content_sha256" in value and "path" in value
    for key, item in value.items():
        if key == "root" and source_descriptor:
            continue
        if key == "packet" and isinstance(value.get("packet_sha256"), str):
            result[key] = {"redacted": True, "sha256": value["packet_sha256"]}
            continue
        if key == "response" and isinstance(value.get("response_sha256"), str):
            result[key] = public_semantic_decision(item, value["response_sha256"])
            continue
        if key == "path" and artifact_descriptor and isinstance(item, str) and os.path.isabs(item):
            result["private_locator"] = True
            continue
        result[key] = sanitize_public_value(item)
    return result
