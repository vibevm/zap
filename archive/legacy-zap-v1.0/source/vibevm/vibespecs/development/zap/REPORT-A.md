# ZAP-A foundation extraction report

## Result

The 616-line reference kernel is now a compatibility entrypoint over a small
`zaplib` package. All five existing CLI commands, eight core event kinds, old
runpy helper access, canonical JSON encoding, immutable `base.json`, append-only
`events.jsonl`, revision/idempotency behavior, raw captures, writer-lock rules,
and pending-tail behavior remain available.

The projection adds only backward-compatible metadata:

- `projection_schema: "zap-projection/1"`
- `capabilities: {"zap.core": 1, "zap.extensions": 1}`
- `extensions: {}`

Extension modules initialize only their own `domain`, `control`, `knowledge`,
or `runtime` namespace. The legacy store schema and base representation remain
`zap/1`; no migration or base rewrite occurs.

## Package boundaries

- `common.py` owns stable constants, refusals, primitive validation, tagged TOML
  round-trip support, canonical JSON packing/parsing, and hashing.
- `graph.py` owns plan/task validation and the legacy structural frontier.
  Cycle checks use iterative DFS, so a deep valid plan no longer hits Python's
  recursion limit.
- `records.py` owns initial projection state, generic envelope/CAS validation,
  strict payload boundaries, core specialized handlers, explicit registry
  composition, the reducer, and existing stop-probe evaluation.
- `storage.py` owns lossless import, immutable-base verification, journal replay,
  non-stealing writer locking, idempotency, CAS append, and pending-tail refusal.
- `cli.py` owns the five-command parser/dispatcher and explicit caller-supplied
  CLI extensions.
- `__init__.py` publishes the maintained internal surface. `zap_state.py`
  deliberately re-exports it for existing runpy callers.

Every event handler receives a deep-copied state and a detached payload only
after exact-field validation. Registry composition refuses duplicate kinds and
key/spec mismatches. There is no dynamic discovery, arbitrary import, or eval.
After every handler effect, the generic reducer validates the complete global
plan before advancing the revision. This also binds future handlers that change
node state, evidence, acceptance, hierarchy, or dependencies.

Authentication data does not enter the generic record envelope. A future
trusted transport must validate signatures and authority bindings before it
calls `record`; the journal stores the deterministic transition that replay can
reproduce. Actor strings inside payload or extension state confer no authority.

`record` now reports the actual post-handler `execution_mode`, and reports the
replayed mode on an idempotent retry. Core-only stores still return `draft`.

The exact callable contracts and sibling-entrypoint composition example are in
`FOUNDATION-API.md`.

## Compatibility evidence

Command run from `vibevm/vibespecs/skills/zap-state/scripts`:

```text
python -B -m unittest test_zap_state.py test_foundation.py
.........................
----------------------------------------------------------------------
Ran 25 tests in 1.181s

OK
```

The 18 original behavior tests pass unchanged. Seven foundation tests add these
checks:

- the compatibility shim exposes every legacy helper used through runpy;
- imported base bytes equal an independent implementation of the legacy JSON
  encoding, with exact source bytes and unknown fields preserved;
- a manually formed old `zap/1` base/receipt store replays and accepts a core
  append without rewriting its base;
- an explicit extension handler receives detached state/payload, records and
  replays deterministically, and preserves a truthful execution mode on new and
  idempotent results;
- an extension attempt to mark a node accepted without global prerequisites is
  rejected after its handler effect, leaving the input state unchanged;
- a valid 1,100-node deep plan passes full graph validation without recursion;
- explicit CLI extension output and unknown-command errors remain machine-readable
  JSON.

No NEXT execution, product runner, owner trust adapter, SQLite store, snapshot,
repair flow, or active-plan mutation was added. Those remain separate tasks.
