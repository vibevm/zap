# ZAP Rust integration contract

Status: R01 accepted integration contract, foundation API revision 9. This
document fixes the shared Rust seams for R03 and the independently owned
implementation tracks. Later implementation may not change the foundation
types or dependency direction below without an explicit API revision recorded
in the campaign checkpoint and coordinated with active consumers.

The accepted requirements are the complete V01-V24 vision and the normative
ZAP flow specifications. Existing Python APIs are compatibility evidence. They
do not determine the Rust module layout or grant implementation status.

## 1. Package workspace and dependency direction

The code-bearing package root is the directory containing `vibe.toml`. It owns
one standalone Cargo workspace:

```text
Cargo.toml
crates/
  zap-wire/       stable foundation wire values and envelopes
  zap-core/       pure kernel, registries, errors, ports and query algebra
  zap-domain/     intent, control, knowledge, economics and planning cells
  zap-store/      transactional zap/2 store, indexes and artifact publication
  zap-runtime/    scheduler, semantic provider and AgentHost orchestration
  zap-api/        typed CLI/HTTP/SSE adapters over application ports
  zap-legacy/     read-only zap/1 codec, validation and migration
  zap-app/        the only production composition root
  zap-cli/        thin binary entrypoint
```

The acyclic crate graph is:

```text
zap-wire
   ^
   +-- zap-core
          ^
          +-- zap-domain
          +-- zap-store
          +-- zap-runtime
          +-- zap-api
                 ^
zap-legacy ------+----> zap-store + zap-domain

zap-app --> zap-domain + zap-store + zap-runtime + zap-api + zap-legacy
zap-cli --> zap-app
```

`zap-domain`, `zap-store`, `zap-runtime`, and `zap-api` are siblings. They may
depend on `zap-core` and `zap-wire`, never on each other. Cross-subsystem use is
through the ports in `zap-core`. `zap-legacy` is deliberately downstream: no
domain, store, runtime, or API crate depends on legacy code. Only the explicit
legacy-import composition in `zap-app`/`zap-cli` links it; normal campaign open,
mutation and query paths cannot call it. `zap-app` supplies the concrete
adapters among sibling ports.

The root manifest uses resolver 3, package version 1.0.0, edition 2024 and
`rust-version = "1.93"`. The workspace excludes `vibevm/vibedeps` and
`vibevm/vibepacks`. Every production crate denies unsafe code unless a later
recorded architecture decision names the exact boundary and proof.

The Rust discipline is a real package dependency. P declares and materializes
`org.vibevm.ai-native/rust-ai-native-lang/1.0.0` into P's own dependency tree.
The workspace dependency is the materialized package path:

```toml
specmark = { package = "core-ai-native-specmark", path = "vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/crates/vendor/core-ai-native-specmark" }
```

This relative path is inside the independently installable ZAP package. A path
to the host repository's `crates/`, another checkout, or a locally invented
no-op macro is invalid. Each crate declares `specmark.workspace = true`, puts a
`specmark::scope!` at each module cell, and uses `#[spec(...)]` on public seams.

R03 creates only the workspace, the foundation types in sections 2-7, empty
but compiling crate entrypoints, and one explicit composition root. It does not
invent semantic payloads or claim later features. A compiled stub is a build
boundary, never a capability.

## 2. File ownership and composition

The three implementation tracks have disjoint ownership:

| Track | Owned production surface |
| --- | --- |
| foundation/store/migration | root Cargo files; `zap-wire`; foundation cells in `zap-core`; `zap-store`; `zap-legacy` |
| semantic planning/control | `zap-domain` except its integrator-owned crate entrypoint |
| runtime/surfaces | `zap-runtime`, `zap-api`, `zap-cli` except integrator-owned entrypoints |

`zap-app/src/composition.rs`, every crate `lib.rs`, and the workspace dependency
table have one integrator owner. A worker adds or changes a cell in an owned
module and requests a composition/API revision instead of editing another
track's entrypoint. `lib.rs` files contain declarations and re-exports only.

Initial module cells are intentionally small and responsibility-based:

| Crate | Cell modules |
| --- | --- |
| `zap-wire` | `ids`, `digests`, `bounded`, `errors`, `command`, `event`, `page` |
| `zap-core` | `records`, `state_reader`, `change_set`, `cells`, `queries`, `routes`, `basis`, `ports`, `commit_service` |
| `zap-domain` | `seams/*`, `intent`, `outcome`, `obligations`, `control`, `stops`, `knowledge`, `evidence`, `economics`, `lowering`, `weak_bundle`, `dreamer`, `completion`, `registration` |
| `zap-store` | `seams/*`, `catalog`, `redb_read`, `redb_write`, `commit`, `indexes`, `artifacts`, `open`, `audit`, `recovery`, `export` |
| `zap-runtime` | `seams/*`, `profiles`, `packets`, `claims`, `scheduler`, `semantic`, `agent_host`, `native_bridge`, `liveness`, `stops`, `reconcile`, `resume`, `goals`, `registration` |
| `zap-api` | `capabilities`, `commands`, `queries`, `http`, `sse`, `sanitize` |
| `zap-legacy` | `codec`, `journal`, `snapshot`, `mapping`, `import` |

Each module is one default cell and imports `crate::seams` plus public core,
not a sibling's private module. `seams/*` is split by concept rather than one
shared god-file; `registration` is the crate's single cell-set composition
point. Promote a cohesive multi-file cell only when its manifest names
the complete file set and one registration point. Storage catalog definitions
and application composition each have exactly one owner.

Each feature crate exports one `cell_set()` and, when applicable, one
`query_set()` and `capability_set()`. The application composition root merges
whole sets. Adding an event inside an existing feature crate changes only its
local registration cell. Adding a new feature crate changes the application
composition once. Duplicate event, query, action or capability identities are
construction errors. There is no filesystem scan, link-time inventory, dynamic
module loading, or state-selected executable code.

## 3. Stable identifiers and digests

No public seam passes interchangeable IDs as `String`. `zap-wire` defines
distinct newtypes, generated from one checked declaration table:

```rust
CampaignId, StoreId, BaseId, CommandId, EventId, TransactionId, ChangeId,
IntentId, OutcomeId, ObligationId, WorkId, ContractId, ReviewId,
DecisionId, SourceId, EvidenceId, DeferralId, LoweringId,
StrategicRevisionId, DreamId, PacketId, JobId, AttemptId, VerificationId,
HoldId, PauseId, HarnessId, CapabilityObservationId, GoalId, CharterId,
PolicyId, ControllerId, CredentialId, SemanticRequestId, EffectId,
BundleId, EncounterId, CandidateId, ChangeAssessmentId, StopRuleId,
DispatchId, ForkId, RiskId, ConditionId, ResourceId, WaitId,
QueryId, AssumptionId, StageAcceptanceId, IntegrationAcceptanceId,
WorkAcceptanceId, ClosureId, PromotionId, FactId, PrincipalId, OperationId,
AdmissionId, CompletionProviderId, AuthorizationRef, ObservationRef, MessageId
```

Each ID has only these public operations. `ZapError` is owned by `zap-wire`, so
these constructors do not create an upward dependency on `zap-core`:

```rust
pub fn parse(value: &str) -> Result<Self, ZapError>;
pub fn as_str(&self) -> &str;
```

For zap/2, an ID is 1..=1024 UTF-8 bytes and matches
`[A-Za-z0-9][A-Za-z0-9._:-]*`. It is never trimmed, case-folded, translated, or
reused for another logical subject of the same type. The legacy reader retains
an original identity as `LegacyId` even when it cannot become a zap/2 ID;
migration then records an explicit deterministic mapping instead of altering
the legacy bytes.

`AuthorizationRef` and `ObservationRef` identify distinct public stored
provenance records. They follow the same checked ID grammar but are not aliases
for evidence IDs, credentials, secrets or authority grants. Parsing either
reference grants no authority; admission checks that the referenced record
exists and has the required campaign scope.

`Revision` and `Sequence` are checked `u64` newtypes. Genesis is the single
event at sequence/revision zero; every later committed logical event advances
both by exactly one. Overflow is a typed refusal. `ProtocolEpoch`, `StoreEpoch`,
`CodecEpoch`, `ReducerEpoch`, and `QueryEpoch` are distinct positive `u32`
newtypes. `StoreEpoch::ZAP2` has numeric value 2 and wire name `zap/2`;
`LegacyEpoch::Zap1` is a separate type and never passes a zap/2 constructor.
The first Rust reducer/query implementations use their own epoch value 1 under
store epoch 2; changing reducer or query meaning advances the corresponding
epoch without rewriting old events.

`Digest32` stores exactly 32 bytes and has lower-case 64-hex wire form.
Domain-specific wrappers prevent digest substitution:

```rust
BaseDigest, CommandDigest, EventDigest, PayloadDigest, SourceDigest,
ArtifactDigest, RelevantBasisDigest, ProjectionDigest, ReducerDigest,
PacketDigest, CapabilityDigest, ContractDigest, LoweringRequestDigest,
BundleDigest, ReturnBundleDigest, GrillDigest, DispatchIntentDigest,
ResumeDigest, GoalDigest, SemanticRequestDigest
```

Constructors either hash canonical bytes in the owning domain or validate an
existing 32-byte value. A digest proves byte identity within its named domain;
it does not prove authority, truth, applicability, lineage, or semantic
equivalence.

`SubjectRef` is a closed enum with one variant for campaign, intent, outcome,
obligation, work, contract, source, evidence, decision, review, deferral,
lowering, dream, job, verification, hold, pause, effect and resource, each
carrying its corresponding ID newtype. It is never a pair of caller strings.
Adding a genuinely new subject family is a `zap-wire` API/codec revision and one
composition change; it does not require edits to unrelated feature crates, but
the compiler intentionally identifies consumers whose exhaustive logic must
understand the new subject.

## 4. Canonical ingress and command envelope

JSON, HTTP and CLI decoding ends at `zap-wire`. Domain cells never receive
`serde_json::Value`, unvalidated maps, or caller-selected Rust type names.
Object inputs reject duplicate members and unknown fields. Non-finite numbers
are not zap/2 numbers; quantities with contractual precision use integer units
or a validated decimal-string newtype. Canonical encoding and its evolution
belong to a named `CodecEpoch`.

The stable in-memory command form is generic over a typed payload:

```rust
pub struct CommandEnvelope<P> {
    pub header: CommandHeader,
    pub reason: CommandReason,
    pub payload: P,
}

pub struct CommandHeader {
    pub protocol: ProtocolEpoch,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub expected_revision: Revision,
    pub kind: EventKind,
    pub causes: Vec<EventId>,
    pub basis: BasisBinding,
}

pub enum BasisBinding {
    NotApplicable,
    Exact(RelevantBasisDigest),
}

pub struct CommandReason {
    pub summary: BoundedText<4096>,
    pub evidence: Vec<EvidenceId>,
    pub decision: Option<DecisionId>,
    pub change: Option<ChangeId>,
}

pub enum OperationRef {
    Command(CommandId),
    Attempt(AttemptId),
    SemanticRequest(SemanticRequestId),
}

pub struct ActorRef {
    pub principal_id: PrincipalId,
    pub operation: OperationRef,
    pub role: PrincipalRole,
}

pub enum AdmittedAuthority {
    AgentData { actor: ActorRef },
    OwnerControl { actor: ActorRef, class: ControlClass, reference: AuthorizationRef },
    Privileged { actor: ActorRef, action: ActionClass, admission: AdmissionId },
    TrustedObservation { actor: ActorRef, source: ObservationRef },
    ServiceInternal { operation: OperationId },
}

impl AdmittedAuthority {
    pub fn actor(&self) -> Option<&ActorRef>;
}

pub struct ValidatedCommand<P> {
    /* private: typed envelope + service-created admission context */
}

impl<P: CommandPayload> ValidatedCommand<P> {
    pub fn header(&self) -> &CommandHeader;
    pub fn reason(&self) -> &CommandReason;
    pub fn payload(&self) -> &P;
    pub fn authority(&self) -> &AdmittedAuthority;
    pub fn completion(&self) -> Option<&CompletionView>;
}
```

`causes` and reason references are sorted, unique, and must exist when the
selected transition requires them. `kind` must equal the registered payload
cell's kind. The envelope contains no actor, role, credential, owner flag, or
authorization assertion. Exact authority arrives as a service-side
`AuthenticatedPrincipal` or `TrustedHostGrant` and is never serialized into a
worker command.

`ValidatedCommand` is constructed only by `CommitService` after route and
principal admission. Its `AdmittedAuthority` contains a public, nonsecret actor
and operation identity suitable for producer/acceptor checks and for the
event's `authority_basis`; it contains no credential. A completion view is
present only when the registered cell descriptor requires the shared
completion gate. Caller JSON cannot provide either field.

The public JSON frame carries `kind` and canonical payload bytes. The registry
selects exactly one cell by `EventKind`, strictly decodes its concrete payload,
and produces `CommandEnvelope<P>`. Type erasure exists only in the registry
adapter after typed decoding; no reducer inspects generic JSON.

An exact retry uses the same `CommandId`, `EventId`, canonical command bytes and
`CommandDigest`. The store returns the recorded `CommitReceipt`. Reusing either
ID with different bytes is `IdempotencyConflict`. A new command always checks
the current revision and relevant basis inside the write transaction.

## 5. Routes, transition cells and pure application

`EventKind` and `ActionClass` are validated newtypes with registered constants;
they are not open caller strings. Existing action spellings remain:
`outcome.adopt`, `adaptive.apply`, `task.update`, `evidence.adjudicate`,
`work.accept`, `stage.accept`, `fact.promote`, `campaign.close`,
`work.dispatch`, `verification.run`, and `plan.lower`.

```rust
pub enum RouteClass {
    DataProposal,
    OwnerControl(ControlClass),
    Privileged(ActionClass),
    TrustedObservation,
    ServiceInternal,
}

pub enum ControlClass {
    CharterActivate,
    CharterAmend,
    CampaignStop,
    PauseResume,
    ActionExceptionGrant,
    ApproachEpochAdvance,
    ChangePolicyActivate,
    ChangeDecisionRecord,
}

pub trait CommandPayload: CanonicalEncode + CanonicalDecode + Send + Sync + 'static {
    const KIND: &'static str;
}

pub trait TransitionCell: Send + Sync + 'static {
    type Payload: CommandPayload;
    type Output: CanonicalEncode + Send + Sync + 'static;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError>;
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError>;
}
```

The heterogeneous registry uses one object-safe adapter, implemented once for
every concrete `TransitionCell`:

```rust
pub trait ErasedTransitionCell: Send + Sync {
    fn descriptor(&self) -> &CellDescriptor;
    fn decode_validate_apply(
        &self,
        state: &dyn StateReader,
        header: &ValidatedHeader,
        reason: &CommandReason,
        payload: &CanonicalPayload,
        changes: &mut ChangeSet,
    ) -> Result<CanonicalOutput, ZapError>;
}

pub trait ErasedQuery: Send + Sync {
    fn descriptor(&self) -> &QueryDescriptor;
    fn decode_execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &CanonicalPayload,
    ) -> Result<CanonicalOutput, ZapError>;
}

pub struct CellSet { /* EventKind -> Arc<dyn ErasedTransitionCell> */ }
pub struct QuerySet { /* QueryId -> Arc<dyn ErasedQuery> */ }
pub struct RouteRegistry { /* EventKind -> RouteClass */ }

impl CellSet {
    pub fn empty() -> Self;
    pub fn register<C: TransitionCell>(&mut self, cell: C) -> Result<(), ZapError>;
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError>;
}
impl QuerySet {
    pub fn empty() -> Self;
    pub fn register<Q: QuerySpec>(&mut self, query: Q) -> Result<(), ZapError>;
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError>;
}
impl RouteRegistry {
    pub fn empty() -> Self;
    pub fn register(&mut self, kind: EventKind, route: RouteClass)
        -> Result<(), ZapError>;
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError>;
}
```

`CanonicalPayload` and `CanonicalOutput` are opaque validated byte wrappers.
Only `zap-wire` can construct them. The blanket erased adapter strictly decodes
`CanonicalPayload` as the concrete `C::Payload` before it calls
`TransitionCell::apply`, and encodes `C::Output` after return. A reducer never
receives either wrapper and cannot downcast to JSON. The query adapter performs
the same strict decode for `Q::Input` before invoking a concrete `QuerySpec`.

`CellSet::compose`, `QuerySet::compose`, and `RouteRegistry::compose` accept
empty iterators for the R03 foundation. They reject duplicates and malformed
descriptors. `CommitService` additionally requires exact equality among cell
kinds and route keys. An empty composition is usable for reads and capability
discovery; every command/query returns `UnsupportedOperation`. A production
profile separately validates its explicit `RequiredCapabilities` before it can
advertise readiness.

`TransitionCell::apply` is pure: no filesystem, network, model, process,
credential, ambient configuration, current clock, random generator, or sleep.
Time, randomness and observations enter as validated payload fields captured at
an effect boundary. `StateReader` is the transaction's immutable pre-state.
`ChangeSet` is an in-memory typed mutation set; it cannot commit itself.
Failure discards it. The store validates referential/index invariants and
commits the logical event, idempotency row, projection changes and mandatory
indexes together.

The object-safe registry adapter owns strict payload decoding and invokes a
concrete `TransitionCell`. `CellDescriptor` fixes event kind, route, payload
schema/epoch, reducer epoch, affected index families and normative requirement
references. Capabilities are generated from the accepted registry; a stub or
unregistered type is absent.

Fallible cell and record descriptor constructors are evaluated once while
building their registries. A construction failure prevents publication of the
set; later reads use the stored checked descriptor and do not repeat validation.

## 6. Structured errors

`zap-wire` owns the shared public `ZapError`, its closed machine codes and the
serialized error envelope; `zap-core` re-exports them. Every fallible public
wire constructor and cross-crate port returns `Result<T, ZapError>`, so
foundation types never depend upward. Individual implementation crates may use
a private layer error enum and `From` conversion at their public boundary. The
binary edge may add presentation context; domain crates do not use `anyhow`.

```rust
pub struct ZapError {
    pub code: ErrorCode,
    pub requirement: RequirementRef,
    pub message: BoundedText<4096>,
    pub fix: FixSurface,
    pub detail: ErrorDetail,
}

pub enum ErrorCode {
    InvalidIdentity, InvalidValue, InvalidFields, UnsupportedEpoch,
    UnsupportedOperation, DuplicateIdentity, MissingReference, Cycle,
    StaleRevision, StaleBasis, IdempotencyConflict, Unauthorized,
    Paused, Held, NeedsEvidence, Conflict, Busy, PendingEffect,
    UnknownEffect, CorruptStore, LegacyIncompatible, LimitExceeded,
    Unavailable, InternalInvariant,
}

pub enum FixSurface {
    Command, Payload, SourceCapture, Authority, Policy, Store,
    Adapter, Configuration, Migration, RetryAfterReconcile,
}
```

`ErrorDetail` is a typed enum for stale values, conflicting IDs, missing
subjects, unsupported epochs, violated limits, active pauses/holds and pending
effects. It never contains credentials, raw provider output, private packet
content, absolute protected roots, or arbitrary debug dumps. Display text uses
the discipline's REQ-citing grammar; callers branch on `ErrorCode` and detail,
not message text.

## 7. Store and query ports frozen for R03

`StoredRecord` is an open compile-time trait so sibling feature crates can
implement it. Implementing it alone grants no storage access. The application
must register its descriptor in a duplicate-free `RecordSet`; every read and
mutation checks that the Rust `TypeId`, family, key codec, value codec and
version codec match that registration.

```rust
pub trait StoredRecord:
    CanonicalEncode + CanonicalDecode + Clone + Send + Sync + 'static
{
    type Key: RecordKey;
    type Version: VersionStamp;
    const FAMILY: &'static str;

    fn key(&self) -> Self::Key;
    fn version(&self) -> Self::Version;
    fn descriptor() -> Result<RecordDescriptor, ZapError>;
}

pub struct RecordSet { /* RecordFamily -> checked type-erased codec */ }
impl RecordSet {
    pub fn empty() -> Self;
    pub fn register<R: StoredRecord>(&mut self) -> Result<(), ZapError>;
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError>;
}
pub trait ErasedRecord: Any + Send + Sync {
    fn descriptor(&self) -> &RecordDescriptor;
    fn as_any(&self) -> &dyn Any;
}
```

`RecordKey` has one canonical binary encoding and `VersionStamp` has an exact
equality operation. `RecordDescriptor` uses a validated `RecordFamily`, never
a raw database table name. The registered codec constructs
`Arc<dyn ErasedRecord>` only after decoding and validating a concrete
`StoredRecord`.

`RecordFamily` is a 1..=128-byte lower-case ASCII identifier with dot/hyphen
segments. `EncodedRecordKey` is an opaque owned byte string created only by a
registered `RecordKey` codec and capped at 4096 bytes. `EncodedKeyRange` uses
typed inclusive/exclusive/unbounded endpoints. `ErasedRecordPage` is exactly
`{items: Vec<Arc<dyn ErasedRecord>>, completeness: Completeness,
last_key: Option<EncodedRecordKey>}`. `PageLimit` is a nonzero `u32` capped by
the service profile. These values expose no redb table guard or raw value bytes.

There are two deliberately different read layers. The monomorphized storage
layer is not dyn-compatible and is implemented by a concrete backend:

```rust
pub trait SnapshotRead {
    fn identity(&self) -> StoreIdentity;
    fn revision(&self) -> Revision;
    fn get<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError>;
    fn scan<R: StoredRecord>(&self, range: KeyRange<R::Key>, limit: PageLimit)
        -> Result<Page<R>, ZapError>;
}

pub trait AtomicWrite: SnapshotRead {
    fn binding(&self) -> TransactionBinding;
    fn apply_commit(
        &mut self,
        intent: &ValidatedCommitIntent,
    ) -> Result<CommitReceipt, ZapError>;
}

pub trait TransactionStore: Send + Sync {
    type Read<'a>: SnapshotRead where Self: 'a;
    type Write<'a>: AtomicWrite where Self: 'a;

    fn read(&self, at: ReadAt) -> Result<Self::Read<'_>, ZapError>;
    fn transact<T>(
        &self,
        permit: &TransactionPermit,
        operation: impl FnOnce(&mut Self::Write<'_>) -> Result<T, ZapError>,
    ) -> Result<T, ZapError>;
}

pub trait StateReader: Send + Sync {
    fn identity(&self) -> StoreIdentity;
    fn revision(&self) -> Revision;
    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError>;
    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError>;
}

pub trait QuerySnapshot: StateReader {
    fn query_epoch(&self) -> QueryEpoch;
    fn limits(&self) -> QueryLimits;
}

pub trait StateReaderExt: StateReader {
    fn get_typed<R: StoredRecord>(&self, key: &R::Key)
        -> Result<Option<R>, ZapError>;
    fn scan_typed<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<RecordPage<R>, ZapError>;
}
```

`SnapshotRead`/`AtomicWrite`/`TransactionStore` are intentionally generic and
must never be used as `dyn` traits. `zap-store` adapts its concrete read
transaction to the object-safe `StateReader`/`QuerySnapshot`. Both traits are
`Send + Sync`; they return owned identity values and owned/`Arc` record pages,
so no table guard or backend borrow crosses the erased boundary. redb 4.2.0's
read transaction is documented `Send + Sync`; the in-memory R03 adapter must
meet the same contract.

A blanket `impl<T: StateReader + ?Sized> StateReaderExt for T` supplies
`get_typed<R>` and `scan_typed<R>` by encoding `R::Key`, checking the registered
descriptor, using `ErasedRecord::as_any().downcast_ref::<R>()`, and cloning the
validated record. A missing/wrong `TypeId` is `InternalInvariant`, never an
unchecked cast. Reducers use this typed extension and never see stored bytes.
`RecordPage<R>` contains only owned records, completeness and last encoded key;
it has no query epoch. A public `QuerySpec` wraps that internal scan in `Page<R>`
using its `QuerySnapshot::query_epoch`, so reducers do not manufacture public
query metadata.

`ChangeSet` is a concrete, noncommitting mutation buffer:

```rust
pub struct ChangeSet { /* typed record mutations and invariant assertions */ }

impl ChangeSet {
    pub fn insert<R: StoredRecord>(&mut self, value: R) -> Result<(), ZapError>;
    pub fn replace<R: StoredRecord>(
        &mut self,
        expected: R::Version,
        value: R,
    ) -> Result<(), ZapError>;
    pub fn remove<R: StoredRecord>(
        &mut self,
        key: R::Key,
        expected: R::Version,
    ) -> Result<(), ZapError>;
}
```

The methods retain concrete records behind `ErasedRecord`; encoding happens
only in the checked store adapter. Index rows are ordinary registered typed
records in index-specific families. Domain code cannot name arbitrary database
tables or raw keys. Journal and idempotency tables are private to `zap-store`;
`ChangeSet` cannot replace or remove their rows and cannot commit itself.

The private transaction and commit privileges are represented by public opaque
types with no public constructors:

```rust
pub struct TransactionPermit { /* private fields; held by CommitService */ }
pub struct TransactionBinding { /* private fields; issued by AtomicWrite */ }
pub struct ValidatedCommitIntent { /* private fields; includes binding */ }

pub enum CommitDisposition { Committed, ExactRetry, ReconciledCommitted }
pub struct CommitReceipt { /* private validated fields */ }
pub enum CommitStatus {
    Committed(CommitReceipt),
    NotCommitted { command_id: CommandId },
    Unknown { command_id: CommandId },
}

impl TransactionPermit {
    pub fn bind(
        &self,
        identity: StoreIdentity,
        head: Revision,
        nonce: TransactionNonce,
    ) -> Result<TransactionBinding, ZapError>;
}

impl CommitReceipt {
    pub fn store(&self) -> &StoreIdentity;
    pub fn command_id(&self) -> &CommandId;
    pub fn event_id(&self) -> &EventId;
    pub fn transaction_id(&self) -> &TransactionId;
    pub fn revision(&self) -> Revision;
    pub fn event_digest(&self) -> EventDigest;
    pub fn output(&self) -> &CanonicalOutput;
    pub fn disposition(&self) -> CommitDisposition;
}
```

`CommitService` creates and retains `TransactionPermit` when its fully checked
composition is constructed. The permit authorizes only entry into that exact
store's write closure; it is not a route/action grant and cannot commit rows.
After entering, `AtomicWrite::binding()` supplies the opaque binding for that
transaction/pre-state. Only `CommitService` constructs
`ValidatedCommitIntent`, after all transaction-bound gates and `ChangeSet`
validation, and embeds that binding.

The intent contains the expected store/head, canonical logical event, exact
idempotency row, validated typed mutations, mandatory index closure,
record/cell registry digests and output receipt. `zap-store` receives read-only
accessors so it can verify and encode the intent. `AtomicWrite` exposes no
ordinary record mutation; its only write operation is `apply_commit` with the
bound intent. The store rejects a foreign/stale transaction binding and
rechecks head, absent event/command keys, registries and mutation/index
completeness before inserting private journal and idempotency rows. Entering a
transaction, route authorization and committing a validated change are thus
three distinct capabilities. This is the R04 trusted commit-intent port, not a
raw bypass.

`TransactionNonce` is a store-generated opaque value and
`TransactionBinding` is cloneable/equality-comparable but neither is a wire
type. The backend receives the unconstructible permit in `transact`, calls
`permit.bind` after reading its identity/head, retains the binding in its write
adapter, and returns it from `AtomicWrite::binding`. A validated intent must
carry that exact binding. `CommitReceipt` is serializable only through its
checked wire projection; constructing a receipt or setting its disposition is
private to core/store reconciliation.

`CommitService<S: TransactionStore>` owns the one official mutation path. Its
constructor requires internally valid `CellSet`, `QuerySet`, `RecordSet`,
`RouteRegistry`, `ReducerEpoch`, `QueryEpoch`, exact `StoreIdentity`, and an
injected authority provider. The sets may all be empty for R03. Construction
rejects a route/cell/schema mismatch or any registered event kind lacking
exactly one classification. A production profile separately supplies
`RequiredCapabilities`; readiness fails until every requirement resolves to a
registered cell/query/record family.

```rust
pub struct CommitServiceBuilder<S: TransactionStore> { /* private fields */ }

impl<S: TransactionStore> CommitServiceBuilder<S> {
    pub fn new(
        store: S,
        identity: StoreIdentity,
        reducer_epoch: ReducerEpoch,
        query_epoch: QueryEpoch,
        trust: Box<dyn TrustBootstrapSource>,
    ) -> Self;
    pub fn cells(self, cells: CellSet) -> Self;
    pub fn queries(self, queries: QuerySet) -> Self;
    pub fn records(self, records: RecordSet) -> Self;
    pub fn routes(self, routes: RouteRegistry) -> Self;
    pub fn required_capabilities(self, required: RequiredCapabilities) -> Self;
    pub fn build(self) -> Result<CommitService<S>, ZapError>;
}

impl<S: TransactionStore> CommitService<S> {
pub fn execute(
    &self,
    principal: PrincipalContext<'_>,
    frame: CanonicalCommandFrame,
) -> Result<CommitReceipt, ZapError>;
pub fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError>;
}
```

`new` performs no I/O and grants nothing. `build` checks identity against the
store, validates all registries/capability requirements, invokes the trusted
bootstrap exactly once, creates the private `TransactionPermit`, then returns
the service. `EmptyTrustBootstrap` is the only public zero-binding source for
R03; it creates no reader, coordinator, Owner or trusted-host principal and
therefore cannot make privileged capabilities ready.

Inside one `TransactionStore::transact`, `execute` checks store/base/campaign,
exact retry, current revision, current relevant basis, route authority, pauses,
holds and transition-specific preconditions; invokes the pure typed cell;
validates the complete `ChangeSet`; and appends one immutable `LogicalEvent`
with the idempotency result and all required projection/index mutations.
External effects are never run inside this transaction.

Read queries are typed specifications:

```rust
pub trait QuerySpec {
    type Input: CanonicalDecode;
    type Item: CanonicalEncode;
    const ID: &'static str;
    fn execute(&self, snapshot: &dyn QuerySnapshot, input: &Self::Input)
        -> Result<Page<Self::Item>, ZapError>;
}
```

`Page` contains store/base/revision, query epoch, bounded items,
`Completeness::{Complete,More(PageCursor),UnknownBoundary}`, and no ambiguous
empty-set claim. A `PageCursor` binds store, base, revision, query ID, normalized
query digest and last key. Foreign, stale or unknown cursors are typed errors.

## 8. Foundation handoff boundary

R03 may implement sections 1-7 now. Its scoped success is:

1. the independent package workspace resolves the real installed `specmark`;
2. every crate compiles with empty capability sets and no Python bridge;
3. identifier, digest, command, route, registry, error and store/query port
   types exist with constructor checks;
4. one in-memory fake can demonstrate a typed cell and exact retry without
   claiming durable storage or product semantics;
5. capability output is empty for unimplemented features.

R03 must not choose economics payloads, AgentHost behavior, redb table layout,
legacy conversion, or semantic event families. Those are later boundaries in
this document and the companion ADRs.

## 9. Relevant basis and semantic fingerprints

`Revision` remains audit order. It is not by itself a semantic dependency.
Every operation whose validity can survive unrelated commits binds this exact
typed basis:

```rust
pub struct RelevantBasis {
    pub purpose: BasisPurpose,
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub policy: Option<PolicyFingerprint>,
    pub intent: Option<IntentFingerprint>,
    pub outcome: Option<OutcomeFingerprint>,
    pub subjects: Vec<SubjectFingerprint>,
    pub dependencies: Vec<DependencyFingerprint>,
    pub contracts: Vec<ContractFingerprint>,
    pub sources: Vec<SourceFingerprint>,
    pub evidence: Vec<EvidenceFingerprint>,
    pub knowledge: Vec<KnowledgeFingerprint>,
    pub capacity: Option<TeamCapacityFingerprint>,
    pub closure: ClosureKnowledge,
    pub digest: RelevantBasisDigest,
}

pub enum BasisPurpose {
    Mutation(EventKind),
    Dispatch(WorkId),
    Verification(VerificationId),
    SemanticRequest(SemanticRequestId),
    Lowering(LoweringId),
    DreamPromotion(DreamId),
    Completion,
}

pub enum ClosureKnowledge {
    Complete,
    Incomplete { unknown: Vec<SubjectRef> },
}
```

Each fingerprint row is a typed subject/relationship identity, semantic epoch
or version, and domain-specific digest. Collections are canonically sorted and
duplicate-free. `RelevantBasisDigest` covers every field except
`observed_revision` and `digest`; unrelated journal progress can therefore be
rebound after recomputing the same relevant basis. An incomplete closure is
part of the digest and never represented by an omitted or empty collection.

`zap-domain` exports an object-safe `BasisProvider` that computes the basis from
`StateReader` for a closed `BasisPurpose`. It also validates a caller's proposed
scope against known affected subjects. A caller may request a wider basis. It
cannot supply a smaller scope than the kernel-derived closure. Semantic
relationships missing from the known graph remain `ClosureKnowledge::Incomplete`
until an admitted assessment records them.

The service recomputes the relevant basis inside the same transaction that
would commit the command. A stale global revision with an equal basis may be
rebound through a recorded causal rebind. A changed basis refuses `StaleBasis`;
changing the command ID does not cure it.

## 10. Trust, principals and service admission

Worker responsibility and mutation authority remain different types:

```rust
pub enum WorkerRole { Senior, Middle, Junior }
pub enum PrincipalRole { Reader, Worker, Coordinator, Owner, TrustedHost }

pub enum PrincipalContext<'a> {
    AgentData(&'a AgentDataGrant),
    TrustedObservation(&'a TrustedObservationGrant),
    Credentialed(&'a AuthenticatedPrincipal),
    ServiceInternal(&'a ServicePermit),
}
```

All grant/permit fields and constructors are owned by `zap-core`. The
composition root never constructs an `AuthenticatedPrincipal` or Owner grant.
That principal exposes only principal ID, role, campaign, allowed control
kinds, allowed action classes, controller epoch and a nonsecret authorization
reference. A credential value is held in `SecretInput`, which implements
neither serialization, clone nor debug and is consumed by the authority.

```rust
pub trait CredentialAuthority: Send + Sync {
    fn authenticate(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<AuthenticatedPrincipal, ZapError>;
    fn authorize_read(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<ReaderGrant, ZapError>;
}

pub trait SecretVerifier: Send + Sync {
    fn verify(&self, secret: SecretInput<'_>) -> bool;
}

pub trait TrustBootstrapSource {
    fn register(&self, registrar: &mut TrustRegistrar<'_>)
        -> Result<(), ZapError>;
}

pub struct TrustRegistrar<'a> { /* private, construction-lifetime only */ }

impl TrustRegistrar<'_> {
    pub fn bind_reader(
        &mut self, id: CredentialId, campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>, reference: AuthorizationRef,
    ) -> Result<(), ZapError>;
    pub fn bind_coordinator(
        &mut self, id: CredentialId, campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>, scope: CoordinatorScope,
        reference: AuthorizationRef,
    ) -> Result<(), ZapError>;
    pub fn bind_owner(
        &mut self, id: CredentialId, campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>, scope: OwnerScope,
        reference: AuthorizationRef,
    ) -> Result<(), ZapError>;
    pub fn bind_trusted_host(
        &mut self, binding: TrustedHostBinding,
    ) -> Result<(), ZapError>;
}
```

The production `CommitServiceBuilder` accepts one `TrustBootstrapSource`,
creates the otherwise unconstructible registrar, invokes it exactly once before
publishing any route, then builds the core-owned `BoundCredentialAuthority` and
private grants. `zap-app` implements the source by reading already protected
startup configuration; no JSON/CLI/HTTP command, campaign record or deserialized
payload can obtain a registrar or call `bind_owner`. The finished service has no
method to add or widen a binding. Test-only constructors may inject a fake under
`cfg(test)` and are absent from the production capability registry.

`OwnerScope`, `CoordinatorScope`, and `TrustedHostBinding` are typed startup
configuration records with campaign, control/action sets and controller epoch;
they contain no credential value and implement no wire deserialization.
Duplicate IDs, cross-campaign scope, an Owner scope without explicit allowed
operations, or a host binding without a controller epoch refuses construction.
`CredentialAuthority` is the read-only core trait implemented in production by
`BoundCredentialAuthority`; adapters never return or construct grant structs.

The mutation route registry is total over registered cells:

- `DataProposal` accepts only an `AgentDataGrant` scoped to that campaign and
  cannot alter readiness, authority, accepted proof or external-effect state.
- `OwnerControl(ControlClass)` accepts only a current campaign-bound Owner
  principal whose trusted bootstrap/credential scope includes that exact
  class. It does not require delegated `ActionClass` authority: charter
  activation/amendment, campaign stop, exact resume/exception/epoch changes,
  economics-policy activation and Owner change decisions use this route.
  Charter draft remains `DataProposal`. Owner control may operate while a
  product pause is active, subject to its cell's exact resume/safe-state rules.
- `TrustedObservation` accepts only a bound trusted-host/coordinator grant and
  records already observed bytes or effects. It grants no product action.
- `Privileged(ActionClass)` requires active charter delegation, exact action
  assessment/admission, current basis, stop/hold clearance and controller
  fencing.
- `ServiceInternal` is constructible only by `CommitService` while executing a
  documented multi-step admission/reconciliation protocol.

Replay receives stored `LogicalEvent` values and calls the same pure cells, but
does not reauthenticate historical authority or repeat effects. Event
`authority_basis` preserves the admitted public reference. Actor/owner/role
text in a payload is always data.

The official service gate order is identity/idempotency, store and CAS,
relevant basis, principal/route, active charter/delegation, Owner pause,
economics hold/admission, source/evidence applicability, transition invariants,
and atomic commit. Owner pause dominates economics release. Trusted result and
safe-state observations remain recordable during pause when required to drain
or reconcile already-started work.

## 11. One completion predicate

The R05 foundation owns these stable record names and families. Their internal
typed fields follow the normative R02 schemas and may evolve within the same
semantic contract without moving store or runtime ownership:

| Rust record | Record family |
| --- | --- |
| `IntentRecord` | `zap.domain.intent` |
| `CharterRecord` | `zap.domain.charter` |
| `OutcomeRecord` | `zap.domain.outcome` |
| `ObligationRecord` | `zap.domain.obligation` |
| `WorkRecord` | `zap.domain.work` |
| `TaskContractRecord` | `zap.domain.contract` |
| `StageAcceptanceRecord` | `zap.domain.stage_acceptance` |
| `DeferralRecord` | `zap.domain.deferral` |
| `EvidenceAdjudicationRecord` | `zap.domain.evidence_adjudication` |
| `IntegrationAcceptanceRecord` | `zap.domain.integration_acceptance` |
| `WorkAcceptanceRecord` | `zap.domain.work_acceptance` |
| `PromotionRecord` | `zap.domain.promotion` |
| `ClosureRecord` | `zap.domain.closure` |

`zap-domain` owns their `StoredRecord` implementations plus `record_set()`,
`cell_set()`, `query_set()` and its completion providers. `zap-core` owns all
record/transition/query traits and shared view types; `zap-store` owns only
encoding and persistence. R05 must not introduce a store dependency.

zap-core additionally owns CandidateProvenanceRecord in record family
zap.core.candidate_provenance. Its exact fields are candidate_id: CandidateId,
producer: ProducerRef, subjects: Vec<SubjectRef>, contract_id: ContractId,
contract_digest: ContractDigest, relevant_basis: RelevantBasisDigest,
artifacts: Vec<ArtifactDigest>, observation: ObservationRef and revision:
Revision.

Construction validates required references and exact sorted unique subject and
artifact lists. Only trusted collection or import-adjudication cells may
populate this family. Work, evidence, stage and integration acceptance inputs
carry a CandidateId; the admitted acceptor is compared with the stored producer
and the relevant subject, contract and basis. A producer-supplied acceptance
identity in command payload is invalid.

`zap-core` owns the composition of one pure completion query consumed by
runtime scheduling and the direct `campaign.close` transition:

```rust
pub struct CompletionView {
    pub campaign_id: CampaignId,
    pub outcome_id: Option<OutcomeId>,
    pub relevant_basis: RelevantBasisDigest,
    pub blockers: Vec<CompletionBlocker>,
    pub eligible: bool,
}

pub enum CompletionBlocker {
    NoActiveOutcome,
    ActiveObligation(ObligationId),
    MissingWorkAcceptance(WorkId),
    MissingIntegration(WorkId),
    ApplicableDeferral(DeferralId),
    MissingPromotion(SubjectRef),
    MissingFinalGate(EvidenceId),
    PendingSelectedChange(ChangeId),
    PendingOwnerDecision(ChangeId),
    PartlyConsumedEnvelope(ChangeId),
    ActiveHold(HoldId),
    ActivePause(PauseId),
    LiveJob(JobId),
    UnknownExternalEffect(EffectId),
    IncompleteKnowledgeBoundary(Vec<SubjectRef>),
}

pub trait CompletionBlockerProvider: Send + Sync {
    fn id(&self) -> CompletionProviderId;
    fn blockers(&self, state: &dyn StateReader)
        -> Result<Vec<CompletionBlocker>, ZapError>;
}

pub struct CompletionProviderSet { /* checked provider-id registry */ }
impl CompletionProviderSet {
    pub fn empty() -> Self;
    pub fn register<P: CompletionBlockerProvider>(&mut self, provider: P)
        -> Result<(), ZapError>;
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError>;
}

pub struct CompletionEvaluator { /* fixed startup composition */ }
impl CompletionEvaluator {
    pub fn new(
        providers: CompletionProviderSet,
        required: Vec<CompletionProviderId>,
    ) -> Result<Self, ZapError>;
    pub fn view(&self, state: &dyn StateReader) -> Result<CompletionView, ZapError>;
}
```

The production required provider IDs are `zap.domain`, `zap.control`,
`zap.economics`, and `zap.runtime`. Feature crates export their disjoint
providers; `zap-app` composes the fixed set once at startup. Missing required
providers refuse readiness and close as an incomplete capability. They never
mean zero blockers. Callers cannot supply providers per request.

For a cell whose descriptor has `requires_completion = true`,
`CommitService` invokes this same evaluator over the transaction pre-state and
places the exact view in private `ValidatedCommand` admission context. The
direct close reducer requires `command.completion()` and `eligible == true`.
`CampaignReadPort::completion_view` delegates to the same configured evaluator.

The constructor sorts and deduplicates blockers. `eligible` is true exactly
when there is an active outcome, every active obligation is currently covered,
all required integration/deferral/promotion/final-gate duties are met, and the
blocker list is empty. An empty frontier has no special meaning. Untrusted
drafts, unselected alternatives, hypothetical Dreamer branches and resolved
rejections do not create blockers.

This fixes the required Rust behavior while preserving the Python economics
candidate as unaccepted evidence. R07 must demonstrate that the same function
blocks both automatic and direct close for active holds, pending decisions and
partly consumed selected envelopes.

## 12. Strategic plans, lowering and packets

Strategy, lowering and rendering are separate typed artifacts:

```rust
pub struct StrategicPlanRevision {
    pub id: StrategicRevisionId,
    pub previous: Option<StrategicRevisionId>,
    pub intent_id: IntentId,
    pub outcome_id: OutcomeId,
    pub obligations: Vec<ObligationId>,
    pub major_nodes: Vec<StrategicNode>,
    pub forks: Vec<ForkId>,
    pub risks: Vec<RiskId>,
    pub integration_conditions: Vec<ConditionId>,
    pub basis: RelevantBasisDigest,
}

pub struct LoweringRequest {
    pub lowering_id: LoweringId,
    pub strategic_revision: StrategicRevisionId,
    pub target: SubjectRef,
    pub required_stage: MaturityStage,
    pub executor: CapabilityRequirement,
    pub permitted_decisions: Vec<DecisionClass>,
    pub source_captures: Vec<SourceFingerprint>,
    pub basis: RelevantBasisDigest,
}

pub struct LoweringRevision {
    pub id: LoweringId,
    pub previous: Option<LoweringId>,
    pub request_digest: LoweringRequestDigest,
    pub work: Vec<WorkContract>,
    pub dependencies: Vec<WorkDependency>,
    pub coverage: Vec<ObligationAssignment>,
    pub forks: Vec<PreparedFork>,
    pub deferrals: Vec<DeferralDisposition>,
    pub integration_owners: Vec<IntegrationOwnership>,
    pub unresolved: Vec<UnknownBoundary>,
}
```

Construction requires an acyclic graph, unique identities, one current
contract per new leaf, a disposition for every inherited obligation/deferral,
an origin for every new work item, typed ownership, and an integration path.
It proves structural coverage, not semantic faithfulness. A semantic lowering
is a privileged `plan.lower` proposal/admission; a render of an unchanged
lowering for another model/context is derived data.

`PreparedFork` contains question/problem identity, premises and unknowns,
alternatives, selection preconditions/authority, costs/value, hazards,
recommendation/reason, rejection conditions and next diagnostic. Selecting it
requires current premises and granted decision class. Unknown preconditions
produce evidence work; they never become false.

```rust
pub struct WorkerPacket {
    pub packet_id: PacketId,
    pub parent: Option<PacketId>,
    pub supersedes: Option<PacketId>,
    pub campaign_id: CampaignId,
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub strategic_revision: StrategicRevisionId,
    pub lowering_revision: LoweringId,
    pub work: WorkId,
    pub contract: WorkContract,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub role: WorkerRole,
    pub desired_profile: DesiredProfile,
    pub resolved_profile: ResolvedProfile,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceClaim>,
    pub sources: Vec<PacketFragment>,
    pub rules: Vec<PacketFragment>,
    pub forks: Vec<ForkId>,
    pub checks: Vec<VerificationPlan>,
    pub safe_stop: SafeStopContract,
    pub unloaded: Vec<ContextOmission>,
    pub result_contract: CandidateContract,
    pub digest: PacketDigest,
}
```

Fragments carry content digest, source/provenance, required/optional reason and
measured byte plus estimated-token cost. They are sorted and content-deduped.
The packet constructor refuses duplicate/conflicting subjects, missing required
context, role/action mismatch, stale basis, an over-budget mandatory set, or a
Senior packet with production write subjects. It never truncates required data
or silently substitutes full boot. `result_contract` always says candidate,
never accepted.

## 13. Weak execution bundles and encounters

```rust
pub struct WeakBundleManifest {
    pub bundle_id: BundleId,
    pub campaign_id: CampaignId,
    pub exported_revision: Revision,
    pub strategic_revision: StrategicRevisionId,
    pub lowering_revision: LoweringId,
    pub required_capabilities: CapabilityRequirement,
    pub packets: Vec<PacketDigest>,
    pub fragments: Vec<ArtifactDigest>,
    pub permitted_decisions: Vec<DecisionClass>,
    pub stop_conditions: Vec<StopRuleId>,
    pub manifest_digest: BundleDigest,
}

pub struct Encounter {
    pub encounter_id: EncounterId,
    pub packet_id: PacketId,
    pub attempt_id: AttemptId,
    pub kind: EncounterKind,
    pub branch: Option<ForkId>,
    pub observations: Vec<ObservationRef>,
    pub artifacts: Vec<ArtifactRef>,
    pub unresolved: Vec<UnknownBoundary>,
    pub effect_state: EffectState,
}

pub struct ReturnBundle {
    pub source_bundle: BundleId,
    pub base_revision: Revision,
    pub encounters: Vec<EncounterId>,
    pub candidates: Vec<CandidateId>,
    pub artifacts: Vec<ArtifactRef>,
    pub digest: ReturnBundleDigest,
}
```

Bundles are portable, content-addressed and credential-free. Import is
idempotent by bundle/encounter/candidate identity and verifies all digests. It
never overwrites an advanced strategy: stale or conflicting returns become
review inputs. A weak-only environment may act inside prepared authority and
capability; anything beyond it becomes `AwaitingRefinement` or
`CapabilityWait`. Fixture/simulated evidence never claims actual local-model
quality.

## 14. Dreamer and grill contracts

```rust
pub struct DreamBranch {
    pub dream_id: DreamId,
    pub base_revision: Revision,
    pub base_strategic_revision: StrategicRevisionId,
    pub attachment: DreamAttachment,
    pub intent: DreamIntent,
    pub grill: GrillState,
    pub delta: DreamDelta,
    pub assumptions: Vec<AssumptionId>,
    pub unknowns: Vec<UnknownBoundary>,
    pub estimate: Option<ChangeAssessmentId>,
    pub status: DreamStatus,
}

pub enum DreamStatus { Exploring, Ready, Stale, Applied, Rejected, Withdrawn }
pub enum GrillState {
    NotOffered,
    Declined,
    InProgress { questions: Vec<GrillQuestion>, answers: Vec<GrillAnswer> },
    Complete { transcript_digest: GrillDigest },
}
pub enum DreamOperation { Add, Remove, Move, Replace }
```

`DreamDelta` is a typed list of operations and obligation dispositions over the
base strategic graph, never a copied live plan. Simulation is data-only: it may
calculate affected scope, alternatives and estimates but cannot dispatch,
alter readiness, consume an Owner decision or create a live economics hold.
Ambiguous attachment has no valid constructor. Grill answers and unresolved
questions are durable and survive compaction.

Promotion creates an ordinary semantic change proposal bound to current basis.
The trusted service resolves concurrent drift, authority, economics and live
job reconciliation before applying it. Remove/unzap records obligation,
dependent work, evidence, artifact and effect dispositions; it never erases
history or retroactively changes original success.

## 15. Agent capabilities, dispatch and results

`AgentCapabilities` is a captured observation, not an adapter claim:

```rust
pub struct AgentCapabilities {
    pub observation_id: CapabilityObservationId,
    pub harness_id: HarnessId,
    pub adapter: AdapterIdentity,
    pub native_workers: CapabilitySupport,
    pub instruction_isolation: InstructionIsolation,
    pub structured_results: CapabilitySupport,
    pub liveness: LivenessCapability,
    pub cancellation: CancellationCapability,
    pub goal: GoalCapability,
    pub models: Vec<ModelCapability>,
    pub context_limit: Option<TokenCount>,
    pub concurrency: Option<NonZeroU32>,
    pub unattended: CapabilitySupport,
    pub environment_fingerprint: CapabilityDigest,
    pub evidence: Vec<ObservationRef>,
}

pub enum CapabilitySupport { Supported, Unsupported, Unknown }
pub enum InstructionIsolation { ExactPacket, KnownInherited, Unknown }
pub struct GoalCapability {
    pub scope: GoalScope,
    pub operations: GoalOperationCapabilities,
}
pub enum GoalOperation { Read, Create, Update, Clear }
pub struct GoalOperationCapabilities {
    pub read: GoalOperationSupport,
    pub create: GoalOperationSupport,
    pub update: GoalOperationSupport,
    pub clear: GoalOperationSupport,
}
pub enum GoalOperationSupport {
    AgentCallable,
    OwnerOnly { command_template: BoundedText<4096> },
    Unsupported,
    Unknown,
}
```

Every operation is observed and cached independently. Support for read/create
does not imply update or clear; an adapter cannot complete an unfinished goal
to simulate replacement. An Owner-only command template belongs only to its
named operation and remains a manual fallback, never an agent-callable claim.
`GoalCapability::new` requires all four operations, one explicit scope, and a
validated template for every Owner-only row.

The cache key is harness ID, adapter/toolset version and effective configuration
digest. A changed key or contradictory observation invalidates it. Discovery
uses static/tool metadata where available and never spends an inference call
for already observable data.

```rust
pub struct DispatchIntent {
    pub dispatch_id: DispatchId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub host: HarnessId,
    pub capability_digest: CapabilityDigest,
    pub role: WorkerRole,
    pub resolved_profile: ResolvedProfile,
    pub workspace: WorkspaceBinding,
    pub relevant_basis: RelevantBasisDigest,
    pub safe_stop: SafeStopContract,
}

pub struct DispatchReceipt {
    pub dispatch_id: DispatchId,
    pub intent_digest: DispatchIntentDigest,
    pub state: DispatchState,
    pub handle: Option<ExternalJobHandle>,
    pub observation: ObservationRef,
}

pub enum DispatchState { AwaitingHarness, Submitted, Starting, Running, Unavailable }
```

`Submitted|Starting|Running` require a bound external handle; `AwaitingHarness`
and `Unavailable` prohibit one. A native harness receipt must be submitted by
the configured bridge and match the committed intent. An intent alone is not a
launch. `ExternalJobHandle` is opaque outside its adapter and binds adapter,
harness, campaign, job, attempt and dispatch digest.

`ProducerRef` is exactly `{actor: ActorRef, job_id: JobId,
attempt_id: AttemptId, packet_id: PacketId}`. `CandidateResult` binds that
producer plus job/attempt/packet/contract/basis, terminal transport
observation, artifacts/digests, satisfied and unsatisfied criteria, checks run,
discoveries, selected branch, unresolved questions, proposed follow-up,
effect state and declared safe boundary. It contains no acceptance flag that a
producer can set. Parsing failure preserves artifact/transport evidence and
returns bounded repair feedback; it does not repeat completed coding.

Work/evidence/stage/integration acceptance loads the referenced `ProducerRef`
and compares it with `ValidatedCommand::authority().actor()`. The exact producer
operation/attempt cannot accept its own candidate; the acceptor must also hold
the independent registered acceptance action. Model-name equality is irrelevant
and a later independently admitted coordinator operation may use the same model.

## 16. Runtime state, reconciliation and resume

The runtime stores orthogonal state rather than one overloaded status:

```rust
pub enum ExecutionState {
    Prepared, DispatchPending, Starting, Running, StopRequested, Stopping,
    Succeeded, Failed, Stopped, Interrupted, UnknownEffect,
}
pub enum CollectionState { Uncollected, Collected, Malformed, CandidateRecorded }
pub enum SafeState { Unknown, NeedsReconcile, NotStarted, Safe, Completed }
pub enum AcceptanceState { Unreviewed, Rejected, Accepted }
pub enum WaitClass {
    RateLimit, ProviderQuota, ProviderAuth, ProviderUnavailable,
    Configuration, Resource, Evidence, ModelResponseInvalid,
}
```

Every job has stable job/attempt/dispatch IDs, packet/contract/basis captures,
subject/resource/integration reservations, external handle, liveness
observation, stop requests/delivery, effect state, candidate and verification
references. Missing liveness does not prove exit. Process exit, stop delivery,
safe state and acceptance remain separate.

The scheduler selects a deterministic maximal ready set subject to read/write
conflicts, named resource capacities, host concurrency, review capacity and
integration-owner capacity. Read/read overlap is legal; read/write and
write/write overlap on the same semantic subject conflict. It commits claims
before dispatch and never holds a database transaction while calling a host,
provider, filesystem tool, build or test.

Retry owns the same logical operation and records classification, attempt
history, backoff basis and retry condition. Unknown external effect, ambiguous
dispatch receipt or partly delivered stop must reconcile before retry. A new
account/session does not reset attempts, approach counts, holds or pauses.

```rust
pub struct ResumeView {
    pub campaign_id: CampaignId,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub accepted_boundary: AcceptedBoundary,
    pub active_assignments: Vec<AssignmentRef>,
    pub pending_effects: Vec<EffectId>,
    pub holds: Vec<HoldId>,
    pub pauses: Vec<PauseId>,
    pub waits: Vec<WaitId>,
    pub relevant_decisions: Vec<DecisionId>,
    pub next_actions: Vec<AdmissibleAction>,
    pub navigation: Vec<QueryHandle>,
    pub digest: ResumeDigest,
}
```

Resume is a deterministic bounded query. It never claims unrecorded reasoning,
actual transport state, or workspace state; the coordinator reobserves those
through adapters before an effectful next action.

## 17. GOAL projection

```rust
pub struct GoalProjection {
    pub goal_id: GoalId,
    pub scope: GoalScope,
    pub campaign_id: CampaignId,
    pub charter_revision: CharterRevision,
    pub outcome_id: OutcomeId,
    pub assignment: Option<WorkId>,
    pub stop_conditions: Vec<StopRuleId>,
    pub completion_evidence: Vec<RequirementRef>,
    pub resume: QueryHandle,
    pub content: BoundedText<16384>,
    pub digest: GoalDigest,
}

pub enum GoalApplicationState {
    NotRequested,
    Applied { operation: GoalOperation, acknowledgment: ObservationRef },
    ManualRequired { operation: GoalOperation, instruction: BoundedText<4096> },
    Unsupported { operation: GoalOperation },
    Unknown { operation: GoalOperation },
    Stale { prior: GoalDigest },
}
```

The campaign goal is a concise umbrella. Per-agent goals derive from current
contracts; complete child text is not concatenated into the umbrella. Goal
application requires a current capability observation and records the actual
revision/digest acknowledgment. Owner-only mode emits one exact instruction per
changed goal. Unsupported/unknown mode relies on packet/resume state and never
claims a goal was set. Goal state reinforces execution and cannot clear
acceptance, holds, pauses or unfinished work.

## 18. Legacy public types

`zap-legacy` exposes only read/import types:

```rust
pub enum LegacyEpoch { Zap1 }
pub enum LegacyKind { Node, Mandate, Task, Event, Source, Other }
pub struct LegacyId { pub kind: LegacyKind, pub original: BoundedText<4096> }
pub enum CurrentImportId {
    Subject(SubjectRef), Event(EventId), Command(CommandId),
    Store(StoreId), Base(BaseId),
}
pub struct ImportIdMap { pub legacy: LegacyId, pub current: CurrentImportId }

pub enum LegacyDigestDomain {
    BaseFileIncludingLf,
    PackedCommandWithoutLf,
    CommittedJournalPrefixIncludingTerminators,
    PendingTailRaw,
    PackedProjectionState,
    ReducerIdentity,
    SnapshotFileIncludingLf,
}
pub struct LegacyDigest { pub domain: LegacyDigestDomain, pub value: Digest32 }
pub struct PendingTail { pub byte_len: u64, pub sha256: LegacyDigest }
pub struct LegacyDiagnostic { pub code: BoundedText<128>, pub message: BoundedText<4096> }
```

`LegacyEpoch::Zap1` has wire value `zap/1`. `PendingTail` requires the
`PendingTailRaw` digest domain and always means uncommitted. `LegacyDiagnostic`
is preserved behavior evidence, never authority. Raw legacy byte surfaces use
domain-specific wrappers so base, journal and snapshot bytes cannot be swapped.
The exact mapping/import algorithm is fixed in `STORAGE-ADR.md`.

## 19. Complete vision acceptance map

Architecture acceptance is not implementation acceptance. The final campaign
must connect each vision section to these seams, normative spec anchors,
implemented cells and current evidence:

| Vision | Primary contract | Campaign evidence owner |
| --- | --- | --- |
| V01 | logical events, pure transitions, resume | R04/R05/R11/R16 |
| V02 | strategic/lowering/packet types and authority routes | R05/R08/R10 |
| V03 | `LoweringRequest/Revision`, coverage and stale basis | R08/R16 |
| V04 | role/profile/authority separation | R08/R11/R12 |
| V05 | packet fragments, isolation and token budget | R08/R12/R16 |
| V06 | typed abstraction mapping and reconstruction duty | R08/R16 |
| V07 | `PreparedFork` and encounter lineage | R08/R09 |
| V08 | weak/return bundles and stale import | R09/R16 |
| V09 | typed bounded query algebra | R04/R13/R17 |
| V10 | reservations, controller fencing and concurrent scheduler | R04/R11/R16 |
| V11 | `AgentHost`, native bridge and explicit subprocess adapter | R11/R16 |
| V12 | packet/message/candidate/liveness distinctions | R08/R11 |
| V13 | effect intent/receipt, reconciliation and `ResumeView` | R04/R11/R12/R16 |
| V14 | capability cache and goal application states | R12/R16 |
| V15 | `DreamBranch`, grill and drift-safe promotion | R10/R16 |
| V16 | relevant basis, adaptation and economics | R06/R07/R16 |
| V17 | route service, holds and shared completion predicate | R05/R07/R13 |
| V18 | package Rust workspace and transactional indexed store | R03/R04/R17 |
| V19 | installed adapters and no host-checkout dependency | R03/R15/R16 |
| V20 | revisioned graph/detail/history/why views | R13/R17 |
| V21 | selected verification and applicable evidence | R06/R08/R16/R18 |
| V22 | zap/1 corpus, zap/2 import and no Python runtime | R14/R15/R18 |
| V23 | requirement/specmap denominator | R02/R18 |
| V24 | accepted execution/release boundary and NEXT/Qwen isolation | R16/R18/R19 |

R18 may close the architecture denominator only when every row has an
implemented registered capability or an explicit accepted disposition, its
specmark edges resolve, required focused evidence is current, and no selected
change, hold, unknown effect or mandatory deferral remains. Stubs, empty
capability sets, old Python evidence and unchecked prose do not satisfy a row.
