"""Shared pure model helpers for the ZAP adaptive domain extension."""
from __future__ import annotations

import copy
from collections.abc import Callable
from typing import Any

from .common import Refusal, exact, identity, need, packed, sha, string, strings
from .records import HandlerSpec

DOMAIN_SCHEMA = "zap-domain/1"
HEX64 = frozenset("0123456789abcdef")
WORK_STATES = {
    "planned", "ready", "active", "candidate", "accepted", "blocked",
    "deferred", "dropped", "superseded",
}
STAGES = {"prototype", "functional", "productized"}
OBLIGATION_DISPOSITIONS = {"retained", "replaced", "excluded", "unattainable"}
OWNERSHIP_ROLES = {"implementation", "verification", "integration", "acceptance"}


def strict(schema: str, required: set[str], optional: set[str] = frozenset()):
    """Build an exact, versioned, detached payload validator."""
    required = set(required) | {"schema"}
    optional = set(optional)

    def validate(value: Any) -> dict[str, Any]:
        exact(value, required, optional)
        need(value["schema"] == schema, "DOMAIN_SCHEMA", f"expected {schema}")
        return copy.deepcopy(value)

    return validate


def mapping(value: Any, label: str) -> dict[str, Any]:
    need(isinstance(value, dict), "DOMAIN_VALUE", f"invalid {label}")
    return value


def rows(value: Any, label: str, *, nonempty: bool = False) -> list[dict[str, Any]]:
    need(isinstance(value, list) and (value or not nonempty), "DOMAIN_VALUE", f"invalid {label}")
    need(all(isinstance(row, dict) for row in value), "DOMAIN_VALUE", f"invalid {label} row")
    return value


def boolean(value: Any, label: str) -> bool:
    need(type(value) is bool, "DOMAIN_VALUE", f"invalid {label}")
    return value


def integer(value: Any, label: str, *, minimum: int = 0) -> int:
    need(type(value) is int and value >= minimum, "DOMAIN_VALUE", f"invalid {label}")
    return value


def digest(value: Any, label: str = "sha256") -> str:
    need(isinstance(value, str) and len(value) == 64 and set(value) <= HEX64,
         "DOMAIN_VALUE", f"invalid {label}")
    return value


def optional_identity(value: Any, label: str) -> str | None:
    if value is None:
        return None
    try:
        return identity(value)
    except Refusal as exc:
        raise Refusal(exc.code, f"invalid {label}") from exc


def unique_ids(values: Any, label: str, *, nonempty: bool = False) -> list[str]:
    values = strings(values, label)
    need(values or not nonempty, "DOMAIN_VALUE", f"empty {label}")
    for value in values:
        identity(value)
    need(len(values) == len(set(values)), "DUPLICATE", f"duplicate {label}")
    return values


def nonblank_list(values: Any, label: str, *, nonempty: bool = False) -> list[str]:
    values = strings(values, label)
    need(values or not nonempty, "DOMAIN_VALUE", f"empty {label}")
    need(all(value.strip() for value in values), "DOMAIN_VALUE", f"blank {label}")
    return values


def content_hash(value: Any) -> str:
    return sha(packed(value))


def require_refs(values: Any, available: set[str], label: str, *, nonempty: bool = False) -> list[str]:
    values = unique_ids(values, label, nonempty=nonempty)
    need(set(values) <= set(available), "REFERENCE", f"unknown {label}")
    return values


def active_obligations(domain: dict[str, Any]) -> set[str]:
    return {key for key, row in domain["obligations"].items() if row["status"] == "active"}


def owned_obligations(domain: dict[str, Any], work_id: str, *, active_only: bool = True,
                      roles: set[str] | None = None) -> set[str]:
    result = set()
    for relation in domain["ownership"]:
        if relation["work_id"] != work_id or (roles is not None and relation["role"] not in roles):
            continue
        key = relation["obligation_id"]
        if not active_only or domain["obligations"][key]["status"] == "active":
            result.add(key)
    return result


def _legacy_obligation(key: str, kind: str, statement: str, state: dict[str, Any],
                       owner_ids: list[str]) -> dict[str, Any]:
    return {
        "id": key,
        "kind": kind,
        "statement": statement,
        "status": "active",
        "created_for_outcome": None,
        "current_outcome_ids": [],
        "essential_declared": None,
        "source": {
            "kind": "mup_import",
            "plan_id": state["plan"]["plan_id"],
            "plan_revision": state["plan"]["revision"],
            "base_sha256": state["base_sha256"],
        },
        "owner_work_ids": sorted(set(owner_ids)),
        "dispositions": [],
        "unmet_portion": statement,
    }


def initial_domain(state: dict[str, Any]) -> dict[str, Any]:
    """Derive the lossless lazy domain projection for a legacy zap/1 base."""
    plan = state["plan"]
    obligations: dict[str, dict[str, Any]] = {}
    ownership: list[dict[str, Any]] = []
    for mandate in plan["mandate"]:
        key = identity(mandate["id"])
        owners = list(mandate.get("nodes", []))
        obligations[key] = _legacy_obligation(key, "legacy_mandate", mandate["text"], state, owners)
        for work_id in owners:
            ownership.append({
                "type": "obligation.owned_by", "obligation_id": key,
                "work_id": work_id, "role": "implementation", "outcome_id": None,
            })
    legacy_acceptance: dict[str, Any] = {}
    for node in plan["node"]:
        node_id = identity(node["id"])
        for index, criterion in enumerate(node.get("acceptance", [])):
            key = f"acceptance:{node_id}:{index}"
            need(key not in obligations, "DUPLICATE", f"derived obligation collision: {key}")
            obligations[key] = _legacy_obligation(key, "legacy_acceptance", criterion, state, [node_id])
            ownership.append({
                "type": "obligation.owned_by", "obligation_id": key,
                "work_id": node_id, "role": "acceptance", "outcome_id": None,
            })
        if node["state"] == "accepted":
            legacy_acceptance[node_id] = {
                "source": "mup_import", "state": "accepted",
                "evidence_refs": list(node.get("evidence", [])),
                "adjudication": "legacy_assertion", "base_sha256": state["base_sha256"],
            }
    task_history = {}
    for node_id, contract in state.get("task_contracts", {}).items():
        task_history[node_id] = {
            "active_version": 0,
            "versions": [{
                "version": 0, "schema": "mup-task-contract/imported",
                "sha256": sha(packed(contract)), "source": "mup_import",
                "contract": copy.deepcopy(contract),
            }],
        }
    return {
        "schema": DOMAIN_SCHEMA,
        "revision": 0,
        "intents": {}, "active_intent_id": None,
        "outcome_revisions": {}, "active_outcome_id": None, "original_outcome_id": None,
        "obligations": obligations, "ownership": ownership,
        "work_nodes": {}, "work_updates": {}, "work_successors": {},
        "task_contracts": task_history,
        "validation_generations": {}, "revalidation_history": [],
        "reviews": {}, "last_applied_review_id": None,
        "evidence_adjudications": {}, "stages": {}, "deferrals": {},
        "acceptances": {}, "integration_acceptances": {},
        "reuse_witnesses": {"evidence": {}, "stages": {}, "acceptances": {}, "integrations": {}},
        "promotions": {}, "legacy_acceptance": legacy_acceptance,
        "closure": None,
    }


def _validate_domain(domain: Any) -> dict[str, Any]:
    need(isinstance(domain, dict) and domain.get("schema") == DOMAIN_SCHEMA,
         "DOMAIN_STORE", "unknown domain projection")
    return domain


def mutable_domain(state: dict[str, Any]) -> dict[str, Any]:
    extensions = state.setdefault("extensions", {})
    need(isinstance(extensions, dict), "DOMAIN_STORE", "invalid extensions projection")
    if "domain" not in extensions:
        extensions["domain"] = initial_domain(state)
    return _validate_domain(extensions["domain"])


def domain_state(state: dict[str, Any]) -> dict[str, Any]:
    """Return a detached domain view without mutating a legacy projection."""
    existing = state.get("extensions", {}).get("domain")
    return copy.deepcopy(_validate_domain(existing) if existing is not None else initial_domain(state))


def require_action(state: dict[str, Any], action_class: str) -> dict[str, Any]:
    """Late import keeps the domain/control dependency one-way."""
    from .control import require_action as control_require_action
    return control_require_action(state, action_class)


def validation_generation(domain: dict[str, Any], work_id: str) -> int:
    return domain.get("validation_generations", {}).get(work_id, 0)


DomainApply = Callable[[dict[str, Any], dict[str, Any], dict[str, Any], str], None]


def domain_handler(kind: str, validate_payload: Callable[[Any], dict[str, Any]],
                   apply_domain: DomainApply) -> HandlerSpec:
    def apply(state: dict[str, Any], payload: dict[str, Any], event_id: str) -> None:
        domain = mutable_domain(state)
        apply_domain(state, domain, payload, event_id)
        domain["revision"] += 1
    return HandlerSpec(kind, validate_payload, apply)


def effective_nodes(state: dict[str, Any], domain: dict[str, Any] | None = None) -> dict[str, dict[str, Any]]:
    domain = domain or domain_state(state)
    result = {row["id"]: copy.deepcopy(row) for row in state["plan"]["node"]}
    result.update(copy.deepcopy(domain["work_nodes"]))
    for key, update in domain["work_updates"].items():
        need(key in result, "REFERENCE", f"unknown work {key}")
        result[key].update(copy.deepcopy(update))
    return result


def work_is_accepted(state: dict[str, Any], domain: dict[str, Any], work_id: str,
                     trail: set[str] | None = None) -> bool:
    nodes = effective_nodes(state, domain)
    if work_id not in nodes:
        return False
    trail = set() if trail is None else set(trail)
    if work_id in trail:
        return False
    from .domain_reuse import work_acceptance_current
    if work_acceptance_current(state, domain, work_id, domain["active_outcome_id"], trail):
        return True
    trail.add(work_id)
    if nodes[work_id]["state"] == "accepted":
        return work_id in domain["legacy_acceptance"]
    if nodes[work_id]["state"] == "superseded":
        successors = domain["work_successors"].get(work_id, [])
        return bool(successors) and all(work_is_accepted(state, domain, key, trail) for key in successors)
    if nodes[work_id]["state"] == "dropped":
        return bool(domain["work_updates"].get(work_id, {}).get("dependency_resolved"))
    return False


def inherited_dependencies(nodes: dict[str, dict[str, Any]], work_id: str) -> list[str]:
    dependencies = []
    cursor = work_id
    while cursor:
        dependencies.extend(nodes[cursor].get("depends_on", []))
        cursor = nodes[cursor].get("parent")
    return list(dict.fromkeys(dependencies))


def domain_frontier(state: dict[str, Any]) -> list[str]:
    """Return domain-ready leaves; policy admission and dispatch remain separate."""
    domain = domain_state(state)
    nodes = effective_nodes(state, domain)
    children = {key: [] for key in nodes}
    for key, node in nodes.items():
        parent = node.get("parent")
        if parent:
            need(parent in nodes, "REFERENCE", f"missing parent of {key}")
            children[parent].append(key)
    ready = []
    for key, node in nodes.items():
        if children[key] or node["state"] not in {"planned", "ready"}:
            continue
        cursor, suppressed = node.get("parent"), False
        while cursor:
            suppressed = suppressed or nodes[cursor]["state"] in {"blocked", "deferred", "dropped", "superseded"}
            cursor = nodes[cursor].get("parent")
        if suppressed:
            continue
        if any(row["status"] == "open" and key in row["work_ids"] for row in domain["deferrals"].values()):
            continue
        if all(work_is_accepted(state, domain, dep) for dep in inherited_dependencies(nodes, key)):
            ready.append(key)
    return sorted(ready, key=lambda key: (nodes[key].get("order", 0), key))


def text(value: Any, label: str) -> str:
    return string(value, label)
