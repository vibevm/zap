"""Derived snapshots bound to an immutable base, reducer, and log prefix."""
from __future__ import annotations

import copy
from pathlib import Path
from typing import Any, Callable, Mapping, MutableSet

from .common import PROJECTION_SCHEMA, Refusal, exact, identity, need, packed, parse, sha
from .graph import validate_plan
from .records import CORE_HANDLERS, HandlerSpec
from .storage import load_base_state, load_store, read_committed_journal, replay_committed, safe_path, write_new, writer_lock

SNAPSHOT_SCHEMA = "zap-snapshot/1"
DEFAULT_REDUCER_VERSION = "zap-reducer/1"
SNAPSHOT_OPERATIONS = {
    "snapshot.create": {"required": ["store", "snapshot_path"], "optional": ["handlers", "reducer_version"],
                        "effect": "derived_snapshot", "canonical": False, "requires_committed_prefix": True},
    "snapshot.load-tail": {"required": ["store", "snapshot_path"], "optional": ["handlers", "reducer_version", "verification_cache"],
                           "effect": "validated_tail_replay", "writes": []},
}


def _reducer(version: str, handlers: Mapping[str, HandlerSpec]) -> dict[str, Any]:
    need(isinstance(version, str) and version.strip(), "SNAPSHOT_VERSION", "reducer version is required")
    kinds = sorted(handlers)
    return {"version": version, "handler_kinds": kinds, "identity_sha256": sha(packed({"version": version, "handler_kinds": kinds}))}


def create_snapshot(
    store: str | Path,
    snapshot_path: str | Path,
    handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS,
    *,
    reducer_version: str = DEFAULT_REDUCER_VERSION,
    capture_hook: Callable[[], None] | None = None,
) -> dict[str, Any]:
    with writer_lock(store):
        state, events, pending = load_store(store, handlers)
        if capture_hook:
            capture_hook()
        lines, observed_pending = read_committed_journal(store)
        need(pending == observed_pending, "SNAPSHOT", "journal boundary changed during snapshot capture")
        last = parse(lines[-1], tagged=True)
        need(last["event_id"] == events[-1]["event_id"] and last["revision"] == state["revision"], "SNAPSHOT", "journal changed during snapshot capture")
        prefix = b"".join(lines)
        reducer = _reducer(reducer_version, handlers)
        snapshot = {
            "schema": SNAPSHOT_SCHEMA,
            "base_sha256": state["base_sha256"],
            "projection_schema": state.get("projection_schema", PROJECTION_SCHEMA),
            "reducer": reducer,
            "revision": state["revision"],
            "committed_prefix": {"sha256": sha(prefix), "bytes": len(prefix), "lines": len(lines), "last_event_id": events[-1]["event_id"]},
            "state_sha256": sha(packed(state)),
            "state": state,
        }
        target = safe_path(snapshot_path)
        need(not target.exists(), "OUTPUT_EXISTS", "snapshot output must be new")
        target.parent.mkdir(parents=True, exist_ok=True)
        write_new(target, packed(snapshot) + b"\n")
    return {"ok": True, "snapshot": str(target), "revision": state["revision"], "base_sha256": state["base_sha256"],
            "prefix_sha256": snapshot["committed_prefix"]["sha256"], "pending_tail": pending}


def _prefix_seen(records: list[dict[str, Any]], base_hash: str, revision: int, last_event_id: str) -> dict[str, dict[str, Any]]:
    need(records, "SNAPSHOT_PREFIX", "snapshot prefix has no import receipt")
    first = records[0]
    exact(first, {"seq", "revision", "previous_revision", "event_id", "kind", "base_sha256"})
    need(first["seq"] == first["revision"] == 0 and first["previous_revision"] is None and first["kind"] == "store.imported" and first["base_sha256"] == base_hash, "SNAPSHOT_PREFIX", "snapshot import receipt differs")
    known: dict[str, dict[str, Any]] = {}
    last_revision = 0
    for event in records:
        key = identity(event["event_id"])
        if key in known:
            need(event == known[key], "SNAPSHOT_PREFIX", "conflicting duplicate in snapshot prefix")
            continue
        known[key] = event
        last_revision = event["revision"]
    need(last_revision == revision and records[-1]["event_id"] == last_event_id, "SNAPSHOT_PREFIX", "snapshot cursor differs from prefix")
    return known


def _verification_key(snapshot: dict[str, Any]) -> str:
    return sha(packed({"base_sha256": snapshot["base_sha256"], "prefix": snapshot["committed_prefix"],
                       "state_sha256": snapshot["state_sha256"], "reducer": snapshot["reducer"]}))


def load_snapshot_tail(
    store: str | Path,
    snapshot_path: str | Path,
    handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS,
    *,
    reducer_version: str = DEFAULT_REDUCER_VERSION,
    verification_cache: MutableSet[str] | None = None,
) -> dict[str, Any]:
    snapshot = parse(safe_path(snapshot_path).read_bytes(), tagged=True)
    exact(snapshot, {"schema", "base_sha256", "projection_schema", "reducer", "revision", "committed_prefix", "state_sha256", "state"})
    need(snapshot["schema"] == SNAPSHOT_SCHEMA and snapshot["projection_schema"] == PROJECTION_SCHEMA, "SNAPSHOT_VERSION", "unsupported snapshot schema")
    need(snapshot["reducer"] == _reducer(reducer_version, handlers), "SNAPSHOT_VERSION", "snapshot reducer identity differs")
    store_path = safe_path(store)
    base_raw = safe_path(store_path / "base.json").read_bytes()
    need(sha(base_raw) == snapshot["base_sha256"], "SNAPSHOT_BASE", "snapshot base identity differs")
    prefix_info = snapshot["committed_prefix"]
    exact(prefix_info, {"sha256", "bytes", "lines", "last_event_id"})
    need(type(prefix_info["bytes"]) is int and type(prefix_info["lines"]) is int and prefix_info["bytes"] >= 0 and prefix_info["lines"] > 0, "SNAPSHOT_PREFIX", "invalid snapshot prefix boundary")
    lines, pending = read_committed_journal(store_path)
    need(len(lines) >= prefix_info["lines"], "SNAPSHOT_PREFIX", "journal is shorter than snapshot prefix")
    prefix_lines = lines[:prefix_info["lines"]]
    prefix = b"".join(prefix_lines)
    need(len(prefix) == prefix_info["bytes"] and sha(prefix) == prefix_info["sha256"], "SNAPSHOT_PREFIX", "committed journal prefix differs")
    state = copy.deepcopy(snapshot["state"])
    need(sha(packed(state)) == snapshot["state_sha256"], "SNAPSHOT_STATE", "snapshot projection hash differs")
    need(state.get("base_sha256") == snapshot["base_sha256"] and state.get("revision") == snapshot["revision"], "SNAPSHOT_STATE", "snapshot projection identity differs")
    validate_plan(state["plan"])
    prefix_records = [parse(line, tagged=True) for line in prefix_lines]
    verification_key = _verification_key(snapshot)
    warm = verification_cache is not None and verification_key in verification_cache
    if warm:
        seen = _prefix_seen(prefix_records, snapshot["base_sha256"], snapshot["revision"], prefix_info["last_event_id"])
    else:
        _, _, _, initial = load_base_state(store_path)
        first = prefix_records[0]
        seen = _prefix_seen([first], snapshot["base_sha256"], 0, first["event_id"])
        derived, _, seen = replay_committed(initial, prefix_records[1:], handlers, seen=seen)
        need(packed(derived) == packed(state), "SNAPSHOT_STATE", "snapshot projection is not derived from its committed prefix")
        need(prefix_records[-1]["event_id"] == prefix_info["last_event_id"], "SNAPSHOT_PREFIX", "snapshot last event differs")
        if verification_cache is not None:
            verification_cache.add(verification_key)
    tail_records = [parse(line, tagged=True) for line in lines[prefix_info["lines"]:]]
    after, applied, _ = replay_committed(state, tail_records, handlers, seen=seen)
    return {"ok": True, "state": after, "cursor": after["revision"], "snapshot_revision": snapshot["revision"],
            "tail_events": applied, "pending_tail": pending, "snapshot_verification": "warm" if warm else "cold",
            "verification_key": verification_key}
