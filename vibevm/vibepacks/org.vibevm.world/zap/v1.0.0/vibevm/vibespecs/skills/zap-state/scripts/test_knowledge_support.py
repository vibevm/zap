"""Shared temporary fixtures for ZAP knowledge/storage tests."""
from __future__ import annotations

import json
from pathlib import Path

from zaplib.records import CORE_HANDLERS, compose_handlers
from zaplib.knowledge import KNOWLEDGE_HANDLERS
from zaplib.storage import import_mup, load_store, record

HANDLERS = compose_handlers(CORE_HANDLERS, KNOWLEDGE_HANDLERS)

PLAN = '''schema = 1
plan_id = "knowledge-fixture"
revision = 8
root_node = "R"
current_node = "T"
future_top = { order = [2, 1] }
[[mandate]]
id = "M"
text = "Keep all constraints"
disposition = "owned"
nodes = ["R", "G", "T", "U"]
[[node]]
id = "R"
parent = ""
title = "Root"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["M"]
acceptance = ["Complete"]
evidence = []
[[node]]
id = "G"
parent = "R"
title = "Group"
kind = "group"
state = "planned"
order = 1
depends_on = []
mandates = ["M"]
acceptance = ["Both"]
evidence = []
[[node]]
id = "T"
parent = "G"
title = "Task"
kind = "atom"
state = "planned"
order = 2
depends_on = []
mandates = ["M"]
acceptance = ["Proof"]
evidence = []
future_node = { keep = true }
[[node]]
id = "U"
parent = "G"
title = "Other"
kind = "atom"
state = "planned"
order = 3
depends_on = []
mandates = ["M"]
acceptance = ["Other proof"]
evidence = []
'''


def task(task_id: str) -> dict:
    return {"id": task_id, "title": task_id, "goal": "Goal", "read_paths": ["input"], "write_paths": ["output"],
            "steps": ["step"], "positive_cases": ["yes"], "negative_cases": ["no"], "checks": ["check"],
            "acceptance": ["accept"], "safe_stop": "stop", "commit_subject": "feat: fixture", "notes": ["keep"],
            "future_task": {"order": [2, 1]}}


def create_inputs(root: Path) -> tuple[Path, Path]:
    plan = root / "plan.toml"
    plan.write_bytes(PLAN.encode("utf-8"))
    tasks = root / "tasks"
    tasks.mkdir()
    group = {"id": "G", "tasks": [task("T"), task("U")], "future_group": "raw"}
    (tasks / "G.json").write_text(json.dumps(group, ensure_ascii=False, indent=2), encoding="utf-8")
    return plan, tasks


def create_store(root: Path, name: str = "store") -> Path:
    plan, tasks = create_inputs(root)
    store = root / name
    import_mup(plan, tasks, store)
    return store


def command(store: Path, kind: str, payload: dict, event_id: str, handlers=HANDLERS) -> dict:
    revision = load_store(store, handlers)[0]["revision"]
    return {"event_id": event_id, "base_revision": revision, "kind": kind,
            "reason": {"summary": "Explicit fixture event"}, "payload": payload}


def append(store: Path, kind: str, payload: dict, event_id: str, handlers=HANDLERS) -> dict:
    return record(store, command(store, kind, payload, event_id, handlers), handlers)
