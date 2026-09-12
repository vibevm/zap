# ZAP-C implementation report

Control and application-service implementation completed against the explicit
foundation handler/storage seam. No active NEXT state, user-home trust
configuration, external process, network service, Git index or publication
state was touched.

## Delivered

`zaplib.control` is the stable facade for 13 pure event handlers in
`CONTROL_HANDLERS`, plus `control_state`, `active_policy`, `active_charter`,
`pause_applies`, `require_action`, `assess_action` and
`convert_legacy_stop_policy`. State is lazy under `extensions.control`; replay
does not consult credentials or effects.

Implementation is split by responsibility: `control_model.py` owns charter
state/validation, `control_policy.py` owns action validation and policy
assessment, `control_runtime.py` owns reducers,
`control_schemas.py` owns machine descriptors, and `control_trust.py` owns
opaque principals/credentials. `control.py` remains the small public facade;
`service.py` contains only application orchestration.

The active charter is bound to campaign ID, immutable base hash, complete
imported-mandate classification and exact charter revision/hash. Activation is
separate from drafting. Amendments retain prior revisions, require the exact
parent hash, advance policy revision when policy bytes change and invalidate
old assessments. A full domain charter also carries the exact immutable intent
proposal ID/hash; legacy control-only charters may preserve its omission.

The frozen delegated action classes are:

```text
outcome.adopt
adaptive.apply
task.update
evidence.adjudicate
work.accept
stage.accept
fact.promote
campaign.close
work.dispatch
verification.run
plan.lower
```

Active stop policies use deterministic three-valued expressions. Action
assessment binds the exact payload hash, sorted source captures, campaign/base,
charter and policy revisions. Nonempty captures are checked against persisted
knowledge-source state. Pause/drain targets are checked against persisted
active runtime jobs. Results distinguish clear, needs-evidence, pause and
too-late, and separately report evaluation, stop delivery and actual safe
state. Pure assessment reports eligibility and what a pause would prevent; it
never claims admission or performed runtime prevention.

Owner stop is whole-campaign and sticky. Delivery and actual-safe-state
acknowledgements are separate CAS transitions. Resume binds the exact active
pause ID and current pause hash and is one-time. One-shot exceptions bind one
pause, action ID/class, payload hash, source-capture hash and charter revision.
Run/branch pauses coexist and affect matching action scopes only; campaign
pauses dominate without deleting narrower pause history.

Approach history is append-only and counted per stable problem/current
owner-selected epoch. Only distinct strategy hashes with semantic failed
outcomes backed by usable observed evidence and a known plan-node problem
count. Retry, provider_error, inconclusive and succeeded do not count.
Legacy core approaches participate through stable problem/strategy keys;
unresolved legacy outcomes cause needs-evidence instead of a false clear.
Epoch advancement preserves all prior history.

`zaplib.service` establishes owner/coordinator/reader principals only from
trusted startup objects or opaque campaign-scoped credentials. Credentials use
`secrets.token_urlsafe` and `hmac.compare_digest`; their values never enter
commands, journal records, action descriptors or route descriptors. Reader
principals have no command/action scopes.

Agent, credentialed-control and trusted-host entry points are separate. Actor,
owner or role labels inside submitted data grant nothing. Internal admission
and reservation-rebind handlers cannot be called through public control routes.

The service is fail-closed over composed handlers. Each non-control handler
must be explicitly classified in `data_kinds`, `action_kinds` or the
disjoint engine-supplied `observation_kinds`; otherwise construction fails.
Core data records are predefined. `plan.refined` remains
available only in the no-charter legacy draft route, then maps to privileged
`plan.lower` after activation.

Trusted observation methods accept campaign-bound owner/coordinator credentials
or the configured host principal without product-action admission. They remain
usable during pause and before charter activation so an actual transport can
record started/result/stopped/unknown-effect receipts and a trusted source
adapter can record captured bytes. Agent, reader and ordinary data routes reject
these kinds. Observation does not grant dispatch, adjudication, acceptance or
promotion.

Action admission creates a durable reservation for one exact logical product
event. A crash before product append does not consume authority irrecoverably:
an unchanged request can rebind its reservation after a fresh assessment.
Changed payload/reason/event identity refuses. A new second approach failure,
source invalidation, charter/pause change or other failed current precondition
blocks rebind. The actual product handler still executes against the current
projection under the shared CAS. An already committed product event returns
idempotently. This is not an atomic external transaction; the runtime owns
durable effect claims, receipts and postcondition reconciliation.

`CONTROL_EVENT_SCHEMAS`, `SERVICE_REQUEST_SCHEMAS` and
`ApplicationService.route_descriptors()` expose machine-readable payload and
trusted-route metadata without secrets or actor claims.

## Integration requirements

The integration entrypoint must compose `CORE_HANDLERS`, `CONTROL_HANDLERS` and
the other explicit registries, then pass that same mapping to storage and the
application service. Domain/runtime/knowledge modules must also pass complete
data/action-kind classifications. Generic public `record` must route through
`ApplicationService.submit_agent`; raw `storage.record` is the trusted
append/replay cell and is not a public authorization surface.

For drain derivation, runtime jobs are read from
`extensions.runtime.jobs` (mapping or list). Active state names currently
recognized are claimed, dispatched, starting, running, stop_requested,
stopping and unknown_effect; rows use job_id/id and optional run_id/branch_id.
Runtime integration should retain or adapt to this documented shape.

For source revalidation, captures are read from
`extensions.knowledge.sources` or `source_captures`, keyed by source ID, with
sha256/content_sha256 and a current/invalidated/unavailable/unknown status.
Knowledge integration should retain or adapt to this documented shape.

The stable public shapes and all exact event payload fields are documented in
`CONTROL-API.md`. The domain worker was sent the action mapping before binding
its privileged handlers.

## Verification

Focused plus preserved legacy regression:

```text
python -B -m unittest -v test_zap_state.py test_control.py test_service.py
Ran 52 tests in 2.716s
OK
```

The 34 new tests cover draft non-authority, incomplete legacy classification,
wrong campaign/base/hash/revision/credential, false owner labels, unknown exact
actions, source invalidation, derived drain sets, sticky run/campaign pauses,
delivery versus actual safe state, too-late assessment, exact resume, one-shot
exceptions, stable approach counting/epochs, active plan-lowering protection,
fail-closed handler classification, credential secrecy, reader scope, exact
retry, crash reservation recovery, negative interleavings, paused trusted
transport collection, agent-forged receipt denial and pre-charter trusted
source capture.

No full Rust/product checks were run, as required by the packet.

The domain worker subsequently reported its two real B/C integration tests
green: proposal versus self-adoption versus credentialed adoption and sticky
pause; then unchanged-charter intent rewrite refusal followed by an
owner-amended exact intent binding and successful adaptive intent/outcome pivot.
