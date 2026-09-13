---
name: zap-state
description: Import or migrate a ZAP campaign store, inspect/query its graph and history, capture sources, create snapshots, and use the authenticated control/backend surface.
---

# ZAP state and queries

Use Python 3.11 or later and this skill's `scripts/zap.py`. The older
`scripts/zap_state.py` remains a compatible entrypoint. All successful
responses and command errors are JSON. See
[the data design](../../flows/zap/ZAP-DATA-AND-VIEWER.xml) for semantics and
[the command contract](../../examples/zap/command-contract.json) for exact JSON
fields and routes; `capabilities` reports the exact composed handler/schema
registries. `--help` describes the CLI. Use `python -B` to avoid bytecode.

Import a captured shared seed or an explicitly selected personal plan into a
**new** local directory. Keep personal captures outside the repository. Never
overwrite the active MUP context or infer authorization from an imported field.

```text
python -B scripts/zap_state.py import-mup --plan PLAN --tasks-dir TASKS --out NEW_STORE
python -B scripts/zap_state.py inspect --store STORE
python -B scripts/zap_state.py events --store STORE --after 0
python -B scripts/zap_state.py record --store STORE --command COMMAND.json
python -B scripts/zap_state.py evaluate-stop --store STORE --input INPUT.json
python -B scripts/zap.py capabilities --store STORE
python -B scripts/zap.py overview --store STORE --limit 100
python -B scripts/zap.py detail --store STORE --kind node --id NODE
python -B scripts/zap.py trust-bootstrap --store STORE --trust-dir PRIVATE_TRUST
```

`inspect` is the complete committed projection. Overview, subgraph, search and
detail are bounded graph/client projections. Read the current revision before
preparing a command. A command names `event_id`, `base_revision`, `kind`,
`reason: {summary, evidence_refs}` and `payload`. Keep the same identity and exact
content when retrying an uncertain record. A stale revision requires rereading
and reconsidering the proposal, not an automatic overwrite. Preserve corrupt or
partial logs as evidence; owner-authenticated repair handles only an exact
uncommitted final fragment and quarantines every original byte.

Ordinary `record` goes through the agent-data route and cannot invoke control,
actions or trusted observations. Owner/coordinator commands use protected trust
configuration and credential files outside the store. Never put token values in
JSON, argv, environment, packets or the journal. Readers have a separate
campaign-bound credential and cannot mutate state.

Stop evaluation takes an input object with `rules` set to the
[example fixture](../../examples/zap/stop-rules.json) and `assessment` containing
`phase` (`before_action` or `after_action`) and the semantic inputs. Missing truth
values remain unknown. The result is a simulation on an unapproved example;
`action_admitted` remains false. Two failed approaches count only for the same
problem; provider failures and retries do not create new architectural approaches.

Use `capture-source` for guarded byte capture into the private content-addressed
store; a task `read_path` is never a content endpoint. Snapshots bind the exact
base, reducer and committed prefix.

Return the actual base/revision/cursor, source identity, stored reason and
limitations. The package implements the runtime and backend, while the
Heroes-style interactive canvas itself remains future work. Installation,
import and migration never activate or start NEXT.
