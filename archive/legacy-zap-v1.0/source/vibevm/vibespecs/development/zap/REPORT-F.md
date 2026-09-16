# ZAP-F application integration report

## Review status

This report is implementation evidence for root review. It does not accept the
package, select ZAP for NEXT, activate a campaign, publish a package, or claim
that the future canvas application exists.

## Composed application surface

`zaplib.engine` composes the public core, control, domain, knowledge and
runtime registries into one 82-handler registry. Every extension handler has
one disjoint data, privileged-action or trusted-observation route; startup
refuses collisions, omissions and missing payload descriptors. Capabilities
are generated from those actual registries under the honest
`zap-payload-descriptor/1` dialect.

The public entrypoint is `scripts/zap.py`. `zap_state.py` retains its five
legacy commands and runpy exports, while ordinary `record` now crosses the
data-only `ApplicationService` route. An active `plan.refined`, acceptance,
dispatch, adjudication, promotion or other product action cannot use that
legacy route.

## Trust, effects and runtime

Trust bootstrap creates campaign/base-bound owner, coordinator and reader
credentials in a protected directory outside the campaign store. Command-line
arguments carry credential-file paths, never credential values. There is no
self-asserted owner role. Reader credentials have no command scope.

Source capture reads a file within one configured root, verifies the bytes did
not change while capturing, stores an immutable content-addressed version, and
only then submits its descriptor as a trusted observation. Generic HTTP and CLI
observation commands refuse source capture kinds, preventing editable JSON from
standing in for an adapter read. Applicability, source reobservation,
adjudication and promotion remain privileged actions.

The CLI builds E's `AutomaticCoordinator` from a strict profile. Runtime
actions use exact service admission; transport and semantic receipts use the
trusted observation route and remain recordable during pause. Runtime data
kinds are empty, so public agents cannot enqueue model-provider spend. The
Codex Sol/xhigh profile uses E's ready adapter and preserves the named provider
authentication homes without placing ZAP control credentials in the process
environment. Forced termination remains explicit and opt-in.

Semantic `eq` stop fields can use F's trusted JSON-process assessment profile.
Its ProcessTransport request is nonblocking and binds the exact action/payload/
source set, stable logical product command, active policy, campaign/base,
bounded relevant facts and configured trusted observations. An unrelated ZAP
revision does not change that basis; changed command/source/observation bytes
do. Cached responses bind that exact request and provide only required
scalar/null values; drain targets are
always derived from current persisted jobs. Pending, stale, failed or null
assessment cannot become false or confer authority, while unrelated action
classes with no semantic field requirement do not spawn the provider.
Authenticated readers can inspect pending/completed request/action/policy/source
bindings, transport state/receipt hashes and observed values at
`/v1/assessments`; private observation paths/content and stdout remain hidden.

## Verified projection cache

Engine, application service, backend and coordinator share one process-local
`ProjectionCache`. Every load rereads exact base and journal bytes, retains
strong reducer object/callable identities, returns detached values, and applies
only complete appended suffix lines. Base/reducer changes cold-replay. A changed
already-verified journal prefix refuses until explicit cache invalidation after
review. Pending tails and corrupt/malformed middle records keep their existing
refusal behavior; no timestamp, disk format, journal rewrite or global plan
limit is introduced.

C's natural-exit delivery seam was completed for a pause captured before
transport submission. A later `already_terminal` acknowledgement resolves the
previously unknown descriptor only from the trusted accepted submission and
the exact natural terminal result, persists the exact attempt/descriptor, and
records `signal_delivered=false`. `stopped` and `interrupted` still refuse that
route; safe-state proof remains independent.

Fact promotion uses D's source reobservation and portable accepted-proof
artifact, then records B's real `domain.fact-promotion-recorded` metadata
through `fact.promote`. It does not append an invented adapter event kind.

## Backend and future canvas data

`BackendApplication` is framework-independent. Its standard-library HTTP
adapter defaults to loopback, requires an explicit nonlocal switch, admits only
exact configured browser origins and authenticates every disclosure. It
provides snapshot, committed tail, a finite resumable SSE batch and a bounded
live follow subscription, paginated overview/subgraph,
search, full entity detail, and registered-handle content reads. Cursor tokens
bind base, revision, query and offset. Foreign bases, gaps and stale pages are
explicit. Responses and SSE frames use the shared tagged wire codec; request
bodies refuse duplicate members, nonfinite numbers and invalid UTF-8. Body and
follow capacities are reported operator settings rather than plan limits.

An incomplete final journal fragment is visible to an authorized reader only
as a hash/length diagnostic over the last committed projection. Every mutation
and runtime tick refuses it. Owner repair verifies the exact tail hash and
keeps quarantined original bytes; middle corruption is not treated as a tail.

Node, edge, unexamined region, decision, job and the remaining domain entities
are independently addressable. Detail includes collected content, provenance,
history, explicit stale/unavailable flags, and node contract/criteria/steps/
checks/sources/evidence/obligations/edges in one response. The backend keeps
semantic knowledge, work type, maturity, execution, structure, relation and
visibility separate from colors, geometry, fog and camera. This supplies the
data contract for the planned bright Heroes 3-style strategy canvas without
implementing its UI.

Snapshots/events redact nested worker packets, raw provider material, full
semantic command payloads, absolute source roots and private artifact locators.
They retain a bounded structured semantic-decision view with request binding,
disposition, selection, public rationale, command reasons, affected IDs and
source/evidence provenance. Content endpoints accept
only a registered handle and exact captured hash; task paths do not become a
filesystem API. Historical captured source versions remain readable after live
drift.

## CLI and package projection

The CLI exposes import/migration, capabilities, graph queries, exact control
and actions, guarded source capture, snapshots, owner repair, proof-bound
promotion, runtime tick/run and backend serve. Import, migration and package
installation do not activate a charter or start execution.

The pure sparse-review builder accepts only changed dispositions and explicit
preservation choices, expands every omitted active obligation to a complete
retained row, and appends nothing. CLI and authenticated reader backend routes
return the fully materialized ordinary review transition; only that complete
form may enter a later `domain.review-proposed` event.

The package manifest ships `zap-draft`, `zap-state` and `zap-run`. The
`zap-run` wrapper resolves a canonical package, installed dependency slot or
unambiguous sibling projected `zap-state` runtime and refuses ambiguity. Public
runtime/backend/CLI contracts live under `flows/zap`; development reports stay
excluded by `.vibeignore`. The skill-creator validator reports the new skill as
valid.

## Verification evidence

Focused F tests use temporary directories and actual loopback HTTP/CLI
subprocesses. They cover route partition, inactive import, auth and CORS,
snapshot-plus-tail continuity, finite-tail and live-follow reconnect,
tagged TOML values, strict duplicate/nonfinite JSON refusal, configurable
transport capacities, foreign/gap/stale cursors,
pending-tail readability and mutation refusal, nested redaction, every
meaningful canvas-detail family used by the fixture, guarded historical
content, forged-source refusal, no read-time artifact-store creation, projected
skill resolution, explicit trust/activation, draft zero-execution tick,
credential-preserving semantic construction, sparse large-plan expansion, a
bounded public semantic-decision explanation, and real B promotion metadata.

Root separately reported an isolated migration of the current NEXT source with
425 nodes, 212 task contracts, 64 mandates and 1,292 derived obligations. Its
exact retry was idempotent, source hashes stayed unchanged, the resulting
campaign remained inactive, and transport activity was zero. This report does
not expose the private fixture path and does not authorize migration of the
live campaign.

Root also reported the authorized isolated Sol/xhigh live probe: one worker
attempt and one independent verification produced and checked exactly 15
artifact bytes with SHA-256
`19b32baf08503ceab0fc41f2e4880162cc8528ec040be59e71eed35787de65af`;
the original outcome closed as accepted at revision 79. A semantic restart
reused the captured artifact/check rather than repeating their effects. No
private path, credential, prompt or provider transcript is part of this report.

The exact F-focused command is:

```text
python -B -m unittest test_engine.py test_artifacts.py test_backend.py test_cli.py
```

Run from `vibevm/vibespecs/skills/zap-state/scripts`, the original F-focused
boundary passed 35 tests. The final frozen existing-feature Windows discovery
ran 238 tests in 589.895 seconds with `OK`. Linux/install/publish evidence is
root-owned and was still pending when this report was updated.

Additional post-review focused gates pass: five trusted assessment-provider
tests (actual loop false dispatches, true pauses, unknown never dispatches,
derived drains, unrelated revision cache reuse, changed command/source and
stale response refusal, unaffected work avoiding provider spawn); three nested
knowledge schema tests; six portable-promotion tests including generation-0
legacy replay; and five projection-cache tests plus the existing service/
snapshot suite.

On a temporary copy of the authorized 425-node/212-task migration store, one
cold load took 0.1127 seconds and five unchanged warm loads averaged 0.00925
seconds (about 12.2x faster). Ten synthetic data-only appends averaged 0.0648
seconds with cache stats `cold=1, suffix=10, warm=26`; no append triggered full
replay. A modified committed event refused `JOURNAL`. The source store remained
read-only, and no source content or private path is recorded here.
