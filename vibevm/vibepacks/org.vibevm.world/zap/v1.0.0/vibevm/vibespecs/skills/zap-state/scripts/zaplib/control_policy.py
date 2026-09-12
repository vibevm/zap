"""Exact action validation and pure stop-policy assessment."""
from __future__ import annotations

from typing import Any

from .common import exact, identity, need, packed, sha
from .records import State
from .control_model import (
    ACTION_CLASS_SET, ACTION_SCHEMA, ASSESSMENT_SCHEMA, HASH, _campaign_id,
    _hash, _identities, _nullable_identity, active_policy, control_state,
    pause_applies,
)

def _validate_capture_list(value: Any) -> list[dict[str, str]]:
    need(isinstance(value, list), "ACTION", "source captures must be a list")
    captures: list[dict[str, str]] = []
    seen: set[str] = set()
    for raw in value:
        exact(raw, {"source_id", "sha256"})
        source_id = identity(raw["source_id"])
        need(source_id not in seen, "DUPLICATE", f"duplicate source capture {source_id}")
        seen.add(source_id)
        captures.append({"source_id": source_id, "sha256": _hash(raw["sha256"], "source capture hash")})
    need([row["source_id"] for row in captures] == sorted(seen), "ORDER", "source captures must be sorted by source_id")
    return captures


def validate_action(value: Any) -> dict[str, Any]:
    exact(value, {
        "schema", "action_id", "action_class", "campaign_id", "base_sha256",
        "charter_revision", "payload_sha256", "source_captures", "branch_id",
        "run_id", "problem_id",
    })
    need(value["schema"] == ACTION_SCHEMA, "ACTION", "unknown action schema")
    need(value["action_class"] in ACTION_CLASS_SET, "ACTION", "unknown delegated action class")
    need(type(value["charter_revision"]) is int and value["charter_revision"] > 0, "ACTION", "invalid charter revision")
    return {
        "schema": ACTION_SCHEMA,
        "action_id": identity(value["action_id"]),
        "action_class": value["action_class"],
        "campaign_id": identity(value["campaign_id"]),
        "base_sha256": _hash(value["base_sha256"], "action base hash"),
        "charter_revision": value["charter_revision"],
        "payload_sha256": _hash(value["payload_sha256"], "action payload hash"),
        "source_captures": _validate_capture_list(value["source_captures"]),
        "branch_id": _nullable_identity(value["branch_id"], "branch id"),
        "run_id": _nullable_identity(value["run_id"], "run id"),
        "problem_id": _nullable_identity(value["problem_id"], "problem id"),
    }


def validate_assessment(value: Any) -> dict[str, Any]:
    exact(value, {"schema", "assessment_id", "policy_id", "policy_revision", "phase", "values", "drain_targets"})
    need(value["schema"] == ASSESSMENT_SCHEMA, "ASSESSMENT", "unknown assessment schema")
    need(type(value["policy_revision"]) is int and value["policy_revision"] > 0, "ASSESSMENT", "invalid policy revision")
    need(value["phase"] in {"before_action", "after_action"}, "ASSESSMENT", "invalid assessment phase")
    need(isinstance(value["values"], dict), "ASSESSMENT", "assessment values must be a mapping")
    values: dict[str, Any] = {}
    for key, item in value["values"].items():
        identity(key)
        need(item is None or type(item) in {str, bool, int, float}, "ASSESSMENT", f"assessment value {key} must be a JSON scalar or null")
        values[key] = item
    drain_targets = _identities(value["drain_targets"], "drain targets")
    return {
        "schema": ASSESSMENT_SCHEMA,
        "assessment_id": identity(value["assessment_id"]),
        "policy_id": identity(value["policy_id"]),
        "policy_revision": value["policy_revision"],
        "phase": value["phase"],
        "values": values,
        "drain_targets": drain_targets,
    }


def _failure_counts(state: State, control: dict[str, Any]) -> tuple[dict[str, dict[str, Any]], list[dict[str, str]]]:
    by_problem: dict[str, dict[str, Any]] = {}
    for row in control["approach_history"]:
        if row["outcome"] != "failed":
            continue
        current_epoch = control["approach_epochs"].get(row["problem_id"], 0)
        if row["epoch"] != current_epoch:
            continue
        record = by_problem.setdefault(row["problem_id"], {"epoch": current_epoch, "strategies": set(), "approach_ids": set()})
        record["strategies"].add(row["strategy_sha256"])
        record["approach_ids"].add(row["approach_id"])
    unresolved: list[dict[str, str]] = []
    legacy = control.get("legacy_approaches")
    if legacy is None:
        legacy = {}
    need(isinstance(legacy, dict), "APPROACH", "legacy approach projection must be a mapping")
    for approach_id, row in legacy.items():
        need(isinstance(row, dict), "APPROACH", "legacy approach row must be a mapping")
        problem_id = identity(row.get("problem_id"))
        stable_approach_id = identity(approach_id)
        strategy_key = row.get("strategy_key")
        need(isinstance(strategy_key, str) and strategy_key, "APPROACH", "legacy approach lacks a stable strategy key")
        epoch = control["approach_epochs"].get(problem_id, 0)
        if epoch != 0:
            continue
        outcome = row.get("outcome")
        if outcome == "failed":
            record = by_problem.setdefault(problem_id, {"epoch": 0, "strategies": set(), "approach_ids": set()})
            record["strategies"].add(sha(packed({"problem_id": problem_id, "strategy_key": strategy_key})))
            record["approach_ids"].add(stable_approach_id)
        elif outcome == "unresolved":
            unresolved.append({"problem_id": problem_id, "approach_id": stable_approach_id})
        else:
            need(outcome == "succeeded", "APPROACH", "legacy approach has an unsupported semantic outcome")
    counts = {
        key: {"epoch": row["epoch"], "count": len(row["strategies"]), "approach_ids": sorted(row["approach_ids"])}
        for key, row in sorted(by_problem.items())
    }
    return counts, sorted(unresolved, key=lambda row: (row["problem_id"], row["approach_id"]))


def _evaluate_expression(expression: dict[str, Any], values: dict[str, Any], state: State, control: dict[str, Any]) -> tuple[bool | None, list[dict[str, Any]]]:
    operator, argument = next(iter(expression.items()))
    if operator in {"all", "any"}:
        results = [_evaluate_expression(item, values, state, control) for item in argument]
        observed = [item[0] for item in results]
        if operator == "all":
            result = False if False in observed else None if None in observed else True
        else:
            result = True if True in observed else None if None in observed else False
        return result, [detail for item in results for detail in item[1]]
    if operator == "not":
        result, details = _evaluate_expression(argument, values, state, control)
        return (None if result is None else not result), details
    if operator == "eq":
        observed = values.get(argument["field"])
        expected = argument["value"]
        need(observed is None or type(observed) is type(expected), "ASSESSMENT", f"assessment type differs for {argument['field']}")
        result = None if observed is None else observed == expected
        return result, [{"field": argument["field"], "observed": observed, "expected": expected, "result": result}]
    if operator == "failed_approaches":
        counts, unresolved = _failure_counts(state, control)
        threshold_met = any(row["count"] >= argument["gte"] for row in counts.values())
        result = True if threshold_met else None if unresolved else False
        return result, [{"failed_by_problem": counts, "unresolved_legacy": unresolved, "threshold": argument["gte"], "result": result}]
    raise Refusal("RULE", "unsupported active stop expression")


def _effective_scope(results: list[dict[str, Any]]) -> tuple[str, str | None]:
    matched = [row for row in results if row["matched"] is True]
    for scope in ("campaign", "branch", "run"):
        row = next((item for item in matched if item["scope"] == scope), None)
        if row is not None:
            return scope, row["scope_id"]
    return "campaign", None


def _affected_active_jobs(state: State, scope: str, scope_id: str | None) -> list[str]:
    extensions = state.get("extensions", {})
    need(isinstance(extensions, dict), "RUNTIME_STATE", "extensions must be a mapping")
    runtime = extensions.get("runtime")
    if runtime is None:
        return []
    need(isinstance(runtime, dict), "RUNTIME_STATE", "runtime projection must be a mapping")
    jobs = runtime.get("jobs", {})
    need(isinstance(jobs, (dict, list)), "RUNTIME_STATE", "runtime jobs must be a mapping or list")
    rows = list(jobs.values()) if isinstance(jobs, dict) else jobs
    active_states = {"claimed", "dispatched", "starting", "running", "stop_requested", "stopping", "unknown_effect"}
    affected: list[str] = []
    for row in rows:
        need(isinstance(row, dict), "RUNTIME_STATE", "runtime job must be a mapping")
        job_id = identity(row.get("job_id", row.get("id")))
        job_state = row.get("state", row.get("actual_state"))
        if job_state not in active_states:
            continue
        if scope == "branch" and row.get("branch_id") != scope_id:
            continue
        if scope == "run" and row.get("run_id", job_id) != scope_id:
            continue
        affected.append(job_id)
    return sorted(set(affected))


def _source_capture_preconditions(state: State, captures: list[dict[str, str]]) -> tuple[bool, list[dict[str, Any]]]:
    if not captures:
        return True, []
    extensions = state.get("extensions", {})
    knowledge = extensions.get("knowledge") if isinstance(extensions, dict) else None
    if not isinstance(knowledge, dict):
        return False, [{"source_id": row["source_id"], "result": None, "reason": "knowledge source registry unavailable"} for row in captures]
    sources = knowledge.get("sources", knowledge.get("source_captures"))
    if not isinstance(sources, dict):
        return False, [{"source_id": row["source_id"], "result": None, "reason": "knowledge source registry unavailable"} for row in captures]
    details: list[dict[str, Any]] = []
    current = True
    for capture in captures:
        row = sources.get(capture["source_id"])
        observed: Any = None
        if not isinstance(row, dict):
            result = None
            reason = "source capture is not recorded"
        else:
            observed = row.get("sha256", row.get("content_sha256"))
            status = row.get("status", "current")
            if not isinstance(observed, str) or HASH.fullmatch(observed) is None:
                result = None
                reason = "source capture has no current hash"
            elif status in {"invalidated", "unavailable", "unknown"}:
                result = None
                reason = f"source capture status is {status}"
            else:
                result = observed == capture["sha256"]
                reason = None if result else "source capture hash changed"
        if result is not True:
            current = False
        details.append({
            "source_id": capture["source_id"],
            "expected": capture["sha256"],
            "observed": observed,
            "result": result,
            "reason": reason,
        })
    return current, details


def assess_action(state: State, action: Any, assessment: Any) -> dict[str, Any]:
    """Evaluate the active versioned stop policy for one exact action."""
    checked_action = validate_action(action)
    checked_assessment = validate_assessment(assessment)
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "no owner charter is active")
    need(checked_action["campaign_id"] == policy["campaign_id"] == _campaign_id(state), "CAMPAIGN", "action campaign differs")
    need(checked_action["base_sha256"] == policy["base_sha256"] == state.get("base_sha256"), "BASE", "action base differs")
    need(checked_action["charter_revision"] == policy["revision"], "STALE_POLICY", "action charter revision differs")
    need(checked_action["action_class"] in policy["allowed_actions"], "AUTHORIZATION", "action class is not delegated")
    stop_policy = policy["stop_policy"]
    need(checked_assessment["policy_id"] == stop_policy["policy_id"] and checked_assessment["policy_revision"] == stop_policy["revision"], "STALE_POLICY", "assessment stop-policy identity differs")

    control = control_state(state)
    sources_current, source_details = _source_capture_preconditions(state, checked_action["source_captures"])
    results: list[dict[str, Any]] = []
    for rule in stop_policy["rules"]:
        if checked_action["action_class"] not in rule["applies_to_actions"]:
            continue
        scope_id = None
        scope_missing = False
        if rule["scope"] == "branch":
            scope_id = checked_action["branch_id"]
            scope_missing = scope_id is None
        elif rule["scope"] == "run":
            scope_id = checked_action["run_id"]
            scope_missing = scope_id is None
        if scope_missing:
            matched, details = None, [{"missing_scope": rule["scope"]}]
        else:
            matched, details = _evaluate_expression(rule["when"], checked_assessment["values"], state, control)
        results.append({
            "id": rule["id"],
            "matched": matched,
            "scope": rule["scope"],
            "scope_id": scope_id,
            "timing": rule["timing"],
            "details": details,
        })

    matched_rules = [row["id"] for row in results if row["matched"] is True]
    unknown_rules = [row["id"] for row in results if row["matched"] is None]
    action_already_occurred = checked_assessment["phase"] == "after_action"
    applicable = pause_applies(
        state,
        branch_id=checked_action["branch_id"],
        run_id=checked_action["run_id"],
    )
    matched_scope, matched_scope_id = _effective_scope(results)
    scope_rank = {"campaign": 0, "branch": 1, "run": 2}
    existing_scope = applicable[0]["scope"] if applicable else None
    if existing_scope is not None and scope_rank[existing_scope] <= scope_rank[matched_scope]:
        effective_scope = existing_scope
        scope_id = applicable[0]["scope_id"]
    else:
        effective_scope = matched_scope
        scope_id = matched_scope_id
    late_rule = action_already_occurred and any(
        row["matched"] is True and row["timing"] == "before_action"
        for row in results
    )
    if late_rule or (action_already_occurred and applicable):
        policy_result = "too_late"
    elif matched_rules or applicable:
        policy_result = "pause"
    elif unknown_rules or not sources_current:
        policy_result = "needs_evidence"
        effective_scope, scope_id = "campaign", None
    else:
        policy_result = "clear"
        effective_scope, scope_id = "campaign", None
    requires_drain = policy_result in {"pause", "too_late"}
    if requires_drain:
        expected_drains = _affected_active_jobs(state, effective_scope, scope_id)
        need(checked_assessment["drain_targets"] == expected_drains, "DRAIN_CAPTURE", "assessment drain targets differ from persisted affected jobs")
    broader_pause_needed = bool(matched_rules) and (
        not applicable or scope_rank[matched_scope] < min(scope_rank[row["scope"]] for row in applicable)
    )
    eligible = policy_result == "clear" and not action_already_occurred
    return {
        "schema": "zap-action-assessment-result/1",
        "assessment_id": checked_assessment["assessment_id"],
        "campaign_id": policy["campaign_id"],
        "base_sha256": policy["base_sha256"],
        "charter_id": policy["charter_id"],
        "charter_revision": policy["revision"],
        "charter_sha256": policy["charter_sha256"],
        "policy_id": stop_policy["policy_id"],
        "policy_revision": stop_policy["revision"],
        "action": checked_action,
        "action_sha256": sha(packed(checked_action)),
        "policy_result": policy_result,
        "matched_rules": matched_rules,
        "unknown_rules": unknown_rules,
        "rules": results,
        "preconditions": {"source_captures_current": sources_current, "source_captures": source_details},
        "effective_scope": effective_scope,
        "scope_id": scope_id,
        "applicable_pause_ids": [row["pause_id"] for row in applicable],
        "materialize_pause": broader_pause_needed,
        "evaluated": {
            "state_revision": state.get("revision"),
            "phase": checked_assessment["phase"],
            "action_already_occurred": action_already_occurred,
        },
        "delivery": {"state": "pending" if requires_drain and checked_assessment["drain_targets"] else "complete" if requires_drain else "not_requested", "required": checked_assessment["drain_targets"]},
        "actual_safe_state": {"state": "unknown" if requires_drain and checked_assessment["drain_targets"] else "reached" if requires_drain else "not_requested", "required": checked_assessment["drain_targets"]},
        "eligible_for_admission": eligible,
        "action_admitted": False,
        "would_prevent": policy_result == "pause",
        "prevention_performed": False,
        "prevented_action": False,
    }
