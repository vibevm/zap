# R07 core repair API contract

Status: root-accepted implementation contract (2026-09-13). This document owns only the
R07 changes required in `zap-core`, the R07 domain adapters, and their
composition. It does not redesign R08.

## Ownership boundary

R07 owns the typed privileged-action impact seam, registered effect
preflight/simulation and actual-after verification, authoritative affected and
dependent closure, opaque independence/safe-job witnesses, R07 admission
integration, and the schema-2 logical-event additions needed to persist those
decisions.

The R07 recovery implementation owner owns the economics behavior behind this seam: linear
forecast lineage, monotone cumulative cost, hold lifecycle, completion rules,
and all unknown-cost policy modes. It consumes this API and does not redefine
it.

R13D owns history/index repair and narrow core read ports. R07 adds the minimum
schema-2 event fields and distinct schema-1 decode/replay entry only; it does
not build history projections, indexes, tombstones, or general read APIs.

## Non-negotiable behavior

1. A registered typed extractor turns the already decoded product payload into
   an impact request. A transaction-bound state provider classifies that
   request as initial baseline, progress, proof, or semantic
   change. Event names and payload claims cannot classify themselves.
2. Every live privileged registration has exactly one impact extractor.
   Missing privileged impact registration refuses composition. Typed adapters
   compose through a registration builder; public constructor combinations do
   not grow with every added adapter.
3. Exempt impact skips ECONOMICS only. Every authentication, authority, current
   basis, hold, Owner pause, evidence, and cell check still runs. The admission
   hook may atomically consume one exact valid CONTROL action exception, so an
   exempt hook `ChangeSet` need not be empty.
4. Every selected semantic effect is strictly decoded by its registered product
   type and simulated through the same pure mutation kernel as the authorized
   transition. Alternative branches simulate independently from the same
   transaction base; they never chain through one another.
5. Simulation may receive non-authorizing identifiers such as `ActorRef` and
   `EventId`. It never receives credentials, permits, grants,
   `ValidatedCommand`, authority-bearing handles, or external-effect adapters.
6. Stored semantic approval survives unrelated global transport revision churn.
   True business versions and relevant basis remain bound. Simulated and actual
   mutation digests are compared in the same current write transaction before
   commit.
7. Affected and dependent closure comes from authoritative current state.
   Caller-provided lists are comparison claims only. Empty caller job IDs never
   prove that no jobs are affected.
8. Independence and safe-job clearance use opaque exact witnesses bound to the
   current transaction, hold, basis, candidate jobs/subjects, and derived
   closure. Every hold creation, resolution, guard, and drain consumer receives
   the complete actual affected-job view.
9. New logical events use schema 2. Schema 1 has a distinct supported
   decode/replay path. Unsupported epochs refuse explicitly; raw historical
   bytes are never reinterpreted as schema 2.

## Acceptance standard

The implementation is accepted only after the real registered service executes
the sequence specified at the end of this contract. DTO shape tests, pure
provider tests, fake `StateReader` tests, and registration-only checks do not
substitute for that service journey.

## Wire types and event epochs

`zap-wire/src/digest.rs` adds six domain-separated digest newtypes through the
existing digest declaration macro, and `zap-wire/src/lib.rs` re-exports them:

```rust
ActionImpactDigest
AffectedScopeDigest
IndependenceDigest
SafeJobDigest
EffectPreflightDigest
EffectMutationDigest
```

`EffectMutationDigest` is required. `ProjectionDigest` is not reused: its
projection meaning does not promise an exact canonical product mutation batch.

`zap-core/src/commit.rs` stops using one deserializable `LogicalEvent` for all
epochs. The epoch probe is deliberately permissive only long enough to read the
number; the selected body decoder remains strict.

```rust
pub const LOGICAL_EVENT_SCHEMA_1: u16 = 1;
pub const LOGICAL_EVENT_SCHEMA_2: u16 = 2;

#[derive(Clone, Debug, Serialize)]
pub struct LogicalEventV1 {
    pub schema_version: u16,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub previous_revision: Revision,
    pub header: CommandHeader,
    pub reason: CommandReason,
    pub transaction_id: TransactionId,
    pub command_digest: CommandDigest,
    pub reducer_epoch: ReducerEpoch,
    pub query_epoch: QueryEpoch,
    pub authority: AdmittedAuthorityV1,
    pub action_admission: Option<ActionAdmissionObservationV1>,
    pub completion: Option<CompletionView>,
    pub dispatch_eligibility: Option<DispatchEligibilityView>,
    pub affected_jobs: Option<AffectedJobView>,
    pub artifacts: Vec<ArtifactDigest>,
    pub payload: Vec<u8>,
    pub output: Vec<u8>,
    pub previous_event_digest: EventDigest,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogicalEventV2 {
    pub schema_version: u16,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub previous_revision: Revision,
    pub header: CommandHeader,
    pub reason: CommandReason,
    pub transaction_id: TransactionId,
    pub command_digest: CommandDigest,
    pub reducer_epoch: ReducerEpoch,
    pub query_epoch: QueryEpoch,
    pub authority: AdmittedAuthority,
    pub command_preflight: CommandPreflightRecord,
    pub action_admission: Option<ActionAdmissionObservation>,
    pub action_preflight: Option<ActionAdmissionPreflightRecord>,
    pub action_outcome: Option<ActionProductOutcome>,
    pub completion: Option<CompletionView>,
    pub dispatch_eligibility: Option<DispatchEligibilityView>,
    pub affected_jobs: Option<AffectedJobView>,
    pub artifacts: Vec<ArtifactDigest>,
    pub payload: Vec<u8>,
    pub output: Vec<u8>,
    pub previous_event_digest: EventDigest,
}

pub enum DecodedLogicalEvent {
    Schema1(LogicalEventV1),
    Schema2(LogicalEventV2),
}

pub fn decode_logical_event(
    payload: &CanonicalPayload,
) -> Result<DecodedLogicalEvent, ZapError>;
```

`decode_logical_event` first decodes only `{ schema_version: u16 }`, then invokes
the exact private `#[serde(deny_unknown_fields)]` input decoder for
`LogicalEventV1` or `LogicalEventV2` and constructs the public validated type.
The public event types deliberately do not implement `Deserialize`. Any other value returns
`ErrorCode::UnsupportedEpoch` with the actual and supported values. No caller
may deserialize bytes directly as `LogicalEventV2`, default a missing v2 field,
or translate a v1 authority/admission object into a v2 one.

Schema 2 is the only live write format after this repair. Schema 1 remains a
supported, separate replay format; its frozen DTOs and reducer registration are
described under replay below.

## One typed registration builder

The additive `single_with_*` constructor family stops growing. Existing
constructors may remain as compatibility wrappers for non-privileged cells, but
all wrappers delegate to this builder and receive the same validation.

```rust
pub struct CellRegistrationBuilder<C: TransitionCell> {
    // C::Payload-typed optional adapters; fields are private.
}

impl<C: TransitionCell> CellRegistrationBuilder<C> {
    pub fn new(cell: C) -> Self;

    pub fn basis<B>(self, adapter: B) -> Result<Self, ZapError>
    where
        B: PayloadBasisScope<C::Payload>;

    pub fn dispatch<D>(self, adapter: D) -> Result<Self, ZapError>
    where
        D: PayloadDispatchEligibility<C::Payload>;

    pub fn affected_jobs<A>(self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadAffectedJobs<C::Payload>;

    pub fn artifacts<A>(self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadArtifacts<C::Payload>;

    pub fn action_impact<I>(self, adapter: I) -> Result<Self, ZapError>
    where
        I: PayloadActionImpact<C::Payload>;

    pub fn effect_contract<E>(self, adapter: E) -> Result<Self, ZapError>
    where
        E: EffectContract<C::Payload>;

    pub fn effect_bundles<E>(self, adapter: E) -> Result<Self, ZapError>
    where
        E: PayloadEffectBundles<C::Payload>;

    pub fn affected_scope<A>(self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadAffectedScope<C::Payload>;

    pub fn safe_jobs<J>(self, adapter: J) -> Result<Self, ZapError>
    where
        J: PayloadSafeJobs<C::Payload>;

    pub fn build(self) -> Result<CellSet, ZapError>;
}
```

Each adapter method refuses a second adapter of the same role instead of
silently replacing it. `build` performs the current kind/descriptor/codec checks
and these new checks:

- `RouteClass::Privileged(_)` requires exactly one `PayloadActionImpact`;
- every other route refuses an action-impact adapter;
- an effect contract is stored on the same erased registration as its product
  `TransitionCell`, so an `EventKind` cannot resolve to one decoder and another
  simulator; and
- any effect-bundle, affected-scope, or safe-job extractor is called only after
  its concrete payload has been strictly decoded.

The erased registration gains object-safe methods corresponding to the
adapters. The state-capable extractors are intentional:

```rust
pub trait PayloadEffectBundles<P: CommandPayload>: Send + Sync + 'static {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<Vec<EffectBundleRequest>, ZapError>;
}

pub trait PayloadAffectedScope<P: CommandPayload>: Send + Sync + 'static {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<AffectedScopeRequest, ZapError>;
}

pub trait PayloadSafeJobs<P: CommandPayload>: Send + Sync + 'static {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<Vec<SafeJobRequest>, ZapError>;
}
```

This is the required resolver for ID-only commands. In particular,
`ChangeAssessmentAdjudicated { assessment_id, .. }` registers a
`PayloadEffectBundles` implementation which uses `StateReaderExt::get_typed`
for exactly that `ChangeAssessmentRecord`, then extracts its alternatives. It
does not guess an object type from an ID, accept serialized records from the
caller, or scan for a merely matching payload digest.

## State-derived privileged-action impact

`zap-core/src/admission/impact.rs` owns the following API:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionImpactRule {
    InitialBaselineOrSemantic { baseline_subject: SubjectRef },
    Progress,
    Proof,
    SemanticChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionImpactClass {
    InitialBaseline,
    Progress,
    Proof,
    SemanticChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionImpactRequest {
    rule: ActionImpactRule,
    work_ids: Vec<WorkId>,
    subjects: Vec<SubjectRef>,
    request_digest: PayloadDigest,
}

impl ActionImpactRequest {
    pub fn new(
        rule: ActionImpactRule,
        work_ids: Vec<WorkId>,
        subjects: Vec<SubjectRef>,
    ) -> Result<Self, ZapError>;
    pub fn rule(&self) -> &ActionImpactRule;
    pub fn work_ids(&self) -> &[WorkId];
    pub fn subjects(&self) -> &[SubjectRef];
    pub const fn request_digest(&self) -> PayloadDigest;
}

pub struct ActionImpactContext<'a> {
    pub action: &'a ActionClass,
    pub kind: &'a EventKind,
    pub event_id: &'a EventId,
    pub payload_digest: PayloadDigest,
    pub relevant_basis: Option<&'a RelevantBasis>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionImpactView {
    pub request_digest: PayloadDigest,
    pub action: ActionClass,
    pub kind: EventKind,
    pub event_id: EventId,
    pub payload_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub class: ActionImpactClass,
    pub relevant_basis: Option<RelevantBasisDigest>,
    pub digest: ActionImpactDigest,
}

impl ActionImpactView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_digest: PayloadDigest,
        action: ActionClass,
        kind: EventKind,
        event_id: EventId,
        payload_digest: PayloadDigest,
        observed_revision: Revision,
        class: ActionImpactClass,
        relevant_basis: Option<RelevantBasisDigest>,
    ) -> Result<Self, ZapError>;
}

pub trait PayloadActionImpact<P: CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P) -> Result<ActionImpactRequest, ZapError>;
}

pub trait ActionImpactProvider: Send + Sync + 'static {
    fn classify(
        &self,
        state: &dyn StateReader,
        context: &ActionImpactContext<'_>,
        request: &ActionImpactRequest,
    ) -> Result<ActionImpactView, ZapError>;
}
```

Constructors sort and deduplicate set fields, reject an empty semantic scope,
and hash every meaning-bearing field. `ActionImpactRequest` is serializable for
canonical hashing but deliberately has no `Deserialize`; only the adapter for
the already decoded payload can create one. Core recomputes and validates the
returned view.

The `ActionImpactDigest` body contains the request digest, action, event kind,
product event ID, payload digest, class, and relevant business-basis digest. It
excludes `observed_revision`, the command ID, command digest, expected global
revision, transaction ID, and event-chain head. This is the stable semantic
binding that survives unrelated transport commits. `RelevantBasisDigest` still
binds policy, subject, dependency, contract, source, evidence, knowledge and
other true business versions; those changes re-adjudicate.

`DomainActionImpactProvider` is the single zap-domain implementation. It reads
the active charter/outcome and the unique baseline boundary. The exact first
charter-bound subject may become `InitialBaseline`; once that boundary exists,
the same rule becomes `SemanticChange`. Missing or ambiguous state refuses.
`Progress` and `Proof` are registered code rules whose domain preconditions are
also checked from state. Payload JSON never contains `ActionImpactClass`.

## Registered effect preflight and simulation

`zap-core/src/effects.rs` owns the request, contract and result types. The
request constructors canonicalize all set-valued fields; effect order and the
committed prefix remain ordered.

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectPreflightRequest {
    effect_id: EffectId,
    index: u32,
    kind: EventKind,
    payload: CanonicalPayload,
    predecessors: Vec<EffectId>,
    product_event_id: EventId,
    declared_subjects: Vec<SubjectRef>,
    relevant_before: RelevantBasisDigest,
    declared_relevant_after: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectPreflightRequestInput {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload: CanonicalPayload,
    pub predecessors: Vec<EffectId>,
    pub product_event_id: EventId,
    pub declared_subjects: Vec<SubjectRef>,
    pub relevant_before: RelevantBasisDigest,
    pub declared_relevant_after: RelevantBasisDigest,
}

impl EffectPreflightRequest {
    pub fn new(
        input: EffectPreflightRequestInput,
    ) -> Result<Self, ZapError>;
    pub fn effect_id(&self) -> &EffectId;
    pub const fn index(&self) -> u32;
    pub fn kind(&self) -> &EventKind;
    pub fn payload(&self) -> &CanonicalPayload;
    pub fn predecessors(&self) -> &[EffectId];
    pub fn product_event_id(&self) -> &EventId;
    pub fn declared_subjects(&self) -> &[SubjectRef];
    pub const fn relevant_before(&self) -> RelevantBasisDigest;
    pub const fn declared_relevant_after(&self) -> RelevantBasisDigest;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectBundleRequest {
    alternative_id: ChangeAlternativeId,
    committed_prefix: Vec<EffectId>,
    initial_basis: RelevantBasisDigest,
    effects: Vec<EffectPreflightRequest>,
    request_digest: PayloadDigest,
}

impl EffectBundleRequest {
    pub fn new(
        alternative_id: ChangeAlternativeId,
        committed_prefix: Vec<EffectId>,
        initial_basis: RelevantBasisDigest,
        effects: Vec<EffectPreflightRequest>,
    ) -> Result<Self, ZapError>;
    pub fn alternative_id(&self) -> &ChangeAlternativeId;
    pub fn effects(&self) -> &[EffectPreflightRequest];
    pub fn committed_prefix(&self) -> &[EffectId];
    pub const fn initial_basis(&self) -> RelevantBasisDigest;
    pub const fn request_digest(&self) -> PayloadDigest;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectScope {
    basis: BasisRequest,
    work_ids: Vec<WorkId>,
    subjects: Vec<SubjectRef>,
    artifacts: Vec<ArtifactDigest>,
}

impl EffectScope {
    pub fn new(
        basis: BasisRequest,
        work_ids: Vec<WorkId>,
        subjects: Vec<SubjectRef>,
        artifacts: Vec<ArtifactDigest>,
    ) -> Result<Self, ZapError>;
    pub fn basis(&self) -> &BasisRequest;
    pub fn work_ids(&self) -> &[WorkId];
    pub fn subjects(&self) -> &[SubjectRef];
    pub fn artifacts(&self) -> &[ArtifactDigest];
}

pub struct EffectSimulationContext {
    store: StoreIdentity,
    actor: Option<ActorRef>,
    effect_id: EffectId,
    product_event_id: EventId,
    observed_revision: Revision,
    relevant_before: RelevantBasisDigest,
}

impl EffectSimulationContext {
    pub fn store(&self) -> &StoreIdentity;
    pub fn actor(&self) -> Option<&ActorRef>;
    pub fn effect_id(&self) -> &EffectId;
    pub fn product_event_id(&self) -> &EventId;
    pub const fn observed_revision(&self) -> Revision;
    pub const fn relevant_before(&self) -> RelevantBasisDigest;
}

pub trait EffectContract<P: CommandPayload>: Send + Sync + 'static {
    fn scope(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &P,
    ) -> Result<EffectScope, ZapError>;

    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &P,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}
```

Core alone constructs `EffectSimulationContext`. `ActorRef` and the event IDs
are facts, not authority. The context has no credential, grant, permit,
`AuthenticatedPrincipal`, `AdmittedAuthority`, `ValidatedCommand`, capability
handle, clock, filesystem/process/model adapter, network client, or external
effect sink. An effect contract calls the same pure mutation kernel as its
authorized transition cell. It does not call the authority-checking cell.

The serializable evidence is:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectPreflightView {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload_digest: PayloadDigest,
    pub product_event_id: EventId,
    pub predecessors: Vec<EffectId>,
    pub work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub artifacts: Vec<ArtifactDigest>,
    pub reducer_epoch: ReducerEpoch,
    pub observed_revision: Revision,
    pub relevant_before: RelevantBasisDigest,
    pub relevant_after: RelevantBasisDigest,
    pub mutation_digest: EffectMutationDigest,
    pub stable_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectBundlePreflightView {
    pub alternative_id: ChangeAlternativeId,
    pub request_digest: PayloadDigest,
    pub committed_prefix: Vec<EffectId>,
    pub effects: Vec<EffectPreflightView>,
    pub initial_basis: RelevantBasisDigest,
    pub final_basis: RelevantBasisDigest,
    pub digest: EffectPreflightDigest,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPreflightRecord {
    pub effect_bundles: Vec<EffectBundlePreflightView>,
    pub affected_scopes: Vec<AffectedScopeView>,
    pub safe_jobs: Vec<SafeJobView>,
}
```

`EffectMutationDigest` hashes the canonical sorted prepared product mutations,
each as `(record family, encoded key, mutation kind, expected version, new
version, canonical value)`. It is calculated only after the `ChangeSet` has
been checked against the registered record families. It includes the concrete
current versions and is used only to compare simulation with the actual product
batch in the same write transaction.

The internal machinery has one implementation shared by preflight and the
actual-product check:

```rust
pub(crate) fn effect_mutation_digest(
    changes: &ChangeSet,
    records: &RecordSet,
    allowed: &[RecordFamily],
) -> Result<EffectMutationDigest, ZapError>;

pub(crate) struct ChangeSetOverlay<'a> {
    // Base reader plus one validated prepared ChangeSet.
}

impl<'a> ChangeSetOverlay<'a> {
    pub(crate) fn new(
        base: &'a dyn StateReader,
        changes: &'a ChangeSet,
        records: &'a RecordSet,
        allowed: &'a [RecordFamily],
    ) -> Result<Self, ZapError>;
}

impl StateReader for ChangeSetOverlay<'_> {
    // get_erased and scan_erased merge insert/replace/remove over the base.
}
```

Overlay reads validate expected versions exactly and use `RecordSet` to decode
the prepared values. They never commit or mutate the base.

`EffectPreflightView::stable_digest` hashes every displayed field except
`observed_revision`, `mutation_digest`, and the digest field itself. The bundle
`EffectPreflightDigest` hashes the alternative ID, request digest, committed
prefix, initial/final relevant basis and ordered stable effect digests. It also
excludes the global observed revision and current mutation digests. Therefore a
stored approval binds the handler epoch, canonical payload, exact effect order,
subjects, artifacts and relevant business bases, while current transport
revision churn does not invalidate it. A changed policy/subject/dependency/
contract/evidence version still changes `RelevantBasisDigest` and refuses.

For each `EffectBundleRequest`, core starts a fresh `ChangeSetOverlay` over the
same transaction pre-state. It strictly decodes each effect through the product
cell registered for `kind`, obtains that registration's `EffectContract`,
derives scope, checks the exact declared subjects, simulates, validates the
declared record families, overlays the mutations, and derives the after-basis
through the fixed `BasisProvider`. It checks the declared before/after chain,
index sequence, product event ID, unique effect IDs and predecessor closure.

Effects within one alternative chain through that alternative's overlay.
Different alternatives never share an overlay: branch B starts from the same
base state even after branch A was simulated. A no-op alternative has an empty
effect list and equal initial/final basis. Missing decoder, effect contract,
basis provider, incomplete scope, malformed canonical payload, or any claimed
scope/basis mismatch refuses preflight.

`EffectBundleRequest::new` requires unique ordered prefix/effect IDs,
`effect.index == committed_prefix.len() + offset`, and every predecessor to be
in the committed prefix or an earlier effect in this bundle. Assessment
branches use an empty prefix; actual selected-effect requests carry the exact
persisted applied prefix.

## Authoritative affected closure and actual jobs

`zap-core/src/execution_views/affected_scope.rs` owns the closure boundary.
The provider derives graph meaning; core attaches the existing runtime
`AffectedJobProvider` result built from that derived meaning.

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffectedScopeRequest {
    roots: Vec<SubjectRef>,
    direct_work_ids: Vec<WorkId>,
    request_digest: PayloadDigest,
}

impl AffectedScopeRequest {
    pub fn new(
        roots: Vec<SubjectRef>,
        direct_work_ids: Vec<WorkId>,
    ) -> Result<Self, ZapError>;
    pub fn roots(&self) -> &[SubjectRef];
    pub fn direct_work_ids(&self) -> &[WorkId];
    pub const fn request_digest(&self) -> PayloadDigest;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffectedScopeCompleteness {
    Complete,
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedAffectedScope {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub completeness: AffectedScopeCompleteness,
    pub relevant_basis: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffectedScopeView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub completeness: AffectedScopeCompleteness,
    pub relevant_basis: RelevantBasisDigest,
    pub jobs: AffectedJobView,
    pub digest: AffectedScopeDigest,
}

pub trait AffectedScopeProvider: Send + Sync + 'static {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError>;

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError>;
}
```

`AffectedScopeRequest::new` requires at least one root or direct work ID. The
domain provider walks current work parent/dependency edges, obligation
ownership, contract subjects, source/knowledge dependencies and known
consumers. It returns affected and dependent work as disjoint sorted sets.
Unknown traversal remains explicitly `Incomplete` with a nonempty unknown
boundary.

Core rejects a provider result whose request digest or observed revision is
wrong. It builds an `AffectedJobRequest` from the union of the *derived*
affected/dependent work and subjects, invokes the fixed `AffectedJobProvider`,
and requires its exact request digest, current revision and
`AffectedJobCompleteness::Complete`. Only then does core construct
`AffectedScopeView`.

The `AffectedScopeDigest` hashes closure fields and relevant basis but excludes
the global observed revision and changing runtime job states. The complete
`AffectedJobView` remains in the view and schema-2 event evidence. Caller lists
are never used to build its query. An empty caller `work_ids` or `safe_job_ids`
list therefore says nothing about actual jobs; only a provider-returned
complete-empty view proves there are none.

## Exact independence and safe-job witnesses

The request and serializable evidence types are:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndependenceRequest {
    hold_id: HoldId,
    held_scope: AffectedScopeDigest,
    candidate: AffectedScopeRequest,
    relevant_basis: RelevantBasisDigest,
    request_digest: PayloadDigest,
}

impl IndependenceRequest {
    pub fn new(
        hold_id: HoldId,
        held_scope: AffectedScopeDigest,
        candidate: AffectedScopeRequest,
        relevant_basis: RelevantBasisDigest,
    ) -> Result<Self, ZapError>;
    pub fn hold_id(&self) -> &HoldId;
    pub const fn held_scope(&self) -> AffectedScopeDigest;
    pub fn candidate(&self) -> &AffectedScopeRequest;
    pub const fn relevant_basis(&self) -> RelevantBasisDigest;
    pub const fn request_digest(&self) -> PayloadDigest;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub hold_id: HoldId,
    pub held_scope: AffectedScopeDigest,
    pub candidate_request_digest: PayloadDigest,
    pub candidate_scope: AffectedScopeDigest,
    pub independent: bool,
    pub unknown_boundary: Vec<SubjectRef>,
    pub relevant_basis: RelevantBasisDigest,
    pub digest: IndependenceDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeJobRequest {
    hold_id: HoldId,
    expected_scope: AffectedScopeDigest,
    current_scope: AffectedScopeRequest,
    request_digest: PayloadDigest,
}

impl SafeJobRequest {
    pub fn new(
        hold_id: HoldId,
        expected_scope: AffectedScopeDigest,
        current_scope: AffectedScopeRequest,
    ) -> Result<Self, ZapError>;
    pub fn hold_id(&self) -> &HoldId;
    pub const fn expected_scope(&self) -> AffectedScopeDigest;
    pub fn current_scope(&self) -> &AffectedScopeRequest;
    pub const fn request_digest(&self) -> PayloadDigest;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeJobView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub hold_id: HoldId,
    pub scope: AffectedScopeView,
    pub job_ids: Vec<JobId>,
    pub all_safe: bool,
    pub digest: SafeJobDigest,
}
```

Resolving a safe-job request rederives the current closure and its complete
actual job view. The resulting scope digest must still equal
`expected_scope`; otherwise resolution is stale and must re-adjudicate. Core
sets `all_safe` only when every returned job has a safe state, no execution is
starting/running/stopping or unknown-effect, and no effect is started or
unknown. A complete-empty job view is safe. A caller's empty list is merely a
claim and is rejected when the returned view contains jobs.

`SafeJobDigest` hashes the request, current affected-scope digest, complete
sorted job observations and `all_safe`, excluding only the global observed
revision already carried for audit. A changed job identity, execution/effect/
safe state or scope therefore invalidates the witness.

Positive views are wrapped by core in nonserializable, service-sealed types:

```rust
#[derive(Clone)]
pub struct IndependenceWitness {
    view: IndependenceView,
    transaction_seal: Arc<()>,
    service_seal: Arc<()>,
}

impl IndependenceWitness {
    pub fn view(&self) -> &IndependenceView;
}

#[derive(Clone)]
pub struct SafeJobWitness {
    view: SafeJobView,
    transaction_seal: Arc<()>,
    service_seal: Arc<()>,
}

impl SafeJobWitness {
    pub fn view(&self) -> &SafeJobView;
}
```

These types have no public constructor and no `Serialize` or `Deserialize`.
Core creates an independence witness only for `independent == true` after the
request, current revision, hold, held scope, candidate scope, basis and service
seal all match the current operation's private transaction seal. It creates a
safe-job witness only for a current exact-scope view with `all_safe == true` and
that same seal. Persisted `IndependenceView` and `SafeJobView` are audit
evidence, never reusable authority.

`change_hold_guard` accepts the preflight witnesses rather than a payload
fingerprint:

```rust
pub fn change_hold_guard(
    state: &dyn StateReader,
    candidate: &AffectedScopeRequest,
    preflight: &ActionAdmissionPreflight<'_>,
) -> Result<ChangeHoldGuard, ZapError>;
```

The guard first obtains the transaction-derived candidate view through
`preflight.affected_scope(candidate.request_digest())`. For every active
incomplete hold, a candidate is clear only when
`preflight.independence_for(hold_id, candidate.request_digest())` returns the
exact current witness. Complete holds still block intersections with the
derived candidate closure, never merely caller roots. General Owner pause is
evaluated first and remains dominant.

## Admission DTOs and provider contract

`zap-core/src/admission/mod.rs` owns the admission protocol. A validated basis
retains both the extraction request and the transaction-derived business view:

```rust
#[derive(Clone, Debug)]
pub struct ActionBasis {
    pub request: BasisRequest,
    pub relevant: RelevantBasis,
}

#[derive(Clone, Debug)]
pub struct ActionAdmissionRequest {
    pub action: ActionClass,
    pub header: CommandHeader,
    pub command_digest: CommandDigest,
    pub payload_digest: PayloadDigest,
    pub basis: Option<ActionBasis>,
    pub impact: ActionImpactView,
}

#[derive(Clone, Debug)]
pub struct ActionAdmissionNeeds {
    selected_effect: Option<EffectBundleRequest>,
    affected_scopes: Vec<AffectedScopeRequest>,
    independence: Vec<IndependenceRequest>,
    safe_jobs: Vec<SafeJobRequest>,
}

impl ActionAdmissionNeeds {
    pub fn new(
        selected_effect: Option<EffectBundleRequest>,
        affected_scopes: Vec<AffectedScopeRequest>,
        independence: Vec<IndependenceRequest>,
        safe_jobs: Vec<SafeJobRequest>,
    ) -> Result<Self, ZapError>;
    pub fn selected_effect(&self) -> Option<&EffectBundleRequest>;
    pub fn affected_scopes(&self) -> &[AffectedScopeRequest];
    pub fn independence(&self) -> &[IndependenceRequest];
    pub fn safe_jobs(&self) -> &[SafeJobRequest];
}
```

`selected_effect` is the actual selected-effect flow, not a generic
independence hint. For `SemanticChange`, core requires exactly one nonempty
bundle whose next effect matches the submitted product event kind, event ID and
payload digest and whose committed prefix matches the domain request. For an
exempt class it requires `None`. Thus the provider cannot admit a semantic
command by requesting only an independence check.

Every independence candidate must also appear in `affected_scopes`; core
derives that candidate closure before asking the provider for independence. For
a semantic action, the selected effect's derived work/subjects must be covered
by the same exact candidate request. A provider cannot use a narrower caller
list for hold guarding than the selected handler reports.

Core resolves the needs into serializable evidence plus sealed witnesses:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAdmissionPreflightRecord {
    pub selected_effect: Option<EffectBundlePreflightView>,
    pub affected_scopes: Vec<AffectedScopeView>,
    pub independence: Vec<IndependenceView>,
    pub safe_jobs: Vec<SafeJobView>,
}

pub struct ActionAdmissionPreflight<'a> {
    record: &'a ActionAdmissionPreflightRecord,
    independence: &'a [IndependenceWitness],
    safe_jobs: &'a [SafeJobWitness],
    transaction_seal: &'a Arc<()>,
    service_seal: &'a Arc<()>,
}

impl ActionAdmissionPreflight<'_> {
    pub fn record(&self) -> &ActionAdmissionPreflightRecord;
    pub fn selected_effect(&self) -> Option<&EffectBundlePreflightView>;
    pub fn affected_scope(
        &self,
        request_digest: PayloadDigest,
    ) -> Option<&AffectedScopeView>;
    pub fn independence_for(
        &self,
        hold_id: &HoldId,
        candidate_request_digest: PayloadDigest,
    ) -> Option<&IndependenceWitness>;
    pub fn safe_jobs_for(&self, hold_id: &HoldId)
        -> Option<&SafeJobWitness>;
}
```

Requests and results are sorted by canonical request digest; duplicates refuse.
There is no public preflight constructor. The borrowed wrapper exists only
during the current transaction.

Authority distinguishes an economics approval from an exemption:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionAdmissionBasis {
    Exempt {
        impact: ActionImpactDigest,
    },
    Economic {
        admission_id: AdmissionId,
        impact: ActionImpactDigest,
        selected_effect: EffectPreflightDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionMutationScope {
    affected_records: Vec<RecordFamily>,
    affected_indexes: Vec<IndexFamily>,
}

impl AdmissionMutationScope {
    pub fn new(
        affected_records: Vec<RecordFamily>,
        affected_indexes: Vec<IndexFamily>,
    ) -> Result<Self, ZapError>;
    pub fn affected_records(&self) -> &[RecordFamily];
    pub fn affected_indexes(&self) -> &[IndexFamily];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionHookDescriptor {
    id: CapabilityId,
    reducer_epoch: ReducerEpoch,
    exempt_scope: AdmissionMutationScope,
    economic_scope: AdmissionMutationScope,
}

impl AdmissionHookDescriptor {
    pub fn new(
        id: CapabilityId,
        reducer_epoch: ReducerEpoch,
        exempt_scope: AdmissionMutationScope,
        economic_scope: AdmissionMutationScope,
    ) -> Result<Self, ZapError>;

    pub fn id(&self) -> &CapabilityId;
    pub const fn reducer_epoch(&self) -> ReducerEpoch;
    pub fn scope_for(
        &self,
        basis: &ActionAdmissionBasis,
    ) -> &AdmissionMutationScope;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAdmissionObservation {
    pub hook_id: CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub basis: ActionAdmissionBasis,
    pub impact: ActionImpactView,
    pub payload: Vec<u8>,
    pub payload_digest: PayloadDigest,
}

impl ActionAdmissionObservation {
    pub fn new<T: Serialize>(
        descriptor: &AdmissionHookDescriptor,
        basis: ActionAdmissionBasis,
        impact: ActionImpactView,
        payload: &T,
    ) -> Result<Self, ZapError>;

    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ZapError>;
}
```

The constructor requires the basis impact digest to equal `impact.digest`.
Core's observation validator additionally requires an economic basis's selected
preflight digest to be present in the current transaction preflight record. The
provider-specific payload records such facts as the exact exception/admission/
hold versions needed for deterministic apply; it carries no credential.

Correction locked: the observation constructor validates only values passed to
it; it does not claim access to the current preflight. Durable approval binds
the stable `EffectPreflightDigest`, while core separately derives the current
preflight/mutation evidence and validates that relation inside execute/replay.

`AdmittedAuthority` changes its live privileged variant and accessors:

```rust
// Private AuthorityKind variant:
Privileged {
    action: ActionClass,
    basis: ActionAdmissionBasis,
}

impl AdmittedAuthority {
    pub fn privileged_action(
        &self,
    ) -> Option<(&ActionClass, &ActionAdmissionBasis)>;

    pub(crate) fn privileged(
        actor: ActorRef,
        action: ActionClass,
        basis: ActionAdmissionBasis,
    ) -> Self;
}
```

Schema-1 authority has a separate frozen representation and is never accepted
by this live constructor.

The after-product evidence and provider trait are:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionProductOutcome {
    pub selected_effect: Option<EffectPreflightDigest>,
    pub mutation_digest: EffectMutationDigest,
    pub relevant_after: Option<RelevantBasisDigest>,
}

pub trait ActionAdmissionProvider: Send + Sync + 'static {
    fn descriptor(&self) -> &AdmissionHookDescriptor;

    fn needs(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError>;

    fn admit(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
        preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError>;

    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;

    fn verify_after(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        outcome: &ActionProductOutcome,
    ) -> Result<(), ZapError>;
}
```

Credential authentication and route entitlement remain core work. The provider
receives the resulting non-authorizing `ActorRef`, which is sufficient for pure
domain separation checks and lets audit rederive the decision without
reconstructing a credential. `admit`, `apply`, `verify_after`, impact
classification, closure, and effect contracts are deterministic state reads
and `ChangeSet` construction only.

The exempt mutation scope for `ChangeControlAdmissionProvider` contains only
the CONTROL record/index families needed to consume one exact action exception.
It contains no `zap.economics.*` family. The economic scope contains the
admission/hold families and the same optional exception family. Core prepares
the hook batch against `descriptor.scope_for(&observation.basis)`, so an exempt
hook can consume a valid one-use exception atomically but cannot mutate an
economics record. Exemption bypasses only the economics approval lookup; pause,
hold, authority, basis, evidence, completion and product checks still run.

## Sealed DataProposal issuance prerequisite

The real R07 proposal/adjudication journey requires a lawful producer for
`RouteClass::DataProposal`. `zap-core/src/trust.rs` replaces the currently
unissuable two-field grant with this sealed exact-frame API:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDataBinding {
    pub principal_id: PrincipalId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub allowed_events: BTreeSet<EventKind>,
}

#[derive(Clone)]
pub struct AgentDataIssuerHandle {
    binding: AgentDataBinding,
    service_seal: Arc<()>,
}

impl TrustRegistrar<'_> {
    pub fn bind_agent_data(
        &mut self,
        binding: AgentDataBinding,
    ) -> Result<AgentDataIssuerHandle, ZapError>;
}

impl AgentDataIssuerHandle {
    pub fn authorize(
        &self,
        frame: &CanonicalCommandFrame,
    ) -> Result<AgentDataGrant, ZapError>;
}

pub struct AgentDataGrant {
    actor: ActorRef,
    store_id: StoreId,
    campaign_id: CampaignId,
    base_id: BaseId,
    command_digest: CommandDigest,
    event_id: EventId,
    kind: EventKind,
    service_seal: Arc<()>,
}

impl AgentDataGrant {
    pub fn actor(&self) -> &ActorRef;
    pub fn campaign_id(&self) -> &CampaignId;

    pub(crate) fn authorizes(
        &self,
        seal: &Arc<()>,
        frame: &CanonicalCommandFrame,
    ) -> bool;
}
```

Only `TrustRegistrar` constructs an issuer handle. Registration rejects an
empty event set and duplicate principal/event bindings; service build verifies
every configured kind is a registered `DataProposal` route. `authorize`
requires exact store/base/campaign identity and an allowed event kind, derives
`ActorRef.operation` directly as the frame's command ID with the configured
principal, and seals the frame digest, event ID and kind into the grant.

`AgentDataGrant` and `AgentDataIssuerHandle` have no public constructor and no
serde implementation. `CommitService` uses `grant.authorizes` in both static
entitlement and in-transaction admission; checking only campaign ID is
insufficient. The grant can satisfy only `DataProposal`. It cannot satisfy or
mint Privileged, OwnerControl, TrustedObservation or ServiceInternal authority,
and caller producer/host/coordinator labels remain untrusted data.

The bootstrap source may retain the handle returned during its one trusted
registration pass and give it to application ingress. This prerequisite also
permits later R08 strategy/encounter proposals and R13 transport to use the same
lawful route; it does not give those components stronger authority or move
their design into R07.

## CommitService construction and validated command access

`CommitServiceBuilder` keeps the current providers and adds these fixed
dependencies:

```rust
impl<S: TransactionStore> CommitServiceBuilder<S> {
    pub fn action_impact_provider(
        self,
        provider: Arc<dyn ActionImpactProvider>,
    ) -> Self;

    pub fn affected_scope_provider(
        self,
        provider: Arc<dyn AffectedScopeProvider>,
    ) -> Self;
}
```

`action_admission_provider`, `basis_provider`, `affected_job_provider` and the
other current setters remain. There is no separately keyed effect-contract
registry: the contract is part of the corresponding typed cell registration.

`build` cannot infer what a provider will return from dynamic `needs`. Once the
R07 admission hook or any privileged route is installed, it therefore
conservatively requires the full fixed R07 set: action-impact, action-admission,
basis, affected-scope and affected-job providers. Registered cell-level bundle,
scope and safe-job adapters also declare those dependencies in `CellSet`, so
build validates them without executing payload code. It validates both
admission mutation scopes against `RecordSet`. Runtime resolution still refuses
any missing provider or undeclared need. The transition registration itself has
already refused a privileged cell without its typed impact extractor.

Core carries resolved command preflight in a nonserializable service-created
runtime container:

```rust
#[derive(Clone)]
pub struct ValidatedCommandPreflight {
    record: CommandPreflightRecord,
    safe_jobs: Vec<SafeJobWitness>,
    transaction_seal: Arc<()>,
    service_seal: Arc<()>,
}

impl<P: CommandPayload> ValidatedCommand<P> {
    pub fn command_preflight(&self) -> &CommandPreflightRecord;

    pub fn effect_preflights(&self) -> &[EffectBundlePreflightView];

    pub fn affected_scope(
        &self,
        request_digest: PayloadDigest,
    ) -> Option<&AffectedScopeView>;

    pub fn safe_jobs_for(
        &self,
        hold_id: &HoldId,
    ) -> Option<&SafeJobWitness>;

    pub fn action_preflight(
        &self,
    ) -> Option<&ActionAdmissionPreflightRecord>;
}
```

`ValidatedHeader` and `ValidatedCommand` own an `Arc<ValidatedCommandPreflight>`
and the optional action-preflight record. Their constructors remain
crate-private. A domain cell can inspect derived evidence or use a sealed safe
witness; a caller cannot construct either.

## Exact live commit sequence

`CommitService::execute` retains exact-retry behavior and performs the following
steps for a new command. All state-dependent steps occur inside one store write
transaction against its current pre-state.

1. Resolve the registered cell, strictly decode its concrete payload, validate
   store/campaign/base/protocol identity, route equality, static credential or
   grant entitlement, artifact availability, command-ID reuse and current
   expected revision.
2. Derive the cell's `BasisRequest`, validate its scope, compute and retain the
   full `RelevantBasis`, and compare its digest with the header binding. A
   request/header applicability mismatch refuses.
3. For a privileged route, call the registered typed impact extractor, then the
   fixed `ActionImpactProvider` with the decoded request and current relevant
   basis. Validate the returned request digest, semantic fields, observed
   revision and recomputed `ActionImpactDigest`.
4. Resolve cell-level effect bundles, affected scope and safe-job requests from
   the already decoded payload. Because these extractors receive the current
   `StateReader`, an ID-only adjudication resolves its exact stored typed
   assessment here. Simulate every alternative independently from the same
   pre-state, derive authoritative closure, and require complete actual jobs.
5. For a privileged route, construct the non-authorizing `ActorRef`, call
   `ActionAdmissionProvider::needs`, and resolve all returned needs. For
   `SemanticChange`, require and run the actual selected-effect bundle and
   compare its next kind, event ID and payload digest with the submitted frame.
   For `InitialBaseline`, `Progress`, or `Proof`, require no selected effect.
6. Call `admit`; validate its hook ID/epoch/payload digest, exact impact, and the
   class-to-basis relation: semantic requires `Economic` with the current
   selected preflight digest, while the other three require `Exempt`. Call
   `apply` into the admission `ChangeSet`. Owner pause and hold checks run here;
   an allowed one-use CONTROL exception is only buffered, not yet consumed.
7. Evaluate all current completion, dispatch eligibility, affected-job and
   artifact checks required by the product descriptor. Build
   `ValidatedHeader`/`ValidatedCommand` with the authority, command preflight,
   action preflight and current views, then call the real product cell into a
   separate product `ChangeSet`.
8. Validate and prepare the product set against the product descriptor and
   compute its `EffectMutationDigest`. For a semantic action, overlay the actual
   product set on the same transaction pre-state, derive the product's relevant
   after-basis using the selected effect contract's `BasisRequest`, and require
   exact equality with the current simulation's mutation digest and after-basis.
   Construct `ActionProductOutcome` and call `verify_after`.
9. Prepare the hook set against `AdmissionHookDescriptor::scope_for` the
   observed basis. Combine hook and product record/index batches with the
   current duplicate-key conflict checks. A collision or any prior failure
   discards both sets, including a buffered exception consumption.
10. Encode `LogicalEventV2`, including the independently rederivable preflight,
    admission and product-outcome evidence, calculate the event digest and apply
    the one `ValidatedCommitIntent`. Only this step consumes an exception,
    advances economics/hold records, mutates product state and advances the
    history head atomically.

The actual/simulated comparison in step 8 is deliberately current-transaction
evidence. The durable approval comparison uses `ActionImpactDigest`,
`EffectPreflightDigest` and `RelevantBasisDigest`; it never compares a stale
global `CommandDigest` or prior transaction's `EffectMutationDigest`. Exact
retry still compares the canonical command digest for the already committed
command ID.

## Distinct replay APIs

Schema-1 compatibility is explicit. `LogicalEventV1`,
`AdmittedAuthorityV1`, `ActionAdmissionRequestV1` and
`ActionAdmissionObservationV1` freeze the current field names, serde form and
meaning. The old privileged authority remains an `AdmissionId`; it is not
converted into `ActionAdmissionBasis::Economic`. Core may use a private
`Schema1Privileged` authority variant while invoking a frozen schema-1 reducer,
but that variant has no live constructor.

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AuthorityKindV1 {
    AgentData,
    OwnerControl {
        class: ControlClass,
        reference: AuthorizationRef,
    },
    Privileged {
        action: ActionClass,
        admission: AdmissionId,
    },
    TrustedObservation {
        source: ObservationRef,
        harness: HarnessId,
    },
    ServiceInternal {
        operation: OperationId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdmittedAuthorityV1 {
    actor: Option<ActorRef>,
    kind: AuthorityKindV1,
}

#[derive(Clone, Debug)]
pub struct ActionAdmissionRequestV1 {
    pub action: ActionClass,
    pub header: CommandHeader,
    pub command_digest: CommandDigest,
    pub payload_digest: PayloadDigest,
    pub basis: Option<BasisRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionHookDescriptorV1 {
    pub id: CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub affected_records: Vec<RecordFamily>,
    pub affected_indexes: Vec<IndexFamily>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAdmissionObservationV1 {
    pub hook_id: CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub admission_id: AdmissionId,
    pub payload: Vec<u8>,
    pub payload_digest: PayloadDigest,
}

pub trait Schema1ActionAdmissionReplay: Send + Sync {
    fn descriptor(&self) -> &AdmissionHookDescriptorV1;

    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequestV1,
        observation: &ActionAdmissionObservationV1,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

pub struct ReplayContext<'a> {
    // Private references supplied by the composition root.
}

pub struct ReplayProviders<'a> {
    pub schema1_admission: Option<&'a dyn Schema1ActionAdmissionReplay>,
    pub action_impact: Option<&'a dyn ActionImpactProvider>,
    pub action_admission: Option<&'a dyn ActionAdmissionProvider>,
    pub basis: Option<&'a dyn BasisProvider>,
    pub affected_scope: Option<&'a dyn AffectedScopeProvider>,
    pub affected_jobs: Option<&'a dyn AffectedJobProvider>,
}

impl<'a> ReplayContext<'a> {
    pub fn new(
        schema1_cells: &'a CellSet,
        schema2_cells: &'a CellSet,
        records: &'a RecordSet,
        providers: ReplayProviders<'a>,
    ) -> Result<Self, ZapError>;
}

pub fn replay_decoded_logical_event(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &DecodedLogicalEvent,
) -> Result<ReplayedTransition, ZapError>;

pub fn replay_logical_event_v1(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &LogicalEventV1,
) -> Result<ReplayedTransition, ZapError>;

pub fn replay_logical_event_v2(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &LogicalEventV2,
) -> Result<ReplayedTransition, ZapError>;
```

R13D owns the history scan/index projection and supplies the appropriate frozen
and current cell sets to this narrow core API. R07 does not add a history query
or index.

Schema-1 replay runs only the frozen schema-1 decoder, authority representation,
cell reducer and admission replay adapter. Its current event output and mutation
batch must remain byte/meaning equivalent. It never manufactures impact,
preflight or action-outcome fields and never writes a schema-1 event.

Schema-2 replay validates the chain/store/revision/header/digests, strictly
decodes the registered schema-2 payload, then reruns the same pure basis,
impact, cell-preflight, admission-needs, selected-effect simulation, closure,
actual-job, independence and safe-job derivations against the historical
pre-state. It calls `admit`, compares the newly derived observation and
preflight records with the stored evidence, calls `apply`, reruns the real
product reducer, recomputes the actual mutation/after-basis outcome, calls
`verify_after`, and compares output and prepared batches. Serialized prepared
mutations or serialized simulation results are never replay authority.

`replay_decoded_logical_event` is an exhaustive enum dispatch. The only
raw-byte entry is `decode_logical_event`; therefore an unsupported schema is
rejected before selecting cells, and schema-1 bytes cannot accidentally enter
the schema-2 reducer.

## R07 domain bindings

R07 supplies `DomainActionImpactProvider`,
`DomainAffectedScopeProvider`, the R07 `EffectContract` implementations, and
the typed payload adapters. The composition root fixes those providers on
`CommitServiceBuilder`; no cell constructs or swaps one.

Every existing privileged cell moves to `CellRegistrationBuilder`. There is no
default or name-based class. The registration audit must account for every
privileged descriptor:

- initial charter-bound intent and outcome adoption use
  `InitialBaselineOrSemantic`; the provider returns initial baseline only while
  that exact baseline component and the economics baseline boundary do not yet
  exist;
- ordinary work lifecycle advancement, dispatch and revalidation readiness use
  `Progress` when their typed target transition is genuinely progress;
- evidence adjudication, current-proof use, stage/work/integration acceptance
  and campaign close use `Proof` when their typed operation only records or
  consumes proof; and
- task-contract/dependency/ownership/obligation changes, renames, lowering,
  deferral/disposition changes, semantic knowledge changes, adaptive apply and
  any post-baseline intent/outcome change use `SemanticChange`.

An operation type capable of more than one meaning chooses the code-defined
`ActionImpactRule` from its decoded enum/fields, then the state provider checks
that choice against the current records. A registration cannot use `Progress`
as a catch-all for an unrecognized variant. Pure packet/view rendering remains
non-privileged and has no impact adapter.

The economics records gain only the bindings needed by these seams:

```rust
// In ChangeEffect:
pub preflight_digest: Option<EffectPreflightDigest>;

// In ChangeAssessmentRecord:
pub scope_roots: Vec<SubjectRef>;
pub scope_direct_work_ids: Vec<WorkId>;
pub affected_scope_digest: Option<AffectedScopeDigest>;

// In ChangeHoldRecord:
pub scope_roots: Vec<SubjectRef>;
pub scope_direct_work_ids: Vec<WorkId>;
pub affected_scope_digest: AffectedScopeDigest;

// In ChangeAdmissionRecord:
pub impact_digest: ActionImpactDigest;
pub effect_preflight_digest: EffectPreflightDigest;

// In OwnerChangeDecisionRecord, ordered like the selected alternative:
pub effect_preflight_digests: Vec<EffectPreflightDigest>;
```

An assessment proposal requires every feasible effect's
`preflight_digest == None` and `affected_scope_digest == None`. The
state-capable adapters on `ChangeAssessmentAdjudicatedCell` load the exact
assessment by ID, build one effect bundle per alternative and one affected
scope request from its effect roots. Core simulates the alternatives
independently and derives closure/jobs. The cell requires a one-to-one result
for every alternative, compares the caller's affected/dependent/subject/unknown
claims with the derived view, writes each derived preflight digest and the
affected-scope digest, and copies only the actual derived job IDs into a new
hold's drain set.

Automatic adjudication and Owner decisions both bind the ordered preflight
digests. Existing ordered effect fingerprints remain envelope evidence; they
do not substitute for handler-aware preflight.

This ID-only cell never trusts a caller-supplied serialized assessment or an ID
matched in an untyped scan. Its registered extractor uses
`get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)` and refuses when
the exact keyed record is absent.

`ChangeHoldRecord.independent_effect_fingerprints` is retired from live guard
decisions. Independence is recomputed from current state for the candidate
command and carried only by the opaque transaction witness. Hold records keep
the roots and scope digest so resolution can rederive the closure and discover
any newly visible job. `drain_job_ids` remains durable work-to-drain evidence,
but it is populated only from the complete `AffectedJobView` and is not itself
proof of current safety.

`ChangeAssessmentAdjudicatedCell` registers the state-capable effect-bundle and
affected-scope adapters. `ChangeHoldResolvedCell` registers a
`PayloadSafeJobs` adapter which loads the exact hold and constructs
`SafeJobRequest` from its stored roots/digest. Resolution requires
`command.safe_jobs_for(hold_id)`, exact equality between payload safe IDs and
the witness's current IDs, no unknown effect, the exact committed prefix and
latest adjudicated forecast/Owner decision. Thus all creation, resolution,
guard and drain inputs originate from complete actual affected-job views.

### ChangeControlAdmissionProvider behavior

`needs` branches on the transaction-derived impact:

- for an exempt impact it returns no selected effect, but still requests the
  affected scope/independence checks required by active holds;
- for a semantic impact it resolves exactly one current unapplied
  `ChangeAdmissionRecord` by product event ID, action, payload and relevant
  business basis, resolves its exact assessment/forecast/Owner decision and
  selected `ChangeEffect`, then returns that effect as
  `selected_effect` together with a request for every active hold that could
  apply.

The selected effect's stored `effect_preflight_digest` must match the newly
derived bundle digest. Its kind, event ID and payload digest must match the live
command. The effect prefix, predecessor closure, assessment, forecast, policy,
Owner decision and relevant-before basis remain exact.

`admit` first enforces general pause and every active hold. For a permitted
initial-baseline/progress/proof command it emits `Exempt`; if an active pause is
covered by exactly one current, command-bound, unconsumed CONTROL exception,
the provider records that exception version for atomic consumption. It emits
`Economic` only for the exact semantic admission and selected preflight.

`apply` on `Exempt` may replace only that CONTROL exception as consumed; it
does not read or mutate an economics admission. `apply` on `Economic` advances
the ordered admission prefix, hold and optional exception exactly as today.
`verify_after` requires the observation basis, stored admission impact and
preflight, current product outcome, final-effect flag and derived after-basis to
agree. Core has already compared actual and simulated mutation digests; the
provider cannot waive that comparison.

`ChangeAdmissionRecord.command_id` remains the exact intended command identity;
its schema-2 `command_digest` field is removed. Live matching uses command ID,
product event ID, `ActionImpactDigest`, payload digest, action,
`EffectPreflightDigest` and relevant business basis. This allows an unrelated
committed transport event to advance the global revision and the command frame
to be rebuilt at that current revision without cancelling an otherwise
unchanged semantic approval. Exact retry of an already committed command still
uses the event's real command ID/digest in core.

## File ownership for implementation

R07 core API ownership is bounded to:

- `zap-wire/src/digest.rs` and `zap-wire/src/lib.rs` for the six digest
  newtypes;
- `zap-core/src/admission/{mod.rs,impact.rs}`, `effects.rs`,
  `execution_views/affected_scope.rs`, `transition.rs`, `authority.rs`,
  `trust.rs`, `commit.rs`, `change.rs`, `execution_views/mod.rs`, and `lib.rs`
  for the DTOs, typed registration, sealed proposal issuance, overlay/digest,
  commit and replay entry points;
- `zap-domain/src/economics/**` plus the narrow privileged cell registrations
  and domain provider composition required to implement the R07 adapters; and
- R07-focused service/contract tests and the R07 report/checkpoint.

The R07 recovery implementation owner retains ownership of forecast/hold/completion/policy
behavior behind this API. Coordinate shared economics files rather than
duplicating that work. R13D retains history/index projections, schema-1 frozen
registry assembly and narrow history read ports. R07 supplies the event DTO and
replay dispatch contract those components call. R08 packet/bundle/return
resolution is outside this document.

## Required acceptance sequence

Acceptance uses a real redb store, the composed production `RecordSet`,
`CellSet`, routes, trust bootstrap, basis/impact/admission/affected/job
providers and completion evaluator. R07 records are created through their real
commands. A test-only cell is acceptable only for the deliberately mismatched
effect-contract/product-kernel negative case.

1. **Registered classification.** Build fails when any privileged cell lacks
   its impact adapter, when a non-privileged cell registers one, or when the
   conservative R07 provider set is incomplete. Obtain a DataProposal grant
   only through the registrar-issued handle; wrong store/base/campaign, kind or
   changed frame refuses, and no caller label grants authority. Through the real
   service, adopt the exact initial charter-bound intent/outcome without an
   economics admission and observe `Exempt(InitialBaseline)`. Repeat a
   post-baseline semantic adoption and observe refusal without economics.
   Execute representative progress and proof commands and observe `Exempt`
   while their ordinary authority, basis, completion and control checks still
   run.
2. **State-capable adjudication and independent alternatives.** Submit an
   assessment proposal through its actual route. Then submit the ID-only
   `ChangeAssessmentAdjudicated`; its extractor must load the stored typed
   assessment. Give alternatives A and B effects whose state results would
   conflict if B inherited A's overlay. Both pass only when each begins at the
   same pre-state, while effects A1/A2 within one branch chain normally.
   Malformed product bytes, an unregistered kind/contract, wrong subjects or a
   wrong declared after-basis refuse before adjudication/hold creation.
3. **Authoritative closure and jobs.** Create work A, a known dependent B and
   current execution observations for both. An assessment claiming only A, or
   supplying empty affected/dependent IDs, refuses. With claims equal to the
   derived closure, adjudication creates one hold whose affected/dependent sets,
   scope digest and drain jobs exactly match the derived scope and complete
   actual job view.
4. **Hold guard and independence.** A start or incompatible mutation for A or B
   is held. With an incomplete boundary, unrelated work C is also held until
   the provider returns a positive witness bound to that hold, C's exact current
   closure and basis. C then proceeds. No serialized digest/fingerprint or
   witness from another hold, candidate, revision or basis clears it. Owner
   pause still blocks C even with a valid independence witness.
5. **Actual selected-effect need.** Prepare a real assessment, forecast/Owner
   decision when required and admission. The admission provider's needs record
   contains the actual selected effect; omitting it, returning another branch,
   wrong prefix, kind, event ID or payload refuses. Independence-only needs can
   never admit `SemanticChange`.
6. **Current simulation versus product.** Execute the selected semantic product
   command. Core strictly decodes it through the registered handler, simulates
   it, runs the authorized product reducer in the same transaction, and proves
   exact `EffectMutationDigest` and relevant-after equality. A deliberately
   divergent pure kernel, altered payload/scope, or wrong after-basis leaves
   admission, hold, exception, product records, event log and head unchanged.
7. **Transport churn versus business drift.** Prepare a semantic approval, then
   commit an unrelated event which advances the store/head revision but changes
   none of the effect's relevant business fingerprints. Submit the same product
   event/payload at the new expected revision: its stable impact/preflight
   bindings remain valid and it commits. In a sibling case, change a relevant
   subject, dependency, contract, policy or proof version; the command refuses
   pending re-adjudication.
8. **Exempt control exception and atomic rollback.** Activate a general Owner
   pause and show an exempt progress/proof action is blocked. Grant one exact
   CONTROL exception. Make the product reducer fail after the hook buffers
   exception consumption; reopen and prove the exception, product and head are
   unchanged. Execute successfully and prove product plus exception consumption
   commit together. Exact retry returns the receipt without consuming again;
   reuse for another command refuses.
9. **Safe resolution from actual jobs.** Attempt hold resolution with caller
   `safe_job_ids = []` while the derived current view contains a job; it refuses.
   Add a newly visible dependent/current job after hold creation and prove the
   rederived scope changes and forces re-adjudication. Once the exact current
   closure is unchanged and every complete actual job is safe, the sealed
   witness permits resolution only with the exact job list, committed prefix,
   latest adjudicated forecast/decision and no unknown effect.
10. **Economics recovery cases.** In the same registered service journey, run
    the R07 recovery matrix: below/exactly/above four hours, every configured
    unknown-cost mode, a linear cumulative forecast crossing the threshold,
    reset/fork rejection, hold lineage, rejection before and after a committed
    prefix, and both direct-close/runtime completion providers. These are
    the R07 recovery implementation owner's responsibility but remain part of joint
    R07 acceptance.
11. **Schema and audit.** Confirm every new commit encodes strict schema 2.
    Close/reopen the store and replay it through `decode_logical_event` plus the
    schema-2 registry, rederiving impact, preflight, hook and product outcome.
    Replay a fixed real schema-1 fixture only through the frozen schema-1
    registry and compare its output/mutations. A schema 0/3 event returns
    `UnsupportedEpoch`; a schema-1 body with v2 fields or a v2 body missing them
    refuses rather than being reinterpreted.
12. **Failure, retry and restart boundary.** For both an exempt action and an
    economics-admitted effect, force product failure after hook preparation and
    prove zero durable mutation. Then commit, exact-retry, restart, reconcile,
    and audit the same effect. The applied prefix advances once, the one-use
    exception advances at most once, and the event/head/record indexes agree.

Run the focused crate gates after the journey (`cargo test -p zap-core`,
`cargo test -p zap-domain`, and the affected zap-store replay tests), then the
repository's required Rust floor. Root reviews the registered paths and accepts
the result; a green DTO/unit suite alone is insufficient.
