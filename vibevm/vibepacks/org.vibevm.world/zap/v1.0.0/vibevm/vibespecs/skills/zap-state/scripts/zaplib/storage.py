"""Immutable base import, append-only journal replay, CAS record, and locking."""
from __future__ import annotations

import base64
from contextlib import contextmanager
import os
from pathlib import Path
import stat
import tomllib
from typing import Any, Iterator, Mapping
import uuid

from .common import SCHEMA, Refusal, exact, identity, need, packed, parse, sha
from .graph import validate_plan, validate_tasks
from .records import CORE_HANDLERS, HandlerSpec, State, apply_command, initial_state


def safe_path(path: str | os.PathLike[str]) -> Path:
    result = Path(path).absolute()
    for part in [result] + list(result.parents):
        if part.exists() or part.is_symlink():
            info = part.lstat()
            need(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0), "PATH", f"symlink/reparse path refused: {part}")
    return result.resolve()


def write_new(path: Path, raw: bytes) -> None:
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def capture(path: Path, raw: bytes) -> dict[str, str]:
    return {"path": str(path), "sha256": sha(raw), "raw_base64": base64.b64encode(raw).decode("ascii")}


def import_mup(plan_path: str | os.PathLike[str], tasks_dir: str | os.PathLike[str], out: str | os.PathLike[str]) -> dict[str, Any]:
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
    base = {"schema": SCHEMA, "plan": plan, "task_contracts": tasks, "sources": {"plan": capture(plan_path, raw), "tasks": sources}}
    base_bytes = packed(base) + b"\n"
    receipt = {"seq": 0, "revision": 0, "previous_revision": None, "event_id": str(uuid.uuid4()), "kind": "store.imported", "base_sha256": sha(base_bytes)}
    out.mkdir(parents=True, exist_ok=False)
    write_new(out / "base.json", base_bytes)
    write_new(out / "events.jsonl", packed(receipt) + b"\n")
    return {"ok": True, "store": str(out), "schema": SCHEMA, "execution_mode": "draft", "plan_nodes": len(nodes), "task_contracts": len(tasks),
            "base_sha256": receipt["base_sha256"], "source_sha256": {"plan": sha(raw), "tasks": {row["path"]: row["sha256"] for row in sources}}, "owner_contract_activated": False}


def load_store(store: str | os.PathLike[str], handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS) -> tuple[State, list[dict[str, Any]], dict[str, Any] | None]:
    store_path = safe_path(store)
    base_raw = safe_path(store_path / "base.json").read_bytes()
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
    lines = safe_path(store_path / "events.jsonl").read_bytes().splitlines(keepends=True)
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
        command = {field: event[field] for field in ("event_id", "kind", "reason", "payload")}
        command["base_revision"] = event["previous_revision"]
        need(sha(packed(command)) == event["command_sha256"], "JOURNAL", "command hash differs")
        state = apply_command(state, command, handlers)
        events.append(event)
        seen[key] = event
    return state, events, pending


@contextmanager
def writer_lock(store: str | os.PathLike[str]) -> Iterator[None]:
    path = safe_path(Path(store) / "writer.lock")
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise Refusal("BUSY", "writer lock exists; inspect live/stale ownership, never steal automatically") from exc
    try:
        os.write(descriptor, packed({"pid": os.getpid()}))
        os.close(descriptor)
        descriptor = None
        yield
    finally:
        if descriptor is not None:
            os.close(descriptor)
        path.unlink()


def record(store: str | os.PathLike[str], command: Any, handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS) -> dict[str, Any]:
    store_path = safe_path(store)
    with writer_lock(store_path):
        state, events, pending = load_store(store_path, handlers)
        need(pending is None, "PENDING_TAIL", "incomplete final journal line; preserve it and repair explicitly before append")
        key = command.get("event_id") if isinstance(command, dict) else None
        duplicate = next((event for event in events if event["event_id"] == key), None)
        command_hash = sha(packed(command))
        if duplicate:
            need(duplicate.get("command_sha256") == command_hash, "IDEMPOTENCY", "event id reused for another command")
            return {"ok": True, "idempotent": True, "event": duplicate, "revision": state["revision"], "execution_mode": state.get("execution_mode", "draft")}
        after = apply_command(state, command, handlers)
        event = {"seq": after["revision"], "revision": after["revision"], "previous_revision": state["revision"],
                 **{field: command[field] for field in ("event_id", "kind", "reason", "payload")}, "command_sha256": command_hash}
        with safe_path(store_path / "events.jsonl").open("ab") as stream:
            stream.write(packed(event) + b"\n")
            stream.flush()
            os.fsync(stream.fileno())
        return {"ok": True, "idempotent": False, "event": event, "revision": after["revision"], "execution_mode": after.get("execution_mode", "draft")}
