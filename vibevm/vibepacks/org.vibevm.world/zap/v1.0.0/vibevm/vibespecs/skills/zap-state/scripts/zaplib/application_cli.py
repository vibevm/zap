"""Public ZAP CLI: legacy data commands plus trusted runtime/backend operations."""
from __future__ import annotations

import argparse
from pathlib import Path
import sys
from typing import Any
import uuid

from .artifacts import (
    PrivateArtifactStore, capture_native_facts_blob, capture_source_blob,
)
from .backend import BackendApplication
from .backend_http import MAX_BODY, serve
from .cli import JsonParser
from .cli_runtime import build_automatic_coordinator
from .cli_promotion import promote_from_cli
from .cli_trust import (
    bootstrap_trust, load_trust, read_credential, select_principal,
)
from .common import Refusal, identity, need, packed, parse, sha
from .engine import ENGINE_CAPTURE_OBSERVATION_KINDS, ENGINE_HANDLERS, Engine, build_engine
from .graph import frontier
from .domain import build_sparse_review_transition
from .migration import migrate_mup
from .records import apply_command, evaluate_stop
from .recovery import repair_pending_tail
from .snapshots import create_snapshot, load_snapshot_tail
from .storage import import_mup, load_store, safe_path, write_new


def _json_file(path: str | Path) -> Any:
    return parse(Path(path).read_bytes())


def _state(store: str | Path) -> dict[str, Any]:
    return load_store(store, ENGINE_HANDLERS)[0]


def _trusted_engine(
    store: str | Path,
    trust_config: str | Path,
    *,
    host_principal_id: str | None = None,
) -> Engine:
    state = _state(store)
    trust, principals = load_trust(trust_config, state, store=store)
    host = select_principal(principals, host_principal_id) if host_principal_id else None
    return Engine(store, trust, host)


def _credential(args: argparse.Namespace) -> tuple[str, str]:
    return identity(args.credential_id), read_credential(args.credential_file)


def _command_file(path: str | Path) -> dict[str, Any]:
    value = _json_file(path)
    need(isinstance(value, dict), "COMMAND", "command file must contain an object")
    return value


def _add_store(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--store", required=True)


def _add_trust(parser: argparse.ArgumentParser, *, credential: bool = True) -> None:
    parser.add_argument("--trust-config", required=True)
    if credential:
        parser.add_argument("--credential-id", required=True)
        parser.add_argument("--credential-file", required=True)


def build_parser() -> JsonParser:
    parser = JsonParser(description="ZAP adaptive campaign runtime and graph backend.")
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("init", "import-mup", "migrate-mup"):
        cmd = commands.add_parser(name)
        cmd.add_argument("--plan", required=True)
        cmd.add_argument("--tasks-dir", required=True)
        cmd.add_argument("--out", required=True)
    for name in ("inspect", "events", "record", "evaluate-stop", "capabilities"):
        cmd = commands.add_parser(name)
        _add_store(cmd)
        if name == "events":
            cmd.add_argument("--after", type=int, default=-1)
        elif name in {"record", "evaluate-stop"}:
            cmd.add_argument("--command" if name == "record" else "--input", required=True, dest="input")
    overview = commands.add_parser("overview")
    _add_store(overview)
    overview.add_argument("--limit", type=int, default=100)
    overview.add_argument("--cursor")
    overview.add_argument("--states", default="")
    overview.add_argument("--work-types", default="")
    overview.add_argument("--knowledge-states", default="")
    overview.add_argument("--entity-kinds", default="")
    subgraph = commands.add_parser("subgraph")
    _add_store(subgraph)
    subgraph.add_argument("--id", required=True)
    subgraph.add_argument("--depth", type=int, default=1)
    subgraph.add_argument("--limit", type=int, default=100)
    subgraph.add_argument("--cursor")
    search = commands.add_parser("search")
    _add_store(search)
    search.add_argument("--query", required=True)
    search.add_argument("--kinds", default="")
    search.add_argument("--limit", type=int, default=100)
    search.add_argument("--cursor")
    detail = commands.add_parser("detail")
    _add_store(detail)
    detail.add_argument("--kind", required=True)
    detail.add_argument("--id", required=True)
    materialize = commands.add_parser("materialize-review-transition")
    _add_store(materialize)
    materialize.add_argument("--request", required=True)
    for name in ("source-content", "artifact-content"):
        content = commands.add_parser(name)
        _add_store(content)
        content.add_argument("--artifact-store", required=True)
        content.add_argument("--id", required=True)
        content.add_argument("--sha256", required=True)
        content.add_argument("--offset", type=int, default=0)
        content.add_argument("--limit", type=int, default=65536)
    trust = commands.add_parser("trust-bootstrap")
    _add_store(trust)
    trust.add_argument("--trust-dir", required=True)
    charter = commands.add_parser("charter-prepare")
    _add_store(charter)
    charter.add_argument("--charter", required=True)
    charter.add_argument("--out-dir", required=True)
    for name in ("control", "observe"):
        cmd = commands.add_parser(name)
        _add_store(cmd)
        _add_trust(cmd)
        cmd.add_argument("--command", required=True, dest="input")
    action = commands.add_parser("action")
    _add_store(action)
    _add_trust(action)
    action.add_argument("--command", required=True, dest="command_input")
    action.add_argument("--action", required=True)
    action.add_argument("--assessment", required=True)
    action.add_argument("--exception-id")
    capture = commands.add_parser("capture-source")
    _add_store(capture)
    _add_trust(capture)
    capture.add_argument("--artifact-store", required=True)
    capture.add_argument("--path", required=True)
    capture.add_argument("--allowed-root", required=True)
    capture.add_argument("--source-id")
    capture.add_argument("--kind", choices=("file", "vibevm_xml_spec"), default="file")
    capture.add_argument("--event-id")
    create = commands.add_parser("snapshot-create")
    _add_store(create)
    create.add_argument("--out", required=True)
    load = commands.add_parser("snapshot-load")
    _add_store(load)
    load.add_argument("--snapshot", required=True)
    repair = commands.add_parser("repair-pending-tail")
    _add_store(repair)
    _add_trust(repair)
    repair.add_argument("--repair-id", required=True)
    repair.add_argument("--expected-tail-sha256", required=True)
    promote = commands.add_parser("promote-fact")
    _add_store(promote)
    _add_trust(promote)
    promote.add_argument("--project-root", required=True)
    promote.add_argument("--artifact-store", required=True)
    promote.add_argument("--promotion-id", required=True)
    promote.add_argument("--fact-id", required=True)
    promote.add_argument("--source-ref", action="append", required=True)
    promote.add_argument("--target", required=True)
    promote.add_argument("--proof-ref", action="append", required=True)
    promote.add_argument("--authorization-ref", required=True)
    promote.add_argument("--assessment")
    for name in ("tick", "run"):
        cmd = commands.add_parser(name)
        _add_store(cmd)
        _add_trust(cmd, credential=False)
        cmd.add_argument("--host-principal-id", required=True)
        cmd.add_argument("--profile", required=True)
        cmd.add_argument("--artifact-store")
        if name == "run":
            cmd.add_argument("--max-ticks", type=int)
    server = commands.add_parser("serve")
    _add_store(server)
    _add_trust(server, credential=False)
    server.add_argument("--host-principal-id", required=True)
    server.add_argument("--artifact-store")
    server.add_argument("--profile")
    server.add_argument("--host", default="127.0.0.1")
    server.add_argument("--port", type=int, default=8765)
    server.add_argument("--allow-nonlocal", action="store_true")
    server.add_argument("--allowed-origin", action="append", default=[])
    server.add_argument("--max-body-bytes", type=int, default=MAX_BODY, help="0 means no HTTP body limit")
    server.add_argument("--max-follow-seconds", type=float, default=30.0)
    return parser


def _split(value: str) -> list[str]:
    return [item for item in value.split(",") if item]


def _dispatch_read(args: argparse.Namespace) -> Any:
    engine = build_engine(args.store)
    backend = BackendApplication(engine)
    if args.command == "capabilities":
        return backend.capabilities()
    if args.command == "overview":
        return backend.overview(
            limit=args.limit, cursor=args.cursor, states=_split(args.states),
            work_types=_split(args.work_types), knowledge_states=_split(args.knowledge_states),
            entity_kinds=_split(args.entity_kinds),
        )
    if args.command == "subgraph":
        return backend.subgraph(args.id, depth=args.depth, limit=args.limit, cursor=args.cursor)
    if args.command == "search":
        return backend.search(args.query, kinds=_split(args.kinds), limit=args.limit, cursor=args.cursor)
    if args.command == "detail":
        return backend.detail(args.kind, args.id)
    raise Refusal("ARGUMENT", "unknown read operation")


def _capture(args: argparse.Namespace) -> dict[str, Any]:
    engine = _trusted_engine(args.store, args.trust_config)
    credential_id, credential = _credential(args)
    state, _events, pending = engine.load()
    need(pending is None, "PENDING_TAIL", "source capture refuses an incomplete journal")
    principal = engine.trust.authenticate(credential_id, credential, state["plan"]["plan_id"])
    need(principal.role in {"owner", "coordinator"}, "AUTHORIZATION", "source capture requires coordinator authority")
    artifacts = PrivateArtifactStore(args.artifact_store, create=True)
    if args.kind == "vibevm_xml_spec":
        captured = capture_native_facts_blob(
            artifacts, args.path, args.allowed_root, source_id=args.source_id,
        )
    else:
        captured = capture_source_blob(
            artifacts, args.path, args.allowed_root, source_id=args.source_id,
        )
    descriptor = captured.get("source", captured.get("capture", {}).get("source"))
    existing = state.get("extensions", {}).get("knowledge", {}).get("sources", {}).get(descriptor["id"])
    if existing is not None and existing["content_sha256"] == descriptor["content_sha256"]:
        return {"ok": True, "idempotent": True, "already_current": True, "capture": captured, "event": None}
    if args.kind == "vibevm_xml_spec" and existing is None:
        kind, payload = "knowledge.native-facts-recorded", {"capture": captured["capture"]}
    elif existing is None:
        kind, payload = "knowledge.source-recorded", {"source": descriptor}
    else:
        kind, payload = "knowledge.source-recaptured", {
            "previous_sha256": existing["content_sha256"], "source": descriptor,
        }
    prior = "new" if existing is None else existing["content_sha256"][:16]
    event_id = args.event_id or f"capture:{descriptor['id']}:{prior}:{descriptor['content_sha256'][:16]}"
    command = {
        "event_id": event_id, "base_revision": state["revision"], "kind": kind,
        "reason": {"summary": f"Trusted byte capture {descriptor['id']}"},
        "payload": payload,
    }
    receipt = engine.service.submit_observation(
        command, credential_id=credential_id, credential=credential,
    )
    return {"ok": True, "capture": captured, "event": receipt}


def _prepare_charter(args: argparse.Namespace) -> dict[str, Any]:
    state = _state(args.store)
    raw = _json_file(args.charter)
    validated = ENGINE_HANDLERS["control.charter-drafted"].validate_payload({"charter": raw})
    charter = validated["charter"]
    draft = {
        "event_id": f"draft:{uuid.uuid4()}", "base_revision": state["revision"],
        "kind": "control.charter-drafted", "reason": {"summary": "Draft exact owner charter"},
        "payload": {"charter": charter},
    }
    after_draft = apply_command(state, draft, ENGINE_HANDLERS)
    activation = {
        "event_id": f"activate:{uuid.uuid4()}", "base_revision": after_draft["revision"],
        "kind": "control.charter-activated", "reason": {"summary": "Activate exact owner charter"},
        "payload": {
            "charter_id": charter["charter_id"], "charter_revision": charter["revision"],
            "charter_sha256": sha(packed(charter)), "campaign_id": charter["campaign_id"],
            "base_sha256": charter["base_sha256"],
        },
    }
    apply_command(after_draft, activation, ENGINE_HANDLERS)
    out = Path(args.out_dir).absolute()
    need(not out.exists(), "OUTPUT_EXISTS", "charter preparation requires a fresh output directory")
    out.mkdir(parents=True)
    out = safe_path(out)
    write_new(out / "charter.normalized.json", packed(charter) + b"\n")
    write_new(out / "draft-command.json", packed(draft) + b"\n")
    write_new(out / "activation-command.json", packed(activation) + b"\n")
    return {
        "ok": True, "charter_sha256": activation["payload"]["charter_sha256"],
        "draft_command": str(out / "draft-command.json"),
        "activation_command": str(out / "activation-command.json"),
        "normalized_charter": str(out / "charter.normalized.json"),
        "execution_started": False,
    }


def dispatch(args: argparse.Namespace) -> Any:
    if args.command in {"init", "import-mup"}:
        return import_mup(args.plan, args.tasks_dir, args.out)
    if args.command == "migrate-mup":
        return migrate_mup(args.plan, args.tasks_dir, args.out)
    if args.command in {"overview", "subgraph", "search", "detail", "capabilities"}:
        return _dispatch_read(args)
    if args.command == "materialize-review-transition":
        state, _events, pending = load_store(args.store, ENGINE_HANDLERS)
        transition = build_sparse_review_transition(state, _json_file(args.request))
        return {"ok": True, "schema": "zap-domain/materialized-review-transition/1",
                "base_sha256": state["base_sha256"], "revision": state["revision"],
                "cursor": state["revision"], "pending_tail": pending, "transition": transition}
    if args.command in {"source-content", "artifact-content"}:
        backend = BackendApplication(build_engine(args.store), PrivateArtifactStore(args.artifact_store))
        method = backend.source_content if args.command == "source-content" else backend.artifact_content
        return method(args.id, args.sha256, offset=args.offset, limit=args.limit)
    if args.command == "trust-bootstrap":
        return bootstrap_trust(args.store, _state(args.store), args.trust_dir)
    if args.command == "charter-prepare":
        return _prepare_charter(args)
    if args.command == "inspect":
        state, _events, pending = load_store(args.store, ENGINE_HANDLERS)
        return {"ok": True, "state": state, "frontier": frontier(state), "dispatch_allowed": False, "pending_tail": pending}
    if args.command == "events":
        state, events, pending = load_store(args.store, ENGINE_HANDLERS)
        need(-1 <= args.after <= state["revision"], "CURSOR", "after must be within committed history")
        return {"ok": True, "events": [event for event in events if event["seq"] > args.after], "cursor": state["revision"], "pending_tail": pending}
    if args.command == "record":
        return build_engine(args.store).service.submit_agent(_command_file(args.input))
    if args.command == "evaluate-stop":
        state, _events, pending = load_store(args.store, ENGINE_HANDLERS)
        need(pending is None, "PENDING_TAIL", "stop evaluation requires an unambiguous journal")
        return evaluate_stop(state, _json_file(args.input))
    if args.command in {"control", "observe"}:
        engine = _trusted_engine(args.store, args.trust_config)
        credential_id, credential = _credential(args)
        command = _command_file(args.input)
        if args.command == "observe":
            kind = command.get("kind")
            if isinstance(kind, str):
                need(kind not in ENGINE_CAPTURE_OBSERVATION_KINDS, "OBSERVATION", "source capture must use capture-source so bytes are read and stored first")
        method = engine.service.submit_control if args.command == "control" else engine.service.submit_observation
        return method(command, credential_id=credential_id, credential=credential)
    if args.command == "action":
        engine = _trusted_engine(args.store, args.trust_config)
        credential_id, credential = _credential(args)
        return engine.service.apply_control_action(
            _command_file(args.command_input), _json_file(args.action), _json_file(args.assessment),
            credential_id=credential_id, credential=credential,
            exception_id=args.exception_id,
        )
    if args.command == "capture-source":
        return _capture(args)
    if args.command == "snapshot-create":
        return create_snapshot(args.store, args.out, ENGINE_HANDLERS)
    if args.command == "snapshot-load":
        return load_snapshot_tail(args.store, args.snapshot, ENGINE_HANDLERS)
    if args.command == "repair-pending-tail":
        state = _state(args.store)
        trust, _principals = load_trust(args.trust_config, state, store=args.store)
        credential_id, credential = _credential(args)
        principal = trust.authenticate(credential_id, credential, state["plan"]["plan_id"])
        need(principal.role == "owner", "AUTHORIZATION", "pending-tail repair requires owner credential")
        return repair_pending_tail(
            args.store, repair_id=args.repair_id,
            expected_tail_sha256=args.expected_tail_sha256,
            handlers=ENGINE_HANDLERS,
        )
    if args.command == "promote-fact":
        engine = _trusted_engine(args.store, args.trust_config)
        credential_id, credential = _credential(args)
        return promote_from_cli(
            engine, credential_id=credential_id, credential=credential,
            project_root=args.project_root, artifact_store=args.artifact_store,
            promotion_id=args.promotion_id, fact_id=args.fact_id,
            source_refs=args.source_ref, target=args.target,
            proof_refs=args.proof_ref, authorization_ref=args.authorization_ref,
            assessment_path=args.assessment,
        )
    if args.command in {"tick", "run"}:
        engine = _trusted_engine(args.store, args.trust_config, host_principal_id=args.host_principal_id)
        artifacts = PrivateArtifactStore(args.artifact_store, create=True) if args.artifact_store else None
        coordinator = build_automatic_coordinator(engine, args.profile, artifacts=artifacts)
        return coordinator.tick() if args.command == "tick" else coordinator.run(max_ticks=args.max_ticks)
    if args.command == "serve":
        engine = _trusted_engine(args.store, args.trust_config, host_principal_id=args.host_principal_id)
        artifacts = PrivateArtifactStore(args.artifact_store, create=args.profile is not None) if args.artifact_store else None
        coordinator = build_automatic_coordinator(engine, args.profile, artifacts=artifacts) if args.profile else None
        need(args.max_body_bytes >= 0, "BIND", "max body bytes must be nonnegative")
        serve(
            BackendApplication(engine, artifacts, coordinator),
            host=args.host, port=args.port, allow_nonlocal=args.allow_nonlocal,
            allowed_origins=args.allowed_origin,
            max_body_bytes=None if args.max_body_bytes == 0 else args.max_body_bytes,
            max_follow_seconds=args.max_follow_seconds,
        )
        return {"ok": True}
    raise Refusal("ARGUMENT", "unsupported command")


def main(argv: list[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        result = dispatch(args)
        print(packed(result).decode("ascii"))
        return 0
    except (Refusal, OSError, ValueError, KeyError, TypeError, RecursionError) as exc:
        print(packed({"ok": False, "code": getattr(exc, "code", "INVALID_INPUT"), "message": str(exc), "dispatch_allowed": False}).decode("ascii"))
        return 2


if __name__ == "__main__":
    sys.exit(main())
