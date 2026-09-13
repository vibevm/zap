"""Pure knowledge reducers: sources, dependencies, applicability, and fog."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any, Iterable, Mapping

from .common import exact, identity, need, packed, sha, string, strings
from .knowledge_schemas import KNOWLEDGE_EVENT_SCHEMAS, KNOWLEDGE_SCHEMA_DIALECT, KNOWLEDGE_SCHEMA_VERSION
from .records import HandlerSpec, State
from .sources import endpoint_key, validate_endpoint, validate_scope, validate_source_descriptor

KNOWLEDGE_VERSION = 1
DEPENDENCY_RELATIONS = {"depends_on", "derived_from", "supports", "verifies", "affects", "consumes"}
REGION_STATES = {"unexamined", "bounded", "evidenced", "invalidated"}
RELEVANCE = {"relevant", "irrelevant", "unknown"}
KNOWLEDGE_EVENT_ROUTES = {
    "knowledge.source-recorded": {"route": "effect_adapter", "action": "evidence.adjudicate", "affects_readiness": False},
    "knowledge.native-facts-recorded": {"route": "effect_adapter", "action": "evidence.adjudicate", "affects_readiness": False},
    "knowledge.source-recaptured": {"route": "effect_adapter", "action": "evidence.adjudicate", "affects_readiness": True},
    "knowledge.source-observation-recorded": {"route": "agent_data", "action": None, "affects_readiness": False},
    "knowledge.source-observed": {"route": "trusted_service", "action": "evidence.adjudicate", "affects_readiness": True},
    "knowledge.dependency-recorded": {"route": "trusted_service", "action": "evidence.adjudicate", "affects_readiness": True},
    "knowledge.closure-assessed": {"route": "trusted_service", "action": "evidence.adjudicate", "affects_readiness": True},
    "knowledge.applicability-assessed": {"route": "trusted_service", "action": "evidence.adjudicate", "affects_readiness": True},
    "knowledge.region-transitioned": {"route": "agent_data", "action": None, "affects_readiness": False},
    "knowledge.region-relevance-set": {"route": "agent_data", "action": None, "affects_readiness": False},
    "knowledge.region-split": {"route": "agent_data", "action": None, "affects_readiness": False},
    "knowledge.region-merged": {"route": "agent_data", "action": None, "affects_readiness": False},
}


def _empty() -> dict[str, Any]:
    return {"version": KNOWLEDGE_VERSION, "sources": {}, "native_facts": {}, "dependencies": {}, "closures": {},
            "closure_history": [], "applicability": {}, "regions": {}, "invalidations": {}, "endpoint_status": {}}


def _knowledge(state: State) -> dict[str, Any]:
    namespace = state.setdefault("extensions", {}).setdefault("knowledge", _empty())
    need(namespace.get("version") == KNOWLEDGE_VERSION, "KNOWLEDGE_VERSION", "unsupported knowledge state version")
    for key, default in _empty().items():
        namespace.setdefault(key, copy.deepcopy(default))
    return namespace


def knowledge_state(state: State) -> dict[str, Any]:
    """Return a detached knowledge projection, including a lazy empty legacy view."""
    namespace = state.get("extensions", {}).get("knowledge")
    if namespace is None:
        return _empty()
    need(namespace.get("version") == KNOWLEDGE_VERSION, "KNOWLEDGE_VERSION", "unsupported knowledge state version")
    return copy.deepcopy(namespace)


def knowledge_snapshot(state: State, region_ids: list[str] | None = None) -> dict[str, Any]:
    """Bind an exact selected fog view without unrelated-knowledge staleness."""
    regions = knowledge_state(state)["regions"]
    selected = sorted(regions) if region_ids is None else strings(region_ids, "knowledge snapshot region ids")
    need(len(set(selected)) == len(selected), "DUPLICATE", "duplicate knowledge snapshot region")
    for region_id in selected:
        identity(region_id)
        need(region_id in regions, "REFERENCE", f"unknown knowledge snapshot region {region_id}")
    rows = {region_id: copy.deepcopy(regions[region_id]) for region_id in selected}
    revision = sum(row.get("revision", 0) for row in rows.values())
    return {"revision": revision, "sha256": sha(packed({"regions": rows})), "regions": rows}


def _endpoint_exists(state: State, endpoint: dict[str, str]) -> bool:
    kind, key = endpoint["kind"], endpoint["id"]
    knowledge = state.get("extensions", {}).get("knowledge", {})
    if kind == "source":
        return key in knowledge.get("sources", {})
    if kind == "fact":
        return key in state.get("facts", {}) or key in knowledge.get("native_facts", {})
    if kind == "evidence":
        return key in state.get("evidence", {})
    if kind == "decision":
        return key in state.get("decisions", {})
    if kind == "task":
        return key in state.get("task_contracts", {})
    if kind == "node":
        return any(node.get("id") == key for node in state.get("plan", {}).get("node", []))
    domain = state.get("extensions", {}).get("domain", {})
    if kind == "obligation":
        return key in domain.get("obligations", {})
    if kind == "outcome":
        return key in domain.get("outcome_revisions", {})
    return False


def _known_endpoint(state: State, value: Any) -> dict[str, str]:
    endpoint = validate_endpoint(value)
    need(_endpoint_exists(state, endpoint), "REFERENCE", f"unknown knowledge endpoint {endpoint_key(endpoint)}")
    return endpoint


def _strict(required: set[str]):
    def validate(value: Any) -> dict[str, Any]:
        exact(value, required)
        return copy.deepcopy(value)
    return validate


def _record_source(namespace: dict[str, Any], descriptor: dict[str, Any], event_id: str) -> None:
    source_id = descriptor["id"]
    need(source_id not in namespace["sources"], "DUPLICATE", f"captured source already exists: {source_id}")
    namespace["sources"][source_id] = {**descriptor, "capture_status": "current", "versions": [descriptor["content_sha256"]], "observation_candidates": [],
                                                "observations": [{"event_id": event_id, "status": "captured", "sha256": descriptor["content_sha256"], "bytes": descriptor["bytes"]}]}
    namespace["endpoint_status"][f"source:{source_id}"] = "current"


def _source_recorded(state: State, payload: dict[str, Any], event_id: str) -> None:
    descriptor = validate_source_descriptor(payload["source"])
    _record_source(_knowledge(state), descriptor, event_id)


def _native_facts_recorded(state: State, payload: dict[str, Any], event_id: str) -> None:
    exact(payload["capture"], {"source", "facts"})
    descriptor = validate_source_descriptor(payload["capture"]["source"])
    need(descriptor["source_kind"] == "vibevm_xml_spec", "SOURCE", "native facts require a VibeVM XML source capture")
    namespace = _knowledge(state)
    _record_source(namespace, descriptor, event_id)
    need(isinstance(payload["capture"]["facts"], list), "FACT", "native facts must be a list")
    seen: set[str] = set()
    for fact in payload["capture"]["facts"]:
        exact(fact, {"id", "address", "marker", "text", "normative_status", "source_id", "source_sha256", "observation_status", "acceptance_status"})
        fact_id = identity(fact["id"])
        need(fact_id not in seen and fact_id not in namespace["native_facts"], "DUPLICATE", f"duplicate native fact {fact_id}")
        seen.add(fact_id)
        need(fact["source_id"] == descriptor["id"] and fact["source_sha256"] == descriptor["content_sha256"], "FACT", "native fact source binding differs")
        string(fact["address"], "fact address")
        identity(fact["marker"])
        string(fact["text"], "fact text")
        need(fact["normative_status"] is None or isinstance(fact["normative_status"], str), "FACT", "invalid normative status")
        need(fact["observation_status"] == "unobserved" and fact["acceptance_status"] == "unassessed", "FACT", "source markup cannot confer observation or acceptance")
        namespace["native_facts"][fact_id] = copy.deepcopy(fact)
        namespace["endpoint_status"][f"fact:{fact_id}"] = "unknown"
        edge_id = identity(f"native:{fact_id}")
        namespace["dependencies"][edge_id] = {"id": edge_id, "prerequisite": {"kind": "source", "id": descriptor["id"]},
                                                   "dependent": {"kind": "fact", "id": fact_id}, "relation": "derived_from"}


def _source_recaptured(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    descriptor = validate_source_descriptor(payload["source"])
    source = namespace["sources"].get(descriptor["id"])
    need(source is not None, "REFERENCE", "recaptured source was not recorded")
    need(payload["previous_sha256"] == source["content_sha256"], "STALE", "recapture previous hash differs")
    need(descriptor["path"] == source["path"] and descriptor["root"] == source["root"], "SOURCE", "recapture locator binding differs")
    changed = descriptor["content_sha256"] != source["content_sha256"] or descriptor["bytes"] != source["bytes"]
    if changed:
        closure = invalidation_closure(state, [{"kind": "source", "id": descriptor["id"]}])
        namespace["invalidations"][event_id] = {"cause": {"kind": "source", "id": descriptor["id"]}, "transition": "recaptured", **closure}
        for endpoint in closure["affected"]:
            namespace["endpoint_status"][endpoint_key(endpoint)] = "stale"
    versions = list(source["versions"])
    versions.append(descriptor["content_sha256"])
    namespace["sources"][descriptor["id"]] = {**descriptor, "capture_status": "current", "versions": versions,
        "observation_candidates": source.get("observation_candidates", []),
        "observations": source["observations"] + [{"event_id": event_id, "status": "recaptured", "sha256": descriptor["content_sha256"], "bytes": descriptor["bytes"]}]}
    namespace["applicability"].pop(descriptor["id"], None)
    namespace["closures"].pop(f"source:{descriptor['id']}", None)
    namespace["endpoint_status"][f"source:{descriptor['id']}"] = "current"


def _validate_observation(value: Any) -> dict[str, Any]:
    exact(value, {"source_id", "observed"})
    identity(value["source_id"])
    exact(value["observed"], {"status", "sha256", "bytes", "detail"})
    observed = value["observed"]
    need(observed["status"] in {"current", "changed", "unavailable"}, "SOURCE", "unknown source observation")
    if observed["status"] == "unavailable":
        need(observed["sha256"] is None and observed["bytes"] is None and isinstance(observed["detail"], str), "SOURCE", "invalid unavailable observation")
    else:
        need(isinstance(observed["sha256"], str) and len(observed["sha256"]) == 64 and type(observed["bytes"]) is int and observed["bytes"] >= 0 and observed["detail"] is None, "SOURCE", "invalid content observation")
    return copy.deepcopy(value)


def invalidation_closure(state: State, roots: Iterable[dict[str, str]]) -> dict[str, Any]:
    """Walk only known dependency edges and expose any incomplete local boundary."""
    namespace = knowledge_state(state)
    edges: dict[str, list[dict[str, str]]] = {}
    for edge in namespace["dependencies"].values():
        edges.setdefault(endpoint_key(edge["prerequisite"]), []).append(edge["dependent"])
    queue = [validate_endpoint(root) for root in roots]
    visited: dict[str, dict[str, str]] = {}
    incomplete = False
    while queue:
        endpoint = queue.pop(0)
        key = endpoint_key(endpoint)
        if key in visited:
            continue
        visited[key] = endpoint
        closure = namespace["closures"].get(key)
        if not closure or closure["status"] != "complete":
            incomplete = True
        queue.extend(edges.get(key, []))
    affected = sorted(visited.values(), key=endpoint_key)
    return {"affected": affected, "incomplete_closure": incomplete}


def _source_observed(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    source = namespace["sources"].get(payload["source_id"])
    need(source is not None, "REFERENCE", "observed source was not recorded")
    observed = payload["observed"]
    if observed["status"] == "current":
        need(observed["sha256"] == source["content_sha256"] and observed["bytes"] == source["bytes"], "SOURCE", "current observation differs from capture")
    elif observed["status"] == "changed":
        need(observed["sha256"] != source["content_sha256"] or observed["bytes"] != source["bytes"], "SOURCE", "changed observation equals capture")
    source["capture_status"] = observed["status"]
    source["observations"].append({"event_id": event_id, **copy.deepcopy(observed)})
    source_key = f"source:{source['id']}"
    namespace["endpoint_status"][source_key] = "stale" if observed["status"] == "changed" else "current" if observed["status"] == "current" else "unknown"
    if observed["status"] == "changed":
        closure = invalidation_closure(state, [{"kind": "source", "id": source["id"]}])
        namespace["invalidations"][event_id] = {"cause": {"kind": "source", "id": source["id"]}, **closure}
        for endpoint in closure["affected"]:
            namespace["endpoint_status"][endpoint_key(endpoint)] = "stale"


def _validate_observation_candidate(value: Any) -> dict[str, Any]:
    exact(value, {"source_id", "observed", "claim", "artifact_refs"})
    validated = _validate_observation({"source_id": value["source_id"], "observed": value["observed"]})
    string(value["claim"], "source observation claim")
    validated["claim"] = value["claim"]
    validated["artifact_refs"] = strings(value["artifact_refs"], "source observation artifact refs")
    return validated


def _source_observation_candidate(state: State, payload: dict[str, Any], event_id: str) -> None:
    source = _knowledge(state)["sources"].get(payload["source_id"])
    need(source is not None, "REFERENCE", "observation candidate source was not recorded")
    source["observation_candidates"].append({**copy.deepcopy(payload), "event_id": event_id, "adjudication": "unverified"})


def _invalidate_upstream_closures(namespace: dict[str, Any], start: str, event_id: str) -> None:
    reverse: dict[str, list[str]] = {}
    for edge in namespace["dependencies"].values():
        reverse.setdefault(endpoint_key(edge["dependent"]), []).append(endpoint_key(edge["prerequisite"]))
    stack = [start]
    visited: set[str] = set()
    while stack:
        key = stack.pop()
        if key in visited:
            continue
        visited.add(key)
        prior = namespace["closures"].get(key)
        if prior and prior.get("status") == "complete":
            namespace["closure_history"].append({**copy.deepcopy(prior), "invalidated_by": event_id})
            namespace["closures"][key] = {**prior, "status": "unknown", "invalidated_by": event_id}
        stack.extend(reverse.get(key, []))


def _dependency_recorded(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    edge_id = identity(payload["id"])
    need(edge_id not in namespace["dependencies"], "DUPLICATE", f"knowledge dependency already exists: {edge_id}")
    prerequisite = _known_endpoint(state, payload["prerequisite"])
    dependent = _known_endpoint(state, payload["dependent"])
    need(prerequisite != dependent, "CYCLE", "self dependency is not allowed")
    need(payload["relation"] in DEPENDENCY_RELATIONS, "DEPENDENCY", "unknown knowledge dependency relation")
    adjacency: dict[str, list[str]] = {}
    for edge in namespace["dependencies"].values():
        adjacency.setdefault(endpoint_key(edge["prerequisite"]), []).append(endpoint_key(edge["dependent"]))
    wanted = endpoint_key(prerequisite)
    queue = [endpoint_key(dependent)]
    visited: set[str] = set()
    while queue:
        current = queue.pop()
        need(current != wanted, "CYCLE", "knowledge dependency would create a cycle")
        if current not in visited:
            visited.add(current)
            queue.extend(adjacency.get(current, []))
    _invalidate_upstream_closures(namespace, endpoint_key(prerequisite), event_id)
    namespace["dependencies"][edge_id] = {"id": edge_id, "prerequisite": prerequisite, "dependent": dependent, "relation": payload["relation"]}


def _closure_assessed(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    subject = _known_endpoint(state, payload["subject"])
    need(payload["status"] in {"complete", "incomplete", "unknown"}, "CLOSURE", "unknown closure status")
    need(isinstance(payload["boundary"], list) and isinstance(payload["missing"], list), "CLOSURE", "closure boundary/missing must be lists")
    boundary = [validate_endpoint(endpoint) for endpoint in payload["boundary"]]
    missing = [validate_endpoint(endpoint) for endpoint in payload["missing"]]
    need(payload["status"] != "complete" or not missing, "CLOSURE", "complete closure cannot list missing inputs")
    evidence_refs = strings(payload["evidence_refs"], "closure evidence refs")
    need(set(evidence_refs) <= state.get("evidence", {}).keys(), "REFERENCE", "unknown closure evidence")
    need(payload["status"] != "complete" or evidence_refs, "CLOSURE", "complete closure requires evidence")
    string(payload["basis"], "closure basis")
    namespace["closures"][endpoint_key(subject)] = {"subject": subject, "status": payload["status"], "boundary": boundary,
        "missing": missing, "evidence_refs": evidence_refs, "basis": payload["basis"], "event_id": event_id}


def _applicability_assessed(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    source_id = identity(payload["source_id"])
    source = namespace["sources"].get(source_id)
    need(source is not None, "REFERENCE", "applicability source was not recorded")
    need(payload["status"] in {"applicable", "not_applicable", "unknown"}, "APPLICABILITY", "unknown applicability result")
    scope = validate_scope(payload["scope"])
    evidence_refs = strings(payload["evidence_refs"], "applicability evidence refs")
    need(set(evidence_refs) <= state.get("evidence", {}).keys(), "REFERENCE", "unknown applicability evidence")
    need(payload["status"] != "applicable" or evidence_refs, "APPLICABILITY", "applicable assessment requires evidence")
    need(payload["status"] != "applicable" or scope["kind"] != "unassessed", "APPLICABILITY", "applicable assessment requires a bounded scope")
    string(payload["basis"], "applicability basis")
    namespace["applicability"][source_id] = {"status": payload["status"], "scope": scope, "evidence_refs": evidence_refs,
        "basis": payload["basis"], "source_sha256": source["content_sha256"], "event_id": event_id}


def _region(namespace: dict[str, Any], state: State, region_id: str) -> dict[str, Any]:
    region = namespace["regions"].get(region_id)
    if region is None:
        core = state.get("unknown_regions", {}).get(region_id)
        need(core is not None, "REFERENCE", "unknown knowledge region")
        region = {"id": region_id, "question": core["question"], "node_refs": copy.deepcopy(core["node_refs"]), "state": "unexamined",
                  "relevance": "unknown", "revision": 0, "history": [], "parents": [], "children": []}
        namespace["regions"][region_id] = region
    return region


def _region_transitioned(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    region = _region(namespace, state, identity(payload["region_id"]))
    need(payload["from"] == region["state"] and payload["to"] in REGION_STATES and payload["to"] != payload["from"], "REGION", "invalid/stale region transition")
    evidence_refs = strings(payload["evidence_refs"], "region evidence refs")
    need(set(evidence_refs) <= state.get("evidence", {}).keys(), "REFERENCE", "unknown region evidence")
    need(payload["to"] != "evidenced" or evidence_refs, "REGION", "evidenced region requires evidence")
    string(payload["reason"], "region transition reason")
    reopened = payload["from"] in {"evidenced", "invalidated"} and payload["to"] in {"unexamined", "bounded"}
    region["state"] = payload["to"]
    region["revision"] += 1
    region["history"].append({**copy.deepcopy(payload), "event_id": event_id, "transition": "reopened" if reopened else "advanced"})


def _region_relevance(state: State, payload: dict[str, Any], event_id: str) -> None:
    region = _region(_knowledge(state), state, identity(payload["region_id"]))
    need(payload["relevance"] in RELEVANCE, "REGION", "unknown region relevance")
    string(payload["reason"], "region relevance reason")
    region["relevance"] = payload["relevance"]
    region["revision"] += 1
    region["history"].append({**copy.deepcopy(payload), "event_id": event_id, "transition": "relevance"})


def _new_region(state: State, namespace: dict[str, Any], value: Any, parents: list[str]) -> dict[str, Any]:
    exact(value, {"id", "question", "node_refs", "relevance"})
    region_id = identity(value["id"])
    need(region_id not in namespace["regions"] and region_id not in state.get("unknown_regions", {}), "DUPLICATE", f"knowledge region already exists: {region_id}")
    string(value["question"], "region question")
    node_refs = strings(value["node_refs"], "region node refs")
    known_nodes = {node["id"] for node in state["plan"]["node"]}
    need(set(node_refs) <= known_nodes, "REFERENCE", "unknown region node")
    need(value["relevance"] in RELEVANCE, "REGION", "unknown region relevance")
    region = {**copy.deepcopy(value), "state": "unexamined", "revision": 1, "history": [], "parents": list(parents), "children": []}
    namespace["regions"][region_id] = region
    return region


def _region_split(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    parent = _region(namespace, state, identity(payload["region_id"]))
    need(isinstance(payload["children"], list) and len(payload["children"]) >= 2, "REGION", "split requires at least two child regions")
    string(payload["reason"], "region split reason")
    children = [_new_region(state, namespace, child, [parent["id"]]) for child in payload["children"]]
    parent["state"] = "invalidated"
    parent["revision"] += 1
    parent["children"].extend(child["id"] for child in children)
    parent["history"].append({"event_id": event_id, "transition": "split", "children": [child["id"] for child in children], "reason": payload["reason"]})


def _region_merged(state: State, payload: dict[str, Any], event_id: str) -> None:
    namespace = _knowledge(state)
    region_ids = strings(payload["region_ids"], "merged region ids")
    need(len(region_ids) >= 2 and len(set(region_ids)) == len(region_ids), "REGION", "merge requires distinct regions")
    origins = [_region(namespace, state, identity(region_id)) for region_id in region_ids]
    string(payload["reason"], "region merge reason")
    merged = _new_region(state, namespace, payload["merged"], region_ids)
    for origin in origins:
        origin["state"] = "invalidated"
        origin["revision"] += 1
        origin["children"].append(merged["id"])
        origin["history"].append({"event_id": event_id, "transition": "merged", "target": merged["id"], "reason": payload["reason"]})


KNOWLEDGE_HANDLERS: Mapping[str, HandlerSpec] = MappingProxyType({spec.kind: spec for spec in (
    HandlerSpec("knowledge.source-recorded", _strict({"source"}), _source_recorded),
    HandlerSpec("knowledge.native-facts-recorded", _strict({"capture"}), _native_facts_recorded),
    HandlerSpec("knowledge.source-recaptured", _strict({"previous_sha256", "source"}), _source_recaptured),
    HandlerSpec("knowledge.source-observation-recorded", _validate_observation_candidate, _source_observation_candidate),
    HandlerSpec("knowledge.source-observed", _validate_observation, _source_observed),
    HandlerSpec("knowledge.dependency-recorded", _strict({"id", "prerequisite", "dependent", "relation"}), _dependency_recorded),
    HandlerSpec("knowledge.closure-assessed", _strict({"subject", "status", "boundary", "missing", "evidence_refs", "basis"}), _closure_assessed),
    HandlerSpec("knowledge.applicability-assessed", _strict({"source_id", "status", "scope", "evidence_refs", "basis"}), _applicability_assessed),
    HandlerSpec("knowledge.region-transitioned", _strict({"region_id", "from", "to", "evidence_refs", "reason"}), _region_transitioned),
    HandlerSpec("knowledge.region-relevance-set", _strict({"region_id", "relevance", "reason"}), _region_relevance),
    HandlerSpec("knowledge.region-split", _strict({"region_id", "children", "reason"}), _region_split),
    HandlerSpec("knowledge.region-merged", _strict({"region_ids", "merged", "reason"}), _region_merged),
)})
