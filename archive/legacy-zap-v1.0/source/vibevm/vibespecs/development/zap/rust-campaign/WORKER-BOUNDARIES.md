# ZAP Rust worker and implementation boundaries

Status: R01 accepted, API revision 7. This file fixes ownership, authority and
port direction before parallel implementation. It describes product agent
behavior; it does not start a campaign or grant a worker authority.

The matching normative R02 anchors are `AGENT-HOST-BRIDGE`,
`AGENT-INTENT-IS-NOT-LAUNCH`, `LOWERING-STRATEGY-SURVIVES`, and
`DREAMER-NONEXECUTABLE`. These names are stable integration references; this
document supplies Rust seams and does not redefine their requirement text.

## 1. Implementation tracks

The package has three concurrent implementation tracks and one composition
owner:

| Track | Owns | May consume | Must not edit |
| --- | --- | --- | --- |
| foundation/store/migration | workspace, `zap-wire`, generic `zap-core`, `zap-store`, `zap-legacy` | normative schemas and frozen legacy corpus | semantic/runtime cells or their entrypoints |
| semantic planning/control | cells under `zap-domain/src/{intent,control,knowledge,economics,lowering,dreamer,acceptance}` | `zap-wire`, `zap-core` ports | store/runtime/API implementation or composition |
| runtime/surfaces | cells under `zap-runtime`, `zap-api`, ordinary `zap-cli` commands | `zap-wire`, `zap-core` ports | semantic/store internals or composition |
| integrator | workspace dependency table, every crate `lib.rs`, `zap-app/src/composition.rs`, production capability profile | each track's whole exported cell/record/query sets | feature implementation owned by a track |

No worker edits another track's file to make its own code compile. It submits an
API-revision request containing the missing type or port, the consuming
signature, and the smallest compatible change. The integrator changes one
composition surface after reviewing both sides.

Feature crates export complete sets rather than individual global mutations:

```rust
pub fn cell_set() -> Result<CellSet, ZapError>;
pub fn record_set() -> Result<RecordSet, ZapError>;
pub fn query_set() -> Result<QuerySet, ZapError>;
pub fn capability_set() -> Result<CapabilitySet, ZapError>;
```

Empty sets are valid during R03 and advertise no behavior. Production startup
compares the composed sets with `RequiredCapabilities` and refuses readiness
when a required operation has no implementation.

## 2. Product roles are not authority

`WorkerRole` is the closed execution-responsibility enum `Senior | Middle |
Junior`. `PrincipalRole` is the authority enum `Reader | Worker | Coordinator |
Owner | TrustedHost`. Model/provider identity, worker role, principal authority,
and transport capability are stored separately.

Senior work may create architecture, specification, lowering and review
candidates. A Senior assignment has an empty production `write_subjects` set;
the service refuses any dispatch whose role capability and contract disagree.
Middle may change production subjects inside an accepted contract. Junior may
perform only task classes explicitly marked bounded and cheaply verifiable. No
producer role accepts its own candidate.

The Owner profile contains desired model/effort values. A captured host
capability produces a distinct resolved profile. Unsupported desired values
remain visible and produce a capability wait; adapters do not silently replace
them. An on-prem profile may resolve every role to the same model while keeping
the role and acceptance boundaries.

## 3. Cross-track application ports

`zap-core` owns only these object-safe application ports:

```rust
pub trait CampaignReadPort: Send + Sync {
    fn snapshot(&self, at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError>;
    fn frontier(&self, request: FrontierRequest)
        -> Result<Page<FrontierWorkView>, ZapError>;
    fn work_execution_view(&self, work: &WorkId, at: ReadAt)
        -> Result<WorkExecutionView, ZapError>;
    fn explain_readiness(&self, work: &WorkId, at: ReadAt)
        -> Result<ReadinessView, ZapError>;
    fn completion_view(&self, at: ReadAt) -> Result<CompletionView, ZapError>;
}

pub trait CommandPort: Send + Sync {
    fn submit(
        &self,
        principal: PrincipalContext<'_>,
        command: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError>;
    fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError>;
}
```

The frontier/runtime DTOs are owned by `zap-core`, not by `zap-domain`:

```rust
pub struct FrontierRequest {
    pub at: ReadAt,
    pub after: Option<PageCursor>,
    pub limit: PageLimit,
}

pub struct FrontierWorkView {
    pub work_id: WorkId,
    pub order: u64,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub required_stage: MaturityStage,
    pub obligation_ids: Vec<ObligationId>,
    pub integration_owner: IntegrationOwner,
    pub relevant_basis: RelevantBasisDigest,
}

pub struct WorkExecutionView {
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub title: BoundedText<1024>,
    pub goal: BoundedText<8192>,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceClaim>,
    pub steps: Vec<BoundedText<4096>>,
    pub positive_cases: Vec<AcceptanceCriterion>,
    pub negative_cases: Vec<AcceptanceCriterion>,
    pub checks: Vec<VerificationPlan>,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub safe_stop: SafeStopContract,
    pub integration_owner: IntegrationOwner,
    pub delivery_route: DeliveryRoute,
    pub required_stage: MaturityStage,
    pub sources: Vec<SourceFingerprint>,
    pub obligation_ids: Vec<ObligationId>,
    pub relevant_basis: RelevantBasisDigest,
}
```

Constructors sort/deduplicate typed sets, require a current contract and at
least one active obligation, validate the delivery route/stage, and bind the
same digest/version/generation used by dispatch and evidence. `frontier`
returns only structurally ready current work and explicit completeness; it is
not dispatch authority. Runtime then applies its own reservation, hold, pause,
capacity and host checks. `work_execution_view` returns the complete bounded
packet input for one selected work item. Runtime never imports a
`zap-domain::StoredRecord`, downcasts a domain type, or reconstructs a contract
from generic JSON.

`ReadinessView` reports typed blockers, active pause/hold IDs, unresolved
effects, missing evidence and its relevant-basis digest. `CompletionView`
reports every active obligation, required integration/deferral/promotion/final
gate, pending selected change or Owner decision, active hold/pause and unknown
effect. Its `ready` value is derived from one shared pure predicate. Runtime and
direct close both consume this view; neither duplicates closure logic.

The current economics implementation remains candidate evidence. The Rust
contract intentionally carries generic completion blockers so the open closure
P1 can be corrected without making an unaccepted Python module authoritative.

## 4. SemanticProvider and AgentHost are different ports

`SemanticProvider` obtains bounded meaning-bearing proposals. `AgentHost`
manages executable workers. Neither trait authorizes or commits product state.

```rust
pub trait SemanticProvider: Send + Sync {
    fn capabilities(&self) -> SemanticCapabilities;
    fn submit(&self, intent: SemanticIntent) -> Result<SemanticReceipt, ZapError>;
    fn observe(&self, receipt: &SemanticReceipt) -> Result<SemanticObservation, ZapError>;
    fn request_stop(&self, receipt: &SemanticReceipt, stop: StopRequest)
        -> Result<StopReceipt, ZapError>;
}

pub trait AgentHost: Send + Sync {
    fn capabilities(&self) -> AgentCapabilities;
    fn dispatch(&self, intent: DispatchIntent) -> Result<DispatchReceipt, ZapError>;
    fn observe(&self, handle: &ExternalJobHandle) -> Result<JobObservation, ZapError>;
    fn collect(&self, handle: &ExternalJobHandle) -> Result<CandidateResult, ZapError>;
    fn request_stop(&self, handle: &ExternalJobHandle, stop: StopRequest)
        -> Result<StopReceipt, ZapError>;
    fn reconcile(&self, intent: &DispatchIntent, known: Option<&DispatchReceipt>)
        -> Result<ReconciliationObservation, ZapError>;
}
```

The semantic request family is closed and typed:

```rust
pub struct SemanticIntent {
    pub request_id: SemanticRequestId,
    pub campaign_id: CampaignId,
    pub kind: SemanticKind,
    pub relevant_basis: RelevantBasisDigest,
    pub capability_digest: CapabilityDigest,
    pub input: SemanticInput,
    pub allowed_output: SemanticOutputContract,
    pub request_digest: SemanticRequestDigest,
}

pub enum SemanticKind {
    Selection, Review, Reassessment, Acceptance, Closure,
    Lowering, Abstraction, ChangeEstimation,
}

pub enum SemanticInput {
    Selection(SelectionContext), Review(ReviewContext),
    Reassessment(ReassessmentContext), Acceptance(AcceptanceContext),
    Closure(CompletionView), Lowering(LoweringRequest),
    Abstraction(AbstractionRequest), ChangeEstimation(ChangeEstimationRequest),
}

pub enum SemanticCandidate {
    SelectedWork(WorkId), DomainProposal(DomainProposal),
    LoweringProposal(LoweringRevision), AbstractionProposal(AbstractionResult),
    ChangeEstimate(ChangeAssessment), NeedsEvidence(EvidenceRequest),
    Wait(ResourceWait), NoChange(NoChangeAssessment),
}
```

Constructors require `kind`, input variant and output contract to agree. A
selection emits no mutation. Review/reassessment may emit only registered data
proposals. Acceptance may propose evidence/stage/integration/work acceptance,
which still passes central gates. Closure receives the shared current
`CompletionView` and may propose only its allowed successful classification;
an ineligible view cannot construct a closure request. Control activation,
Owner decision, pause/resume, internal admission and trusted observation are
absent from every semantic output contract.

`SemanticObservation` binds request ID/digest, capability digest, relevant
basis, provider state, sanitized candidate and raw-response artifact digest.
Provider success does not imply application. A stale response remains evidence
and is not silently rebound unless recomputation proves the same relevant
basis. Raw provider text is private artifact data, never a reducer payload.

All arguments and results are closed typed records. `ExternalJobHandle` is an
opaque, adapter-scoped value bound to `HarnessId`, adapter version, campaign,
job, attempt and dispatch-intent digest. It is not a command line or trusted
principal. Provider output is a candidate until a typed transition cell and
central acceptance admit it.

A standalone Rust process cannot invoke tools that exist only inside an
enclosing harness session. For a native host, the runtime first commits a
`DispatchIntent`; a harness driver performs the native tool call and submits a
bound `DispatchReceipt`. Intent without receipt is not launch. After interruption
the adapter reconciles the same intent/handle before any retry. A subprocess
adapter is a separate explicit `AgentHost` and is never reported or selected as
native fallback.

## 5. Context, packets and result boundary

Every assignment binds campaign/store/base, strategic and lowering revisions,
contract version/digest, relevant basis, role, desired and resolved profile,
read/write subjects, resources, integration owner, sources, checks, safe stop,
known forks, packet lineage and explicit unloaded context. Required fragments
are deduplicated by content digest. The packet builder refuses an over-budget
required set; it does not truncate it or load the full boot lane implicitly.

A result distinguishes transport completion, durable checkpoint, candidate
artifacts, verification observations, semantic acceptance and safe state. The
runtime persists the dispatch intent, receipt, handle, liveness state, useful
checkpoint, pending effects and next admissible operation. A context compact,
process restart or account change reconstructs from those records and never
creates a new logical attempt merely because narration was lost.

Heartbeats are coalesced operational observations. Meaningful state changes and
useful checkpoints become logical events; repeated unchanged liveness does not
inflate semantic history.

## 6. Foundation release to R03

R03 may create the workspace and shared wire/error/registry/port skeleton from
`RUST-API.md`. It owns no semantic or runtime claim. The integrator keeps empty
sets lawful and capabilities honest. RUST-API revision 7 resolves the R04
storage boundary: generic backend reads stay monomorphized, reducers use the
object-safe typed-record adapter, and only a transaction-bound
`ValidatedCommitIntent` can reach the store's private event/idempotency tables.
R03 need not implement durable behavior to expose those compile-time seams.
