# ZAP foundation internal API

This API is the maintained Python 3.11 standard-library boundary for sibling
ZAP modules. `zap_state.py` is a compatibility entrypoint and re-exports the
legacy helper surface; new code should import the owning `zaplib` module.

## State and schema

`zaplib.records.initial_state(base: dict, base_hash: str) -> dict` constructs a
fresh projection. It retains `schema == "zap/1"` and adds the backward-compatible
projection markers `projection_schema == "zap-projection/1"`,
`capabilities == {"zap.core": 1, "zap.extensions": 1}`, and `extensions == {}`.
Sibling reducers own lazily initialized namespaced dictionaries below
`state["extensions"]`; reserved first-level names are `domain`, `control`,
`knowledge`, and `runtime`. A reducer must never treat an actor string or data inside this
dictionary as authority.

`zaplib.graph.validate_plan(plan) -> (nodes, inherited_dependencies, children)`
validates the complete global MUP graph. `acyclic(graph) -> None` uses iterative
DFS and has no recursion-depth plan limit. `validate_tasks(groups, nodes) ->
tasks` validates task groups while preserving task extension fields. `frontier(state)
-> list[str]` returns the existing structural leaf frontier; it grants no
dispatch authority.

Common strict validators and canonical encoding are in `zaplib.common`:
`need`, `exact`, `string`, `strings`, `identity`, `wire`, `unwire`, `packed`,
`parse`, and `sha`. Expected invalid input raises `Refusal(code, message)`.

## Event handlers and reducer

An event handler is declared as:

```python
HandlerSpec(
    kind: str,
    validate_payload: Callable[[Any], dict[str, Any]],
    apply: Callable[[dict[str, Any], dict[str, Any], str], None],
)
```

The three `apply` arguments are a deep-copied projection, the detached payload
returned by `validate_payload`, and the validated event ID. `apply` mutates the
copied projection in place and returns `None`. It must be deterministic and may
only derive state from its arguments. It does not append a journal record,
perform I/O, consult current time, or authenticate an actor.

`strict_payload(required: set[str], optional=()) -> validator` returns the
standard exact-field validator. A specialized module may instead supply a
validator with deeper type and value checks, but it must return a detached
plain dictionary. A handler therefore never sees an unvalidated command
payload. TypedDict definitions may narrow that dictionary inside the owning
module.

`CORE_HANDLERS` is the immutable registry for the eight legacy event kinds.
`compose_handlers(*sources) -> Mapping[str, HandlerSpec]` creates an immutable
registry from explicit mappings, iterables, or individual specs. It refuses a
registry key/spec mismatch and duplicate kinds, so extensions cannot silently
override core behavior. There is no module discovery, `eval`, entry-point scan,
or arbitrary import.

`validate_command_envelope(state, command) -> (event_id, kind)` validates the
exact generic envelope, event identity, revision CAS, reason, and existing
reason references. Authentication metadata is deliberately outside this
envelope. A trusted service must verify signatures, bindings, role, scope, and
freshness before calling the generic record API; replay consumes only the
stored deterministic transition.

`apply_command(state, command, handlers=CORE_HANDLERS) -> new_state` performs
envelope/CAS validation, selects an explicit handler, deep-copies state,
strictly validates payload, applies the handler, validates the complete global
plan after the effect, and advances revision by one. The input state is never
mutated. The final global validation applies to extension handlers too, so node
state, acceptance, evidence, dependency, or hierarchy changes cannot bypass
the plan invariants.

## Storage

`zaplib.storage.import_mup(plan_path, tasks_dir, out) -> result` preserves the
legacy `zap/1` base bytes, canonical JSON encoding, raw captures, base hash, and
receipt shape. It creates only a fresh `base.json` and `events.jsonl` store.

`load_store(store, handlers=CORE_HANDLERS) -> (state, events, pending_tail)`
validates immutable base captures and deterministically replays committed JSONL
records with the supplied registry. Legacy stores need no migration. A final
line without newline is returned as pending evidence and is not replayed.

`record(store, command, handlers=CORE_HANDLERS) -> result` holds the existing
non-stealing writer lock, loads and replays with the same registry, enforces
pending-tail and idempotency rules, then appends one fsynced canonical JSONL
record. Its `execution_mode` result reflects the replayed state for an
idempotent retry and the post-handler state for a new event, with `draft` only
as a compatibility fallback. Extensions must pass the same composed registry to both `record` and
all later `load_store` calls. No active MUP plan file is written.

`safe_path`, `write_new`, `capture`, and `writer_lock` remain available for the
reference store implementation. A sibling service should call `record` rather
than reproduce append semantics.

## CLI extension

An explicit CLI command is declared as:

```python
CliExtension(
    name: str,
    configure: Callable[[argparse.ArgumentParser], None],
    execute: Callable[[argparse.Namespace, CliContext], Any],
)
```

`configure` adds arguments to its already-created subparser. `execute` returns
the JSON-serializable result that `main` prints. `CliContext.handlers` is the
registry selected by the entrypoint. State-changing commands call
`zaplib.storage.record`; the CLI callback does not mutate a projection or base
file directly.

`build_parser(extensions=()) -> (parser, extension_map)` refuses names that
collide with the five core commands. `dispatch(args, context, extension_map) ->
result` runs one selected operation. `main(argv=None, *, handlers=CORE_HANDLERS,
extensions=()) -> int` prints one canonical JSON value and returns 0 on success
or 2 with `ok`, `code`, `message`, and `dispatch_allowed` on handled failure.
The compatibility entrypoint calls this default unchanged.

Sibling entrypoints wire fixed imports explicitly:

```python
from zaplib import CORE_HANDLERS, compose_handlers, main
from zaplib.domain import DOMAIN_HANDLERS, DOMAIN_COMMANDS

HANDLERS = compose_handlers(CORE_HANDLERS, DOMAIN_HANDLERS)
raise SystemExit(main(handlers=HANDLERS, extensions=DOMAIN_COMMANDS))
```

The entrypoint owns this composition. Store content never chooses Python code.
