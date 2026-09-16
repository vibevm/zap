# Independent R11/R12 runtime review

Status: changes required before runtime capability or durability acceptance.
This is a read-only source review; no test or model/host probe was run. Missing
redb/application composition is already known. The findings below identify
additional runtime behavior that remains absent or unsafe even after a store is
connected.

## Findings

### P0 — a committed claim can dispatch after a new pause, hold, or charter change

`Coordinator::step` sends every unreceipted `DispatchPending` job to
`AgentHost::dispatch` before it reads current frontier/readiness
(`zap-runtime/src/coordinator.rs:99-119`; readiness is first read at 135-158).
`JobClaimCell` checks only the supplied job's structural state and uniqueness;
it does not rederive current work readiness, conflicts, charter, hold, pause,
contract, or relevant basis inside the committing transaction
(`zap-runtime/src/transitions.rs:89-109`). A pause or hold committed after claim
can therefore be bypassed at the actual effect boundary, and a privileged
caller with `work.dispatch` can submit a structurally valid claim that never
came from the scheduler.

Smallest fix: add a transaction-bound dispatch-eligibility provider/gate and a
registered pre-effect authorization transition. Immediately before host
dispatch, recheck the exact stored work/job/basis plus pause/hold/charter and
commit authorization; leave newly blocked intents pending for reconciliation.
The claim cell must repeat conflict/resource/contract eligibility rather than
trust `RuntimeCommandFactory`.

### P0 — native launch state is volatile and `AwaitingHarness` has no advancement path

`NativeBridge` keeps intents, receipts, uncertainty, candidates and stops only
in `Mutex<BTreeMap>` (`native_bridge.rs:16-43`). Driver submissions update that
map only (`56-82`, `85-137`). The persisted `AwaitingHarness` receipt is then a
dead end: coordinator recognizes it at `coordinator.rs:120-125` but never polls
or commits an upgraded driver receipt, eventually returning `AwaitingHarness`
at 183-186. After restart an empty bridge reports an absent intent as
`NotStarted` (`native_bridge.rs:233-253`), which is not proof that an
agent-mediated call did not occur. For any effectful `AgentHost`, the
receipt-none path at `coordinator.rs:110-115` also calls `dispatch` before
reconciliation, so idempotency is merely assumed.

Smallest fix: make the bridge an ephemeral mailbox over stored intent/receipt
state. Trusted driver ingress must commit receipt/unknown-delivery/observation
events. Coordinator must reconcile every unconfirmed intent before dispatch or
redispatch, and a registered reconciliation transition must advance
AwaitingHarness to submitted/running/terminal/unknown.

### P1 — host capability and driver provenance can be asserted without the bound evidence

`AgentCapabilities::validate` checks model duplication and zero context only;
it allows supported native execution with empty evidence and does not validate
goal scope/operations (`zap-core/src/agent/capabilities.rs:164-187`).
`NativeBridge::new` is infallible and accepts the supplied record directly
(`native_bridge.rs:36-43`); `dispatch` checks only
`native_workers == Supported`, not the intent's capability digest, adapter, or
harness against that record (`146-170`). The focused fixture itself assigns the
intent an unrelated `CapabilityDigest::hash(b"capabilities")` after building a
different capability record (`tests/runtime_contracts.rs:436-472`), and the
bridge accepts it. Public driver observation/stop methods also accept typed IDs
without a trusted-driver grant or pending-stop/effect binding
(`native_bridge.rs:85-137`).

Smallest fix: make bridge construction fallible and evidence-bearing; require
exact capability digest, observation ID, harness and adapter match at dispatch.
Replace free handle/receipt construction with intent-bound constructors, and
route driver receipt, observation, candidate and stop delivery through trusted
observation transitions that verify stored provenance and effect IDs.

### P1 — registered behavior covers only claim and initial dispatch receipt

Six record families are registered, but only `JobClaimCell` and
`DispatchReceiptCell` exist in `cell_set`/`route_set`
(`zap-runtime/src/registration.rs:20-50`). Coordinator handles unknown-effect
reconciliation, initial dispatch, one claim and a completion read only
(`coordinator.rs:99-188`). There is no official transition path for the other
reported durable behavior.

Before advertising full R11/R12 capability, register and compose transitions
and command factories for: driver receipt replacement/reconciliation; job
status and terminal observation; candidate collection into core candidate
provenance; stop request/delivery and independent safe state; verification
claim/result; retry history, wait and release; capability observation/current
cache pointer; goal request/ack/stale state; heartbeat/useful checkpoint;
malformed-response repair; and semantic request/receipt/result/wait. Each needs
an actual coordinator or trusted-ingress path. Merely registering a
`StoredRecord` does not make it durable behavior.

### P1 — message and semantic kinds are not bound to their payload types

`AgentMessage<P>` stores `kind` independently from arbitrary `P`; validation
checks lineage/profile only (`zap-runtime/src/protocol.rs:35-75`). Any payload
can therefore be labeled candidate, stop receipt, or verification result.
Likewise blanket implementations make every serde type a `SemanticInput` and
`SemanticOutputContract`, while `SemanticIntent::build` accepts an unrelated
caller-supplied `SemanticKind` (`zap-core/src/agent/semantic.rs:25-44,70-100`).

Smallest fix: use a closed payload enum or sealed payload traits with associated
`KIND`; builders derive the kind and per-kind output policy. Keep semantic
capabilities absent until R08 supplies the closed selection/review/acceptance/
closure contracts.

### P1 — reconciled unknown effects cannot release the recorded retry condition

`RetryHistory::record` allows a blocking execution state only with
`ReconciledNotRunning` (`zap-runtime/src/retry.rs:84-92`), but `retry_due` then
tests that same immutable execution state and returns false while it remains
blocking (`112-123`). A changed outcome under the same attempt ID is rejected
as conflicting (`93-102`).

Smallest fix: bind `ReconciledNotRunning` to a later reconciliation
observation/digest and let a registered retry-release transition evaluate that
current record; do not infer reconciliation from the immutable attempt outcome.

### P2 — goal and cache helpers are projections, not yet honest durable control

`plan_goal_application` receives one operation support row and never checks the
goal's scope (`zap-core/src/agent/goals.rs:128-147`), so `GoalScope::None` can
still yield `Invoke`. Generated campaign and assignment text omits the supplied
stop conditions and completion evidence; assignment text also omits its safe
boundary and acceptance references (`zap-runtime/src/goals.rs:23-54,57-85`).
`CapabilityCache` is another local `BTreeMap`, and a contradictory observation
immediately replaces the current value (`capability_cache.rs:48-89`).

Smallest fix: validate `GoalCapability` as a whole and plan with required scope;
render bounded stop/completion/safe references in GOAL content. Persist goal and
capability changes through registered transitions; contradiction should become
unknown/pending adjudication rather than silently current.

### P2 — “nonblocking coordinator” is an unenforced adapter promise

Every `AgentHost` operation is synchronous (`zap-core/src/agent/host.rs:16-30`)
and coordinator calls `reconcile`, `dispatch`, `capabilities`, and several read
ports directly in one step (`coordinator.rs:83-174`). No type or scheduler
boundary prevents an adapter from waiting on a remote host and freezing peer
collection.

Smallest fix: require host methods to be bounded local submit/poll operations,
or run them on dedicated effect tasks and let coordinator consume durable
receipts. Keep database transactions outside that work.

## Evidence limit

The reported scoped test exits are not disputed, but the inspected test file
does not instantiate `Coordinator`, a store, `CommandPort`, or trusted driver
route. Its restart claim is canonical encode/decode of `RetryHistoryRecord`,
and its native/capability/cache cases operate on fresh in-memory objects. Those
tests support DTO and local-algorithm behavior only; they do not yet establish
durable native execution, pause/hold enforcement, or restart reconciliation.

## Follow-up evidence boundary

The repair now includes actual Coordinator, CommitService, registered runtime
cells and redb scenarios. Root observed that the first scenarios recreate an
empty native mailbox while retaining the open database/service; this proves
mailbox-loss behavior, not a cold database reopen. Add one coherent drop of all
database/service handles, reopen, reconstruct the driver, and continue from
persisted unknown-effect and collected lifecycle state. The test eligibility
provider currently uses an AtomicBool; actual persisted Owner pause and
economics holds remain required R07/R16 composition evidence.
