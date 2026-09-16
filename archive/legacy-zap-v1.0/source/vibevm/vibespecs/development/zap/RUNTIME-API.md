# ZAP integration contract — implementation work, 2026-09-13

Root owns architecture and acceptance. Implementation workers use Sol/xhigh.
This file coordinates a bounded implementation; permanent laws belong in the
flow XML and public command/capability contracts. Remove this coordination
material from the release payload after its unique decisions are promoted.

## Baseline

Host worktree `vibevm-next`, branch `next`, accepted baseline `860fcfb7`.
P is this package's `v1.0.0` root. Current sources, MUP context, NEXT425/212,
boot and host dependency selection are preserved. Full ZAP and publication are
authorized; NEXT product execution and the Qwik application are not this task.

## Stable kernel seam (worker A)

- `zaplib.common`: `Refusal`, exact field/type/identity checks, tagged JSON,
  canonical packing/hashing. Preserve legacy runpy helpers via zap_state.py.
- `zaplib.records.HandlerSpec(kind, validate_payload, apply)`;
  `CORE_HANDLERS`, `compose_handlers(*registries_or_specs)`;
  `apply_command(state, command, handlers=CORE_HANDLERS)` checks envelope,
  reason, references, expected revision, copies state, then dispatches.
- `zaplib.storage.load_store(store, handlers=...)` and
  `record(store, command, handlers=...)` use the same explicit registry.
- `zaplib.cli.CliExtension(name, configure, execute)` and
  `main(argv=None, *, handlers=..., extensions=())` are legacy CLI seams.
- Refer to FOUNDATION-API.md for exact callable signatures when A lands.

## Extension ownership

- Domain B: `domain.py`, `domain_*.py`, tests `test_domain*.py`.
  Public `DOMAIN_HANDLERS`, `domain_state(state)`, `domain_frontier(state)`.
  Namespace `state['extensions']['domain']`: intents, outcome revisions,
  obligations, reviews, task contract history, stages, deferrals, acceptances,
  promotions and explicit dispositions. Keep existing core records visible.
- Control C: `control.py`, `service.py`, tests `test_control*.py`.
  Public `CONTROL_HANDLERS`, `control_state(state)`,
  `assess_action(state, action, assessment)` and an application service with
  explicit trusted principal. Namespace `extensions.control` stores charters,
  active charter, pauses, active pause, one-shot exceptions, approach epochs.
- Knowledge D: `knowledge.py`, `sources.py`, `snapshots.py`, related tests.
  `KNOWLEDGE_HANDLERS`, captured sources and typed dependency/applicability
  records in `extensions.knowledge`; integrate existing facts/evidence.
- Runtime E: `runtime.py`, `transport.py`, `worker_host.py`, related tests.
  `RUNTIME_HANDLERS`, persistent jobs/attempts/waits/reservations in
  `extensions.runtime`. Effects use an injected authorized event writer.
- Integration F: `engine.py`, public CLI/backend, migration and end-to-end
  tests, skills/docs/manifests. It composes all explicit handler registries.

Do not introduce circular imports. Pure reducers can inspect other extension
state through documented public read helpers, or receive an explicit policy
hook where needed. They never invoke transports or fetch evidence. Lazy empty
namespaces preserve legacy imports; versions and capabilities remain explicit.
The application service authorizes before append against the captured revision;
CAS refuses changed state. CLI generic record must not bypass that service to
invoke privileged handlers. Replay reproduces an already authorized transition;
trust is not supplied by an event's editable actor string.

## Interoperation and policy

Control offers a stable policy view with campaign/base identity, active charter
revision, delegated action classes, allowed outcome/obligation changes,
essential obligation identities, current stop rules and sticky pause. Domain
adoption/review/acceptance must use it; review proposals themselves are harmless
data. Work done while inactive stays draft, no dispatch/acceptance self-grant.

Initial credentialed backend separates owner, coordinator and worker/agent
capabilities. Credentials belong to a trusted host/control directory outside
the campaign store, and never enter worker argv/env, packets, journal or source.
Possession establishes the configured principal; do not promise protection
against root/same-user processes that can read the control credentials.

Scope/check commands and process transports use explicit argv plus cwd and
captured inputs. Never shell-evaluate source task prose. A started worker is
not a completed task; a producer result is not central acceptance. Stopped,
unknown-effect and completed operations remain distinct and durable.

## Route and acceptance

A foundation → B domain and C control → D knowledge/storage and E runner →
F backend/CLI/migration/integration → independent focused review and repairs →
final package-only regression/real subprocess exercise/NEXT isolation proof →
accurate public docs and published-package install verification.

No requirement is dropped merely because its current prototype is absent.
Root checks the complete spec denominator before publication. Workers leave
bounded durable reports and exact test exits; root reviews and accepts.
