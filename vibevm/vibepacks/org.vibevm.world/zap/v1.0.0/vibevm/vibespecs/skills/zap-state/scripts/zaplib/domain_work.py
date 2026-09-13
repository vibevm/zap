"""Versioned task contracts and authorized work transitions."""
from __future__ import annotations

from typing import Any

from .common import exact, identity, need
from .domain_model import (
    STAGES, WORK_STATES, content_hash, domain_frontier, domain_handler,
    effective_nodes, integer, mapping, nonblank_list, owned_obligations,
    require_action, require_refs, strict, text, unique_ids,
)

TRANSITIONS = {
    "planned": {"ready", "blocked", "deferred", "dropped", "superseded"},
    "ready": {"active", "blocked", "deferred", "dropped", "superseded"},
    "active": {"candidate", "blocked"},
    "candidate": {"ready", "blocked", "deferred", "dropped", "superseded"},
    "blocked": {"planned", "ready", "deferred", "dropped", "superseded"},
    "deferred": {"planned", "ready", "dropped", "superseded"},
    "accepted": set(), "dropped": set(), "superseded": set(),
}
CONTRACT_FIELDS = {
    "schema", "contract_id", "work_id", "title", "goal", "read_subjects",
    "write_subjects", "resources", "steps", "positive_cases", "negative_cases",
    "checks", "acceptance", "safe_stop", "integration_owner", "delivery_route",
    "required_stage", "source_handles", "obligation_ids",
}


def validate_contract(contract: Any, *, work_id: str | None = None) -> dict[str, Any]:
    exact(contract, CONTRACT_FIELDS)
    need(contract["schema"] == "zap-task-contract/1", "DOMAIN_SCHEMA", "invalid task contract schema")
    identity(contract["contract_id"]); identity(contract["work_id"])
    if work_id is not None:
        need(contract["work_id"] == work_id, "DOMAIN_CONTRACT", "contract work identity differs")
    for field in ("title", "goal", "safe_stop", "integration_owner"):
        text(contract[field], f"contract {field}")
    identity(contract["integration_owner"])
    for field in "read_subjects write_subjects resources steps positive_cases negative_cases checks acceptance source_handles".split():
        nonblank_list(contract[field], f"contract {field}",
                      nonempty=field in {"steps", "positive_cases", "negative_cases", "checks", "acceptance"})
    route = unique_ids(contract["delivery_route"], "delivery route", nonempty=True)
    order = ["prototype", "functional", "productized"]
    if "direct" in route:
        need(route == ["direct"], "DOMAIN_CONTRACT", "direct route cannot contain stages")
    else:
        need(all(stage in STAGES for stage in route) and route == sorted(route, key=order.index),
             "DOMAIN_CONTRACT", "staged route is invalid")
    need(contract["required_stage"] in STAGES, "DOMAIN_CONTRACT", "required stage is invalid")
    unique_ids(contract["obligation_ids"], "contract obligations", nonempty=True)
    return contract


def _contract_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/task-contract-replaced/1", {"work_id", "expected_version", "contract"})(value)
    identity(value["work_id"]); integer(value["expected_version"], "expected contract version")
    validate_contract(mapping(value["contract"], "task contract"), work_id=value["work_id"])
    return value


def _apply_contract(state, domain, payload, event_id):
    require_action(state, "task.update")
    need(payload["work_id"] in effective_nodes(state, domain), "REFERENCE", "work missing")
    need(set(payload["contract"]["obligation_ids"]) == owned_obligations(domain, payload["work_id"]),
         "DOMAIN_CONTRACT", "task contract must bind every current work obligation exactly")
    contract_id = payload["contract"]["contract_id"]
    need(not any(work_id != payload["work_id"] and any(
        row.get("contract", {}).get("contract_id") == contract_id for row in history["versions"])
        for work_id, history in domain["task_contracts"].items()),
        "DUPLICATE", "task contract identity belongs to another work item")
    history = domain["task_contracts"].get(payload["work_id"], {"active_version": -1, "versions": []})
    need(history["active_version"] == payload["expected_version"], "DOMAIN_STALE", "task contract version differs")
    version = payload["expected_version"] + 1
    history["versions"].append({
        "version": version, "schema": "zap-task-contract/1", "source": "domain_event",
        "sha256": content_hash(payload["contract"]), "contract": payload["contract"], "event_id": event_id,
    })
    history["active_version"] = version
    domain["task_contracts"][payload["work_id"]] = history


def _revalidation_payload(value):
    value = strict("zap-domain/work-revalidation-readied/1", {
        "work_id", "review_id", "job_id", "from_generation", "expected_state",
    })(value)
    identity(value["work_id"]); identity(value["review_id"]); identity(value["job_id"])
    integer(value["from_generation"], "validation generation")
    need(value["expected_state"] in WORK_STATES, "DOMAIN_VALUE", "invalid revalidation work state")
    return value


def _apply_revalidation(state, domain, payload, event_id):
    require_action(state, "plan.lower")
    review = domain["reviews"].get(payload["review_id"])
    need(review is not None and review["status"] == "applied", "DOMAIN_REVIEW", "applied revalidation review missing")
    need(any(row["work_id"] == payload["work_id"] and row["operation"] == "revalidate"
             for row in review["transition"]["work_changes"]),
         "DOMAIN_REVIEW", "review did not require work revalidation")
    need(any(row["job_id"] == payload["job_id"] and row["action"] == "revalidate"
             for row in review["transition"]["job_reconciliation"]),
         "DOMAIN_REVIEW", "review did not reconcile this job for revalidation")
    runtime = state.get("extensions", {}).get("runtime", {})
    reconciliation = runtime.get("reconciliations", {}).get(payload["review_id"])
    item = reconciliation.get("items", {}).get(payload["job_id"]) if isinstance(reconciliation, dict) else None
    job = runtime.get("jobs", {}).get(payload["job_id"])
    need(item is not None and item.get("status") == "released" and job is not None
         and job.get("state") == "retry_released" and job.get("work_id") == payload["work_id"],
         "DOMAIN_REVIEW", "prior job is not safely released for revalidation")
    generation = domain.setdefault("validation_generations", {}).get(payload["work_id"], 0)
    need(generation == payload["from_generation"], "DOMAIN_STALE", "validation generation changed")
    nodes = effective_nodes(state, domain)
    node = nodes.get(payload["work_id"])
    need(node is not None and node["state"] == payload["expected_state"],
         "DOMAIN_STALE", "revalidation work state changed")
    need(domain["work_updates"].get(payload["work_id"], {}).get("revalidation_required") is True,
         "DOMAIN_REVIEW", "work is not marked for revalidation")
    need(domain["work_updates"][payload["work_id"]].get("revalidation_from_generation") == generation,
         "DOMAIN_STALE", "pending revalidation generation differs")
    active_states = {"claimed", "dispatched", "starting", "running", "stop_requested", "stopping", "unknown_effect"}
    need(not any(key != payload["job_id"] and row.get("work_id") == payload["work_id"]
                 and row.get("state") in active_states for key, row in runtime.get("jobs", {}).items()),
         "DOMAIN_REVIEW", "another work attempt is still active")
    domain["validation_generations"][payload["work_id"]] = generation + 1
    domain["work_updates"].setdefault(payload["work_id"], {})["state"] = "ready"
    domain.setdefault("revalidation_history", []).append({
        "work_id": payload["work_id"], "review_id": payload["review_id"], "job_id": payload["job_id"],
        "from_generation": generation, "to_generation": generation + 1,
        "previous_state": payload["expected_state"], "event_id": event_id,
    })


def _validate_rename(value):
    value = strict("zap-domain/work-renamed/1", {"work_id", "expected_title", "new_title"})(value)
    identity(value["work_id"]); text(value["expected_title"], "expected title"); text(value["new_title"], "new title")
    return value


def _apply_rename(state, domain, payload, event_id):
    require_action(state, "plan.lower")
    nodes = effective_nodes(state, domain); key = payload["work_id"]
    need(key in nodes and nodes[key]["title"] == payload["expected_title"], "DOMAIN_STALE", "work title differs")
    domain["work_updates"].setdefault(key, {})["title"] = payload["new_title"]


def _validate_transition(value):
    value = strict("zap-domain/work-transitioned/1", {"work_id", "from_state", "to_state", "successor_ids"})(value)
    identity(value["work_id"])
    need(value["from_state"] in WORK_STATES and value["to_state"] in WORK_STATES, "DOMAIN_VALUE", "invalid work state")
    unique_ids(value["successor_ids"], "work successors")
    return value


def _apply_transition(state, domain, payload, event_id):
    require_action(state, "plan.lower")
    nodes = effective_nodes(state, domain); key = payload["work_id"]
    need(key in nodes and nodes[key]["state"] == payload["from_state"], "DOMAIN_STALE", "work state differs")
    need(payload["to_state"] in TRANSITIONS[payload["from_state"]], "DOMAIN_TRANSITION", "invalid work transition")
    need(payload["to_state"] != "active", "DOMAIN_TRANSITION", "use domain.work-dispatched for execution")
    successors = payload["successor_ids"]
    if payload["to_state"] == "superseded":
        require_refs(successors, set(nodes) - {key}, "work successors", nonempty=True)
        need(not owned_obligations(domain, key), "DOMAIN_OBLIGATION", "superseded work still owns active obligations")
    else:
        need(not successors, "DOMAIN_TRANSITION", "only superseded work names successors")
    if payload["to_state"] == "dropped":
        need(not owned_obligations(domain, key), "DOMAIN_OBLIGATION", "dropped work still owns active obligations")
    if payload["to_state"] == "deferred":
        open_rows = [row for row in domain["deferrals"].values() if row["status"] == "open" and key in row["work_ids"]]
        covered = {oid for row in open_rows for oid in row["obligation_ids"]}
        need(owned_obligations(domain, key) <= covered, "DOMAIN_DEFERRAL", "deferral does not cover active work obligations")
    update = domain["work_updates"].setdefault(key, {}); update["state"] = payload["to_state"]
    if payload["to_state"] == "dropped": update["dependency_resolved"] = True
    if successors: domain["work_successors"][key] = successors


def _validate_dispatch(value):
    value = strict("zap-domain/work-dispatched/1", {"work_id", "from_state", "job_id"})(value)
    identity(value["work_id"]); identity(value["job_id"])
    need(value["from_state"] == "ready", "DOMAIN_TRANSITION", "dispatch requires ready work")
    return value


def _apply_dispatch(state, domain, payload, event_id):
    require_action(state, "work.dispatch")
    nodes = effective_nodes(state, domain); key = payload["work_id"]
    need(domain["active_outcome_id"] is not None and domain["closure"] is None,
         "DOMAIN_TRANSITION", "dispatch requires an active, open outcome")
    need(key in nodes and nodes[key]["state"] == payload["from_state"], "DOMAIN_STALE", "work state differs")
    need(key in domain_frontier(state), "DOMAIN_TRANSITION", "work is not in the domain frontier")
    history = domain["task_contracts"].get(key)
    need(history is not None, "DOMAIN_CONTRACT", "dispatch work lacks a task contract")
    contract = next((row for row in history["versions"] if row["version"] == history["active_version"]), None)
    need(contract is not None and contract["schema"] == "zap-task-contract/1",
         "DOMAIN_CONTRACT", "dispatch requires a current versioned ZAP task contract")
    need(owned_obligations(domain, key), "DOMAIN_OBLIGATION", "dispatch work has no current obligation")
    update = domain["work_updates"].setdefault(key, {})
    update["state"] = "active"; update["active_job_id"] = payload["job_id"]


DOMAIN_WORK_HANDLERS = {
    spec.kind: spec for spec in (
        domain_handler("domain.task-contract-replaced", _contract_payload, _apply_contract),
        domain_handler("domain.work-revalidation-readied", _revalidation_payload, _apply_revalidation),
        domain_handler("domain.work-renamed", _validate_rename, _apply_rename),
        domain_handler("domain.work-transitioned", _validate_transition, _apply_transition),
        domain_handler("domain.work-dispatched", _validate_dispatch, _apply_dispatch),
    )
}

__all__ = ("DOMAIN_WORK_HANDLERS", "validate_contract")
