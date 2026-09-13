"""Durable projection for adaptive-review job reconciliation."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any

from .common import exact, identity, need, packed, sha, string
from .domain import domain_state
from .records import HandlerSpec

RECONCILIATION_SCHEMA = "zap-runtime-reconciliation/1"
ACTIONS = {"continue", "finish_compatible", "drain", "preserve_candidate", "revalidate"}
PHASES = {"draining", "complete", "safe_to_release", "needs_reconcile"}
COMPATIBILITY = {"compatible", "incompatible", "unknown"}
SAFE_STATES = {"unknown", "safe", "completed", "not_started", "needs_reconcile"}


def _runtime(state):
    runtime = state.setdefault("extensions", {}).get("runtime")
    need(isinstance(runtime, dict) and runtime.get("schema") == "zap-runtime/1",
         "RUNTIME_STATE", "reconciliation requires the runtime projection")
    runtime.setdefault("reconciliations", {})
    return runtime


def _strict(schema, fields):
    def validate(value):
        exact(value, {"schema", *fields})
        need(value["schema"] == schema, "RUNTIME_SCHEMA", f"expected {schema}")
        return copy.deepcopy(value)
    return validate


def _capture(value):
    exact(value, {"source_id", "sha256"}); identity(value["source_id"])
    need(isinstance(value["sha256"], str) and len(value["sha256"]) == 64,
         "RUNTIME_VALUE", "invalid source capture hash")


def _plan_payload(value):
    value = _strict("zap-runtime/reconciliation-planned/1", {
        "review_id", "review_event_id", "outcome_id", "items",
    })(value)
    for field in ("review_id", "review_event_id", "outcome_id"):
        identity(value[field])
    need(isinstance(value["items"], list) and value["items"],
         "RUNTIME_VALUE", "reconciliation plan needs jobs")
    seen = set()
    for row in value["items"]:
        exact(row, {"job_id", "work_id", "attempt_id", "action", "safe_boundary", "reason",
                    "captured_state", "contract_version", "contract_sha256", "source_captures"})
        for field in ("job_id", "work_id", "attempt_id"):
            identity(row[field])
        need(row["job_id"] not in seen, "DUPLICATE", "duplicate reconciliation job"); seen.add(row["job_id"])
        need(row["action"] in ACTIONS, "RUNTIME_VALUE", "invalid reconciliation action")
        string(row["safe_boundary"], "safe boundary"); string(row["reason"], "reconciliation reason")
        string(row["captured_state"], "captured job state")
        need(type(row["contract_version"]) is int and row["contract_version"] >= 0,
             "RUNTIME_VALUE", "invalid captured contract version")
        need(isinstance(row["contract_sha256"], str) and len(row["contract_sha256"]) == 64,
             "RUNTIME_VALUE", "invalid captured contract hash")
        need(isinstance(row["source_captures"], list), "RUNTIME_VALUE", "invalid source captures")
        for capture in row["source_captures"]: _capture(capture)
    return value


def _compatibility(value):
    exact(value, {"status", "contract_version", "contract_sha256", "source_status", "obligation_ids", "reasons"})
    need(value["status"] in COMPATIBILITY, "RUNTIME_VALUE", "invalid compatibility status")
    need(value["contract_version"] is None or type(value["contract_version"]) is int,
         "RUNTIME_VALUE", "invalid compatibility contract version")
    need(value["contract_sha256"] is None or isinstance(value["contract_sha256"], str)
         and len(value["contract_sha256"]) == 64, "RUNTIME_VALUE", "invalid compatibility contract hash")
    need(value["source_status"] in {"current", "stale", "unknown"},
         "RUNTIME_VALUE", "invalid compatibility source status")
    need(isinstance(value["obligation_ids"], list) and len(value["obligation_ids"]) == len(set(value["obligation_ids"])),
         "RUNTIME_VALUE", "invalid compatibility obligations")
    for key in value["obligation_ids"]: identity(key)
    need(isinstance(value["reasons"], list), "RUNTIME_VALUE", "invalid compatibility reasons")
    for reason in value["reasons"]: string(reason, "compatibility reason")


def _safe_state(value):
    exact(value, {"status", "receipt_sha256", "receipt"})
    need(value["status"] in SAFE_STATES, "RUNTIME_VALUE", "invalid reconciliation safe state")
    if value["status"] in {"safe", "completed", "not_started"}:
        need(isinstance(value["receipt"], dict) and isinstance(value["receipt_sha256"], str)
             and sha(packed(value["receipt"])) == value["receipt_sha256"],
             "RUNTIME_VALUE", "verified safe state needs an exact receipt")
    else:
        need(value["receipt"] is None and value["receipt_sha256"] is None,
             "RUNTIME_VALUE", "unverified safe state cannot carry a proof receipt")


def _progress_payload(value):
    value = _strict("zap-runtime/reconciliation-progress/1", {
        "review_id", "job_id", "phase", "compatibility", "transport", "safe_state", "detail",
    })(value)
    identity(value["review_id"]); identity(value["job_id"])
    need(value["phase"] in PHASES, "RUNTIME_VALUE", "invalid reconciliation phase")
    _compatibility(value["compatibility"])
    need(value["transport"] is None or isinstance(value["transport"], dict),
         "RUNTIME_VALUE", "invalid reconciliation transport receipt")
    _safe_state(value["safe_state"]); string(value["detail"], "reconciliation detail")
    return value


def _release_payload(value):
    value = _strict("zap-runtime/reconciliation-revalidation-released/1", {
        "review_id", "job_id", "work_id", "expected_job_state", "expected_work_state", "from_generation",
    })(value)
    for field in ("review_id", "job_id", "work_id"):
        identity(value[field])
    string(value["expected_job_state"], "expected job state")
    string(value["expected_work_state"], "expected work state")
    need(type(value["from_generation"]) is int and value["from_generation"] >= 0,
         "RUNTIME_VALUE", "invalid released validation generation")
    return value


def _planned(state, payload, event_id):
    runtime = _runtime(state); domain = domain_state(state)
    need(payload["review_id"] not in runtime["reconciliations"], "DUPLICATE", "reconciliation plan exists")
    review = domain["reviews"].get(payload["review_id"])
    need(review is not None and review["status"] == "applied"
         and review["applied_event_id"] == payload["review_event_id"]
         and domain["active_outcome_id"] == payload["outcome_id"],
         "RUNTIME_STALE", "applied review binding differs")
    planned = {row["job_id"]: row for row in review["transition"]["job_reconciliation"]}
    captured = {row["job_id"]: row for row in review["captures"]["jobs"]}
    need(set(planned) == set(captured) == {row["job_id"] for row in payload["items"]},
         "RUNTIME_STALE", "reconciliation jobs differ from applied review")
    items = {}
    for item in payload["items"]:
        job = runtime["jobs"].get(item["job_id"]); plan = planned[item["job_id"]]; capture = captured[item["job_id"]]
        need(job is not None and job["work_id"] == item["work_id"] and job["attempt_id"] == item["attempt_id"]
             and item["captured_state"] == capture["status"] and item["attempt_id"] == capture["attempt_id"]
             and all(item[key] == plan[key] for key in ("action", "safe_boundary", "reason"))
             and item["contract_version"] == job["contract_version"]
             and item["contract_sha256"] == job["contract_sha256"]
             and item["source_captures"] == job["source_captures"],
             "RUNTIME_STALE", "reconciliation item binding differs")
        items[item["job_id"]] = {**copy.deepcopy(item), "status": "pending", "history": []}
    runtime["reconciliations"][payload["review_id"]] = {
        **copy.deepcopy(payload), "schema": RECONCILIATION_SCHEMA, "event_id": event_id,
        "state": "pending", "items": items,
    }


def _progress(state, payload, event_id):
    runtime = _runtime(state); plan = runtime["reconciliations"].get(payload["review_id"])
    item = plan.get("items", {}).get(payload["job_id"]) if isinstance(plan, dict) else None
    need(item is not None and item["status"] != "complete"
         and (item["status"] != "released" or item["action"] == "revalidate" and payload["phase"] == "complete"),
         "RUNTIME_STALE", "reconciliation item is not pending")
    action = item["action"]
    if payload["phase"] == "complete":
        if action == "revalidate":
            need(any(row.get("review_id") == payload["review_id"] and row.get("job_id") == payload["job_id"]
                     for row in domain_state(state).get("revalidation_history", [])),
                 "RUNTIME_STALE", "revalidation work was not readied")
        else:
            need(action == "drain" or payload["compatibility"]["status"] == "compatible",
                 "RUNTIME_STALE", "reconciliation completion lacks compatibility")
        if action == "drain":
            need(payload["safe_state"]["status"] in {"safe", "completed", "not_started"},
                 "RUNTIME_STALE", "drain completion lacks verified safe state")
        item["status"] = "complete"
    elif payload["phase"] == "safe_to_release":
        need(action == "revalidate" and payload["safe_state"]["status"] in {"safe", "completed", "not_started"},
             "RUNTIME_STALE", "revalidation release lacks verified safe state")
        item["status"] = "safe_to_release"
    elif payload["phase"] == "needs_reconcile":
        item["status"] = "needs_reconcile"
    else:
        need(action in {"drain", "revalidate"}, "RUNTIME_STALE", "only stop actions may drain")
        item["status"] = "draining"
    item["history"].append({**copy.deepcopy(payload), "event_id": event_id})
    states = {row["status"] for row in plan["items"].values()}
    plan["state"] = "complete" if states == {"complete"} else "needs_reconcile" if "needs_reconcile" in states else "pending"


def _released(state, payload, event_id):
    runtime = _runtime(state); plan = runtime["reconciliations"].get(payload["review_id"])
    item = plan.get("items", {}).get(payload["job_id"]) if isinstance(plan, dict) else None
    job = runtime["jobs"].get(payload["job_id"])
    need(item is not None and item["action"] == "revalidate" and item["status"] == "safe_to_release",
         "RUNTIME_STALE", "revalidation is not safely releasable")
    need(job is not None and job["work_id"] == payload["work_id"]
         and job["state"] == payload["expected_job_state"],
         "RUNTIME_STALE", "revalidation job state differs")
    job["state"] = "retry_released"; job["reconciliation_review_id"] = payload["review_id"]
    runtime["attempts"][job["attempt_id"]]["state"] = "retry_released"
    runtime["reservations"][job["job_id"]]["active"] = False
    for row in runtime["verification_jobs"].values():
        if row["work_job_id"] == job["job_id"]:
            row["revalidation_disposition"] = {"status": "superseded", "review_id": payload["review_id"], "event_id": event_id}
    item["status"] = "released"; item["released_event_id"] = event_id
    item["expected_work_state"] = payload["expected_work_state"]
    item["from_generation"] = payload["from_generation"]
    plan["state"] = "pending"


def _handler(kind, validator, apply):
    def wrapped(state, payload, event_id):
        apply(state, payload, event_id)
        _runtime(state)["revision"] += 1
    return HandlerSpec(kind, validator, wrapped)


RECONCILIATION_HANDLERS = MappingProxyType({spec.kind: spec for spec in (
    _handler("runtime.reconciliation-planned", _plan_payload, _planned),
    _handler("runtime.reconciliation-progress-observed", _progress_payload, _progress),
    _handler("runtime.reconciliation-revalidation-released", _release_payload, _released),
)})
_ID = {"type": "string", "pattern": "^[A-Za-z0-9._:-]+$"}
_TEXT = {"type": "string", "minLength": 1}
_SHA = {"type": "string", "pattern": "^[0-9a-f]{64}$"}
_CAPTURE = {"type": "object", "additionalProperties": False, "required": ["source_id", "sha256"],
            "properties": {"source_id": _ID, "sha256": _SHA}}
_ITEM = {"type": "object", "additionalProperties": False,
         "required": ["job_id", "work_id", "attempt_id", "action", "safe_boundary", "reason", "captured_state",
                      "contract_version", "contract_sha256", "source_captures"],
         "properties": {"job_id": _ID, "work_id": _ID, "attempt_id": _ID, "action": {"enum": sorted(ACTIONS)},
                        "safe_boundary": _TEXT, "reason": _TEXT, "captured_state": _TEXT,
                        "contract_version": {"type": "integer", "minimum": 0}, "contract_sha256": _SHA,
                        "source_captures": {"type": "array", "items": _CAPTURE}}}
_COMPATIBILITY = {"type": "object", "additionalProperties": False,
                  "required": ["status", "contract_version", "contract_sha256", "source_status", "obligation_ids", "reasons"],
                  "properties": {"status": {"enum": sorted(COMPATIBILITY)},
                                 "contract_version": {"type": ["integer", "null"]},
                                 "contract_sha256": {"anyOf": [_SHA, {"type": "null"}]},
                                 "source_status": {"enum": ["current", "stale", "unknown"]},
                                 "obligation_ids": {"type": "array", "items": _ID, "uniqueItems": True},
                                 "reasons": {"type": "array", "items": _TEXT}}}
_SAFE_STATE = {"type": "object", "additionalProperties": False,
               "required": ["status", "receipt_sha256", "receipt"],
               "properties": {"status": {"enum": sorted(SAFE_STATES)},
                              "receipt_sha256": {"anyOf": [_SHA, {"type": "null"}]},
                              "receipt": {"type": ["object", "null"]}}}


def _event_schema(schema, properties):
    return {"type": "object", "additionalProperties": False, "required": ["schema", *properties],
            "properties": {"schema": {"const": schema}, **properties}}


RECONCILIATION_EVENT_SCHEMAS = MappingProxyType({
    "runtime.reconciliation-planned": _event_schema("zap-runtime/reconciliation-planned/1", {
        "review_id": _ID, "review_event_id": _ID, "outcome_id": _ID,
        "items": {"type": "array", "items": _ITEM, "minItems": 1},
    }),
    "runtime.reconciliation-progress-observed": _event_schema("zap-runtime/reconciliation-progress/1", {
        "review_id": _ID, "job_id": _ID, "phase": {"enum": sorted(PHASES)},
        "compatibility": _COMPATIBILITY, "transport": {"type": ["object", "null"]},
        "safe_state": _SAFE_STATE, "detail": _TEXT,
    }),
    "runtime.reconciliation-revalidation-released": _event_schema(
        "zap-runtime/reconciliation-revalidation-released/1", {
            "review_id": _ID, "job_id": _ID, "work_id": _ID, "expected_job_state": _TEXT,
            "expected_work_state": _TEXT, "from_generation": {"type": "integer", "minimum": 0},
        }),
})
RECONCILIATION_ACTION_KINDS = MappingProxyType({"runtime.reconciliation-revalidation-released": "plan.lower"})
RECONCILIATION_DATA_KINDS = frozenset()
RECONCILIATION_OBSERVATION_KINDS = frozenset(set(RECONCILIATION_HANDLERS) - set(RECONCILIATION_ACTION_KINDS))
RECONCILIATION_EVENT_ROUTES = MappingProxyType({
    kind: ({"route": "action", "action": RECONCILIATION_ACTION_KINDS[kind]}
           if kind in RECONCILIATION_ACTION_KINDS else {"route": "trusted_observation", "action": None})
    for kind in RECONCILIATION_HANDLERS
})

__all__ = (
    "RECONCILIATION_ACTION_KINDS", "RECONCILIATION_DATA_KINDS", "RECONCILIATION_EVENT_ROUTES",
    "RECONCILIATION_EVENT_SCHEMAS", "RECONCILIATION_HANDLERS", "RECONCILIATION_OBSERVATION_KINDS",
    "RECONCILIATION_SCHEMA",
)
