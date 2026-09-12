"""Pure reducers for accepted ZAP control events."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any, Mapping

from .common import exact, identity, need, packed, sha, string
from .records import HandlerSpec, State
from .control_model import (
    ACTION_CLASS_SET, _activation_payload, _campaign_id,
    _charter_payload, _control_mut, _entry, _hash, _identities, _legacy_approaches, _legacy_sources,
    _nullable_identity, _sync_active_pause, _sync_execution_mode,
    _validate_charter_against_state, active_policy, pause_applies,
)
from .control_policy import _affected_active_jobs, assess_action, validate_action, validate_assessment

def _draft_charter(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    charter = payload["charter"]
    if control["legacy_sources"] is None:
        control["legacy_sources"] = _legacy_sources(state)
    if control["legacy_approaches"] is None:
        control["legacy_approaches"] = _legacy_approaches(state)
    _validate_charter_against_state(state, charter, activation=False, legacy_sources=control["legacy_sources"])
    need(charter["revision"] == 1 and charter["parent_sha256"] is None, "CHARTER", "a draft starts at revision 1 without a parent")
    revisions = control["charters"].setdefault(charter["charter_id"], [])
    need(not revisions, "DUPLICATE", "charter id already exists")
    revisions.append({"charter": copy.deepcopy(charter), "sha256": sha(packed(charter)), "event_id": event_id, "status": "draft"})


def _activate_charter(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    need(control["active_charter"] is None, "CHARTER", "a charter is already active; amend it instead")
    need(payload["campaign_id"] == _campaign_id(state) and payload["base_sha256"] == state.get("base_sha256"), "BASE", "activation identity differs from store")
    entry = _entry(control, payload["charter_id"], payload["charter_revision"])
    need(entry["sha256"] == payload["charter_sha256"], "CHARTER", "activation charter hash differs")
    _validate_charter_against_state(state, entry["charter"], activation=True, legacy_sources=control["legacy_sources"])
    entry["status"] = "active"
    control["active_charter"] = {"charter_id": payload["charter_id"], "revision": payload["charter_revision"], "sha256": payload["charter_sha256"], "event_id": event_id}
    state["execution_mode"] = "active"
    state["owner_contract"] = copy.deepcopy(control["active_charter"])


def _amend_charter(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    active = control["active_charter"]
    need(isinstance(active, dict), "INACTIVE", "cannot amend without an active charter")
    charter = payload["charter"]
    need(charter["charter_id"] == active["charter_id"], "CHARTER", "amendment charter id differs")
    need(charter["revision"] == active["revision"] + 1, "CHARTER", "amendment revision must advance exactly once")
    need(charter["parent_sha256"] == active["sha256"], "CHARTER", "amendment parent hash differs")
    _validate_charter_against_state(state, charter, activation=True, legacy_sources=control["legacy_sources"])
    prior = _entry(control, active["charter_id"], active["revision"])
    old_policy = prior["charter"]["stop_policy"]
    new_policy = charter["stop_policy"]
    if packed(old_policy["rules"]) == packed(new_policy["rules"]) and old_policy["policy_id"] == new_policy["policy_id"]:
        need(new_policy["revision"] == old_policy["revision"], "RULE", "unchanged stop policy must retain its revision")
    else:
        need(new_policy["policy_id"] == old_policy["policy_id"] and new_policy["revision"] == old_policy["revision"] + 1, "RULE", "changed stop policy must advance its revision")
    prior["status"] = "superseded"
    entry = {"charter": copy.deepcopy(charter), "sha256": sha(packed(charter)), "event_id": event_id, "status": "active"}
    control["charters"][active["charter_id"]].append(entry)
    control["active_charter"] = {"charter_id": charter["charter_id"], "revision": charter["revision"], "sha256": entry["sha256"], "event_id": event_id}
    control["pending_grant"] = None
    state["owner_contract"] = copy.deepcopy(control["active_charter"])


def _assessment_payload(value: Any) -> dict[str, Any]:
    exact(value, {"action", "assessment"})
    return {"action": validate_action(value["action"]), "assessment": validate_assessment(value["assessment"])}


def _pause_body(pause: dict[str, Any]) -> dict[str, Any]:
    return {key: copy.deepcopy(item) for key, item in pause.items() if key != "pause_sha256"}


def _rehash_pause(pause: dict[str, Any]) -> None:
    pause["pause_sha256"] = sha(packed(_pause_body(pause)))


def _new_pause(
    pause_id: str,
    origin: str,
    scope: str,
    scope_id: str | None,
    charter_revision: int,
    drain_targets: list[str],
    evaluated: dict[str, Any],
    event_id: str,
) -> dict[str, Any]:
    complete = not drain_targets
    pause = {
        "pause_id": pause_id,
        "origin": origin,
        "status": "active",
        "scope": scope,
        "scope_id": scope_id,
        "charter_revision": charter_revision,
        "created_event_id": event_id,
        "evaluated": copy.deepcopy(evaluated),
        "delivery": {"required": copy.deepcopy(drain_targets), "acknowledgements": {}, "state": "complete" if complete else "pending"},
        "actual_safe_state": {"required": copy.deepcopy(drain_targets), "acknowledgements": {}, "state": "reached" if complete else "unknown"},
        "resume": None,
    }
    _rehash_pause(pause)
    return pause


def _action_assessed(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    assessment_id = payload["assessment"]["assessment_id"]
    need(assessment_id not in control["assessments"], "DUPLICATE", "assessment id already exists")
    report = assess_action(state, payload["action"], payload["assessment"])
    report["event_id"] = event_id
    control["assessments"][assessment_id] = report
    if report["policy_result"] in {"pause", "too_late"} and report["materialize_pause"]:
        pause_id = identity(f"pause:{assessment_id}")
        pause = _new_pause(
            pause_id,
            "rule",
            report["effective_scope"],
            report["scope_id"],
            report["charter_revision"],
            payload["assessment"]["drain_targets"],
            {
                "assessment_id": assessment_id,
                "policy_result": report["policy_result"],
                "matched_rules": report["matched_rules"],
                "action_sha256": report["action_sha256"],
                "action_already_occurred": report["evaluated"]["action_already_occurred"],
            },
            event_id,
        )
        control["pauses"][pause_id] = pause
        _sync_active_pause(control)
        _sync_execution_mode(state, control)


def _owner_stop_payload(value: Any) -> dict[str, Any]:
    exact(value, {"pause_id", "campaign_id", "base_sha256", "charter_revision", "reason", "drain_targets"})
    need(type(value["charter_revision"]) is int and value["charter_revision"] > 0, "PAUSE", "invalid charter revision")
    return {
        "pause_id": identity(value["pause_id"]),
        "campaign_id": identity(value["campaign_id"]),
        "base_sha256": _hash(value["base_sha256"], "owner-stop base hash"),
        "charter_revision": value["charter_revision"],
        "reason": string(value["reason"], "owner-stop reason"),
        "drain_targets": _identities(value["drain_targets"], "drain targets"),
    }


def _owner_stop(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "cannot stop an inactive campaign")
    need(payload["campaign_id"] == policy["campaign_id"] and payload["base_sha256"] == policy["base_sha256"] and payload["charter_revision"] == policy["revision"], "STALE_POLICY", "owner stop identity differs")
    need(payload["drain_targets"] == _affected_active_jobs(state, "campaign", None), "DRAIN_CAPTURE", "owner stop must drain every persisted active job")
    need(
        not any(control["pauses"][pause_id]["scope"] == "campaign" for pause_id in control["active_pauses"]),
        "PAUSE",
        "a whole-campaign sticky pause is already active",
    )
    need(payload["pause_id"] not in control["pauses"], "DUPLICATE", "pause id already exists")
    pause = _new_pause(
        payload["pause_id"], "owner", "campaign", None, policy["revision"],
        payload["drain_targets"], {"policy_result": "owner_stop", "reason": payload["reason"]}, event_id,
    )
    control["pauses"][payload["pause_id"]] = pause
    _sync_active_pause(control)
    _sync_execution_mode(state, control)


def _delivery_payload(value: Any) -> dict[str, Any]:
    exact(value, {"pause_id", "pause_sha256", "subject_id", "state", "receipt_sha256"})
    need(value["state"] in {"delivered", "unreachable"}, "PAUSE", "invalid stop-delivery state")
    return {
        "pause_id": identity(value["pause_id"]),
        "pause_sha256": _hash(value["pause_sha256"], "pause hash"),
        "subject_id": identity(value["subject_id"]),
        "state": value["state"],
        "receipt_sha256": _hash(value["receipt_sha256"], "delivery receipt hash"),
    }


def _safe_payload(value: Any) -> dict[str, Any]:
    exact(value, {"pause_id", "pause_sha256", "run_id", "state", "receipt_sha256"})
    need(value["state"] in {"safe", "completed", "not_started", "unknown_effect"}, "PAUSE", "invalid actual-safe-state acknowledgement")
    return {
        "pause_id": identity(value["pause_id"]),
        "pause_sha256": _hash(value["pause_sha256"], "pause hash"),
        "run_id": identity(value["run_id"]),
        "state": value["state"],
        "receipt_sha256": _hash(value["receipt_sha256"], "safe-state receipt hash"),
    }


def _active_pause(control: dict[str, Any], pause_id: str, pause_sha256: str) -> dict[str, Any]:
    need(pause_id in control["active_pauses"], "PAUSE", "pause is not an active sticky pause")
    pause = control["pauses"].get(pause_id)
    need(isinstance(pause, dict) and pause["status"] == "active", "PAUSE", "pause is not active")
    need(pause["pause_sha256"] == pause_sha256 == sha(packed(_pause_body(pause))), "PAUSE", "pause hash differs")
    return pause


def _delivery_ack(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    pause = _active_pause(control, payload["pause_id"], payload["pause_sha256"])
    need(payload["subject_id"] in pause["delivery"]["required"], "PAUSE", "delivery subject was not captured for this pause")
    pause["delivery"]["acknowledgements"][payload["subject_id"]] = {"state": payload["state"], "receipt_sha256": payload["receipt_sha256"], "event_id": event_id}
    pause["delivery"]["state"] = "complete" if all(pause["delivery"]["acknowledgements"].get(key, {}).get("state") == "delivered" for key in pause["delivery"]["required"]) else "pending"
    _rehash_pause(pause)


def _safe_ack(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    pause = _active_pause(control, payload["pause_id"], payload["pause_sha256"])
    need(payload["run_id"] in pause["actual_safe_state"]["required"], "PAUSE", "run was not captured for this pause")
    pause["actual_safe_state"]["acknowledgements"][payload["run_id"]] = {"state": payload["state"], "receipt_sha256": payload["receipt_sha256"], "event_id": event_id}
    safe = {"safe", "completed", "not_started"}
    pause["actual_safe_state"]["state"] = "reached" if all(pause["actual_safe_state"]["acknowledgements"].get(key, {}).get("state") in safe for key in pause["actual_safe_state"]["required"]) else "unknown"
    _rehash_pause(pause)


def _resume_payload(value: Any) -> dict[str, Any]:
    exact(value, {"pause_id", "pause_sha256", "decision"})
    return {"pause_id": identity(value["pause_id"]), "pause_sha256": _hash(value["pause_sha256"], "pause hash"), "decision": string(value["decision"], "resume decision")}


def _resume(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    pause = _active_pause(control, payload["pause_id"], payload["pause_sha256"])
    need(pause["delivery"]["state"] == "complete", "PAUSE", "stop delivery is incomplete")
    need(pause["actual_safe_state"]["state"] == "reached", "PAUSE", "actual safe state is not established")
    pause["status"] = "resumed"
    pause["resume"] = {"decision": payload["decision"], "event_id": event_id}
    _rehash_pause(pause)
    control["pending_grant"] = None
    _sync_active_pause(control)
    _sync_execution_mode(state, control)


def _exception_payload(value: Any) -> dict[str, Any]:
    exact(value, {
        "exception_id", "pause_id", "pause_sha256", "action_id", "action_class",
        "payload_sha256", "source_captures_sha256", "charter_revision", "reason",
    })
    need(value["action_class"] in ACTION_CLASS_SET, "ACTION", "unknown exception action class")
    need(type(value["charter_revision"]) is int and value["charter_revision"] > 0, "ACTION", "invalid exception charter revision")
    return {
        "exception_id": identity(value["exception_id"]),
        "pause_id": identity(value["pause_id"]),
        "pause_sha256": _hash(value["pause_sha256"], "pause hash"),
        "action_id": identity(value["action_id"]),
        "action_class": value["action_class"],
        "payload_sha256": _hash(value["payload_sha256"], "exception payload hash"),
        "source_captures_sha256": _hash(value["source_captures_sha256"], "exception source-captures hash"),
        "charter_revision": value["charter_revision"],
        "reason": string(value["reason"], "exception reason"),
    }


def _grant_exception(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    pause = _active_pause(control, payload["pause_id"], payload["pause_sha256"])
    policy = active_policy(state)
    need(policy is not None and payload["charter_revision"] == policy["revision"] == pause["charter_revision"], "STALE_POLICY", "exception charter revision differs")
    need(payload["action_class"] in policy["allowed_actions"], "AUTHORIZATION", "exception action is not delegated")
    need(payload["exception_id"] not in control["exceptions"], "DUPLICATE", "exception id already exists")
    control["exceptions"][payload["exception_id"]] = {**copy.deepcopy(payload), "granted_event_id": event_id, "consumed": False, "consumed_event_id": None}


def _admission_payload(value: Any) -> dict[str, Any]:
    exact(value, {
        "admission_id", "action", "assessment_id", "exception_id", "command_kind",
        "product_event_id", "product_command_sha256",
    })
    return {
        "admission_id": identity(value["admission_id"]),
        "action": validate_action(value["action"]),
        "assessment_id": identity(value["assessment_id"]),
        "exception_id": _nullable_identity(value["exception_id"], "exception id"),
        "command_kind": identity(value["command_kind"]),
        "product_event_id": identity(value["product_event_id"]),
        "product_command_sha256": _hash(value["product_command_sha256"], "logical product-command hash"),
    }


def _admit_action(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    need(payload["admission_id"] not in control["admissions"], "DUPLICATE", "admission id already exists")
    report = control["assessments"].get(payload["assessment_id"])
    need(isinstance(report, dict), "ASSESSMENT", "action assessment is not recorded")
    action = payload["action"]
    need(report["action_sha256"] == sha(packed(action)) and report["action"] == action, "ASSESSMENT", "assessment belongs to another action")
    policy = active_policy(state)
    need(policy is not None and action["charter_revision"] == policy["revision"] and report["charter_sha256"] == policy["charter_sha256"], "STALE_POLICY", "assessment was invalidated by charter amendment")
    exception_id = payload["exception_id"]
    blocking_pause_ids = report["applicable_pause_ids"]
    if not blocking_pause_ids:
        need(report["eligible_for_admission"] and exception_id is None, "ASSESSMENT", "only an eligible current assessment admits an action")
        reserved_pause_id = None
    else:
        need(report["policy_result"] == "pause" and len(blocking_pause_ids) == 1 and exception_id is not None, "PAUSED", "each applicable sticky pause requires exact owner resolution")
        exception = control["exceptions"].get(exception_id)
        need(isinstance(exception, dict) and not exception["consumed"], "EXCEPTION", "exception is missing or already consumed")
        pause = control["pauses"][blocking_pause_ids[0]]
        need(exception["pause_id"] == pause["pause_id"] and exception["pause_sha256"] == pause["pause_sha256"], "EXCEPTION", "exception pause binding differs")
        need(exception["charter_revision"] == policy["revision"], "STALE_POLICY", "exception charter revision differs")
        need(exception["action_id"] == action["action_id"] and exception["action_class"] == action["action_class"] and exception["payload_sha256"] == action["payload_sha256"], "EXCEPTION", "exception action binding differs")
        need(exception["source_captures_sha256"] == sha(packed(action["source_captures"])), "EXCEPTION", "exception source captures differ")
        exception["consumed"] = True
        exception["consumed_event_id"] = event_id
        reserved_pause_id = pause["pause_id"]
    admission = {
        "admission_id": payload["admission_id"],
        "action_sha256": sha(packed(action)),
        "action": copy.deepcopy(action),
        "action_class": action["action_class"],
        "assessment_id": payload["assessment_id"],
        "exception_id": exception_id,
        "command_kind": payload["command_kind"],
        "product_event_id": payload["product_event_id"],
        "product_command_sha256": payload["product_command_sha256"],
        "charter_sha256": policy["charter_sha256"],
        "pause_id": reserved_pause_id,
        "pause_sha256": control["pauses"][reserved_pause_id]["pause_sha256"] if reserved_pause_id else None,
        "event_id": event_id,
        "for_revision": state["revision"] + 1,
    }
    control["admissions"][payload["admission_id"]] = admission
    control["pending_grant"] = copy.deepcopy(admission)


def _rebind_payload(value: Any) -> dict[str, Any]:
    exact(value, {"admission_id", "assessment_id", "product_event_id", "product_command_sha256"})
    return {
        "admission_id": identity(value["admission_id"]),
        "assessment_id": identity(value["assessment_id"]),
        "product_event_id": identity(value["product_event_id"]),
        "product_command_sha256": _hash(value["product_command_sha256"], "logical product-command hash"),
    }


def _rebind_reservation(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    admission = control["admissions"].get(payload["admission_id"])
    need(isinstance(admission, dict), "AUTHORIZATION", "exact action reservation does not exist")
    need(
        admission["product_event_id"] == payload["product_event_id"]
        and admission["product_command_sha256"] == payload["product_command_sha256"],
        "AUTHORIZATION",
        "action reservation binding differs",
    )
    report = control["assessments"].get(payload["assessment_id"])
    need(
        isinstance(report, dict)
        and report["action_sha256"] == admission["action_sha256"],
        "ASSESSMENT",
        "reservation rebind needs a fresh assessment of the same exact action",
    )
    policy = active_policy(state)
    need(
        policy is not None
        and admission["action"]["charter_revision"] == policy["revision"]
        and admission["charter_sha256"] == policy["charter_sha256"],
        "STALE_POLICY",
        "action reservation was invalidated by charter amendment",
    )
    applicable = pause_applies(
        state,
        branch_id=admission["action"]["branch_id"],
        run_id=admission["action"]["run_id"],
    )
    applicable_ids = [row["pause_id"] for row in applicable]
    if admission["pause_id"] is not None:
        need(applicable_ids == [admission["pause_id"]], "PAUSED", "reserved action pause set changed")
        pause = control["pauses"].get(admission["pause_id"])
        need(
            isinstance(pause, dict) and pause["status"] == "active"
            and pause["pause_sha256"] == admission["pause_sha256"],
            "PAUSED",
            "reserved exception pause binding changed",
        )
        need(report["policy_result"] == "pause", "ASSESSMENT", "reserved exception no longer matches the current pause assessment")
    else:
        need(not applicable_ids and report["eligible_for_admission"], "ASSESSMENT", "reserved action is no longer eligible")
    grant = copy.deepcopy(admission)
    grant["assessment_id"] = payload["assessment_id"]
    grant["rebound_event_id"] = event_id
    grant["for_revision"] = state["revision"] + 1
    control["pending_grant"] = grant


def _approach_payload(value: Any) -> dict[str, Any]:
    exact(value, {"record_id", "problem_id", "approach_id", "strategy_sha256", "outcome", "evidence_refs"})
    need(value["outcome"] in {"failed", "succeeded", "inconclusive", "provider_error", "retry"}, "APPROACH", "unknown approach outcome")
    evidence_refs = _identities(value["evidence_refs"], "approach evidence refs")
    need(value["outcome"] not in {"failed", "succeeded"} or evidence_refs, "APPROACH", "semantic approach outcome needs evidence")
    return {
        "record_id": identity(value["record_id"]),
        "problem_id": identity(value["problem_id"]),
        "approach_id": identity(value["approach_id"]),
        "strategy_sha256": _hash(value["strategy_sha256"], "approach strategy hash"),
        "outcome": value["outcome"],
        "evidence_refs": evidence_refs,
    }


def _record_approach(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    need(not any(row["record_id"] == payload["record_id"] for row in control["approach_history"]), "DUPLICATE", "approach outcome record id exists")
    known_problems = {row["id"] for row in state.get("plan", {}).get("node", []) if isinstance(row, dict) and isinstance(row.get("id"), str)}
    need(payload["problem_id"] in known_problems, "REFERENCE", "approach problem must name a known stable plan node")
    missing = sorted(set(payload["evidence_refs"]) - set(state.get("evidence", {})))
    need(not missing, "REFERENCE", f"unknown approach evidence: {', '.join(missing)}")
    if payload["outcome"] in {"failed", "succeeded"}:
        need(
            any(state["evidence"][key].get("result") in {"observed_pass", "observed_fail"} for key in payload["evidence_refs"]),
            "APPROACH",
            "semantic approach outcome needs usable observed evidence",
        )
        need(not any(row["problem_id"] == payload["problem_id"] and row["approach_id"] == payload["approach_id"] and row["outcome"] in {"failed", "succeeded"} for row in control["approach_history"]), "APPROACH", "semantic outcome already recorded for approach")
    control["approach_history"].append({
        **copy.deepcopy(payload),
        "epoch": control["approach_epochs"].get(payload["problem_id"], 0),
        "event_id": event_id,
    })


def _epoch_payload(value: Any) -> dict[str, Any]:
    exact(value, {"problem_id", "expected_epoch", "new_epoch", "reason"})
    need(type(value["expected_epoch"]) is int and value["expected_epoch"] >= 0, "APPROACH", "expected epoch must be nonnegative")
    need(type(value["new_epoch"]) is int and value["new_epoch"] > 0, "APPROACH", "new epoch must be positive")
    return {
        "problem_id": identity(value["problem_id"]),
        "expected_epoch": value["expected_epoch"],
        "new_epoch": value["new_epoch"],
        "reason": string(value["reason"], "approach epoch reason"),
    }


def _advance_epoch(state: State, payload: dict[str, Any], event_id: str) -> None:
    control = _control_mut(state)
    current = control["approach_epochs"].get(payload["problem_id"], 0)
    need(payload["expected_epoch"] == current and payload["new_epoch"] == current + 1, "STALE", "approach epoch must advance the exact current epoch")
    control["approach_epochs"][payload["problem_id"]] = payload["new_epoch"]
    control["epoch_history"].append({**copy.deepcopy(payload), "event_id": event_id})


CONTROL_HANDLERS: Mapping[str, HandlerSpec] = MappingProxyType({
    spec.kind: spec for spec in (
        HandlerSpec("control.charter-drafted", _charter_payload, _draft_charter),
        HandlerSpec("control.charter-activated", _activation_payload, _activate_charter),
        HandlerSpec("control.charter-amended", _charter_payload, _amend_charter),
        HandlerSpec("control.action-assessed", _assessment_payload, _action_assessed),
        HandlerSpec("control.action-admitted", _admission_payload, _admit_action),
        HandlerSpec("control.action-reservation-rebound", _rebind_payload, _rebind_reservation),
        HandlerSpec("control.owner-stop-requested", _owner_stop_payload, _owner_stop),
        HandlerSpec("control.pause-delivery-acknowledged", _delivery_payload, _delivery_ack),
        HandlerSpec("control.pause-safe-state-acknowledged", _safe_payload, _safe_ack),
        HandlerSpec("control.pause-resumed", _resume_payload, _resume),
        HandlerSpec("control.action-exception-granted", _exception_payload, _grant_exception),
        HandlerSpec("control.approach-outcome-recorded", _approach_payload, _record_approach),
        HandlerSpec("control.approach-epoch-advanced", _epoch_payload, _advance_epoch),
    )
})
