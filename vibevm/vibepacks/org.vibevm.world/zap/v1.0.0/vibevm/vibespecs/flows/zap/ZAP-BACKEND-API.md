# ZAP backend API

The package exposes an authenticated Python standard-library HTTP backend for
independent clients. It is the data/runtime backend for a future serious
interactive strategy-map canvas; it does not contain the canvas application.

The server binds `127.0.0.1:8765` by default. Any non-loopback address requires
`--allow-nonlocal`. Browser origins are exact configured strings. There is no
wildcard CORS mode. Every request authenticates before campaign data is
disclosed with:

```text
X-ZAP-Credential-ID: reader-credential
Authorization: Bearer <opaque credential>
```

Reader credentials are campaign/base bound and cannot mutate. Owner and
coordinator credentials remain independently scoped. Credential values never
enter commands, events, packets, worker argv/environment or API responses.

## Read routes

- `GET /v1/capabilities` — composed handlers, descriptors, routes and roles.
- `GET /v1/snapshot` — one committed projection and cursor.
- `GET /v1/events?after=N` — committed event tail.
- `GET /v1/stream` — one finite SSE tail batch; `Last-Event-ID` resumes at an exact revision.
- `GET /v1/follow` — live SSE subscription until the configured bounded wait;
  disconnect and reconnect use the last delivered event revision.
- `GET /v1/graph/overview` — paginated summaries, counts and semantic visual
  dimensions; `entity_kinds` selects nodes, edges, regions, jobs, decisions or
  other registered entity families (the compatibility default is `node`).
- `GET /v1/graph/subgraph?id=ID&depth=N` — bounded neighborhood and typed edges.
- `GET /v1/search?q=TEXT` — paginated search over addressable entities.
- `GET /v1/assessments?id=ID` — pending/completed trusted assessment request,
  action/policy/source bindings, transport receipt hash and observed scalar
  values; omit `id` to list requests known to the running adapter.
- `GET /v1/entities/<kind>/<id>` — complete selected-element inspector.
- `GET /v1/content/source/<id>?sha256=...` and
  `/v1/content/artifact/<handle>?sha256=...` — captured bytes by registered
  handle and exact hash.

Every result carries base identity, revision and cursor. Page cursors bind base,
revision, query/filters and offset. Foreign base, future event cursor,
changed-revision page cursor and cursor reuse for another query are explicit
conflicts or gaps.

Capabilities also carry the actual backend operation descriptors, entity-kind
registry, command authority routes and page/content limits. Each event declares
its dialect/version: knowledge payloads are exact nested JSON Schema draft
2020-12; remaining `zap-payload-descriptor/1` maps and backend
`zap-operation-descriptor/1` maps are machine-readable constraints without a
claim that they are standards-complete JSON Schema.

Every entity detail response carries explicit content, provenance, history,
staleness and unavailable-field metadata. Node detail returns the available contract/history, goal/steps,
criteria/checks, sources, evidence, obligations, typed edges, regions,
classification and work overlay. Edge and region IDs are independently
inspectable. Omission by pagination/filtering is page metadata; it is not called
unexamined. Knowledge states `unexamined`, `bounded`, `evidenced` and
`invalidated` remain semantic data. Explicit irrelevance/exclusion is separate.

The backend provides renderer-independent visual dimensions: knowledge,
work type, maturity, execution, structure, edge relation and visibility. It
supplies no authoritative color, geometry or camera. The future Heroes 3-style
bright illustrated map may use color, geometry, fog, icons, paths and in-canvas
panels without turning presentation state into campaign truth.

Content endpoints never accept filesystem paths. An imported task `read_path`
is not a read capability. Actual capture stores immutable content-addressed
blobs privately. Historical hashes remain readable when the live source
changes; live reobservation reports drift instead of displaying changed bytes
as the captured version.

Raw provider transcripts/prompts, worker packets, absolute source roots,
credentials and raw transport output are removed from public snapshots/events.
A validated coordinator response keeps a bounded public decision projection:
request binding, disposition, selected work, concise rationale, command reasons,
affected IDs and source/evidence provenance. It omits full command payloads and
marks provider material redacted, so the viewer can explain why work was
selected or applied without exposing private effect inputs. Transport output
requires a separately registered private handle and hash.

Completed action assessments also remain in the ordinary control event/history
projection. The assessment query makes a still-pending provider operation
visible without exposing its private observation path/content or transport
stdout. A process restart reconstructs an exact request when the same prepared
action is retried; no ephemeral tick message is treated as authority.

## Command routes

- `POST /v1/review-transition/materialize` — authenticated pure sparse-to-full
  review transition builder; reader credentials may use it and it appends
  nothing.
- `POST /v1/agent` — exact data-only command.
- `POST /v1/control` — authenticated owner/coordinator control.
- `POST /v1/observation` — registered trusted runtime/effect receipt during draft or pause.
- `POST /v1/action` — exact command/action/assessment through admission.
- `POST /v1/tick` — one configured coordinator iteration; the credential
  principal must exactly match the configured runtime host principal.

Unknown fields are refused. Agent JSON cannot establish owner/coordinator
identity. Reader credentials cannot use POST routes. Source capture descriptors
are refused on the generic observation route: the `capture-source` adapter must
first read guarded bytes and store the immutable blob.

HTTP request JSON uses the same strict parser as files: duplicate members,
NaN/Infinity and invalid UTF-8 refuse. The default 2 MiB request-body capacity
is an operator setting, exposed by capabilities and configurable with
`--max-body-bytes` (`0` means unlimited); it is not a campaign or plan limit.
`--max-follow-seconds` bounds one live subscription and is also reported.

## Continuity and damaged tails

Snapshot and event-tail reads use the same committed boundary. A client reading
snapshot cursor N and then events after N cannot lose a committed event. SSE
uses the same integer cursor. `/stream` deliberately closes after the current
batch; `/follow` polls committed replay until its operator-bounded deadline or
client disconnect and emits a final cursor marker on a normal close.

An incomplete final journal fragment remains visible to an authorized reader as
`pending_tail` while the last committed state stays readable. Mutation and
automatic coordination refuse until explicit owner-authenticated repair.
Newline-terminated middle corruption refuses; it never becomes a removable
tail.
