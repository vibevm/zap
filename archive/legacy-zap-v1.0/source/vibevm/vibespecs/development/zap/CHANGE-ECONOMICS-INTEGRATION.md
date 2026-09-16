# ZAP change economics integration plan

This is an implementation plan for the contract in
`flows/zap/ZAP-CHANGE-ECONOMICS.xml`. It does not add a gate, handler, route,
or runtime job. Implementation starts only after the pure economics and runtime
contracts below are frozen.

## Freeze prerequisites

The current design API describes assessment effects as hashes, while
`EXACT-ENVELOPE` requires immutable typed payloads and deterministic simulation
of each ordered effect. The implementation contract must therefore freeze one
canonical full effect-bundle shape. Each effect needs at least its event kind,
strict payload, payload hash, subject IDs, predecessor effect IDs, and scoped
relevant-basis hashes before and after simulation. The bundle hash must cover
order and every field. A hash-only producer claim is insufficient.

The relevant basis must also be a named, strict structure. It must include the
active intent/outcome and policy bindings, affected graph and dependency
closure, applicable source and proof captures, material contracts, and the
configured team-capacity fingerprint. It must exclude economics events, hold
receipts, the bundle's already recorded effects, and unrelated progress.
Unknown closure is data in the basis and never an omitted field. E owns the
final technical API for the canonical structure and the read-only
Domain/Knowledge helpers that construct and compare it; B checks its alignment
with the normative XML.

E's proposed estimator boundary is accepted for integration: a nonblocking
`ChangeEstimator.submit/poll` consumes `zap-change-estimate-request/1` with the
full validated effect bundle, scoped basis, affected closure and fog, team
model, policy and budget, alternatives, and source captures. Its result is an
untrusted, exactly bound assessment proposal. Pure economics validation and
deterministic adjudication decide the recommendation. The estimator cannot
admit its own result.

Before code begins, E must freeze the exact effect, scoped-basis, estimator
request/result, event, registry-map, and persisted lifecycle schemas, including
stable request identity, receipt, staleness, provider/resource wait, retry, and
terminal failure. E must also freeze how a stored bundle is referenced by
`reason.decision_ref`; B then checks the contract against the normative XML. A
producer's reference is only a lookup hint; service recomputation supplies
authority.

## Composition and classification

`engine.py` will compose economics, control, domain, knowledge, and runtime
registries from their exported maps. Composition must prove that every handler
has exactly one public route class or is explicitly internal, that route sets
are disjoint, that schemas exist for every event, and that no handler or schema
key collides.

`CHANGE_EVENT_CLASSIFICATION` is a second, independent classification applied
to every product handler. It classifies a typed payload as semantic,
baseline-exempt, progress/reconciliation, proof/observation, or economics-owned.
Service construction fails if a product handler is absent. Payload-sensitive
variants such as `domain.review-applied` use strict typed classifiers rather
than kind-name or prose matching. New product handlers cannot silently inherit
an exempt class.

Economics proposals remain agent data, Owner policy activation and decisions
remain credentialed control, runtime receipts remain trusted observations, and
work release remains action `plan.lower`. Adjudication, holds, admissions,
consumption, and hold resolution are internal handlers. No generic agent,
host, action, or control submission method may accept an internal kind.

The first exact charter-bound intent/outcome envelope is the only baseline
exception. The service derives it from the active charter binding and imported
baseline; a caller cannot label a command as initial. Replay never invokes this
admission classifier.

## Service admission transaction

All public mutations continue through `ApplicationService`. The order for a
new request is:

1. authenticate the route and validate the command envelope;
2. recover an exact duplicate or existing reservation before applying CAS;
3. validate the product payload and classify its typed variant;
4. run existing charter, action, pause, source, graph, obligation, acceptance,
   and hold checks;
5. for a semantic change, resolve the stored bundle through
   `reason.decision_ref`, recompute its scoped basis and affected closure, and
   validate the current policy, assessment, recommendation, and Owner decision;
6. reserve only the next unconsumed effect with its exact command hash and
   expected scoped basis;
7. apply the product handler through the existing registry and CAS path, then
   record the product event identity and bundle advancement.

An automatic admission is available only for a trusted, deterministic `take`
adjudication that is bounded under the active threshold. An Owner decision is
required when the estimate is above threshold or the conservative unknown rule
requires it. Approval does not bypass any existing authorization or safety
check.

The reservation uses the existing crash-safe action pattern. An exact retry
after a crash between reservation and product append finds the reservation
before rejecting the old base revision, recomputes every relevant
precondition, and either applies the same effect once or refuses it as stale.
Unrelated appends may rebind the physical revision. A changed command, source,
policy, closure, contract, proof, team fingerprint, or scoped basis may not.
An applied effect is never replayed, and a pending reservation is never treated
as a completed product mutation. The final consumption marker remains
reconcilable if its append is interrupted.

Historical `zap/1` events replay through reducers without economics authority
or synthetic approvals. The lazy projection supplies the default policy as a
read result only. The mandatory gate applies when a new semantic mutation is
submitted through the service.

## Runtime and estimator

The coordinator stores a candidate semantic command as a proposal with its
full effect bundle. It does not apply the product event while economics is
pending. Existing command response schemas must allow the optional
`reason.decision_ref` for a stored change or decision, but must not allow a
model to emit internal adjudication, admission, hold, or Owner decision events.

The estimator request includes the actual bounded command payloads and
relevant collected facts, not only hashes. Registered source or artifact
content may be supplied through the existing verified private-artifact reader;
arbitrary paths and raw provider packets are excluded. The public result keeps
category bases, assumptions, alternatives, uncertainty, recommendation, and a
concise rationale. Private prompts, hidden reasoning, credentials, and raw
provider bodies remain private.

Estimator work is asynchronous. A pending provider or resource wait returns
control to the tick so receipt collection, Owner stops, safe drain, and
independent work continue. The request ID derives from the effect-bundle hash,
scoped basis, policy, team fingerprint, estimator contract, and relevant source
captures. It excludes global state revision and unrelated progress. Exact
retries reuse the durable job and receipt. A response is accepted only for the
same request and current relevant basis; stale results are recorded and never
adjudicated for admission. Quota, provider, Owner, and passive external waits
remain operational waits and do not become fabricated engineering effort.

`runtime_change_holds.py` checks `change_holds_for` before selection, claim,
submission, verification, semantic application, and readiness advancement.
The service repeats the hold check. Affected live jobs use existing durable
cooperative stop and safe-state machinery; delivery, termination, safe state,
and external-effect reconciliation remain separate. Independent work proceeds
when complete captured closure proves independence. An Owner pause continues
to dominate a change hold.

## CLI and backend

The CLI will expose proposal preparation, assessment proposal/status, active
policy inspection and Owner activation, exact Owner decision, pending changes,
hold status, cost diagnostics, and explicit reconciliation. Every mutation
uses the existing service route and the external trust configuration. There is
no `--as-owner` label and no global `--disable-change-economics` switch.

The backend will expose authenticated, read-only, paginated operations for:

- active and historical change policies;
- pending changes and full change detail;
- assessment categories, elapsed and agent-hour intervals, passive wait,
  uncertainty, alternatives, captures, attribution, and recommendation;
- Owner decisions and exact bound effect bundles;
- holds, affected and dependent work, drain and safe-state facts;
- admission/consumption progress, staleness, partial application, cumulative
  attributable actual cost, and remaining forecast.

Opening these views performs no estimate or mutation. Responses use the shared
wire codec, stable base/revision/cursors, read credentials, and existing SSE
tail/follow paths. Detail responses retain public reasons and provenance while
redacting private provider material. Capabilities and request schemas are
generated from the actual registries and operation descriptors.

Mutating HTTP routes reuse the existing agent-data, Owner-control, action, and
trusted-observation service methods. Internal economics events have no public
HTTP route. A reader can inspect an Owner decision but cannot submit one.

## Fixture and migration strategy

Tests must exercise the default mandatory gate. They must not globally disable
it or classify all test commands as exempt.

- Reducer and historical replay fixtures remain unchanged; replay creates no
  authorization receipt.
- Baseline fixtures explicitly build the initial charter-bound intent/outcome
  exception where that is the behavior under test.
- Semantic service fixtures create a strict full effect bundle, scoped basis,
  proposed assessment, trusted adjudication, and either a bounded automatic
  admission or exact Owner decision. Helpers return the `decision_ref` used by
  the real command; they do not patch the classifier or service gate.
- Progress, proof, source, transport, and proposal fixtures prove their exact
  exempt classifications rather than relying on a test-wide bypass.
- Runtime fixtures use a deterministic nonblocking fake estimator with durable
  submit/poll behavior. Separate cases cover pending, stale, unknown, provider
  failure, retry, and independent progress.
- Legacy stores with no economics extension replay byte-for-byte and accept a
  new semantic change only after the new gate. No migration writes synthetic
  historical assessments.
- Large-plan fixtures derive affected closure and incremental attribution from
  real typed state, then vary one relevant capture at a time to prove staleness
  without making unrelated progress stale.

The required matrix includes 3.9, 4.0, and 4.0001-hour boundaries; mandatory,
safeguard, and optional cases; false necessity; cheaper and no-op alternatives;
sunk and retained baseline exclusion; threshold-crossing and unbounded
unknowns; exact Owner choices; multi-effect order and one-shot consumption;
crash recovery; affected/dependent holds; independent work; safe drain;
Owner-pause precedence; rejection/revision/deferral; and partial-envelope
reconciliation.

## Proposed owned-file split

E owns the technical API and pure/runtime implementation:

- `zaplib/change_economics_model.py`, `change_economics.py`, and cohesive schema
  modules if needed;
- economics handlers, schemas, routes, operations, policy evaluation, typed
  product classification, scoped basis, affected closure, and read helpers;
- `control_*` policy activation and Owner decision handlers;
- `zaplib/runtime_change_holds.py` and the minimal runtime loop/scheduler hooks;
- estimator lifecycle events, nonblocking submit/poll, durable wait/retry, hold
  guards, stop/drain, restart, and release behavior;
- coordinator/provider request and response support for full effect bundles and
  `reason.decision_ref`;
- focused economics, domain-basis, control, replay, threshold, estimator, hold,
  independent-progress, restart, and safe-state tests.

B owns normative alignment of `ZAP-CHANGE-ECONOMICS.xml` with E's frozen
technical contract. B does not supply implementation from the intermediate API
draft unless root assigns a later narrow change.

F owns application integration after E's technical surface freezes and B
confirms normative alignment:

- `zaplib/service_change_economics.py` plus minimal `service.py` facade hooks;
- `zaplib/engine.py` registry/classification composition and fail-closed checks;
- estimator adapter/profile loading only if E leaves it as an injected public
  seam;
- CLI/backend query, command, schema, capability, SSE, and detail integration;
- fixture builders and service/engine/CLI/backend cross-module tests;
- public CLI/backend/Python API and ready profile/example updates.

Shared files receive only narrow imports or registry hooks from their owner.
No module should exceed 500 lines. Implementation acceptance requires B's
normative alignment check, focused E and F suites, then one combined discovery
and isolated install proof before release status changes.
