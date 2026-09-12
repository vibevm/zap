"""Intent, outcome, work graph, lowering, contracts and deferrals."""
from __future__ import annotations

from typing import Any

from .common import exact, identity, need, string, strings
from .domain_model import (
    OBLIGATION_DISPOSITIONS, OWNERSHIP_ROLES, STAGES, active_obligations,
    boolean, content_hash, digest, domain_handler, effective_nodes, integer,
    nonblank_list, optional_identity, owned_obligations, require_action, rows,
    strict, text, unique_ids,
)
from .domain_work import validate_contract

KINDS = {"portfolio", "campaign", "phase", "workstream", "group", "atom", "gate", "horizon"}
INTENT_FIELDS = {
    "schema", "intent_id", "revision", "previous_intent_id", "summary",
    "beneficiaries", "values", "constraints", "source_refs",
}


def _intent_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/intent-proposed/1", {
        "intent_id", "revision", "previous_intent_id", "summary", "beneficiaries",
        "values", "constraints", "source_refs",
    })(value)
    identity(value["intent_id"])
    integer(value["revision"], "intent revision", minimum=1)
    optional_identity(value["previous_intent_id"], "previous intent")
    text(value["summary"], "intent summary")
    for field in ("beneficiaries", "values", "constraints", "source_refs"):
        nonblank_list(value[field], field, nonempty=field in {"beneficiaries", "values"})
    return value


def intent_fingerprint(value: Any) -> str:
    """Hash the exact normative intent proposal bound by owner control."""
    proposal = _intent_payload(value)
    return content_hash({key: proposal[key] for key in INTENT_FIELDS})


def _apply_intent_proposed(state, domain, payload, event_id):
    key = payload["intent_id"]
    need(key not in domain["intents"], "DUPLICATE", "intent identity already exists")
    previous = payload["previous_intent_id"]
    if previous is None:
        need(payload["revision"] == 1, "DOMAIN_REVISION", "initial intent revision must be one")
    else:
        need(previous in domain["intents"], "REFERENCE", "previous intent missing")
        need(payload["revision"] == domain["intents"][previous]["revision"] + 1,
             "DOMAIN_REVISION", "intent revision is not consecutive")
    domain["intents"][key] = {**payload, "status": "proposed", "event_id": event_id}


def _adopt_intent(state, domain, key, event_id, *, action="outcome.adopt"):
    policy = require_action(state, action)
    need(key in domain["intents"] and domain["intents"][key]["status"] == "proposed",
         "DOMAIN_INTENT", "intent proposal missing")
    binding = policy.get("intent_binding")
    need(isinstance(binding, dict) and set(binding) == {"intent_id", "sha256"},
         "DOMAIN_POLICY", "active charter lacks exact intent binding")
    proposal = {field: domain["intents"][key][field] for field in INTENT_FIELDS}
    need(binding["intent_id"] == key and binding["sha256"] == intent_fingerprint(proposal),
         "DOMAIN_POLICY", "intent proposal differs from active owner binding")
    charter_id = identity(policy.get("charter_id"))
    charter_revision = integer(policy.get("revision"), "charter revision", minimum=1)
    charter_sha256 = digest(policy.get("charter_sha256"), "charter sha256")
    previous = domain["active_intent_id"]
    need(domain["intents"][key]["previous_intent_id"] == previous,
         "DOMAIN_STALE", "intent proposal is not based on the active intent")
    if previous:
        previous_binding = domain["intents"][previous]["owner_binding"]
        need(charter_revision > previous_binding["charter_revision"]
             and charter_sha256 != previous_binding["charter_sha256"],
             "DOMAIN_POLICY", "intent revision requires a new owner charter amendment")
        domain["intents"][previous]["status"] = "superseded"
    domain["intents"][key]["status"] = "active"
    domain["intents"][key]["adopted_event_id"] = event_id
    domain["intents"][key]["owner_binding"] = {
        "intent_id": key, "sha256": binding["sha256"], "charter_id": charter_id,
        "charter_revision": charter_revision, "charter_sha256": charter_sha256,
    }
    domain["active_intent_id"] = key


def _apply_intent_adopted(state, domain, payload, event_id):
    need(domain["active_outcome_id"] is None, "DOMAIN_INTENT", "revise intent with an atomic adaptive outcome transition")
    _adopt_intent(state, domain, payload["intent_id"], event_id)


def _validate_intent_adopted(value):
    value = strict("zap-domain/intent-adopted/1", {"intent_id"})(value)
    identity(value["intent_id"])
    return value


def _obligation_row(row: dict[str, Any]) -> None:
    exact(row, {"id", "statement", "essential", "source_refs", "owners"})
    identity(row["id"])
    text(row["statement"], "obligation statement")
    boolean(row["essential"], "essential")
    nonblank_list(row["source_refs"], "obligation source refs")
    rows(row["owners"], "obligation owners", nonempty=True)
    seen = set()
    for owner in row["owners"]:
        exact(owner, {"work_id", "role"})
        identity(owner["work_id"])
        need(owner["role"] in OWNERSHIP_ROLES, "DOMAIN_VALUE", "invalid ownership role")
        pair = (owner["work_id"], owner["role"])
        need(pair not in seen, "DUPLICATE", "duplicate obligation owner")
        seen.add(pair)


def _outcome_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/outcome-proposed/1", {
        "outcome_id", "revision", "previous_outcome_id", "intent_id", "summary",
        "benefits", "guarantees", "tradeoffs", "obligations",
    })(value)
    identity(value["outcome_id"])
    integer(value["revision"], "outcome revision", minimum=1)
    optional_identity(value["previous_outcome_id"], "previous outcome")
    identity(value["intent_id"])
    text(value["summary"], "outcome summary")
    for field in ("benefits", "guarantees", "tradeoffs"):
        nonblank_list(value[field], field, nonempty=field in {"benefits", "guarantees"})
    seen = set()
    for row in rows(value["obligations"], "outcome obligations"):
        _obligation_row(row)
        need(row["id"] not in seen, "DUPLICATE", "duplicate proposed obligation")
        seen.add(row["id"])
    return value


def _apply_outcome_proposed(state, domain, payload, event_id):
    key = payload["outcome_id"]
    need(key not in domain["outcome_revisions"], "DUPLICATE", "outcome identity already exists")
    need(payload["intent_id"] in domain["intents"], "REFERENCE", "intent missing")
    previous = payload["previous_outcome_id"]
    if previous is None:
        need(payload["revision"] == 1 and domain["active_outcome_id"] is None,
             "DOMAIN_REVISION", "initial outcome revision must be one")
    else:
        need(previous in domain["outcome_revisions"], "REFERENCE", "previous outcome missing")
        need(payload["revision"] == domain["outcome_revisions"][previous]["revision"] + 1,
             "DOMAIN_REVISION", "outcome revision is not consecutive")
    domain["outcome_revisions"][key] = {**payload, "status": "proposed", "event_id": event_id}


def _disposition_rows(value: Any) -> list[dict[str, Any]]:
    result = rows(value, "obligation dispositions")
    seen = set()
    for row in result:
        exact(row, {"obligation_id", "disposition", "successor_ids", "unmet_portion", "reason"})
        key = identity(row["obligation_id"])
        need(key not in seen, "DUPLICATE", "duplicate obligation disposition")
        seen.add(key)
        need(row["disposition"] in OBLIGATION_DISPOSITIONS, "DOMAIN_VALUE", "invalid disposition")
        unique_ids(row["successor_ids"], "successor ids")
        string(row["unmet_portion"], "unmet portion", empty=True)
        text(row["reason"], "disposition reason")
        if row["disposition"] == "replaced":
            need(row["successor_ids"], "DOMAIN_OBLIGATION", "replacement needs successors")
        else:
            need(not row["successor_ids"], "DOMAIN_OBLIGATION", "only replacement names successors")
        if row["disposition"] in {"replaced", "excluded", "unattainable"}:
            text(row["unmet_portion"], "unmet portion")
    return result


def _adopt_outcome(state, domain, outcome_id, dispositions, event_id, *, action_checked=False):
    policy = require_action(state, "outcome.adopt") if not action_checked else require_action(state, "adaptive.apply")
    outcome = domain["outcome_revisions"].get(outcome_id)
    need(outcome and outcome["status"] == "proposed", "DOMAIN_OUTCOME", "outcome proposal missing")
    need(outcome["intent_id"] == domain["active_intent_id"], "DOMAIN_STALE", "outcome intent is not active")
    current = domain["active_outcome_id"]
    need(outcome["previous_outcome_id"] == current, "DOMAIN_STALE", "outcome is not based on active revision")
    nodes = effective_nodes(state, domain)
    proposed = {row["id"]: row for row in outcome["obligations"]}
    essential = set(policy["adaptation"]["essential_obligations"])
    need(not set(proposed) & set(domain["obligations"]), "DUPLICATE", "obligation identity already exists")
    for row in proposed.values():
        need(row["essential"] == (row["id"] in essential),
             "DOMAIN_POLICY", "new obligation essential status differs from owner policy")
        for owner in row["owners"]:
            need(owner["work_id"] in nodes, "REFERENCE", "obligation owner work missing")
    active_before = active_obligations(domain)
    if current is None:
        need(not dispositions, "DOMAIN_OBLIGATION", "initial outcome retains imported obligations implicitly")
    else:
        need(policy["adaptation"]["allow_target_revision"], "DOMAIN_POLICY", "target revision is not delegated")
        by_id = {row["obligation_id"]: row for row in dispositions}
        need(set(by_id) == active_before, "DOMAIN_OBLIGATION", "every active obligation needs one disposition")
        mutable = set(policy["adaptation"]["mutable_obligations"])
        allowed = set(policy["adaptation"]["allowed_dispositions"])
        for key, row in by_id.items():
            disposition = row["disposition"]
            need(disposition in allowed, "DOMAIN_POLICY", "obligation disposition is not delegated")
            if key not in mutable or key in essential:
                need(disposition == "retained", "DOMAIN_POLICY", "immutable or essential obligation must be retained")
            need(set(row["successor_ids"]) <= set(proposed), "REFERENCE", "replacement successor missing")
    for key, row in proposed.items():
        domain["obligations"][key] = {
            "id": key, "kind": "outcome_obligation", "statement": row["statement"],
            "status": "active", "created_for_outcome": outcome_id,
            "current_outcome_ids": [outcome_id], "essential_declared": row["essential"],
            "source": {"kind": "outcome_proposal", "source_refs": list(row["source_refs"])},
            "owner_work_ids": sorted({owner["work_id"] for owner in row["owners"]}),
            "dispositions": [], "unmet_portion": row["statement"],
        }
        for owner in row["owners"]:
            domain["ownership"].append({
                "type": "obligation.owned_by", "obligation_id": key,
                "work_id": owner["work_id"], "role": owner["role"], "outcome_id": outcome_id,
            })
    if current is None:
        for key in active_before:
            domain["obligations"][key]["current_outcome_ids"].append(outcome_id)
    else:
        for row in dispositions:
            obligation = domain["obligations"][row["obligation_id"]]
            obligation["dispositions"].append({**row, "outcome_id": outcome_id, "event_id": event_id})
            if row["disposition"] == "retained":
                obligation["current_outcome_ids"].append(outcome_id)
            else:
                obligation["status"] = row["disposition"]
                obligation["unmet_portion"] = row["unmet_portion"]
    if current:
        domain["outcome_revisions"][current]["status"] = "superseded"
    outcome["status"] = "active"
    outcome["adopted_event_id"] = event_id
    outcome["obligation_dispositions"] = dispositions
    domain["active_outcome_id"] = outcome_id
    domain["original_outcome_id"] = domain["original_outcome_id"] or outcome_id


def _apply_outcome_adopted(state, domain, payload, event_id):
    _adopt_outcome(state, domain, payload["outcome_id"], payload["obligation_dispositions"], event_id)


def _lower_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/plan-lowered/1", {
        "parent_id", "nodes", "edges", "coverage", "contracts", "integration_owner",
    })(value)
    identity(value["parent_id"])
    identity(value["integration_owner"])
    seen = set()
    for node in rows(value["nodes"], "lowered nodes", nonempty=True):
        exact(node, {"id", "parent", "title", "kind", "state", "order", "depends_on", "acceptance", "required_stage"})
        key = identity(node["id"])
        need(key not in seen, "DUPLICATE", "duplicate lowered work")
        seen.add(key)
        identity(node["parent"])
        text(node["title"], "work title")
        need(node["kind"] in KINDS and node["state"] == "planned", "DOMAIN_WORK", "invalid lowered work")
        integer(node["order"], "work order")
        unique_ids(node["depends_on"], "work dependencies")
        nonblank_list(node["acceptance"], "work acceptance")
        need(node["required_stage"] in STAGES, "DOMAIN_WORK", "invalid required stage")
    for edge in rows(value["edges"], "lowering edges"):
        exact(edge, {"node_id", "depends_on"})
        identity(edge["node_id"])
        unique_ids(edge["depends_on"], "edge dependencies", nonempty=True)
    covered = set()
    for row in rows(value["coverage"], "obligation coverage", nonempty=True):
        exact(row, {"obligation_id", "assignments"})
        key = identity(row["obligation_id"])
        need(key not in covered, "DUPLICATE", "duplicate obligation coverage")
        covered.add(key)
        seen_assignments = set()
        for assignment in rows(row["assignments"], "coverage assignments", nonempty=True):
            exact(assignment, {"work_id", "role"})
            identity(assignment["work_id"])
            need(assignment["role"] in OWNERSHIP_ROLES, "DOMAIN_VALUE", "invalid ownership role")
            pair = (assignment["work_id"], assignment["role"])
            need(pair not in seen_assignments, "DUPLICATE", "duplicate coverage assignment")
            seen_assignments.add(pair)
    for contract in rows(value["contracts"], "lowering contracts"):
        validate_contract(contract)
    return value


def _acyclic(nodes):
    visiting, done = set(), set()
    for start in nodes:
        stack = [(start, False)]
        while stack:
            key, leaving = stack.pop()
            need(key in nodes, "REFERENCE", f"unknown work dependency {key}")
            if leaving:
                visiting.discard(key); done.add(key); continue
            if key in done:
                continue
            need(key not in visiting, "CYCLE", f"work dependency cycle through {key}")
            visiting.add(key); stack.append((key, True))
            dependencies = list(nodes[key].get("depends_on", []))
            parent = nodes[key].get("parent")
            if parent:
                dependencies.append(parent)
            stack.extend((dep, False) for dep in dependencies)


def _apply_lower(state, domain, payload, event_id):
    require_action(state, "plan.lower")
    existing = effective_nodes(state, domain)
    parent = payload["parent_id"]
    need(parent in existing and existing[parent]["state"] not in {"accepted", "dropped", "superseded"},
         "DOMAIN_WORK", "cannot lower missing or terminal work")
    additions = {row["id"]: row for row in payload["nodes"]}
    need(not set(additions) & set(existing), "DUPLICATE", "lowered work already exists")
    for row in additions.values():
        need(row["parent"] == parent or row["parent"] in additions, "DOMAIN_WORK", "lowered work leaves subtree")
        need(set(row["depends_on"]) <= set(existing) | set(additions), "REFERENCE", "lowered dependency missing")
    need(payload["integration_owner"] in additions, "DOMAIN_WORK", "integration owner must be new work")
    all_nodes = {**existing, **additions}
    for edge in payload["edges"]:
        need(edge["node_id"] in additions, "DOMAIN_WORK", "edge target must be lowered work")
        need(set(edge["depends_on"]) <= set(all_nodes), "REFERENCE", "edge dependency missing")
        all_nodes[edge["node_id"]] = {**all_nodes[edge["node_id"]], "depends_on": list(dict.fromkeys(all_nodes[edge["node_id"]]["depends_on"] + edge["depends_on"]))}
    _acyclic(all_nodes)
    expected = owned_obligations(domain, parent, active_only=True)
    actual = {row["obligation_id"] for row in payload["coverage"]}
    need(actual == expected, "DOMAIN_COVERAGE", "current parent obligations are not covered exactly")
    assigned_work = set()
    for row in payload["coverage"]:
        for assignment in row["assignments"]:
            need(assignment["work_id"] in additions, "DOMAIN_COVERAGE", "coverage must name lowered work")
            assigned_work.add(assignment["work_id"])
    need(assigned_work == set(additions), "DOMAIN_COVERAGE", "every lowered work needs obligation origin")
    children = {key: [] for key in additions}
    for key, row in additions.items():
        if row["parent"] in children:
            children[row["parent"]].append(key)
    contracts = {row["work_id"]: row for row in payload["contracts"]}
    need(len(contracts) == len(payload["contracts"]), "DUPLICATE", "duplicate lowered work contract")
    need(len({row["contract_id"] for row in payload["contracts"]}) == len(payload["contracts"]),
         "DUPLICATE", "duplicate lowered contract identity")
    historical_contract_ids = {
        version.get("contract", {}).get("contract_id")
        for history in domain["task_contracts"].values() for version in history["versions"]
    }
    need(not {row["contract_id"] for row in payload["contracts"]} & historical_contract_ids,
         "DUPLICATE", "lowered contract identity already exists")
    leaf_ids = {key for key, child_ids in children.items() if not child_ids}
    need(set(contracts) == leaf_ids, "DOMAIN_CONTRACT", "every lowered leaf needs exactly one task contract")
    assigned = {key: set() for key in additions}
    for row in payload["coverage"]:
        for assignment in row["assignments"]:
            assigned[assignment["work_id"]].add(row["obligation_id"])
    for key in leaf_ids:
        generated = {f"acceptance:{key}:{index}" for index in range(len(additions[key]["acceptance"]))}
        need(set(contracts[key]["obligation_ids"]) == assigned[key] | generated,
             "DOMAIN_CONTRACT", "lowered task contract obligation binding differs")
    for key in additions:
        domain["work_nodes"][key] = all_nodes[key]
    outcome_id = domain["active_outcome_id"]
    for row in payload["coverage"]:
        for assignment in row["assignments"]:
            domain["ownership"].append({
                "type": "obligation.owned_by", "obligation_id": row["obligation_id"],
                "work_id": assignment["work_id"], "role": assignment["role"], "outcome_id": outcome_id,
            })
    for key, row in additions.items():
        for index, criterion in enumerate(row["acceptance"]):
            obligation_id = f"acceptance:{key}:{index}"
            need(obligation_id not in domain["obligations"], "DUPLICATE", "acceptance obligation collision")
            domain["obligations"][obligation_id] = {
                "id": obligation_id, "kind": "lowered_acceptance", "statement": criterion,
                "status": "active", "created_for_outcome": outcome_id,
                "current_outcome_ids": [outcome_id] if outcome_id else [], "essential_declared": None,
                "source": {"kind": "lowering", "event_id": event_id}, "owner_work_ids": [key],
                "dispositions": [], "unmet_portion": criterion,
            }
            domain["ownership"].append({
                "type": "obligation.owned_by", "obligation_id": obligation_id,
                "work_id": key, "role": "acceptance", "outcome_id": outcome_id,
            })
    for key, contract in contracts.items():
        domain["task_contracts"][key] = {
            "active_version": 1,
            "versions": [{"version": 1, "schema": "zap-task-contract/1", "source": "lowering",
                          "sha256": content_hash(contract), "contract": contract, "event_id": event_id}],
        }


def _validate_adopt(value):
    value = strict("zap-domain/outcome-adopted/1", {"outcome_id", "obligation_dispositions"})(value)
    identity(value["outcome_id"]); _disposition_rows(value["obligation_dispositions"])
    return value


DOMAIN_GRAPH_HANDLERS = {
    spec.kind: spec for spec in (
        domain_handler("domain.intent-proposed", _intent_payload, _apply_intent_proposed),
        domain_handler("domain.intent-adopted", _validate_intent_adopted, _apply_intent_adopted),
        domain_handler("domain.outcome-proposed", _outcome_payload, _apply_outcome_proposed),
        domain_handler("domain.outcome-adopted", _validate_adopt, _apply_outcome_adopted),
        domain_handler("domain.plan-lowered", _lower_payload, _apply_lower),
    )
}
