"""Read-only large-graph projections, pagination, search, and entity detail."""
from __future__ import annotations

import base64
import copy
from typing import Any, Iterable

from .common import Refusal, identity, need, packed, parse, sha
from .domain import domain_state
from .knowledge import knowledge_state
from .runtime import runtime_state
from .backend_sanitize import sanitize_event, sanitize_public_value

DEFAULT_PAGE = 100
MAX_PAGE = 500
ENTITY_KINDS = frozenset({
    "acceptance", "approach", "decision", "deferral", "edge", "evidence",
    "fact", "integration", "job", "node", "obligation", "outcome",
    "promotion", "region", "review", "source", "stage", "verification",
})


def _page_token(base_sha256: str, revision: int, query_sha256: str, offset: int) -> str:
    raw = packed({"base_sha256": base_sha256, "revision": revision, "query_sha256": query_sha256, "offset": offset})
    return base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=")


def _decode_token(token: str) -> dict[str, Any]:
    need(isinstance(token, str) and token, "CURSOR", "page cursor must be a string")
    try:
        raw = base64.urlsafe_b64decode(token + "=" * (-len(token) % 4))
        value = parse(raw)
    except (ValueError, TypeError) as exc:
        raise Refusal("CURSOR", "invalid page cursor") from exc
    need(isinstance(value, dict) and set(value) == {"base_sha256", "revision", "query_sha256", "offset"}, "CURSOR", "invalid page cursor")
    need(type(value["revision"]) is int and type(value["offset"]) is int and value["offset"] >= 0, "CURSOR", "invalid page cursor boundary")
    return value


def paginate(
    state: dict[str, Any],
    items: list[dict[str, Any]],
    query: dict[str, Any],
    *,
    limit: int = DEFAULT_PAGE,
    cursor: str | None = None,
) -> dict[str, Any]:
    need(type(limit) is int and 0 < limit <= MAX_PAGE, "PAGE", "page limit is outside 1..500")
    query_sha256 = sha(packed(query))
    offset = 0
    if cursor is not None:
        decoded = _decode_token(cursor)
        need(decoded["base_sha256"] == state["base_sha256"], "FOREIGN_BASE", "page cursor belongs to another base")
        need(decoded["revision"] == state["revision"], "PAGE_STALE", "page cursor belongs to another revision")
        need(decoded["query_sha256"] == query_sha256, "CURSOR", "page cursor belongs to another query")
        offset = decoded["offset"]
    need(offset <= len(items), "CURSOR_GAP", "page cursor is beyond the result set")
    page = items[offset:offset + limit]
    next_offset = offset + len(page)
    complete = next_offset >= len(items)
    return {
        "offset": offset,
        "limit": limit,
        "returned": len(page),
        "total": len(items),
        "complete": complete,
        "next_cursor": None if complete else _page_token(state["base_sha256"], state["revision"], query_sha256, next_offset),
        "items": page,
    }


def _plan_nodes(state: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {row["id"]: row for row in state["plan"]["node"]}


def _jobs_by_work(runtime: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    result: dict[str, list[dict[str, Any]]] = {}
    for row in runtime["jobs"].values():
        result.setdefault(row["work_id"], []).append(row)
    return result


def _regions(state: dict[str, Any], knowledge: dict[str, Any]) -> dict[str, dict[str, Any]]:
    regions = {
        key: {
            "id": key, "question": row["question"], "node_refs": copy.deepcopy(row["node_refs"]),
            "state": "unexamined", "relevance": "unknown", "revision": 0,
            "history": [], "parents": [], "children": [],
        }
        for key, row in state.get("unknown_regions", {}).items()
    }
    regions.update(copy.deepcopy(knowledge["regions"]))
    return regions


def _knowledge_for_node(regions: dict[str, dict[str, Any]], node_id: str) -> str:
    related = [
        row for row in regions.values()
        if node_id in row.get("node_refs", []) or node_id in row.get("work_ids", [])
    ]
    if any(row.get("state") == "invalidated" for row in related):
        return "invalidated"
    if any(row.get("state") == "unexamined" for row in related):
        return "unexamined"
    if any(row.get("state") == "bounded" for row in related):
        return "bounded"
    if related and all(row.get("state") == "evidenced" for row in related):
        return "evidenced"
    return "unmapped"


def _visibility_for_node(regions: dict[str, dict[str, Any]], node_id: str, state: str) -> str:
    if state in {"dropped", "superseded"}:
        return "excluded"
    related = [
        row for row in regions.values()
        if node_id in row.get("node_refs", []) or node_id in row.get("work_ids", [])
    ]
    if related and all(row.get("relevance") == "irrelevant" for row in related):
        return "excluded"
    return "present"


def node_summaries(state: dict[str, Any]) -> list[dict[str, Any]]:
    domain, knowledge, runtime = domain_state(state), knowledge_state(state), runtime_state(state)
    regions = _regions(state, knowledge)
    jobs = _jobs_by_work(runtime)
    summaries = []
    for node_id, node in sorted(_plan_nodes(state).items()):
        update = domain["work_updates"].get(node_id, {})
        classification = state.get("classifications", {}).get(node_id, {})
        current_jobs = jobs.get(node_id, [])
        execution_state = current_jobs[-1]["state"] if current_jobs else update.get("state", node["state"])
        work_state = update.get("state", node["state"])
        summaries.append({
            "id": node_id,
            "entity_kind": "node",
            "title": update.get("title", node["title"]),
            "node_kind": node["kind"],
            "parent_id": node["parent"] or None,
            "state": work_state,
            "order": node["order"],
            "visual": {
                "knowledge_state": _knowledge_for_node(regions, node_id),
                "work_type": classification.get("work_type", "unclassified"),
                "maturity": classification.get("maturity", "unspecified"),
                "execution_state": execution_state,
                "visibility": _visibility_for_node(regions, node_id, work_state),
            },
        })
    return summaries


def _other_entity_summaries(state: dict[str, Any], kinds: set[str]) -> list[dict[str, Any]]:
    collections = _entity_collections(state)
    endpoint_status = knowledge_state(state)["endpoint_status"]
    summaries = []
    for kind in sorted(kinds - {"node"}):
        for key, row in sorted(collections[kind].items()):
            stale = _stale_entity(endpoint_status, kind, key, row)
            summary = {
                "id": key, "entity_kind": kind, "label": _label(kind, key, row),
                "state": row.get("state", row.get("status")),
                "visual": {
                    "knowledge_state": "invalidated" if stale else row.get("state") if kind == "region" else None,
                    "execution_state": row.get("state") if kind in {"job", "verification"} else None,
                    "visibility": "excluded" if row.get("state") in {"excluded", "dropped", "superseded"} else "present",
                },
            }
            if kind == "edge":
                summary.update({key: row[key] for key in ("source", "target", "relation")})
            summaries.append(summary)
    return summaries


def graph_edges(state: dict[str, Any]) -> list[dict[str, Any]]:
    edges: list[dict[str, Any]] = []
    for node in state["plan"]["node"]:
        if node["parent"]:
            edge = {"source": node["parent"], "target": node["id"], "relation": "contains"}
            edges.append({"id": f"edge:{sha(packed(edge))}", **edge})
        for dependency in node["depends_on"]:
            edge = {"source": node["id"], "target": dependency, "relation": "depends_on"}
            edges.append({"id": f"edge:{sha(packed(edge))}", **edge})
    for edge in knowledge_state(state)["dependencies"].values():
        edges.append({
            "id": f"knowledge:{edge['id']}",
            "source": f"{edge['prerequisite']['kind']}:{edge['prerequisite']['id']}",
            "target": f"{edge['dependent']['kind']}:{edge['dependent']['id']}",
            "relation": edge["relation"],
        })
    domain = domain_state(state)
    for owner in domain["ownership"]:
        edge = {
            "source": owner["work_id"],
            "target": f"obligation:{owner['obligation_id']}",
            "relation": owner["role"],
        }
        edges.append({"id": f"ownership:{sha(packed(edge))}", **edge})
    return sorted(edges, key=lambda row: row["id"])


def overview(
    state: dict[str, Any],
    *,
    limit: int = DEFAULT_PAGE,
    cursor: str | None = None,
    states: Iterable[str] = (),
    work_types: Iterable[str] = (),
    knowledge_states: Iterable[str] = (),
    entity_kinds: Iterable[str] = (),
) -> dict[str, Any]:
    requested_kinds = sorted(set(entity_kinds))
    need(set(requested_kinds) <= ENTITY_KINDS, "GRAPH", "unknown overview entity kind")
    effective_kinds = set(requested_kinds or ["node"])
    filters = {
        "states": sorted(set(states)),
        "work_types": sorted(set(work_types)),
        "knowledge_states": sorted(set(knowledge_states)),
        "entity_kinds": requested_kinds,
    }
    items = (node_summaries(state) if "node" in effective_kinds else []) + _other_entity_summaries(state, effective_kinds)
    if filters["states"]:
        items = [row for row in items if row["state"] in filters["states"]]
    if filters["work_types"]:
        items = [row for row in items if row["visual"].get("work_type") in filters["work_types"]]
    if filters["knowledge_states"]:
        items = [row for row in items if row["visual"].get("knowledge_state") in filters["knowledge_states"]]
    items.sort(key=lambda row: (row["entity_kind"], row["id"]))
    page = paginate(state, items, {"kind": "overview", "effective_entity_kinds": sorted(effective_kinds), **filters}, limit=limit, cursor=cursor)
    domain, knowledge, runtime = domain_state(state), knowledge_state(state), runtime_state(state)
    return {
        "schema": "zap-graph-overview/1",
        "base_sha256": state["base_sha256"],
        "revision": state["revision"],
        "cursor": state["revision"],
        "filters": filters,
        "page": page,
        "counts": {
            "nodes": len(state["plan"]["node"]),
            "edges": len(graph_edges(state)),
            "regions": len(_regions(state, knowledge)),
            "decisions": len(state["decisions"]),
            "jobs": len(runtime["jobs"]),
            "outcomes": len(domain["outcome_revisions"]),
            "obligations": len(domain["obligations"]),
        },
        "data_state": {
            "page_complete": page["complete"],
            "unloaded_items": page["total"] - page["offset"] - page["returned"],
            "filtered": bool(requested_kinds or filters["states"] or filters["work_types"] or filters["knowledge_states"]),
            "knowledge_unknown_is_not_unloaded": True,
        },
        "visual_dimensions": {
            "knowledge": "visual.knowledge_state",
            "work_type": "visual.work_type",
            "execution": "visual.execution_state",
            "structure": "node_kind",
            "edge_semantics": "relation",
            "visibility": "visual.visibility",
            "colors_or_camera": None,
        },
    }


def subgraph(
    state: dict[str, Any],
    node_id: str,
    *,
    depth: int = 1,
    limit: int = DEFAULT_PAGE,
    cursor: str | None = None,
) -> dict[str, Any]:
    node_id = identity(node_id)
    need(type(depth) is int and 0 <= depth <= 5, "GRAPH", "subgraph depth is outside 0..5")
    nodes = {row["id"]: row for row in node_summaries(state)}
    need(node_id in nodes, "REFERENCE", "unknown graph node")
    edges = graph_edges(state)
    selected = {node_id}
    for _ in range(depth):
        selected |= {
            endpoint
            for edge in edges
            if edge["source"] in selected or edge["target"] in selected
            for endpoint in (edge["source"], edge["target"])
            if endpoint in nodes
        }
    items = [nodes[key] for key in sorted(selected)]
    page = paginate(state, items, {"kind": "subgraph", "node_id": node_id, "depth": depth}, limit=limit, cursor=cursor)
    returned = {row["id"] for row in page["items"]}
    return {
        "schema": "zap-subgraph/1",
        "base_sha256": state["base_sha256"],
        "revision": state["revision"],
        "root_id": node_id,
        "cursor": state["revision"],
        "depth": depth,
        "page": page,
        "edges": [edge for edge in edges if edge["source"] in returned and edge["target"] in returned],
    }


def _entity_collections(state: dict[str, Any]) -> dict[str, dict[str, Any]]:
    domain, knowledge, runtime = domain_state(state), knowledge_state(state), runtime_state(state)
    result = {
        "node": _plan_nodes(state),
        "edge": {row["id"]: row for row in graph_edges(state)},
        "region": _regions(state, knowledge),
        "decision": state["decisions"],
        "evidence": state["evidence"],
        "fact": {**state["facts"], **knowledge["native_facts"]},
        "source": knowledge["sources"],
        "job": runtime["jobs"],
        "verification": runtime["verification_jobs"],
        "outcome": domain["outcome_revisions"],
        "obligation": domain["obligations"],
        "review": domain["reviews"],
        "deferral": domain["deferrals"],
        "stage": domain["stages"],
        "acceptance": domain["acceptances"],
        "integration": domain["integration_acceptances"],
        "promotion": domain["promotions"],
        "approach": state["approaches"],
    }
    need(set(result) == ENTITY_KINDS, "BACKEND", "entity collection registry differs")
    return result


def _label(kind: str, key: str, row: dict[str, Any]) -> str:
    for field in ("title", "summary", "statement", "question", "claim", "path", "id"):
        value = row.get(field)
        if isinstance(value, str) and value:
            return value
    return f"{kind}:{key}"


def search(
    state: dict[str, Any],
    query: str,
    *,
    kinds: Iterable[str] = (),
    limit: int = DEFAULT_PAGE,
    cursor: str | None = None,
) -> dict[str, Any]:
    need(isinstance(query, str) and query.strip(), "SEARCH", "search query is required")
    needle = query.casefold().strip()
    selected_kinds = sorted(set(kinds))
    collections = _entity_collections(state)
    if selected_kinds:
        need(set(selected_kinds) <= set(collections), "SEARCH", "unknown entity kind filter")
    results = []
    for kind in selected_kinds or sorted(collections):
        for key, row in collections[kind].items():
            if not isinstance(row, dict):
                continue
            label = _label(kind, key, row)
            haystack = (key + " " + label + " " + repr(row)).casefold()
            if needle in haystack:
                results.append({"kind": kind, "id": key, "label": label})
    results.sort(key=lambda row: (row["kind"], row["label"].casefold(), row["id"]))
    return {
        "schema": "zap-search/1",
        "base_sha256": state["base_sha256"],
        "revision": state["revision"],
        "cursor": state["revision"],
        "query": query,
        "kinds": selected_kinds,
        "page": paginate(state, results, {"kind": "search", "query": needle, "kinds": selected_kinds}, limit=limit, cursor=cursor),
    }


def _mentions(value: Any, target: str) -> bool:
    if isinstance(value, dict):
        return any(_mentions(item, target) for item in value.values())
    if isinstance(value, list):
        return any(_mentions(item, target) for item in value)
    return value == target


def _stale_entity(endpoint_status: dict[str, Any], kind: str, entity_id: str, row: dict[str, Any]) -> bool:
    stale_markers = {"changed", "invalidated", "stale", "unavailable"}
    endpoint_keys = [f"{kind}:{entity_id}"]
    if kind == "node":
        endpoint_keys.append(f"task:{entity_id}")
    return (
        any(row.get(key) in stale_markers for key in ("state", "status", "capture_status"))
        or row.get("invalidated_by") is not None
        or any(endpoint_status.get(key) == "stale" for key in endpoint_keys)
    )


def entity_detail(state: dict[str, Any], events: list[dict[str, Any]], kind: str, entity_id: str) -> dict[str, Any]:
    kind, entity_id = identity(kind), identity(entity_id)
    collections = _entity_collections(state)
    need(kind in collections and entity_id in collections[kind], "REFERENCE", "unknown entity")
    row = sanitize_public_value(copy.deepcopy(collections[kind][entity_id]))
    history = [sanitize_event(event) for event in events if _mentions(event.get("payload"), entity_id)]
    unavailable_fields: list[str] = []
    provenance = "event_history" if history else "derived" if kind == "edge" else "base_import" if kind == "node" else "unavailable"
    detail: dict[str, Any] = {
        "entity": row,
        "history": history,
        "availability": {
            "content": "guarded_handle" if kind == "source" else "available",
            "provenance": provenance,
            "history": "available" if history else "unavailable",
            "stale": _stale_entity(knowledge_state(state)["endpoint_status"], kind, entity_id, row),
            "unavailable_fields": unavailable_fields,
        },
    }
    if kind == "source":
        row.pop("root", None)
    if kind == "node":
        domain, knowledge = domain_state(state), knowledge_state(state)
        task_history = domain["task_contracts"].get(entity_id)
        current_contract = None
        if task_history:
            current_contract = next((item for item in task_history["versions"] if item["version"] == task_history["active_version"]), None)
        evidence = {
            key: copy.deepcopy(item)
            for key, item in state["evidence"].items()
            if entity_id in item.get("node_refs", [])
        }
        regions = {
            key: copy.deepcopy(item)
            for key, item in _regions(state, knowledge).items()
            if entity_id in item.get("node_refs", []) or entity_id in item.get("work_ids", [])
        }
        edges = [edge for edge in graph_edges(state) if edge["source"] == entity_id or edge["target"] == entity_id]
        contract = current_contract["contract"] if current_contract else state["task_contracts"].get(entity_id)
        if contract is None:
            unavailable_fields.extend(["contract", "steps", "checks", "source_handles"])
        detail.update({
            "classification": copy.deepcopy(state.get("classifications", {}).get(entity_id)),
            "work_update": copy.deepcopy(domain["work_updates"].get(entity_id)),
            "contract_history": copy.deepcopy(task_history),
            "contract": copy.deepcopy(contract),
            "criteria": copy.deepcopy(row.get("acceptance", [])),
            "steps": copy.deepcopy(contract.get("steps", []) if isinstance(contract, dict) else []),
            "checks": copy.deepcopy(contract.get("checks", []) if isinstance(contract, dict) else []),
            "source_handles": copy.deepcopy(contract.get("source_handles", []) if isinstance(contract, dict) else []),
            "evidence": evidence,
            "regions": regions,
            "edges": edges,
            "obligation_ids": sorted({
                owner["obligation_id"] for owner in domain["ownership"] if owner["work_id"] == entity_id
            }),
        })
    return {
        "schema": "zap-entity-detail/1",
        "base_sha256": state["base_sha256"],
        "revision": state["revision"],
        "cursor": state["revision"],
        "kind": kind,
        "id": entity_id,
        "detail": detail,
    }
