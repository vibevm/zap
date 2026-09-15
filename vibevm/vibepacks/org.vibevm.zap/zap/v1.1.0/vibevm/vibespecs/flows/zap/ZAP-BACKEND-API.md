# ZAP backend API

The Rust application service exposes a strict authenticated HTTP/1.1 machine
interface for agents and independent clients. It is a backend for a future
strategy-map canvas; no UI is included.

The server accepts a configured loopback address only. Every request authenticates
before campaign data is disclosed:

```text
X-ZAP-Credential-ID: <configured credential id>
Authorization: Bearer <opaque credential bytes>
```

Reader, Owner, coordinator, data and trusted-observation channels are distinct.
A credential ID or actor label in JSON creates no authority. Credential values
never enter events, packets, snapshots, responses or archives.

## Read routes

The following bodyless GET routes require the reader credential:

- `GET /v1/capabilities`
- `GET /v1/snapshot`
- `GET /v1/events?after=N`
- `GET /v1/stream`

`/v1/events` returns a bounded committed page. `/v1/stream` returns the current
bounded page as server-sent events and then closes; reconnect using the returned
cursor. Snapshot, event and query responses bind store identity and revision.
Foreign, stale or future cursors produce a typed conflict/gap rather than an
incomplete success.

The server also accepts strict POST requests whose complete `MachineRequest`
body must match the route:

- `POST /v1/query`
- `POST /v1/events`
- `POST /v1/stream`

Query IDs and their availability come from `/v1/capabilities`. Results carry
bounded completeness and continuation data defined by the selected registered
query. Opening a view performs no model call or semantic mutation.

## Protected and service routes

- `POST /v1/command` — authenticated Owner/coordinator command route.
- `POST /v1/control` — authenticated control command route.
- `POST /v1/agent` — configured data-channel proposal route.
- `POST /v1/observation` — configured trusted-observation route.
- `POST /v1/runtime/step` — one coordinator iteration.
- `POST /v1/runtime/run` — an explicit bounded number of iterations.
- `POST /v1/runtime/inspect` — read-only durable job/runtime inspection.
- `POST /v1/native` — pending-intent and exact prepare-launch, deterministic
  bridge restoration, atomic known-refusal/unknown/started observation,
  bounded retry release, and recovered job-observation/candidate collection.
- `POST /v1/prepare/bundle` — read-only strict effect bundle preparation.
- `POST /v1/prepare/comparison` — read-only alternatives/comparison preparation.
- `POST /v1/prepare/projected-record` — one record from the same prepared final
  overlay used by the effect kernel.
- `POST /v1/archive/publish` — trusted publication of a configured no-clobber
  portable bundle archive.
- `POST /v1/archive/verify` — read-only archive and manifest verification.
- `POST /v1/archive/entry` — read one bounded typed entry from verified archive
  bytes without the live campaign store.
- `POST /v1/reconcile` — inspect a protected command's exact durable outcome.
- `POST /v1/change/admission` — a credentialed coordinator operation whose
  configured action scope must cover every selected registered effect. It
  re-prepares one exact proposed assessment, durably adjudicates it and prepares
  its selected admission without applying the product command. The request binds
  operation, store, revision, action, source and current assessment digests,
  selected alternative, its ordered applied prefix, comparison draft and exact
  next product command. Each call prepares one next effect while retaining the
  aggregate assessment and alternative. The backend derives the registered
  action-impact digest and ServiceInternal payloads;
  those payloads remain unavailable through general routes. A required Owner
  decision returns a durable held boundary and can be resumed after the exact
  Owner decision is recorded through the separate Owner control channel. Stable internal command identities make lost-response and
  restart retries reconcile before advancing. Latest-forecast discovery is
  bounded to 4,096 forecast records in 512-record pages; exceeding that bound
  returns typed `limit_exceeded`. The caller must reduce or archive forecast
  history through a future supported maintenance policy before retrying; the
  orchestration never falls back to an unbounded scan.

`POST /v1/prepare/projected-record` can inspect one known registered current record
without applying an effect. Send `PreparationRead::Current`, an empty-prefix/empty-effect
draft with an explicit empty-root Completion basis, NotApplicable policy and capacity,
and a selector containing the record family plus canonical key bytes. The service
derives the real basis and reads the selector on one immutable snapshot. The selector
is independent of the basis roots; Completion basis work can be global and bounded, so
this is not a constant-cost whole-store read. Present data is the complete typed
canonical record value with its observed revision. Missing data is `None`, but an absent
unknown family also does not establish registration. Keys must be 1–4096 bytes. The
backend method adds no product/effect/admission write and no revision; configured HTTP
request/response caps remain separate transport limits.

Reader credentials authorize query, preparation, runtime inspection, archive
verification/entry reads and reconciliation. Mutation, runtime progression,
native launch preparation and archive publication use their configured protected
channels. The server never trusts a client-selected role.

Every native recovery target includes exact JobId/DispatchId. Spawn observations
add a stable observation CommandId, consumed authorization revision and
observation time. One registered transition persists outcome, capacity,
history, reconciliation, job/auth state and wait at one revision. Unknown never
relaunches; a known refusal retries only after the due wait, a separately
observed available slot and all current gates. Terminal state is not slot
capacity. Restoration and result collection reuse the persisted handle and
create no new launch ticket.

A protected command that finishes before the configured submission timeout
returns its committed or rejected result. If the timeout expires while work may
still be active, the response is `Unknown { command_id, command_digest }` and
the task retains the service instance. The client reconciles that identity;
absence is not reported while an in-flight submission is known.

## Consistency and limits

Bodies use closed typed JSON. Duplicate members, malformed framing, invalid
UTF-8, unsupported method/route pairs, mismatched route/request kinds, oversized
bodies and duplicate `Content-Length` refuse. GET requests accept no body.
Connection, request, response, page, runtime-step and timeout limits are explicit
configuration; a limit response does not weaken campaign semantics.

Capabilities derive from actual schema, cell, route, query, native-host and
adapter registrations. Desired profiles are separate. Unsupported operations
remain listed or refuse; a specification or packet never advertises itself as
runtime support.

Snapshot and tail share a committed boundary. Exact retry and cold reopen use
the same registered reducers and preflight rules. Endpoint and lease records are
create-new, instance-bound and removed only by the owning service or exact stale
recovery. Store corruption, pending external effects and stale business basis
remain explicit; transport restart does not decide them.

Packet material, workspaces and archives use configured filesystem roots,
relative paths, size bounds, digests and reparse-point checks. The `ZAPBNDL2`
archive contains canonical typed `ZAPENTRY2` bodies and no authority credential.
Historical export uses immutable claim-time evidence rather than silently
recapturing changed current files.

Public snapshots and responses omit credentials and raw private transport
material. The backend supplies typed state and provenance for a future viewer,
not authoritative color, geometry, camera state or acceptance inferred from
presentation.
