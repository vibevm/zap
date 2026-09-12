"""Explicit pending-tail repair with immutable quarantine evidence."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
from typing import Any, Callable, Mapping
import uuid

from .common import exact, identity, need, packed, parse, sha
from .records import CORE_HANDLERS, HandlerSpec
from .storage import load_store, read_committed_journal, safe_path, write_new, writer_lock

REPAIR_SCHEMA = "zap-journal-repair/1"
RECOVERY_OPERATIONS = {
    "repair-pending-tail": {"required": ["store", "repair_id", "expected_tail_sha256"], "optional": ["handlers"],
                            "effect": "quarantine_and_remove_uncommitted_tail", "requires_tail_hash": True,
                            "middle_corruption": "refuse", "writer_lock": "exclusive_no_steal", "activates_charter": False, "executes_commands": False},
}


def _receipt(repair_dir: Path) -> dict[str, Any]:
    value = parse(safe_path(repair_dir / "repair-receipt.json").read_bytes(), tagged=True)
    exact(value, {"schema", "repair_id", "mode", "base_sha256", "revision", "old_journal", "committed_journal", "pending_tail", "authority"})
    need(value["schema"] == REPAIR_SCHEMA and value["mode"] == "remove_uncommitted_final_fragment", "REPAIR", "unsupported repair receipt")
    return value


def _prepare_quarantine(store: Path, repair_dir: Path, repair_id: str, expected_tail_sha256: str, handlers: Mapping[str, HandlerSpec]) -> dict[str, Any]:
    state, _, pending = load_store(store, handlers)
    need(pending is not None, "PENDING_TAIL", "journal has no incomplete final fragment")
    need(pending["sha256"] == expected_tail_sha256, "STALE", "pending tail hash differs")
    journal = safe_path(store / "events.jsonl").read_bytes()
    lines, observed = read_committed_journal(store)
    need(observed == pending, "STALE", "journal boundary changed during repair preparation")
    committed = b"".join(lines)
    tail = journal[len(committed):]
    need(sha(tail) == expected_tail_sha256 and len(tail) == pending["bytes"], "REPAIR", "pending tail capture differs")
    receipt = {
        "schema": REPAIR_SCHEMA,
        "repair_id": repair_id,
        "mode": "remove_uncommitted_final_fragment",
        "base_sha256": state["base_sha256"],
        "revision": state["revision"],
        "old_journal": {"sha256": sha(journal), "bytes": len(journal)},
        "committed_journal": {"sha256": sha(committed), "bytes": len(committed)},
        "pending_tail": {"sha256": sha(tail), "bytes": len(tail)},
        "authority": {"kind": "explicit_repair_request", "charter_activated": False, "commands_executed": False},
    }
    staging = safe_path(repair_dir.parent / f".{repair_id}.{uuid.uuid4().hex}.tmp")
    staging.mkdir(exist_ok=False)
    published = False
    try:
        write_new(staging / "original-events.jsonl", journal)
        write_new(staging / "pending-tail.bin", tail)
        write_new(staging / "committed-events.jsonl", committed)
        write_new(staging / "repair-receipt.json", packed(receipt) + b"\n")
        staging.rename(repair_dir)
        published = True
    finally:
        if not published and staging.exists():
            need(staging.parent == repair_dir.parent, "PATH", "repair staging escaped recovery directory")
            shutil.rmtree(staging)
    return receipt


def repair_pending_tail(
    store: str | os.PathLike[str],
    *,
    repair_id: str,
    expected_tail_sha256: str,
    handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS,
    fault: Callable[[str], None] | None = None,
) -> dict[str, Any]:
    """Quarantine and remove exactly one verified uncommitted final fragment."""
    repair_id = identity(repair_id)
    need(isinstance(expected_tail_sha256, str) and len(expected_tail_sha256) == 64, "REPAIR", "expected pending-tail hash is required")
    store_path = safe_path(store)
    with writer_lock(store_path):
        recovery_root = safe_path(store_path / "recovery")
        recovery_root.mkdir(exist_ok=True)
        repair_dir = safe_path(recovery_root / repair_id)
        receipt = _receipt(repair_dir) if repair_dir.exists() else _prepare_quarantine(store_path, repair_dir, repair_id, expected_tail_sha256, handlers)
        need(receipt["repair_id"] == repair_id and receipt["pending_tail"]["sha256"] == expected_tail_sha256, "REPAIR", "existing repair receipt differs")
        original = safe_path(repair_dir / "original-events.jsonl").read_bytes()
        committed = safe_path(repair_dir / "committed-events.jsonl").read_bytes()
        tail = safe_path(repair_dir / "pending-tail.bin").read_bytes()
        need(sha(original) == receipt["old_journal"]["sha256"] and len(original) == receipt["old_journal"]["bytes"], "REPAIR", "quarantined journal differs")
        need(sha(committed) == receipt["committed_journal"]["sha256"] and len(committed) == receipt["committed_journal"]["bytes"], "REPAIR", "quarantined committed prefix differs")
        need(sha(tail) == receipt["pending_tail"]["sha256"] and original == committed + tail, "REPAIR", "quarantined tail differs")
        journal_path = safe_path(store_path / "events.jsonl")
        current = journal_path.read_bytes()
        need(current in {original, committed}, "STALE", "journal changed outside the prepared repair")
        already_switched = current == committed
        if fault and not already_switched:
            fault("after_quarantine")
        if not already_switched:
            replacement = safe_path(store_path / f".events.repair-{repair_id}-{uuid.uuid4().hex}.tmp")
            try:
                write_new(replacement, committed)
                os.replace(replacement, journal_path)
            finally:
                if replacement.exists():
                    replacement.unlink()
        if fault:
            fault("after_switch")
        completion_path = safe_path(repair_dir / "switch-completed.json")
        completion = {"schema": REPAIR_SCHEMA, "repair_id": repair_id, "journal_sha256": sha(committed), "revision": receipt["revision"],
                      "status": "committed_prefix_active", "charter_activated": False, "commands_executed": False}
        if completion_path.exists():
            need(parse(completion_path.read_bytes(), tagged=True) == completion, "REPAIR", "repair completion receipt differs")
        else:
            write_new(completion_path, packed(completion) + b"\n")
        state, _, pending = load_store(store_path, handlers)
        need(pending is None and state["revision"] == receipt["revision"], "REPAIR", "repaired journal does not replay to prepared boundary")
        return {"ok": True, "repair_id": repair_id, "revision": state["revision"], "quarantine": str(repair_dir),
                "old_journal_sha256": receipt["old_journal"]["sha256"], "journal_sha256": receipt["committed_journal"]["sha256"],
                "pending_tail_sha256": expected_tail_sha256, "switched": not already_switched, "charter_activated": False, "commands_executed": False}
