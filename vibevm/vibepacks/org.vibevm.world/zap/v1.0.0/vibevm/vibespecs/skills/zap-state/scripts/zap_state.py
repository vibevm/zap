#!/usr/bin/env python3
"""Original ZAP reference data kernel: import, journal, projection and stop probes.

This is not an executor or an owner-authority adapter. Imported stores stay draft.
The file lock coordinates this CLI, not a malicious writer with filesystem access.
"""
from __future__ import annotations

import argparse
import base64
import copy
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import sys
import tomllib
import uuid
from contextlib import contextmanager

SCHEMA = "zap/1"
ID = re.compile(r"^[A-Za-z0-9._:-]+$")
STATES = {"planned", "ready", "active", "candidate", "accepted", "blocked", "deferred", "dropped", "superseded"}
KINDS = {"portfolio", "campaign", "phase", "workstream", "group", "atom", "gate", "horizon"}
TERMINAL = {"accepted", "deferred", "dropped", "superseded"}
WORK_TYPES = {"evidence", "decision", "change", "verification", "integration"}
MATURITY = {"unspecified", "prototype", "functional", "productized"}
TASK_FIELDS = {"id", "title", "goal", "read_paths", "write_paths", "steps", "positive_cases", "negative_cases", "checks", "acceptance", "safe_stop", "commit_subject", "notes"}
COLLECTIONS = ("unknown_regions", "evidence", "facts", "decisions", "approaches")


class Refusal(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def need(value, code, message):
    if not value:
        raise Refusal(code, message)


def exact(value, required, optional=()):
    need(isinstance(value, dict) and set(required) <= set(value) <= set(required) | set(optional),
         "FIELDS", f"expected fields {sorted(required)}; optional {sorted(optional)}")


def string(value, label, empty=False):
    need(isinstance(value, str) and (empty or value.strip()), "VALUE", f"invalid {label}")
    return value


def strings(value, label):
    need(isinstance(value, list) and all(isinstance(v, str) and v.strip() for v in value), "VALUE", f"invalid {label}")
    return value


def identity(value):
    need(isinstance(value, str) and ID.fullmatch(value), "IDENTITY", "invalid stable identity")
    return value


def wire(value):
    """JSON representation also preserves uncommon TOML dates/nonfinite numbers."""
    if isinstance(value, (dt.datetime, dt.date, dt.time)):
        return {"$zap_type": type(value).__name__, "value": value.isoformat()}
    if isinstance(value, float) and not math.isfinite(value):
        return {"$zap_type": "float", "value": repr(value)}
    if isinstance(value, dict):
        items = {k: wire(v) for k, v in value.items()}
        return {"$zap_type": "mapping", "value": list(items.items())} if "$zap_type" in value else items
    if isinstance(value, list):
        return [wire(v) for v in value]
    return value


def unwire(value):
    if isinstance(value, list):
        return [unwire(v) for v in value]
    if isinstance(value, dict):
        if "$zap_type" in value:
            exact(value, {"$zap_type", "value"})
            kind, raw = value["$zap_type"], value["value"]
            if kind == "mapping":
                return {k: unwire(v) for k, v in raw}
            if kind in {"datetime", "date", "time"}:
                return getattr(dt, kind).fromisoformat(raw)
            if kind == "float" and raw in {"nan", "inf", "-inf"}:
                return float(raw)
            raise Refusal("ENCODING", "unknown tagged TOML value")
        return {k: unwire(v) for k, v in value.items()}
    return value


def packed(value):
    return json.dumps(wire(value), ensure_ascii=True, separators=(",", ":"), sort_keys=True, allow_nan=False).encode("utf-8")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, "DUPLICATE", f"duplicate JSON member {key}")
        result[key] = value
    return result


def parse(raw, tagged=False):
    value = json.loads(raw, object_pairs_hook=unique_object,
                       parse_constant=lambda _: (_ for _ in ()).throw(Refusal("ENCODING", "non-JSON number")))
    return unwire(value) if tagged else value


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def acyclic(graph):
    active, done = set(), set()
    def visit(key):
        need(key in graph, "REFERENCE", f"unknown node {key}")
        need(key not in active, "CYCLE", f"cycle through {key}")
        if key in done:
            return
        active.add(key)
        for dep in graph[key]:
            visit(dep)
        active.remove(key)
        done.add(key)
    for key in graph:
        visit(key)


def validate_plan(plan):
    need(isinstance(plan, dict) and type(plan.get("schema")) is int and plan["schema"] == 1, "PLAN", "MUP schema must be 1")
    identity(plan.get("plan_id"))
    need(type(plan.get("revision")) is int and plan["revision"] >= 0, "PLAN", "invalid plan revision")
    nodes, mandates = {}, {}
    for field, target in (("node", nodes), ("mandate", mandates)):
        need(isinstance(plan.get(field), list), "PLAN", f"missing {field} array")
        for row in plan[field]:
            need(isinstance(row, dict), "PLAN", f"invalid {field}")
            key = identity(row.get("id"))
            need(key not in target, "DUPLICATE", f"duplicate {field} {key}")
            target[key] = row
    root = plan.get("root_node")
    need(root in nodes and plan.get("current_node") in nodes, "REFERENCE", "root/current node missing")
    parents, dependencies, children = {}, {}, {key: [] for key in nodes}
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
        cursor, deps = key, []
        while cursor:
            deps.extend(nodes[cursor]["depends_on"])
            if cursor != root:
                need(nodes[cursor]["parent"], "PLAN", f"node outside root {key}")
            cursor = nodes[cursor]["parent"]
        dependencies[key] = list(dict.fromkeys(deps))
    graph = {key: deps + [c for c in children[key] if nodes[c]["state"] not in TERMINAL - {"accepted"}] for key, deps in dependencies.items()}
    acyclic(graph)
    for key, row in nodes.items():
        if row["state"] == "accepted":
            need(row["evidence"] and all(nodes[d]["state"] in TERMINAL for d in graph[key]), "PLAN", f"accepted node lacks complete evidence/prerequisites: {key}")
        for mid in row["mandates"]:
            need(key in mandates[mid].get("nodes", []), "REFERENCE", f"one-way mandate {mid}")
    for mid, row in mandates.items():
        string(row.get("text"), "mandate text")
        string(row.get("disposition"), "mandate disposition")
        refs = strings(row.get("nodes"), "mandate nodes")
        need(set(refs) <= nodes.keys() and all(mid in nodes[key]["mandates"] for key in refs), "REFERENCE", f"invalid mandate links {mid}")
    return nodes, dependencies, children


def validate_tasks(groups, nodes):
    tasks, parents = {}, set()
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
                    need(isinstance(task[field], list) and all(isinstance(v, str) for v in task[field]), "TASK", f"invalid task field {field}")
            tasks[key] = copy.deepcopy(task)
    return tasks


def safe_path(path):
    path = Path(path).absolute()
    for part in [path] + list(path.parents):
        if part.exists() or part.is_symlink():
            info = part.lstat()
            need(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0), "PATH", f"symlink/reparse path refused: {part}")
    return path.resolve()


def write_new(path, raw):
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def capture(path, raw):
    return {"path": str(path), "sha256": sha(raw), "raw_base64": base64.b64encode(raw).decode("ascii")}


def import_mup(plan_path, tasks_dir, out):
    plan_path, tasks_dir, out = map(safe_path, (plan_path, tasks_dir, out))
    need(not out.exists(), "OUTPUT_EXISTS", "import requires a fresh output directory")
    need(not plan_path.is_relative_to(out) and not out.is_relative_to(plan_path) and not tasks_dir.is_relative_to(out) and not out.is_relative_to(tasks_dir), "PATH", "output intersects imported sources")
    raw = plan_path.read_bytes()
    plan = tomllib.loads(raw.decode("utf-8"))
    nodes, _, _ = validate_plan(plan)
    groups, sources = [], []
    paths = sorted(tasks_dir.glob("*.json"))
    need(paths, "TASK", "no task files")
    for path in paths:
        path = safe_path(path)
        source = path.read_bytes()
        groups.append(parse(source))
        sources.append(capture(path, source))
    tasks = validate_tasks(groups, nodes)
    base = {"schema": SCHEMA, "plan": plan, "task_contracts": tasks,
            "sources": {"plan": capture(plan_path, raw), "tasks": sources}}
    base_bytes = packed(base) + b"\n"
    receipt = {"seq": 0, "revision": 0, "previous_revision": None,
               "event_id": str(uuid.uuid4()), "kind": "store.imported", "base_sha256": sha(base_bytes)}
    out.mkdir(parents=True, exist_ok=False)
    write_new(out / "base.json", base_bytes)
    write_new(out / "events.jsonl", packed(receipt) + b"\n")
    return {"ok": True, "store": str(out), "schema": SCHEMA, "execution_mode": "draft",
            "plan_nodes": len(nodes), "task_contracts": len(tasks), "base_sha256": receipt["base_sha256"],
            "source_sha256": {"plan": sha(raw), "tasks": {row["path"]: row["sha256"] for row in sources}},
            "owner_contract_activated": False}


def initial_state(base, base_hash):
    nodes, _, _ = validate_plan(base["plan"])
    return {"schema": SCHEMA, "revision": 0, "base_sha256": base_hash,
            "execution_mode": "draft", "owner_contract": None,
            "plan": copy.deepcopy(base["plan"]), "task_contracts": copy.deepcopy(base["task_contracts"]),
            "classifications": {key: {"work_type": "unclassified", "maturity": "unspecified", "assertion_status": "declared"} for key in nodes},
            **{key: {} for key in COLLECTIONS}, "relations": []}


def refs(state, values, collection):
    strings(values, collection + " refs")
    available = {n["id"] for n in state["plan"]["node"]} if collection == "node" else state[collection].keys()
    need(set(values) <= set(available), "REFERENCE", f"unknown {collection} reference")


def relations(state, kind, key, payload):
    for field, target, relation in (("node_refs", "node", "about"), ("evidence_refs", "evidence", "supported_by")):
        for ref in payload.get(field, []):
            state["relations"].append({"source_kind": kind, "source_id": key, "relation": relation, "target_kind": target, "target_id": ref})


def add_record(state, collection, payload):
    key = identity(payload["id"])
    need(key not in state[collection], "DUPLICATE", f"record already exists: {key}")
    for field, target in (("node_refs", "node"), ("evidence_refs", "evidence")):
        if field in payload:
            refs(state, payload[field], target)
    state[collection][key] = copy.deepcopy(payload)
    relations(state, collection, key, payload)


def refine(state, payload):
    exact(payload, {"parent_id", "nodes", "edges", "coverage"})
    nodes, _, _ = validate_plan(state["plan"])
    parent = payload["parent_id"]
    need(parent in nodes and nodes[parent]["state"] not in TERMINAL, "REFINEMENT", "cannot refine missing/terminal parent")
    need(isinstance(payload["nodes"], list) and payload["nodes"], "REFINEMENT", "refinement must add nodes")
    additions = {identity(n.get("id")): n for n in payload["nodes"] if isinstance(n, dict)}
    need(len(additions) == len(payload["nodes"]) and not additions.keys() & nodes.keys(), "REFINEMENT", "duplicate/malformed added nodes")
    for node in additions.values():
        exact(node, {"id", "parent", "title", "kind", "state", "order", "depends_on", "mandates", "acceptance", "evidence"},
              {"zoom", "contract", "contract_sha256"})
        need(node.get("parent") in additions or node.get("parent") == parent, "REFINEMENT", "new node leaves parent subtree")
        need(node.get("state") == "planned", "REFINEMENT", "new nodes must remain planned")
        need(set(nodes[parent]["mandates"]) <= set(node.get("mandates", [])), "REFINEMENT", "parent mandates dropped")
    state["plan"]["node"].extend(copy.deepcopy(payload["nodes"]))
    for mandate in state["plan"]["mandate"]:
        mandate["nodes"].extend(key for key, n in additions.items() if mandate["id"] in n.get("mandates", []) and key not in mandate["nodes"])
    validate_plan(state["plan"])
    all_nodes = {n["id"]: n for n in state["plan"]["node"]}
    need(isinstance(payload["edges"], list), "REFINEMENT", "edges must be a list")
    for edge in payload["edges"]:
        exact(edge, {"node_id", "depends_on"})
        key = edge["node_id"]
        need(key in all_nodes and all_nodes[key]["state"] not in TERMINAL, "REFINEMENT", "cannot alter terminal prerequisites")
        cursor = key
        while cursor and cursor != parent:
            cursor = all_nodes[cursor]["parent"]
        need(cursor == parent, "REFINEMENT", "edge target leaves refinement subtree")
        for dep in strings(edge["depends_on"], "dependencies"):
            if dep not in all_nodes[key]["depends_on"]:
                all_nodes[key]["depends_on"].append(dep)
    need(isinstance(payload["coverage"], list), "REFINEMENT", "coverage must be a list")
    covered, indices = set(), set()
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


def apply_command(state, command):
    exact(command, {"event_id", "base_revision", "kind", "reason", "payload"})
    identity(command["event_id"])
    need(type(command["base_revision"]) is int and command["base_revision"] == state["revision"], "STALE", "base revision differs")
    reason = command["reason"]
    exact(reason, {"summary"}, {"evidence_refs", "decision_ref"})
    string(reason["summary"], "reason summary")
    refs(state, reason.get("evidence_refs", []), "evidence")
    if "decision_ref" in reason:
        need(reason["decision_ref"] in state["decisions"], "REFERENCE", "unknown reason decision")
    state = copy.deepcopy(state)
    kind, p = command["kind"], command["payload"]
    if kind == "node.classified":
        exact(p, {"node_id", "work_type", "maturity"})
        refs(state, [p["node_id"]], "node")
        need(p["work_type"] in WORK_TYPES and p["maturity"] in MATURITY, "CLASSIFICATION", "invalid work type/maturity")
        state["classifications"][p["node_id"]] = {**{k: p[k] for k in ("work_type", "maturity")}, "assertion_status": "declared"}
    elif kind == "knowledge.region-recorded":
        exact(p, {"id", "question", "node_refs"})
        string(p["question"], "question")
        add_record(state, "unknown_regions", p)
    elif kind == "evidence.recorded":
        exact(p, {"id", "claim", "subject", "result", "artifact_refs", "node_refs"})
        for key in ("claim", "subject"):
            string(p[key], key)
        need(p["result"] in {"observed_pass", "observed_fail", "unavailable", "inconclusive"}, "EVIDENCE", "invalid observation result")
        strings(p["artifact_refs"], "artifact refs")
        add_record(state, "evidence", {**p, "adjudication": "unverified"})
    elif kind == "fact.recorded":
        exact(p, {"id", "statement", "status", "node_refs", "evidence_refs", "source_refs"})
        string(p["statement"], "statement")
        need(p["status"] in {"hypothesis", "asserted", "observed"}, "FACT", "recording is not verification")
        strings(p["source_refs"], "source refs")
        need(p["status"] != "observed" or p["evidence_refs"], "FACT", "observed assertion needs evidence pointers")
        add_record(state, "facts", {**p, "adjudication": "unverified"})
    elif kind == "decision.recorded":
        exact(p, {"id", "question", "alternatives", "chosen", "rationale", "authority_ref", "consequences", "node_refs", "evidence_refs"})
        for key in ("question", "rationale", "authority_ref"):
            string(p[key], key)
        need(isinstance(p["alternatives"], list) and len(p["alternatives"]) >= 2, "DECISION", "name at least two alternatives")
        choices = set()
        for row in p["alternatives"]:
            exact(row, {"id", "description"})
            need(identity(row["id"]) not in choices, "DUPLICATE", "duplicate alternative")
            choices.add(row["id"])
            string(row["description"], "alternative description")
        need(p["chosen"] in choices, "DECISION", "chosen alternative missing")
        strings(p["consequences"], "consequences")
        add_record(state, "decisions", {**p, "authority_origin": "unverified", "activates_contract": False})
    elif kind == "approach.declared":
        exact(p, {"id", "problem_id", "description"})
        refs(state, [p["problem_id"]], "node")
        description = " ".join(string(p["description"], "approach description").casefold().split())
        need(not any(a["problem_id"] == p["problem_id"] and a["strategy_key"] == description for a in state["approaches"].values()), "APPROACH", "renamed duplicate strategy; reuse the declared approach id")
        add_record(state, "approaches", {**p, "strategy_key": description, "outcome": "unresolved", "verdicts": []})
        state["relations"].append({"source_kind": "approaches", "source_id": p["id"], "relation": "addresses", "target_kind": "node", "target_id": p["problem_id"]})
    elif kind == "approach.verdict":
        exact(p, {"approach_id", "outcome", "evidence_refs"})
        need(p["approach_id"] in state["approaches"], "APPROACH", "declare approach before verdict")
        refs(state, p["evidence_refs"], "evidence")
        need(p["outcome"] in {"failed", "succeeded", "inconclusive", "infrastructure_failure"}, "APPROACH", "unknown verdict")
        approach = state["approaches"][p["approach_id"]]
        need(approach["outcome"] == "unresolved", "APPROACH", "semantic verdict already recorded")
        if p["outcome"] in {"failed", "succeeded"}:
            need(any(state["evidence"][key]["result"] in {"observed_pass", "observed_fail"} for key in p["evidence_refs"]),
                 "APPROACH", "semantic verdict needs an observed result; unavailable/inconclusive alone is insufficient")
            approach["outcome"] = p["outcome"]
        approach["verdicts"].append({**p, "event_id": command["event_id"]})
    elif kind == "plan.refined":
        refine(state, p)
    else:
        raise Refusal("KIND", "unsupported event kind; no owner control or execution transitions exist")
    state["revision"] += 1
    return state


def load_store(store):
    store = safe_path(store)
    base_raw = safe_path(store / "base.json").read_bytes()
    base = parse(base_raw, tagged=True)
    need(base.get("schema") == SCHEMA, "STORE", "unknown store schema")
    source = base["sources"]["plan"]
    raw_plan = base64.b64decode(source["raw_base64"], validate=True)
    need(sha(raw_plan) == source["sha256"] and packed(tomllib.loads(raw_plan.decode("utf-8"))) == packed(base["plan"]), "STORE", "imported plan capture differs")
    nodes, _, _ = validate_plan(base["plan"])
    groups = []
    for row in base["sources"]["tasks"]:
        raw = base64.b64decode(row["raw_base64"], validate=True)
        need(sha(raw) == row["sha256"], "STORE", "task source hash differs")
        groups.append(parse(raw))
    need(packed(validate_tasks(groups, nodes)) == packed(base["task_contracts"]), "STORE", "task captures differ")
    lines = safe_path(store / "events.jsonl").read_bytes().splitlines(keepends=True)
    pending = None
    if lines and not lines[-1].endswith(b"\n"):
        tail = lines.pop()
        pending = {"bytes": len(tail), "sha256": sha(tail)}
    need(lines, "JOURNAL", "no committed import receipt")
    first = parse(lines[0], tagged=True)
    exact(first, {"seq", "revision", "previous_revision", "event_id", "kind", "base_sha256"})
    need(type(first["seq"]) is int and type(first["revision"]) is int and first["seq"] == first["revision"] == 0 and first["previous_revision"] is None and first["kind"] == "store.imported" and first["base_sha256"] == sha(base_raw), "JOURNAL", "import receipt differs")
    identity(first["event_id"])
    state, events, seen = initial_state(base, sha(base_raw)), [first], {first["event_id"]: first}
    for line in lines[1:]:
        event = parse(line, tagged=True)
        exact(event, {"seq", "revision", "previous_revision", "event_id", "kind", "reason", "payload", "command_sha256"})
        key = identity(event["event_id"])
        if key in seen:
            need(event == seen[key], "JOURNAL", "conflicting duplicate event")
            continue
        need(type(event["seq"]) is int and type(event["revision"]) is int and event["seq"] == event["revision"] == state["revision"] + 1, "JOURNAL", "sequence/revision gap")
        command = {k: event[k] for k in ("event_id", "kind", "reason", "payload")}
        command["base_revision"] = event["previous_revision"]
        need(sha(packed(command)) == event["command_sha256"], "JOURNAL", "command hash differs")
        state = apply_command(state, command)
        events.append(event)
        seen[key] = event
    return state, events, pending


@contextmanager
def writer_lock(store):
    path = safe_path(Path(store) / "writer.lock")
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise Refusal("BUSY", "writer lock exists; inspect live/stale ownership, never steal automatically") from exc
    try:
        os.write(fd, packed({"pid": os.getpid()}))
        os.close(fd)
        fd = None
        yield
    finally:
        if fd is not None:
            os.close(fd)
        path.unlink()


def record(store, command):
    store = safe_path(store)
    with writer_lock(store):
        state, events, pending = load_store(store)
        need(pending is None, "PENDING_TAIL", "incomplete final journal line; preserve it and repair explicitly before append")
        key = command.get("event_id") if isinstance(command, dict) else None
        duplicate = next((e for e in events if e["event_id"] == key), None)
        command_hash = sha(packed(command))
        if duplicate:
            need(duplicate.get("command_sha256") == command_hash, "IDEMPOTENCY", "event id reused for another command")
            return {"ok": True, "idempotent": True, "event": duplicate, "revision": state["revision"], "execution_mode": "draft"}
        after = apply_command(state, command)
        event = {"seq": after["revision"], "revision": after["revision"], "previous_revision": state["revision"],
                 **{k: command[k] for k in ("event_id", "kind", "reason", "payload")}, "command_sha256": command_hash}
        with safe_path(store / "events.jsonl").open("ab") as stream:
            stream.write(packed(event) + b"\n")
            stream.flush()
            os.fsync(stream.fileno())
        return {"ok": True, "idempotent": False, "event": event, "revision": after["revision"], "execution_mode": "draft"}


def frontier(state):
    nodes, deps, children = validate_plan(state["plan"])
    ready = []
    for key, node in nodes.items():
        if children[key] or node["state"] not in {"planned", "ready", "active"}:
            continue
        cursor, suppressed = node["parent"], False
        while cursor:
            suppressed |= nodes[cursor]["state"] in {"blocked", "deferred", "dropped", "superseded"}
            cursor = nodes[cursor]["parent"]
        if not suppressed and all(nodes[d]["state"] in TERMINAL for d in deps[key]):
            ready.append(key)
    return sorted(ready, key=lambda key: (nodes[key]["order"], key))


def expression(expr, assessment, state):
    need(isinstance(expr, dict) and len(expr) == 1, "RULE", "one expression operator required")
    operator, arg = next(iter(expr.items()))
    if operator in {"all", "any"}:
        need(isinstance(arg, list) and arg, "RULE", "nonempty logical operands required")
        results = [expression(x, assessment, state) for x in arg]
        values = [r[0] for r in results]
        if operator == "all":
            value = False if False in values else None if None in values else True
        else:
            value = True if True in values else None if None in values else False
        return value, [item for r in results for item in r[1]]
    if operator == "not":
        value, details = expression(arg, assessment, state)
        return None if value is None else not value, details
    if operator == "eq":
        exact(arg, {"field", "value"})
        string(arg["field"], "assessment field")
        need(arg["value"] is not None and isinstance(arg["value"], (str, bool, int, float)), "RULE", "comparison needs concrete scalar")
        value = assessment.get(arg["field"])
        need(value is None or type(value) is type(arg["value"]), "ASSESSMENT", "assessment scalar type differs from predicate")
        result = None if value is None else type(value) is type(arg["value"]) and value == arg["value"]
        return result, [{"field": arg["field"], "observed": value, "result": result}]
    if operator == "failed_approaches":
        exact(arg, {"gte"})
        need(type(arg["gte"]) is int and arg["gte"] > 0, "RULE", "positive approach threshold required")
        failures = {}
        for approach in state["approaches"].values():
            if approach["outcome"] == "failed":
                failures.setdefault(approach["problem_id"], []).append(approach["id"])
        return any(len(ids) >= arg["gte"] for ids in failures.values()), [{"failed_by_problem": failures, "threshold": arg["gte"]}]
    raise Refusal("RULE", "unknown stop expression")


def evaluate_stop(state, data):
    exact(data, {"rules", "assessment"})
    rules, assessment = data["rules"], data["assessment"]
    exact(rules, {"schema", "status", "rules"})
    need(rules["schema"] == "zap-stop/1" and rules["status"] == "example_unapproved", "RULE", "CLI accepts only unapproved rule probes; no activation adapter")
    need(isinstance(assessment, dict) and assessment.get("phase") in {"before_action", "after_action"}, "ASSESSMENT", "declare whether action already happened")
    need(isinstance(rules["rules"], list) and rules["rules"], "RULE", "rules missing")
    results, ids = [], set()
    for rule in rules["rules"]:
        exact(rule, {"id", "when", "scope", "timing"})
        key = identity(rule["id"])
        need(key not in ids, "DUPLICATE", "duplicate stop rule")
        ids.add(key)
        need(rule["scope"] == "run" and rule["timing"] in {"before_action", "before_next_action"}, "RULE", "unsupported stop scope/timing")
        value, details = expression(rule["when"], assessment, state)
        results.append({"id": key, "matched": value, "scope": "run", "timing": rule["timing"], "details": details})
    matched = [r["id"] for r in results if r["matched"] is True]
    unknown = [r["id"] for r in results if r["matched"] is None]
    after = assessment["phase"] == "after_action"
    late = after and any(r["matched"] is True and r["timing"] == "before_action" for r in results)
    result = "too_late" if late else "pause" if matched else "needs_evidence" if unknown else "clear"
    return {"ok": True, "revision": state["revision"], "execution_mode": "draft", "policy_origin": "unverified_input",
            "policy_result": result, "matched_rules": matched, "unknown_rules": unknown, "rules": results,
            "scope": "run", "action_admitted": False, "owner_contract_activated": False,
            "action_already_occurred": after, "prevented_action": False,
            "response": "No new jobs; drain existing operations safely and prepare owner options." if matched else "Resolve unknown assessment evidence before action." if unknown else "No rule matched; draft mode still grants no execution authority."}


class JsonParser(argparse.ArgumentParser):
    def error(self, message):
        raise Refusal("ARGUMENT", message)


def main(argv=None):
    parser = JsonParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    importer = commands.add_parser("import-mup")
    for flag in ("plan", "tasks-dir", "out"):
        importer.add_argument("--" + flag, required=True)
    for name in ("inspect", "events", "record", "evaluate-stop"):
        command = commands.add_parser(name)
        command.add_argument("--store", required=True)
        if name == "events":
            command.add_argument("--after", type=int, default=-1)
        if name in {"record", "evaluate-stop"}:
            command.add_argument("--command" if name == "record" else "--input", required=True, dest="input")
    try:
        args = parser.parse_args(argv)
        if args.command == "import-mup":
            result = import_mup(args.plan, args.tasks_dir, args.out)
        elif args.command == "record":
            result = record(args.store, parse(Path(args.input).read_bytes()))
        else:
            state, events, pending = load_store(args.store)
            if args.command == "inspect":
                result = {"ok": True, "state": state, "frontier": frontier(state), "dispatch_allowed": False, "pending_tail": pending}
            elif args.command == "events":
                need(-1 <= args.after <= state["revision"], "CURSOR", "after must be within the committed journal revision")
                result = {"ok": True, "events": [e for e in events if e["seq"] > args.after], "cursor": state["revision"], "pending_tail": pending}
            else:
                need(pending is None, "PENDING_TAIL", "stop evaluation requires an unambiguous journal")
                result = evaluate_stop(state, parse(Path(args.input).read_bytes()))
        print(packed(result).decode("ascii"))
        return 0
    except (Refusal, OSError, ValueError, KeyError, TypeError, RecursionError) as exc:
        print(packed({"ok": False, "code": getattr(exc, "code", "INVALID_INPUT"), "message": str(exc), "dispatch_allowed": False}).decode("ascii"))
        return 2


if __name__ == "__main__":
    sys.exit(main())
