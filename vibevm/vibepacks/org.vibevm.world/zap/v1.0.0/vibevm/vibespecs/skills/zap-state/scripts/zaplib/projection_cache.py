"""Thread-safe exact-prefix in-process cache for verified ZAP projections."""
from __future__ import annotations

import copy
from dataclasses import dataclass
import os
from pathlib import Path
import threading
from typing import Any, Mapping

from .common import Refusal, exact, need, packed, parse, sha
from .records import HandlerSpec, apply_command
from .storage import load_store, replay_committed, safe_path, writer_lock

PROJECTION_CACHE_OPERATIONS = {
    "projection.cache.load": {"effect": "verified_in_process_projection", "writes": [],
                              "identity": ["exact_base_bytes", "exact_committed_prefix", "reducer_objects"]},
    "projection.cache.invalidate": {"effect": "drop_process_local_verified_boundary", "writes": []},
}


@dataclass
class _Entry:
    base_raw: bytes
    committed_prefix: bytes
    reducer_identity: tuple[Any, ...]
    state: dict[str, Any]
    events: list[dict[str, Any]]


def _reducer_identity(handlers: Mapping[str, HandlerSpec]) -> tuple[Any, ...]:
    return tuple(
        (kind, spec, spec.validate_payload, spec.apply)
        for kind, spec in sorted(handlers.items())
    )


def _same_reducer(left: tuple[Any, ...], right: tuple[Any, ...]) -> bool:
    return len(left) == len(right) and all(
        a[0] == b[0] and a[1] is b[1] and a[2] is b[2] and a[3] is b[3]
        for a, b in zip(left, right)
    )


def _journal(raw: bytes) -> tuple[bytes, list[bytes], dict[str, Any] | None]:
    lines = raw.splitlines(keepends=True)
    pending = None
    if lines and not lines[-1].endswith(b"\n"):
        tail = lines.pop()
        pending = {"bytes": len(tail), "sha256": sha(tail)}
    return b"".join(lines), lines, pending


class ProjectionCache:
    """Cache a detached replay boundary per store for this process lifetime.

    Every load rereads exact base and journal bytes. Complete appended lines are
    suffix-replayed. A changed verified history requires explicit invalidation.
    """

    def __init__(self):
        self._lock = threading.RLock()
        self._entries: dict[Path, _Entry] = {}
        self._stats = {"cold_loads": 0, "warm_hits": 0, "suffix_loads": 0,
                       "prefix_drifts": 0, "invalidations": 0}

    @staticmethod
    def _read(store: Path) -> tuple[bytes, bytes, list[bytes], dict[str, Any] | None]:
        base_raw = safe_path(store / "base.json").read_bytes()
        journal_raw = safe_path(store / "events.jsonl").read_bytes()
        committed, lines, pending = _journal(journal_raw)
        return base_raw, committed, lines, pending

    @staticmethod
    def _detached(entry: _Entry, pending: dict[str, Any] | None):
        return copy.deepcopy(entry.state), copy.deepcopy(entry.events), copy.deepcopy(pending)

    def _cold(self, store: Path, handlers: Mapping[str, HandlerSpec], reducer_identity, *, attempts=3):
        for _ in range(attempts):
            before = self._read(store)
            state, events, pending = load_store(store, handlers)
            after = self._read(store)
            if before[0] != after[0] or not after[1].startswith(before[1]):
                continue
            target_revision = state["revision"]
            prefix_lines = []
            for line in after[2]:
                event = parse(line, tagged=True)
                if event["revision"] > target_revision:
                    break
                prefix_lines.append(line)
            prefix = b"".join(prefix_lines)
            known = {}
            physical_events = []
            for index, line in enumerate(prefix_lines):
                event = parse(line, tagged=True)
                if index == 0:
                    exact(event, {"seq", "revision", "previous_revision", "event_id", "kind", "base_sha256"})
                else:
                    exact(event, {"seq", "revision", "previous_revision", "event_id", "kind", "reason", "payload", "command_sha256"})
                prior = known.get(event["event_id"])
                if prior is None:
                    known[event["event_id"]] = event
                    physical_events.append(event)
                else:
                    need(prior == event, "JOURNAL", "conflicting duplicate event")
            if physical_events != events:
                continue
            suffix = after[1][len(prefix):]
            if suffix:
                suffix_lines = suffix.splitlines(keepends=True)
                state, applied, _seen = replay_committed(
                    state, (parse(line, tagged=True) for line in suffix_lines),
                    handlers, seen=known,
                )
                events = [*events, *applied]
            entry = _Entry(after[0], after[1], reducer_identity,
                           copy.deepcopy(state), copy.deepcopy(events))
            self._entries[store] = entry
            self._stats["cold_loads"] += 1
            return self._detached(entry, after[3])
        raise Refusal("CACHE_RACE", "store changed repeatedly during cold replay")

    def load(self, store: Any, handlers: Mapping[str, HandlerSpec]):
        path = safe_path(store)
        reducer_identity = _reducer_identity(handlers)
        with self._lock:
            base_raw, committed, _lines, pending = self._read(path)
            entry = self._entries.get(path)
            if entry is None or entry.base_raw != base_raw or not _same_reducer(entry.reducer_identity, reducer_identity):
                return self._cold(path, handlers, reducer_identity)
            if committed == entry.committed_prefix:
                self._stats["warm_hits"] += 1
                return self._detached(entry, pending)
            if not committed.startswith(entry.committed_prefix):
                self._stats["prefix_drifts"] += 1
                load_store(path, handlers)
                raise Refusal("CACHE_PREFIX_DRIFT", "committed journal history differs; invalidate cache explicitly after review")
            suffix = committed[len(entry.committed_prefix):]
            suffix_lines = suffix.splitlines(keepends=True)
            need(all(line.endswith(b"\n") for line in suffix_lines), "JOURNAL", "appended committed suffix is not line bounded")
            state = copy.deepcopy(entry.state)
            seen = {event["event_id"]: event for event in entry.events}
            state, applied, _seen = replay_committed(
                state, (parse(line, tagged=True) for line in suffix_lines),
                handlers, seen=seen,
            )
            updated = _Entry(base_raw, committed, reducer_identity, state,
                             [*entry.events, *applied])
            self._entries[path] = updated
            self._stats["suffix_loads"] += 1
            return self._detached(updated, pending)

    def record(self, store: Any, command: Any, handlers: Mapping[str, HandlerSpec]):
        path = safe_path(store)
        with writer_lock(path):
            state, events, pending = self.load(path, handlers)
            need(pending is None, "PENDING_TAIL", "incomplete final journal line; preserve it and repair explicitly before append")
            key = command.get("event_id") if isinstance(command, dict) else None
            duplicate = next((event for event in events if event["event_id"] == key), None)
            command_hash = sha(packed(command))
            if duplicate:
                need(duplicate.get("command_sha256") == command_hash, "IDEMPOTENCY", "event id reused for another command")
                return {"ok": True, "idempotent": True, "event": duplicate,
                        "revision": state["revision"], "execution_mode": state.get("execution_mode", "draft")}
            after = apply_command(state, command, handlers)
            event = {"seq": after["revision"], "revision": after["revision"],
                     "previous_revision": state["revision"],
                     **{field: command[field] for field in ("event_id", "kind", "reason", "payload")},
                     "command_sha256": command_hash}
            with safe_path(path / "events.jsonl").open("ab") as stream:
                stream.write(packed(event) + b"\n")
                stream.flush()
                os.fsync(stream.fileno())
            verified, _events, _pending = self.load(path, handlers)
            need(verified["revision"] == after["revision"], "CACHE_RACE", "durable append revision differs")
            return {"ok": True, "idempotent": False, "event": event,
                    "revision": after["revision"], "execution_mode": after.get("execution_mode", "draft")}

    def invalidate(self, store: Any | None = None) -> None:
        with self._lock:
            if store is None:
                self._entries.clear()
            else:
                self._entries.pop(safe_path(store), None)
            self._stats["invalidations"] += 1

    def stats(self) -> dict[str, int]:
        with self._lock:
            return dict(self._stats)


__all__ = ("PROJECTION_CACHE_OPERATIONS", "ProjectionCache")
