"""Non-destructive, resumable MUP migration with exact source reporting."""
from __future__ import annotations

import base64
import copy
import os
from pathlib import Path
from typing import Any, Callable
import uuid

from .common import SCHEMA, TASK_FIELDS, need, packed, parse, sha
from .storage import import_mup, load_store, safe_path, write_new

MIGRATION_SCHEMA = "zap-migration-report/1"
MIGRATION_OPERATIONS = {
    "migrate-mup": {"required": ["plan_path", "tasks_dir", "out"], "optional": ["report_path"],
                    "effect": "new_zap_store_and_report", "source_writes": False, "activates_charter": False, "executes_commands": False},
}
PLAN_FIELDS = {"schema", "plan_id", "revision", "root_node", "current_node", "node", "mandate"}
NODE_FIELDS = {"id", "parent", "title", "kind", "state", "order", "depends_on", "mandates", "acceptance", "evidence", "zoom", "contract", "contract_sha256"}
MANDATE_FIELDS = {"id", "text", "disposition", "nodes"}


def _unknown(container: dict[str, Any], known: set[str], prefix: str) -> list[dict[str, Any]]:
    return [{"path": f"{prefix}.{key}", "value": copy.deepcopy(container[key])} for key in sorted(set(container) - known)]


def _source_snapshot(plan: Path, tasks: Path) -> dict[str, dict[str, Any]]:
    paths = [plan] + [safe_path(path) for path in sorted(tasks.glob("*.json"))]
    need(len(paths) > 1, "TASK", "no task files")
    result = {}
    for path in paths:
        raw = path.read_bytes()
        result[str(path)] = {"sha256": sha(raw), "bytes": len(raw)}
    return result


def _captured_sources(base: dict[str, Any]) -> dict[str, dict[str, Any]]:
    rows = [base["sources"]["plan"], *base["sources"]["tasks"]]
    result = {}
    for row in rows:
        raw = base64.b64decode(row["raw_base64"], validate=True)
        result[row["path"]] = {"sha256": row["sha256"], "bytes": len(raw)}
    return result


def _import_result(store: Path, base: dict[str, Any], state: dict[str, Any]) -> dict[str, Any]:
    task_hashes = {row["path"]: row["sha256"] for row in base["sources"]["tasks"]}
    return {"ok": True, "store": str(store), "schema": SCHEMA, "execution_mode": "draft", "plan_nodes": len(base["plan"]["node"]),
            "task_contracts": len(base["task_contracts"]), "base_sha256": state["base_sha256"],
            "source_sha256": {"plan": base["sources"]["plan"]["sha256"], "tasks": task_hashes}, "owner_contract_activated": False}


def build_migration_report(store: str | os.PathLike[str]) -> dict[str, Any]:
    store_path = safe_path(store)
    state, events, pending = load_store(store_path)
    need(pending is None and state["revision"] == 0 and len(events) == 1, "MIGRATION", "migration report requires a fresh imported store")
    base_raw = safe_path(store_path / "base.json").read_bytes()
    base = parse(base_raw, tagged=True)
    plan = base["plan"]
    unknown = _unknown(plan, PLAN_FIELDS, "plan")
    nodes = []
    for index, node in enumerate(plan["node"]):
        unknown.extend(_unknown(node, NODE_FIELDS, f"plan.node[{index}]"))
        nodes.append({"source_id": node["id"], "zap_node_id": node["id"], "kind": node["kind"], "state": node["state"],
                      "constraints": {key: copy.deepcopy(node[key]) for key in ("parent", "depends_on", "mandates", "acceptance", "evidence")}})
    mandates = []
    for index, mandate in enumerate(plan["mandate"]):
        unknown.extend(_unknown(mandate, MANDATE_FIELDS, f"plan.mandate[{index}]"))
        mandates.append({"source_id": mandate["id"], "zap_mandate_id": mandate["id"], "text": mandate["text"],
                         "disposition": mandate["disposition"], "nodes": copy.deepcopy(mandate["nodes"])})
    task_rows = []
    for task_id, task in base["task_contracts"].items():
        unknown.extend(_unknown(task, TASK_FIELDS, f"task_contracts.{task_id}"))
        task_rows.append({"source_id": task_id, "zap_task_id": task_id, "contract": copy.deepcopy(task)})
    for index, source in enumerate(base["sources"]["tasks"]):
        group = parse(base64.b64decode(source["raw_base64"], validate=True))
        unknown.extend(_unknown(group, {"id", "tasks"}, f"sources.tasks[{index}]"))
    return {"schema": MIGRATION_SCHEMA, "store": str(store_path), "base_sha256": sha(base_raw), "source_captures": copy.deepcopy(base["sources"]),
            "mapping": {"nodes": nodes, "mandates": mandates, "task_contracts": task_rows}, "unknown_fields": unknown,
            "preservation": {"raw_sources": True, "parsed_plan": True, "task_extensions": True, "constraints": True,
                             "node_count": len(nodes), "mandate_count": len(mandates), "task_count": len(task_rows)},
            "authority_classification": {"imported_labels": "unverified_data", "charter_activated": False, "commands_executed": False}}


def _publish_report(target: Path, raw: bytes) -> bool:
    if target.exists():
        need(target.is_file() and target.read_bytes() == raw, "OUTPUT_EXISTS", "existing migration report differs")
        return False
    temporary = safe_path(target.parent / f".{target.name}.{uuid.uuid4().hex}.tmp")
    try:
        write_new(temporary, raw)
        os.link(temporary, target)
    finally:
        if temporary.exists():
            temporary.unlink()
    return True


def migrate_mup(
    plan_path: str | os.PathLike[str], tasks_dir: str | os.PathLike[str], out: str | os.PathLike[str], *,
    report_path: str | os.PathLike[str] | None = None, fault: Callable[[str], None] | None = None,
) -> dict[str, Any]:
    plan, tasks, store = safe_path(plan_path), safe_path(tasks_dir), safe_path(out)
    before = _source_snapshot(plan, tasks)
    if fault:
        fault("after_source_snapshot")
    recovered_store = store.exists()
    if recovered_store:
        state, events, pending = load_store(store)
        need(pending is None and state["revision"] == 0 and len(events) == 1, "MIGRATION", "existing migration store is not a fresh exact import")
        base = parse(safe_path(store / "base.json").read_bytes(), tagged=True)
        imported = _import_result(store, base, state)
    else:
        imported = import_mup(plan, tasks, store, fault=fault)
        state = load_store(store)[0]
        base = parse(safe_path(store / "base.json").read_bytes(), tagged=True)
    if fault:
        fault("after_import")
    after = _source_snapshot(plan, tasks)
    need(before == after, "MIGRATION", "migration source path-set or content changed")
    need(_captured_sources(base) == before, "MIGRATION", "import capture does not match the stable source snapshot")
    report = build_migration_report(store)
    target = safe_path(report_path) if report_path is not None else safe_path(store / "migration-report.json")
    need(target.parent == store, "PATH", "migration report must be inside the new store")
    if fault:
        fault("before_report")
    report_created = _publish_report(target, packed(report) + b"\n")
    if fault:
        fault("after_report")
    return {**imported, "migration_report": str(target), "migration_schema": MIGRATION_SCHEMA, "source_unchanged": True,
            "store_recovered": recovered_store, "report_created": report_created, "charter_activated": False, "commands_executed": False}
