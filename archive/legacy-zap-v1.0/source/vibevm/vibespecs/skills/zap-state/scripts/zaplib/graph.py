"""MUP graph, task-contract, and structural frontier validation."""
from __future__ import annotations

import copy
from typing import Any

from .common import KINDS, STATES, TASK_FIELDS, TERMINAL, identity, need, string, strings


def acyclic(graph: dict[str, list[str]]) -> None:
    """Validate references and cycles with an iterative DFS safe for deep plans."""
    active: set[str] = set()
    done: set[str] = set()
    for root in graph:
        if root in done:
            continue
        active.add(root)
        stack: list[tuple[str, Any]] = [(root, iter(graph[root]))]
        while stack:
            key, edges = stack[-1]
            try:
                dependency = next(edges)
            except StopIteration:
                stack.pop()
                active.remove(key)
                done.add(key)
                continue
            need(dependency in graph, "REFERENCE", f"unknown node {dependency}")
            need(dependency not in active, "CYCLE", f"cycle through {dependency}")
            if dependency in done:
                continue
            active.add(dependency)
            stack.append((dependency, iter(graph[dependency])))


def validate_plan(plan: Any) -> tuple[dict[str, dict[str, Any]], dict[str, list[str]], dict[str, list[str]]]:
    need(isinstance(plan, dict) and type(plan.get("schema")) is int and plan["schema"] == 1, "PLAN", "MUP schema must be 1")
    identity(plan.get("plan_id"))
    need(type(plan.get("revision")) is int and plan["revision"] >= 0, "PLAN", "invalid plan revision")
    nodes: dict[str, dict[str, Any]] = {}
    mandates: dict[str, dict[str, Any]] = {}
    for field, target in (("node", nodes), ("mandate", mandates)):
        need(isinstance(plan.get(field), list), "PLAN", f"missing {field} array")
        for row in plan[field]:
            need(isinstance(row, dict), "PLAN", f"invalid {field}")
            key = identity(row.get("id"))
            need(key not in target, "DUPLICATE", f"duplicate {field} {key}")
            target[key] = row
    root = plan.get("root_node")
    need(root in nodes and plan.get("current_node") in nodes, "REFERENCE", "root/current node missing")
    parents: dict[str, list[str]] = {}
    dependencies: dict[str, list[str]] = {}
    children = {key: [] for key in nodes}
    for key, row in nodes.items():
        parent = string(row.get("parent"), "parent", empty=True)
        need(not parent or parent in nodes, "REFERENCE", f"missing parent of {key}")
        parents[key] = [parent] if parent else []
        if parent:
            children[parent].append(key)
        for field in ("depends_on", "mandates", "acceptance", "evidence"):
            strings(row.get(field), field)
        need(set(row["depends_on"]) <= nodes.keys() and set(row["mandates"]) <= mandates.keys(), "REFERENCE", f"missing reference in {key}")
        string(row.get("title"), "title")
        need(row.get("state") in STATES and row.get("kind") in KINDS and type(row.get("order")) is int, "PLAN", f"invalid node shape {key}")
    acyclic(parents)
    need(not nodes[root]["parent"], "PLAN", "root has parent")
    for key in nodes:
        cursor, inherited = key, []
        while cursor:
            inherited.extend(nodes[cursor]["depends_on"])
            if cursor != root:
                need(nodes[cursor]["parent"], "PLAN", f"node outside root {key}")
            cursor = nodes[cursor]["parent"]
        dependencies[key] = list(dict.fromkeys(inherited))
    graph = {
        key: inherited + [child for child in children[key] if nodes[child]["state"] not in TERMINAL - {"accepted"}]
        for key, inherited in dependencies.items()
    }
    acyclic(graph)
    for key, row in nodes.items():
        if row["state"] == "accepted":
            need(row["evidence"] and all(nodes[dependency]["state"] in TERMINAL for dependency in graph[key]), "PLAN", f"accepted node lacks complete evidence/prerequisites: {key}")
        for mandate_id in row["mandates"]:
            need(key in mandates[mandate_id].get("nodes", []), "REFERENCE", f"one-way mandate {mandate_id}")
    for mandate_id, row in mandates.items():
        string(row.get("text"), "mandate text")
        string(row.get("disposition"), "mandate disposition")
        references = strings(row.get("nodes"), "mandate nodes")
        need(set(references) <= nodes.keys() and all(mandate_id in nodes[key]["mandates"] for key in references), "REFERENCE", f"invalid mandate links {mandate_id}")
    return nodes, dependencies, children


def validate_tasks(groups: Any, nodes: dict[str, dict[str, Any]]) -> dict[str, dict[str, Any]]:
    tasks: dict[str, dict[str, Any]] = {}
    parents: set[str] = set()
    for group in groups:
        need(isinstance(group, dict), "TASK", "invalid task group")
        parent = identity(group.get("id"))
        need(parent in nodes and parent not in parents, "TASK", f"unknown/duplicate task group {parent}")
        parents.add(parent)
        need(isinstance(group.get("tasks"), list) and group["tasks"], "TASK", "empty task group")
        for task in group["tasks"]:
            need(isinstance(task, dict) and TASK_FIELDS <= task.keys(), "TASK", "incomplete task contract")
            key = identity(task["id"])
            need(key in nodes and key not in tasks and nodes[key]["parent"] == parent, "TASK", f"unknown/duplicate/wrong-parent task {key}")
            for field in TASK_FIELDS - {"id"}:
                if field in {"title", "goal", "safe_stop", "commit_subject"}:
                    string(task[field], field)
                else:
                    need(isinstance(task[field], list) and all(isinstance(item, str) for item in task[field]), "TASK", f"invalid task field {field}")
            tasks[key] = copy.deepcopy(task)
    return tasks


def frontier(state: dict[str, Any]) -> list[str]:
    nodes, dependencies, children = validate_plan(state["plan"])
    ready = []
    for key, node in nodes.items():
        if children[key] or node["state"] not in {"planned", "ready", "active"}:
            continue
        cursor, suppressed = node["parent"], False
        while cursor:
            suppressed |= nodes[cursor]["state"] in {"blocked", "deferred", "dropped", "superseded"}
            cursor = nodes[cursor]["parent"]
        if not suppressed and all(nodes[dependency]["state"] in TERMINAL for dependency in dependencies[key]):
            ready.append(key)
    return sorted(ready, key=lambda node_id: (nodes[node_id]["order"], node_id))
