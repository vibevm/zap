"""Projection state, strict event handlers, replay reducer, and stop probes."""
from __future__ import annotations

import copy
from dataclasses import dataclass
from types import MappingProxyType
from typing import Any, Callable, Iterable, Mapping, TypedDict, cast

from .common import CAPABILITIES, COLLECTIONS, MATURITY, PROJECTION_SCHEMA, SCHEMA, TERMINAL, WORK_TYPES, Refusal, exact, identity, need, string, strings
from .graph import validate_plan

State = dict[str, Any]
Payload = dict[str, Any]
PayloadValidator = Callable[[Any], Payload]
PayloadApplier = Callable[[State, Payload, str], None]


class NodeClassifiedPayload(TypedDict):
    node_id: str
    work_type: str
    maturity: str


class RegionRecordedPayload(TypedDict):
    id: str
    question: str
    node_refs: list[str]


@dataclass(frozen=True)
class HandlerSpec:
    """One explicitly wired event kind and its strict payload boundary."""

    kind: str
    validate_payload: PayloadValidator
    apply: PayloadApplier


def initial_state(base: dict[str, Any], base_hash: str) -> State:
    nodes, _, _ = validate_plan(base["plan"])
    return {
        "schema": SCHEMA,
        "projection_schema": PROJECTION_SCHEMA,
        "capabilities": copy.deepcopy(CAPABILITIES),
        "revision": 0,
        "base_sha256": base_hash,
        "execution_mode": "draft",
        "owner_contract": None,
        "plan": copy.deepcopy(base["plan"]),
        "task_contracts": copy.deepcopy(base["task_contracts"]),
        "classifications": {key: {"work_type": "unclassified", "maturity": "unspecified", "assertion_status": "declared"} for key in nodes},
        **{key: {} for key in COLLECTIONS},
        "relations": [],
        "extensions": {},
    }


def refs(state: State, values: Any, collection: str) -> None:
    strings(values, collection + " refs")
    available = {node["id"] for node in state["plan"]["node"]} if collection == "node" else state[collection].keys()
    need(set(values) <= set(available), "REFERENCE", f"unknown {collection} reference")


def relations(state: State, kind: str, key: str, payload: Mapping[str, Any]) -> None:
    for field, target, relation in (("node_refs", "node", "about"), ("evidence_refs", "evidence", "supported_by")):
        for reference in payload.get(field, []):
            state["relations"].append({"source_kind": kind, "source_id": key, "relation": relation, "target_kind": target, "target_id": reference})


def add_record(state: State, collection: str, payload: Payload) -> None:
    key = identity(payload["id"])
    need(key not in state[collection], "DUPLICATE", f"record already exists: {key}")
    for field, target in (("node_refs", "node"), ("evidence_refs", "evidence")):
        if field in payload:
            refs(state, payload[field], target)
    state[collection][key] = copy.deepcopy(payload)
    relations(state, collection, key, payload)


def strict_payload(required: set[str], optional: set[str] | tuple[str, ...] = ()) -> PayloadValidator:
    """Build a validator that returns a detached, exact-field payload."""
    def validate(value: Any) -> Payload:
        exact(value, required, optional)
        return copy.deepcopy(cast(Payload, value))
    return validate


def refine(state: State, payload: Payload) -> None:
    exact(payload, {"parent_id", "nodes", "edges", "coverage"})
    nodes, _, _ = validate_plan(state["plan"])
    parent = payload["parent_id"]
    need(parent in nodes and nodes[parent]["state"] not in TERMINAL, "REFINEMENT", "cannot refine missing/terminal parent")
    need(isinstance(payload["nodes"], list) and payload["nodes"], "REFINEMENT", "refinement must add nodes")
    additions = {identity(node.get("id")): node for node in payload["nodes"] if isinstance(node, dict)}
    need(len(additions) == len(payload["nodes"]) and not additions.keys() & nodes.keys(), "REFINEMENT", "duplicate/malformed added nodes")
    for node in additions.values():
        exact(node, {"id", "parent", "title", "kind", "state", "order", "depends_on", "mandates", "acceptance", "evidence"}, {"zoom", "contract", "contract_sha256"})
        need(node.get("parent") in additions or node.get("parent") == parent, "REFINEMENT", "new node leaves parent subtree")
        need(node.get("state") == "planned", "REFINEMENT", "new nodes must remain planned")
        need(set(nodes[parent]["mandates"]) <= set(node.get("mandates", [])), "REFINEMENT", "parent mandates dropped")
    state["plan"]["node"].extend(copy.deepcopy(payload["nodes"]))
    for mandate in state["plan"]["mandate"]:
        mandate["nodes"].extend(key for key, node in additions.items() if mandate["id"] in node.get("mandates", []) and key not in mandate["nodes"])
    validate_plan(state["plan"])
    all_nodes = {node["id"]: node for node in state["plan"]["node"]}
    need(isinstance(payload["edges"], list), "REFINEMENT", "edges must be a list")
    for edge in payload["edges"]:
        exact(edge, {"node_id", "depends_on"})
        key = edge["node_id"]
        need(key in all_nodes and all_nodes[key]["state"] not in TERMINAL, "REFINEMENT", "cannot alter terminal prerequisites")
        cursor = key
        while cursor and cursor != parent:
            cursor = all_nodes[cursor]["parent"]
        need(cursor == parent, "REFINEMENT", "edge target leaves refinement subtree")
        for dependency in strings(edge["depends_on"], "dependencies"):
            if dependency not in all_nodes[key]["depends_on"]:
                all_nodes[key]["depends_on"].append(dependency)
    need(isinstance(payload["coverage"], list), "REFINEMENT", "coverage must be a list")
    covered: set[str] = set()
    indices: set[int] = set()
    for row in payload["coverage"]:
        exact(row, {"acceptance_index", "node_ids"})
        index = row["acceptance_index"]
        need(type(index) is int and index not in indices and 0 <= index < len(nodes[parent]["acceptance"]), "REFINEMENT", "invalid/duplicate acceptance index")
        children = strings(row["node_ids"], "coverage children")
        need(children and set(children) <= additions.keys(), "REFINEMENT", "coverage must name new children")
        indices.add(index)
        covered.update(children)
        for key in children:
            state["relations"].append({"source_kind": "node", "source_id": parent, "relation": "acceptance_lowered_to", "acceptance_index": index, "target_kind": "node", "target_id": key, "semantic_review": "unverified"})
    need(indices == set(range(len(nodes[parent]["acceptance"]))) and covered == additions.keys(), "REFINEMENT", "parent acceptance or child origin unaccounted")
    validate_plan(state["plan"])
    for key in additions:
        state["classifications"][key] = {"work_type": "unclassified", "maturity": "unspecified", "assertion_status": "declared"}


def _classify(state: State, raw: Payload, _event_id: str) -> None:
    payload = cast(NodeClassifiedPayload, raw)
    refs(state, [payload["node_id"]], "node")
    need(payload["work_type"] in WORK_TYPES and payload["maturity"] in MATURITY, "CLASSIFICATION", "invalid work type/maturity")
    state["classifications"][payload["node_id"]] = {"work_type": payload["work_type"], "maturity": payload["maturity"], "assertion_status": "declared"}


def _region(state: State, raw: Payload, _event_id: str) -> None:
    payload = cast(RegionRecordedPayload, raw)
    string(payload["question"], "question")
    add_record(state, "unknown_regions", cast(Payload, payload))


def _evidence(state: State, payload: Payload, _event_id: str) -> None:
    for key in ("claim", "subject"):
        string(payload[key], key)
    need(payload["result"] in {"observed_pass", "observed_fail", "unavailable", "inconclusive"}, "EVIDENCE", "invalid observation result")
    strings(payload["artifact_refs"], "artifact refs")
    add_record(state, "evidence", {**payload, "adjudication": "unverified"})


def _fact(state: State, payload: Payload, _event_id: str) -> None:
    string(payload["statement"], "statement")
    need(payload["status"] in {"hypothesis", "asserted", "observed"}, "FACT", "recording is not verification")
    strings(payload["source_refs"], "source refs")
    need(payload["status"] != "observed" or payload["evidence_refs"], "FACT", "observed assertion needs evidence pointers")
    add_record(state, "facts", {**payload, "adjudication": "unverified"})


def _decision(state: State, payload: Payload, _event_id: str) -> None:
    for key in ("question", "rationale", "authority_ref"):
        string(payload[key], key)
    need(isinstance(payload["alternatives"], list) and len(payload["alternatives"]) >= 2, "DECISION", "name at least two alternatives")
    choices: set[str] = set()
    for row in payload["alternatives"]:
        exact(row, {"id", "description"})
        need(identity(row["id"]) not in choices, "DUPLICATE", "duplicate alternative")
        choices.add(row["id"])
        string(row["description"], "alternative description")
    need(payload["chosen"] in choices, "DECISION", "chosen alternative missing")
    strings(payload["consequences"], "consequences")
    add_record(state, "decisions", {**payload, "authority_origin": "unverified", "activates_contract": False})


def _approach_declared(state: State, payload: Payload, _event_id: str) -> None:
    refs(state, [payload["problem_id"]], "node")
    description = " ".join(string(payload["description"], "approach description").casefold().split())
    need(not any(approach["problem_id"] == payload["problem_id"] and approach["strategy_key"] == description for approach in state["approaches"].values()), "APPROACH", "renamed duplicate strategy; reuse the declared approach id")
    add_record(state, "approaches", {**payload, "strategy_key": description, "outcome": "unresolved", "verdicts": []})
    state["relations"].append({"source_kind": "approaches", "source_id": payload["id"], "relation": "addresses", "target_kind": "node", "target_id": payload["problem_id"]})


def _approach_verdict(state: State, payload: Payload, event_id: str) -> None:
    need(payload["approach_id"] in state["approaches"], "APPROACH", "declare approach before verdict")
    refs(state, payload["evidence_refs"], "evidence")
    need(payload["outcome"] in {"failed", "succeeded", "inconclusive", "infrastructure_failure"}, "APPROACH", "unknown verdict")
    approach = state["approaches"][payload["approach_id"]]
    need(approach["outcome"] == "unresolved", "APPROACH", "semantic verdict already recorded")
    if payload["outcome"] in {"failed", "succeeded"}:
        need(any(state["evidence"][key]["result"] in {"observed_pass", "observed_fail"} for key in payload["evidence_refs"]), "APPROACH", "semantic verdict needs an observed result; unavailable/inconclusive alone is insufficient")
        approach["outcome"] = payload["outcome"]
    approach["verdicts"].append({**payload, "event_id": event_id})


def _refine(state: State, payload: Payload, _event_id: str) -> None:
    refine(state, payload)


CORE_HANDLERS: Mapping[str, HandlerSpec] = MappingProxyType({
    spec.kind: spec for spec in (
        HandlerSpec("node.classified", strict_payload({"node_id", "work_type", "maturity"}), _classify),
        HandlerSpec("knowledge.region-recorded", strict_payload({"id", "question", "node_refs"}), _region),
        HandlerSpec("evidence.recorded", strict_payload({"id", "claim", "subject", "result", "artifact_refs", "node_refs"}), _evidence),
        HandlerSpec("fact.recorded", strict_payload({"id", "statement", "status", "node_refs", "evidence_refs", "source_refs"}), _fact),
        HandlerSpec("decision.recorded", strict_payload({"id", "question", "alternatives", "chosen", "rationale", "authority_ref", "consequences", "node_refs", "evidence_refs"}), _decision),
        HandlerSpec("approach.declared", strict_payload({"id", "problem_id", "description"}), _approach_declared),
        HandlerSpec("approach.verdict", strict_payload({"approach_id", "outcome", "evidence_refs"}), _approach_verdict),
        HandlerSpec("plan.refined", strict_payload({"parent_id", "nodes", "edges", "coverage"}), _refine),
    )
})


def compose_handlers(*sources: Mapping[str, HandlerSpec] | Iterable[HandlerSpec] | HandlerSpec) -> Mapping[str, HandlerSpec]:
    """Return an immutable explicit registry, refusing mismatches and overrides."""
    result: dict[str, HandlerSpec] = {}
    for source in sources:
        if isinstance(source, HandlerSpec):
            entries = [(source.kind, source)]
        elif isinstance(source, Mapping):
            entries = list(source.items())
        else:
            entries = [(spec.kind, spec) for spec in source]
        for key, spec in entries:
            identity(key)
            need(isinstance(spec, HandlerSpec) and spec.kind == key, "HANDLER", "handler registry key differs from declared kind")
            need(key not in result, "HANDLER", f"duplicate handler kind {key}")
            result[key] = spec
    return MappingProxyType(result)


def validate_command_envelope(state: State, command: Any) -> tuple[str, str]:
    exact(command, {"event_id", "base_revision", "kind", "reason", "payload"})
    event_id = identity(command["event_id"])
    need(type(command["base_revision"]) is int and command["base_revision"] == state["revision"], "STALE", "base revision differs")
    reason = command["reason"]
    exact(reason, {"summary"}, {"evidence_refs", "decision_ref"})
    string(reason["summary"], "reason summary")
    refs(state, reason.get("evidence_refs", []), "evidence")
    if "decision_ref" in reason:
        need(reason["decision_ref"] in state["decisions"], "REFERENCE", "unknown reason decision")
    return event_id, command["kind"]


def apply_command(state: State, command: Any, handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS) -> State:
    event_id, kind = validate_command_envelope(state, command)
    try:
        handler = handlers.get(kind)
    except TypeError as exc:
        raise Refusal("KIND", "unsupported event kind; no owner control or execution transitions exist") from exc
    if handler is None:
        raise Refusal("KIND", "unsupported event kind; no owner control or execution transitions exist")
    after = copy.deepcopy(state)
    payload = handler.validate_payload(command["payload"])
    handler.apply(after, payload, event_id)
    validate_plan(after["plan"])
    after["revision"] += 1
    return after


def expression(expr: Any, assessment: dict[str, Any], state: State) -> tuple[bool | None, list[dict[str, Any]]]:
    need(isinstance(expr, dict) and len(expr) == 1, "RULE", "one expression operator required")
    operator, argument = next(iter(expr.items()))
    if operator in {"all", "any"}:
        need(isinstance(argument, list) and argument, "RULE", "nonempty logical operands required")
        results = [expression(item, assessment, state) for item in argument]
        values = [result[0] for result in results]
        value = (False if False in values else None if None in values else True) if operator == "all" else (True if True in values else None if None in values else False)
        return value, [item for result in results for item in result[1]]
    if operator == "not":
        value, details = expression(argument, assessment, state)
        return (None if value is None else not value), details
    if operator == "eq":
        exact(argument, {"field", "value"})
        string(argument["field"], "assessment field")
        need(argument["value"] is not None and isinstance(argument["value"], (str, bool, int, float)), "RULE", "comparison needs concrete scalar")
        value = assessment.get(argument["field"])
        need(value is None or type(value) is type(argument["value"]), "ASSESSMENT", "assessment scalar type differs from predicate")
        result = None if value is None else type(value) is type(argument["value"]) and value == argument["value"]
        return result, [{"field": argument["field"], "observed": value, "result": result}]
    if operator == "failed_approaches":
        exact(argument, {"gte"})
        need(type(argument["gte"]) is int and argument["gte"] > 0, "RULE", "positive approach threshold required")
        failures: dict[str, list[str]] = {}
        for approach in state["approaches"].values():
            if approach["outcome"] == "failed":
                failures.setdefault(approach["problem_id"], []).append(approach["id"])
        return any(len(ids) >= argument["gte"] for ids in failures.values()), [{"failed_by_problem": failures, "threshold": argument["gte"]}]
    raise Refusal("RULE", "unknown stop expression")


def evaluate_stop(state: State, data: Any) -> dict[str, Any]:
    exact(data, {"rules", "assessment"})
    rules, assessment = data["rules"], data["assessment"]
    exact(rules, {"schema", "status", "rules"})
    need(rules["schema"] == "zap-stop/1" and rules["status"] == "example_unapproved", "RULE", "CLI accepts only unapproved rule probes; no activation adapter")
    need(isinstance(assessment, dict) and assessment.get("phase") in {"before_action", "after_action"}, "ASSESSMENT", "declare whether action already happened")
    need(isinstance(rules["rules"], list) and rules["rules"], "RULE", "rules missing")
    results: list[dict[str, Any]] = []
    ids: set[str] = set()
    for rule in rules["rules"]:
        exact(rule, {"id", "when", "scope", "timing"})
        key = identity(rule["id"])
        need(key not in ids, "DUPLICATE", "duplicate stop rule")
        ids.add(key)
        need(rule["scope"] == "run" and rule["timing"] in {"before_action", "before_next_action"}, "RULE", "unsupported stop scope/timing")
        value, details = expression(rule["when"], assessment, state)
        results.append({"id": key, "matched": value, "scope": "run", "timing": rule["timing"], "details": details})
    matched = [result["id"] for result in results if result["matched"] is True]
    unknown = [result["id"] for result in results if result["matched"] is None]
    after = assessment["phase"] == "after_action"
    late = after and any(result["matched"] is True and result["timing"] == "before_action" for result in results)
    policy_result = "too_late" if late else "pause" if matched else "needs_evidence" if unknown else "clear"
    return {"ok": True, "revision": state["revision"], "execution_mode": "draft", "policy_origin": "unverified_input", "policy_result": policy_result,
            "matched_rules": matched, "unknown_rules": unknown, "rules": results, "scope": "run", "action_admitted": False,
            "owner_contract_activated": False, "action_already_occurred": after, "prevented_action": False,
            "response": "No new jobs; drain existing operations safely and prepare owner options." if matched else "Resolve unknown assessment evidence before action." if unknown else "No rule matched; draft mode still grants no execution authority."}
