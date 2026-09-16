# R17 runtime scale — first-page starvation removal

Status: candidate complete; source released for root review

## Result

The runtime scheduler, completion provider, native-driver wait check and
application runtime frontier no longer infer absence from a bounded record
family prefix. The application runtime-start guard also no longer scans a small
prefix of historical pauses and holds before entering the coordinator.

Four derived index families implement the bounded read paths:

| Family | Contributor | Current contents |
| --- | --- | --- |
| `zap.runtime.work-ready.v1` | domain `WorkRecord` | exactly Ready work, ordered by fixed-width lowercase u32 hex order and then WorkId |
| `zap.runtime.relevant-job.v1` | runtime `RuntimeJobRecord` | nonterminal jobs plus terminal jobs whose effect is Started or Unknown |
| `zap.runtime.pending-authorization.v1` | runtime `PreEffectAuthorizationRecord` | Consumed or UnknownEffect authorization, with exact JobId and DispatchId |
| `zap.runtime.wait-by-job.v1` | runtime `RuntimeWaitRecord` | waits partitioned by exact JobId and ordered by WaitId |

Each family has an explicit algorithm fingerprint. Work/runtime record callbacks
add and remove rows with their record mutation, and every runtime cell descriptor
derives the union of runtime-owned and core active-observation families from its
affected record families. Fresh application creation and explicit rebuild now
compose domain, core-observation and runtime families/algorithms.

The existing domain viewer index meanings are unchanged. `WorkRecord` callback
logic moved to `viewer_indexes/runtime.rs` to keep the parent below the file
limit. The three previously orphaned viewer catalog/scope helpers now carry the
existing mandatory-index contract edge. No event, authority, public DTO or
query epoch changed.

## Runtime read semantics

Coordinator job discovery reads complete index pages under one 65,536-unit
aggregate budget, charging every scan attempt as well as returned/cursor rows
and exact record reads. A noncomplete empty page, repeated continuation,
missing/stale catalog, mismatched algorithm or conflicting record/index row
refuses without a record scan fallback.

Relevant-job and pending-authorization rows are unioned by JobId. Every pending
row independently resolves its DispatchId, rejects duplicate dispatch bindings
for one job, and then checks the exact current job/authorization relationship.
The coordinator retains the initial StoreIdentity and Revision and uses
`ReadAt::Revision` for frontier, work, readiness, packet and completion reads.
Any identity/revision drift refuses, including an otherwise empty outcome.

Terminal execution with Started or Unknown effect retains active occupancy.
Completion reports terminal Started as `LiveJob`; it does not relabel that known
state as unknown. Unknown effect remains `UnknownExternalEffect`.

Ready-work discovery uses a separate runtime frontier cursor. The public
`PageCursor` binds store/base/revision/query epoch, the named runtime query,
normalized Ready/order semantics and the exact embedded `IndexCursor`.
Continuation is ordered by numeric `(Work.order, WorkId)`. Wrong-query,
foreign-store, stale-revision and incompatible-catalog cursors refuse.
`UnknownBoundary` refuses before candidate selection.

Packet availability is resolved before tentative scheduling capacity is
allocated. A packetless early Ready item therefore cannot consume a shared
resource/host slot and starve a later packet-backed item on the same page.
Packetless items are remembered while further pages are consumed; `PacketWait`
is returned only after bounded complete discovery finds no runnable packet.
Readiness/claim refusals are accumulated across pages, and a selected item from
an earlier ordered page still wins over later pages.

The native driver reads only the requested job's wait partition. Runtime
completion reads the complete relevant-job index. Neither path scans its record
family.

## Runtime-start guard

`ApplicationService::runtime_step` now calls the domain read-only
`ensure_runtime_start_unblocked` helper. It reads the existing active-pause
partition for the current Campaign first, then the existing global nonreleased
hold partition. Pause precedence remains intact. Resumed pauses and Released
holds contribute no active guard row. Every other hold status is retained, and
a nonreleased hold refuses starts when `hold_all_starts` is true or it carries
unknown effect identities.

## Focused evidence

- The generated application frontier fixture contains 4,101 unrelated Accepted
  Work records and three later Ready records. With page limit 1 it returns
  order 2/WorkId A, order 2/WorkId B, then order 10, with two real continuations,
  three exact Work reads and zero record-family scans. Wrong-query, foreign,
  stale and missing-catalog cases refuse.
- The real redb runtime journey inserts 4,101 settled RuntimeJob records in 11
  deterministic batches ahead of `job-real-1`. The actual coordinator reaches
  and records that later job's dispatch receipt while its snapshot decorator
  rejects any record-family scan; observed scan count is zero.
- The same actual `Coordinator::step` fixture first presents two items on one
  page with capacity 1: the first has no packet and the second does. A deliberate
  prepare-probe refusal proves the second is reached without committing. It then
  uses page limit 1 over two pages and claims the packet-backed second item.
- Unit cases reject revision drift between frontier pages and reject
  `UnknownBoundary` before selection. A near-limit budget case proves every
  additional scan attempt crosses the finite bound.
- Real-store recovery asserts relevant and pending-authorization rows before
  and after reopen, including terminal Unknown recovery. The native retry
  journey asserts wait-row insertion, `Busy` driver refusal and atomic row
  removal on release.
- Runtime completion refuses a missing catalog with `UnsupportedEpoch`, and a
  stale revision or mismatched relevant-job algorithm with `Unavailable`.
- With application runtime page limit 1, the real `runtime_step` path advances
  after two historical Resumed pause records and after two historical Released
  hold records. An active campaign pause returns `Paused`; a Deferred global
  hold with `hold_all_starts` returns `Held`.
- Runtime unit tests cover terminal Started/Unknown occupancy and the distinct
  Started live-job versus Unknown-effect completion classification.

The 4,101-record shapes are deterministic generated fixtures, not model output
or a new benchmark. No latency claim is made.

## Retained regressions and gates

- `cargo test -p zap-runtime --test runtime_persistence`: 5/5 passed, including
  the 4,101-row/reopen recovery and native retry journeys (38.46 seconds final
  run).
- `cargo test -p zap-runtime --lib`: 6/6 passed.
- Focused app ready-frontier unit: 1/1 passed; focused domain numeric suffix
  unit: 1/1 passed.
- `cargo test -p zap-app --test application_server`: 4/4 passed.
- Exact packet-resolution service journey: 1/1 passed.
- Exact deterministic two-job nonempty completion journey: 1/1 passed.
- Strict `cargo clippy` over all targets of `zap-domain`, `zap-runtime` and
  `zap-app` passed with `--no-deps -- -D warnings`.
- Exact formatting and format check passed; no affected source exceeds 600
  lines. The enlarged runtime recovery file was split through the focused
  `runtime_scale_support.rs` helper and is 595 lines.
- Scoped conform checks for domain, runtime and app each report zero findings.
- `rust-ai-native specmap --gate --path .` reports zero gated orphans.
- Final health reports all nine crates public-type-gated, zero baseline debt and
  no file over 600.

The retained deterministic completion fixture previously wrote its native
probe phase marker into the shared system temporary parent even in deterministic
mode, producing repeatable Windows `AlreadyExists` failures. The marker is now
created only when `ZAP_R16_NATIVE_PROBE_DIR` selects the durable native-probe
flow. The deterministic assertions and native-probe create-new behavior remain.

The runtime, app and core guide sections describing coordinator discovery,
application runtime composition and work-readiness cursor handling were updated
as `guide r2` units. Root owns the final coherent specmap regeneration after all
source/guide writers freeze; this worker ran the read-only zero-orphan gate and
did not overwrite `specmap.json`.

## Final-validation legacy import repair

Root's first final package run exposed one causal regression in
`legacy_activation`: the production importer registered and committed indexed
projection rows into a new destination without first creating its derived index
catalog. The subsequent real prepared lowering correctly refused that store as
`UnsupportedEpoch`; the activation fixture was not stale.

Import staging now reuses the exact crate-private application package family and
algorithm composition and runs `rebuild_indexes_v2` at the store's current head
before the import commit. A new store therefore receives its Genesis catalog,
while interrupted staging recovery reconstructs derived rows at its exact
current revision. Ordinary opens still refuse missing catalogs, and no canonical
event, imported identity, receipt field or source byte changes. The exact
inactive revision-zero import-to-activation test passes 1/1, all nine legacy
import publication/retry/staging recovery tests pass, strict zap-app all-target
clippy passes, formatting is clean and scoped zap-app conform reports zero.
