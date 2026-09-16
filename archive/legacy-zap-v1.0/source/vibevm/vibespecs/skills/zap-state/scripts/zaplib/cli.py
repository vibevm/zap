"""Reference CLI plus an explicit, caller-supplied command extension seam."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping

from .common import Refusal, identity, need, packed, parse
from .graph import frontier
from .records import CORE_HANDLERS, HandlerSpec, evaluate_stop
from .storage import import_mup, load_store, record


class JsonParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise Refusal("ARGUMENT", message)


@dataclass(frozen=True)
class CliContext:
    handlers: Mapping[str, HandlerSpec]


@dataclass(frozen=True)
class CliExtension:
    """An explicitly supplied CLI command; no discovery or dynamic import occurs."""

    name: str
    configure: Callable[[argparse.ArgumentParser], None]
    execute: Callable[[argparse.Namespace, CliContext], Any]


def build_parser(extensions: Iterable[CliExtension] = ()) -> tuple[JsonParser, dict[str, CliExtension]]:
    parser = JsonParser(description="ZAP reference data kernel: import, journal, projection and stop probes.")
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
    extension_map: dict[str, CliExtension] = {}
    reserved = {"import-mup", "inspect", "events", "record", "evaluate-stop"}
    for extension in extensions:
        identity(extension.name)
        need(extension.name not in reserved and extension.name not in extension_map, "ARGUMENT", f"duplicate CLI command {extension.name}")
        extension_parser = commands.add_parser(extension.name)
        extension.configure(extension_parser)
        extension_map[extension.name] = extension
    return parser, extension_map


def dispatch(args: argparse.Namespace, context: CliContext, extensions: Mapping[str, CliExtension]) -> Any:
    if args.command == "import-mup":
        return import_mup(args.plan, args.tasks_dir, args.out)
    if args.command == "record":
        return record(args.store, parse(Path(args.input).read_bytes()), context.handlers)
    if args.command in extensions:
        return extensions[args.command].execute(args, context)
    state, events, pending = load_store(args.store, context.handlers)
    if args.command == "inspect":
        return {"ok": True, "state": state, "frontier": frontier(state), "dispatch_allowed": False, "pending_tail": pending}
    if args.command == "events":
        need(-1 <= args.after <= state["revision"], "CURSOR", "after must be within the committed journal revision")
        return {"ok": True, "events": [event for event in events if event["seq"] > args.after], "cursor": state["revision"], "pending_tail": pending}
    need(pending is None, "PENDING_TAIL", "stop evaluation requires an unambiguous journal")
    return evaluate_stop(state, parse(Path(args.input).read_bytes()))


def main(argv: list[str] | None = None, *, handlers: Mapping[str, HandlerSpec] = CORE_HANDLERS, extensions: Iterable[CliExtension] = ()) -> int:
    try:
        parser, extension_map = build_parser(extensions)
        args = parser.parse_args(argv)
        result = dispatch(args, CliContext(handlers), extension_map)
        print(packed(result).decode("ascii"))
        return 0
    except (Refusal, OSError, ValueError, KeyError, TypeError, RecursionError) as exc:
        print(packed({"ok": False, "code": getattr(exc, "code", "INVALID_INPUT"), "message": str(exc), "dispatch_allowed": False}).decode("ascii"))
        return 2
