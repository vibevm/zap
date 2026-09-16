"""Control data validation, active-policy views, and pure stop assessment."""
from __future__ import annotations

import copy
import re
from typing import Any

from .common import Refusal, exact, identity, need, packed, sha, string, strings
from .records import State

CONTROL_SCHEMA = "zap-control/1"
CHARTER_SCHEMA = "zap-charter/1"
STOP_POLICY_SCHEMA = "zap-stop-policy/1"
ACTION_SCHEMA = "zap-action/1"
ASSESSMENT_SCHEMA = "zap-assessment/1"

ACTION_CLASSES = (
    "adaptive.apply",
    "campaign.close",
    "evidence.adjudicate",
    "fact.promote",
    "outcome.adopt",
    "plan.lower",
    "stage.accept",
    "task.update",
    "verification.run",
    "work.accept",
    "work.dispatch",
)
ACTION_CLASS_SET = frozenset(ACTION_CLASSES)
DISPOSITIONS = frozenset({"retained", "replaced", "excluded", "unattainable"})
LEGACY_DISPOSITIONS = frozenset({"retained", "owner_decision_required", "superseded"})
HASH = re.compile(r"^[0-9a-f]{64}$")

OWNER_EVENT_KINDS = frozenset({
    "control.charter-activated",
    "control.charter-amended",
    "control.owner-stop-requested",
    "control.pause-resumed",
    "control.action-exception-granted",
    "control.approach-epoch-advanced",
})
COORDINATOR_EVENT_KINDS = frozenset({
    "control.action-assessed",
    "control.pause-delivery-acknowledged",
    "control.pause-safe-state-acknowledged",
    "control.approach-outcome-recorded",
})
INTERNAL_EVENT_KINDS = frozenset({
    "control.action-admitted",
    "control.action-reservation-rebound",
})
DATA_EVENT_KINDS = frozenset({"control.charter-drafted"})
PRIVILEGED_EVENT_KINDS = OWNER_EVENT_KINDS | COORDINATOR_EVENT_KINDS | INTERNAL_EVENT_KINDS


def _empty_control() -> dict[str, Any]:
    result = {
        "schema": CONTROL_SCHEMA,
        "charters": {},
        "legacy_sources": None,
        "legacy_approaches": None,
        "active_charter": None,
        "assessments": {},
        "pauses": {},
        "active_pause": None,
        "active_pauses": [],
        "exceptions": {},
        "approach_epochs": {},
        "approach_history": [],
        "epoch_history": [],
        "admissions": {},
        "pending_grant": None,
    }
    return result


def _control_mut(state: State) -> dict[str, Any]:
    extensions = state.setdefault("extensions", {})
    need(isinstance(extensions, dict), "CONTROL_STATE", "extensions must be a mapping")
    control = extensions.setdefault("control", _empty_control())
    need(isinstance(control, dict) and control.get("schema") == CONTROL_SCHEMA, "CONTROL_STATE", "unknown control projection")
    if "active_pauses" not in control:
        control["active_pauses"] = [control["active_pause"]] if control.get("active_pause") else []
    capabilities = state.setdefault("capabilities", {})
    need(isinstance(capabilities, dict), "CONTROL_STATE", "capabilities must be a mapping")
    capabilities["zap.control"] = 1
    return control


def control_state(state: State) -> dict[str, Any]:
    """Return a detached control projection without lazily mutating ``state``."""
    extensions = state.get("extensions", {})
    need(isinstance(extensions, dict), "CONTROL_STATE", "extensions must be a mapping")
    raw = extensions.get("control")
    if raw is None:
        return _empty_control()
    need(isinstance(raw, dict) and raw.get("schema") == CONTROL_SCHEMA, "CONTROL_STATE", "unknown control projection")
    result = copy.deepcopy(raw)
    if "active_pauses" not in result:
        result["active_pauses"] = [result["active_pause"]] if result.get("active_pause") else []
    return result


def _sync_active_pause(control: dict[str, Any]) -> None:
    active = sorted(
        pause_id for pause_id, pause in control["pauses"].items()
        if isinstance(pause, dict) and pause.get("status") == "active"
    )
    control["active_pauses"] = active
    campaign = next((pause_id for pause_id in active if control["pauses"][pause_id]["scope"] == "campaign"), None)
    control["active_pause"] = campaign or (active[0] if active else None)


def _sync_execution_mode(state: State, control: dict[str, Any]) -> None:
    has_campaign_pause = any(control["pauses"][pause_id]["scope"] == "campaign" for pause_id in control["active_pauses"])
    state["execution_mode"] = "paused" if has_campaign_pause else "active"


def pause_applies(
    state: State,
    *,
    branch_id: str | None = None,
    run_id: str | None = None,
) -> list[dict[str, Any]]:
    """Return detached active pauses that affect the named execution scope."""
    control = control_state(state)
    result = []
    for pause_id in control["active_pauses"]:
        pause = control["pauses"].get(pause_id)
        if not isinstance(pause, dict) or pause.get("status") != "active":
            continue
        scope = pause["scope"]
        applies = (
            scope == "campaign"
            or (scope == "branch" and branch_id is not None and pause["scope_id"] == branch_id)
            or (scope == "run" and run_id is not None and pause["scope_id"] == run_id)
        )
        if applies:
            result.append(copy.deepcopy(pause))
    return sorted(result, key=lambda row: ({"campaign": 0, "branch": 1, "run": 2}[row["scope"]], row["pause_id"]))


def _hash(value: Any, label: str) -> str:
    need(isinstance(value, str) and HASH.fullmatch(value) is not None, "HASH", f"invalid {label}")
    return value


def _identities(value: Any, label: str, *, nullable: bool = False) -> list[str]:
    if nullable and value is None:
        return []
    result = strings(value, label)
    for item in result:
        identity(item)
    need(result == sorted(set(result)), "ORDER", f"{label} must be sorted and unique")
    return result


def _nullable_identity(value: Any, label: str) -> str | None:
    if value is None:
        return None
    return identity(string(value, label))


def _campaign_id(state: State) -> str:
    plan = state.get("plan")
    need(isinstance(plan, dict), "CONTROL_STATE", "state has no plan")
    return identity(plan.get("plan_id"))


def _validate_expression(expression: Any) -> dict[str, Any]:
    need(isinstance(expression, dict) and len(expression) == 1, "RULE", "one stop expression operator required")
    operator, argument = next(iter(expression.items()))
    if operator in {"all", "any"}:
        need(isinstance(argument, list) and argument, "RULE", "logical operands must be nonempty")
        return {operator: [_validate_expression(item) for item in argument]}
    if operator == "not":
        return {operator: _validate_expression(argument)}
    if operator == "eq":
        exact(argument, {"field", "value"})
        field = identity(argument["field"])
        value = argument["value"]
        need(value is not None and type(value) in {str, bool, int, float}, "RULE", "comparison needs a concrete scalar")
        return {operator: {"field": field, "value": value}}
    if operator == "failed_approaches":
        exact(argument, {"gte"})
        need(type(argument["gte"]) is int and argument["gte"] > 0, "RULE", "approach threshold must be positive")
        return {operator: {"gte": argument["gte"]}}
    raise Refusal("RULE", f"unsupported stop expression operator {operator!r}")


def _validate_stop_policy(value: Any) -> dict[str, Any]:
    exact(value, {"schema", "policy_id", "revision", "rules"})
    need(value["schema"] == STOP_POLICY_SCHEMA, "RULE", "unknown active stop-policy schema")
    policy_id = identity(value["policy_id"])
    revision = value["revision"]
    need(type(revision) is int and revision > 0, "RULE", "stop-policy revision must be positive")
    need(isinstance(value["rules"], list), "RULE", "stop-policy rules must be a list")
    rules: list[dict[str, Any]] = []
    seen: set[str] = set()
    for raw in value["rules"]:
        exact(raw, {"id", "applies_to_actions", "scope", "timing", "when"})
        rule_id = identity(raw["id"])
        need(rule_id not in seen, "DUPLICATE", f"duplicate stop rule {rule_id}")
        seen.add(rule_id)
        actions = strings(raw["applies_to_actions"], "rule actions")
        need(actions and actions == sorted(set(actions)) and set(actions) <= ACTION_CLASS_SET, "RULE", "rule actions must be sorted supported action classes")
        need(raw["scope"] in {"campaign", "branch", "run"}, "RULE", "unsupported stop scope")
        need(raw["timing"] in {"before_action", "before_next_action"}, "RULE", "unsupported stop timing")
        rules.append({
            "id": rule_id,
            "applies_to_actions": actions,
            "scope": raw["scope"],
            "timing": raw["timing"],
            "when": _validate_expression(raw["when"]),
        })
    need([rule["id"] for rule in rules] == sorted(seen), "ORDER", "stop rules must be sorted by id")
    return {"schema": STOP_POLICY_SCHEMA, "policy_id": policy_id, "revision": revision, "rules": rules}


def convert_legacy_stop_policy(
    value: Any,
    *,
    policy_id: str,
    revision: int,
    applies_to_actions: list[str],
) -> dict[str, Any]:
    """Prepare an explicit owner-reviewable policy from a ``zap-stop/1`` probe.

    The legacy schema used ``scope = run`` for a response that stopped the
    whole campaign. Conversion therefore maps that wire value to ``campaign``;
    silently narrowing it to one job would change the accepted policy.
    """
    exact(value, {"schema", "status", "rules"})
    need(value["schema"] == "zap-stop/1" and value["status"] == "example_unapproved", "RULE", "input is not a legacy unapproved stop-policy probe")
    identity(policy_id)
    need(type(revision) is int and revision > 0, "RULE", "stop-policy revision must be positive")
    need(applies_to_actions == sorted(set(applies_to_actions)) and applies_to_actions and set(applies_to_actions) <= ACTION_CLASS_SET, "RULE", "converted rule actions must be sorted supported classes")
    need(isinstance(value["rules"], list), "RULE", "legacy rules must be a list")
    converted = []
    for raw in value["rules"]:
        exact(raw, {"id", "when", "scope", "timing"})
        need(raw["scope"] == "run", "RULE", "unsupported legacy stop scope")
        converted.append({
            "id": identity(raw["id"]),
            "applies_to_actions": copy.deepcopy(applies_to_actions),
            "scope": "campaign",
            "timing": raw["timing"],
            "when": _validate_expression(raw["when"]),
        })
    converted.sort(key=lambda row: row["id"])
    return _validate_stop_policy({"schema": STOP_POLICY_SCHEMA, "policy_id": policy_id, "revision": revision, "rules": converted})


def _validate_charter(value: Any) -> dict[str, Any]:
    exact(value, {
        "schema", "charter_id", "campaign_id", "base_sha256", "revision",
        "parent_sha256", "intent", "expected_outcome", "delegation",
        "legacy_authority", "stop_policy",
    }, {"intent_binding"})
    need(value["schema"] == CHARTER_SCHEMA, "CHARTER", "unknown charter schema")
    charter_id = identity(value["charter_id"])
    campaign_id = identity(value["campaign_id"])
    base_sha256 = _hash(value["base_sha256"], "charter base hash")
    revision = value["revision"]
    need(type(revision) is int and revision > 0, "CHARTER", "charter revision must be positive")
    parent_sha256 = value["parent_sha256"]
    if parent_sha256 is not None:
        _hash(parent_sha256, "parent charter hash")
    intent = string(value["intent"], "charter intent")
    intent_binding = value.get("intent_binding")
    if intent_binding is not None:
        exact(intent_binding, {"intent_id", "sha256"})
        intent_binding = {
            "intent_id": identity(intent_binding["intent_id"]),
            "sha256": _hash(intent_binding["sha256"], "intent proposal hash"),
        }

    outcome = value["expected_outcome"]
    exact(outcome, {"outcome_id", "summary"})
    expected_outcome = {"outcome_id": identity(outcome["outcome_id"]), "summary": string(outcome["summary"], "expected outcome summary")}

    delegation = value["delegation"]
    exact(delegation, {"allowed_actions", "adaptation"})
    allowed_actions = strings(delegation["allowed_actions"], "allowed actions")
    need(allowed_actions == sorted(set(allowed_actions)) and set(allowed_actions) <= ACTION_CLASS_SET, "CHARTER", "allowed actions must be sorted supported action classes")
    adaptation = delegation["adaptation"]
    exact(adaptation, {"allow_target_revision", "mutable_obligations", "essential_obligations", "allowed_dispositions"})
    need(type(adaptation["allow_target_revision"]) is bool, "CHARTER", "allow_target_revision must be boolean")
    mutable = _identities(adaptation["mutable_obligations"], "mutable obligations")
    essential = _identities(adaptation["essential_obligations"], "essential obligations")
    dispositions = strings(adaptation["allowed_dispositions"], "allowed dispositions")
    need(dispositions == sorted(set(dispositions)) and set(dispositions) <= DISPOSITIONS, "CHARTER", "invalid allowed dispositions")

    need(isinstance(value["legacy_authority"], list), "CHARTER", "legacy authority must be a list")
    legacy: list[dict[str, Any]] = []
    legacy_ids: set[str] = set()
    for raw in value["legacy_authority"]:
        exact(raw, {"id", "disposition", "source_sha256", "replacement_ref"})
        key = identity(raw["id"])
        need(key not in legacy_ids, "DUPLICATE", f"duplicate legacy classification {key}")
        legacy_ids.add(key)
        disposition = raw["disposition"]
        need(disposition in LEGACY_DISPOSITIONS, "CHARTER", "invalid legacy authority disposition")
        source_sha256 = _hash(raw["source_sha256"], "legacy source hash")
        replacement = _nullable_identity(raw["replacement_ref"], "legacy replacement reference")
        need(disposition == "superseded" or replacement is None, "CHARTER", "only superseded legacy authority has a replacement")
        need(disposition != "superseded" or replacement is not None, "CHARTER", "superseded legacy authority needs a replacement")
        legacy.append({"id": key, "disposition": disposition, "source_sha256": source_sha256, "replacement_ref": replacement})
    need([row["id"] for row in legacy] == sorted(legacy_ids), "ORDER", "legacy classifications must be sorted by id")

    result = {
        "schema": CHARTER_SCHEMA,
        "charter_id": charter_id,
        "campaign_id": campaign_id,
        "base_sha256": base_sha256,
        "revision": revision,
        "parent_sha256": parent_sha256,
        "intent": intent,
        "expected_outcome": expected_outcome,
        "delegation": {
            "allowed_actions": allowed_actions,
            "adaptation": {
                "allow_target_revision": adaptation["allow_target_revision"],
                "mutable_obligations": mutable,
                "essential_obligations": essential,
                "allowed_dispositions": dispositions,
            },
        },
        "legacy_authority": legacy,
        "stop_policy": _validate_stop_policy(value["stop_policy"]),
    }
    if "intent_binding" in value:
        result["intent_binding"] = intent_binding
    return result


def _legacy_sources(state: State) -> dict[str, str]:
    plan = state.get("plan", {})
    mandates = plan.get("mandate", []) if isinstance(plan, dict) else []
    need(isinstance(mandates, list), "CHARTER", "plan mandate collection is invalid")
    result: dict[str, str] = {}
    for mandate in mandates:
        need(isinstance(mandate, dict), "CHARTER", "legacy mandate is invalid")
        key = identity(mandate.get("id"))
        need(key not in result, "DUPLICATE", f"duplicate legacy mandate {key}")
        result[key] = sha(packed(mandate))
    return result


def _legacy_approaches(state: State) -> dict[str, Any]:
    approaches = state.get("approaches", {})
    need(isinstance(approaches, dict), "APPROACH", "legacy approach projection must be a mapping")
    return copy.deepcopy(approaches)


def _validate_charter_against_state(
    state: State,
    charter: dict[str, Any],
    *,
    activation: bool,
    legacy_sources: dict[str, str] | None = None,
) -> None:
    need(charter["campaign_id"] == _campaign_id(state), "CAMPAIGN", "charter campaign differs from store")
    need(charter["base_sha256"] == state.get("base_sha256"), "BASE", "charter base differs from store")
    expected = _legacy_sources(state) if legacy_sources is None else legacy_sources
    actual = {row["id"]: row for row in charter["legacy_authority"]}
    need(set(actual) == set(expected), "LEGACY_AUTHORITY", "legacy authority classification is incomplete or foreign")
    for key, source_sha256 in expected.items():
        need(actual[key]["source_sha256"] == source_sha256, "LEGACY_AUTHORITY", f"legacy source capture differs for {key}")
    if activation:
        unresolved = sorted(key for key, row in actual.items() if row["disposition"] == "owner_decision_required")
        need(not unresolved, "LEGACY_AUTHORITY", f"legacy authority still requires owner decision: {', '.join(unresolved)}")


def _charter_payload(value: Any) -> dict[str, Any]:
    exact(value, {"charter"})
    return {"charter": _validate_charter(value["charter"])}


def _activation_payload(value: Any) -> dict[str, Any]:
    exact(value, {"charter_id", "charter_revision", "charter_sha256", "campaign_id", "base_sha256"})
    need(type(value["charter_revision"]) is int and value["charter_revision"] > 0, "CHARTER", "invalid charter revision")
    return {
        "charter_id": identity(value["charter_id"]),
        "charter_revision": value["charter_revision"],
        "charter_sha256": _hash(value["charter_sha256"], "charter hash"),
        "campaign_id": identity(value["campaign_id"]),
        "base_sha256": _hash(value["base_sha256"], "activation base hash"),
    }


def _entry(control: dict[str, Any], charter_id: str, revision: int) -> dict[str, Any]:
    revisions = control["charters"].get(charter_id, [])
    found = next((row for row in revisions if row["charter"]["revision"] == revision), None)
    need(found is not None, "CHARTER", "referenced charter revision does not exist")
    return found


def _active_entry(state: State) -> tuple[dict[str, Any], dict[str, Any]]:
    control = control_state(state)
    active = control["active_charter"]
    need(isinstance(active, dict), "INACTIVE", "no owner charter is active")
    entry = _entry(control, active["charter_id"], active["revision"])
    need(entry["sha256"] == active["sha256"] == sha(packed(entry["charter"])), "CONTROL_STATE", "active charter identity differs")
    return control, entry


def active_policy(state: State) -> dict[str, Any] | None:
    control = control_state(state)
    active = control["active_charter"]
    if active is None:
        return None
    entry = _entry(control, active["charter_id"], active["revision"])
    charter = entry["charter"]
    need(entry["sha256"] == active["sha256"] == sha(packed(charter)), "CONTROL_STATE", "active charter identity differs")
    return {
        "campaign_id": charter["campaign_id"],
        "base_sha256": charter["base_sha256"],
        "revision": charter["revision"],
        "allowed_actions": copy.deepcopy(charter["delegation"]["allowed_actions"]),
        "adaptation": copy.deepcopy(charter["delegation"]["adaptation"]),
        "charter_id": charter["charter_id"],
        "charter_sha256": entry["sha256"],
        "intent_binding": copy.deepcopy(charter.get("intent_binding")),
        "stop_policy": copy.deepcopy(charter["stop_policy"]),
        "pause": copy.deepcopy(control["pauses"].get(control["active_pause"])) if control["active_pause"] else None,
        "pauses": [copy.deepcopy(control["pauses"][pause_id]) for pause_id in control["active_pauses"]],
    }


def active_charter(state: State) -> dict[str, Any] | None:
    """Return the exact active charter and its content hash."""
    policy = active_policy(state)
    if policy is None:
        return None
    control = control_state(state)
    entry = _entry(control, policy["charter_id"], policy["revision"])
    return {"charter": copy.deepcopy(entry["charter"]), "sha256": entry["sha256"]}


def require_action(
    state: State,
    action_class: str,
    *,
    branch_id: str | None = None,
    run_id: str | None = None,
) -> dict[str, Any]:
    need(action_class in ACTION_CLASS_SET, "ACTION", "unknown delegated action class")
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "no owner charter is active")
    need(action_class in policy["allowed_actions"], "AUTHORIZATION", f"charter does not delegate {action_class}")
    control = control_state(state)
    blocking = pause_applies(state, branch_id=branch_id, run_id=run_id)
    if blocking:
        grant = control.get("pending_grant")
        allowed_by_grant = (
            isinstance(grant, dict)
            and grant.get("action_class") == action_class
            and grant.get("for_revision") == state.get("revision")
            and (branch_id is None or grant.get("action", {}).get("branch_id") == branch_id)
            and (run_id is None or grant.get("action", {}).get("run_id") == run_id)
        )
        need(allowed_by_grant, "PAUSED", f"action scope has sticky pause {blocking[0]['pause_id']}")
    return policy
