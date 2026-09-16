# R08 executable lowering and packet-resolution contract

Status: complete bounded candidate for root acceptance (2026-09-13).

## Ownership boundary

R08 owns one live semantic lowering operation that atomically binds the exact
current strategy and lowering to the executable `WorkRecord` and
`TaskContractRecord` graph, and one app-composed packet-resolution seam that
derives a `RuntimeJobClaim` from authoritative stored state.

The accepted R07 impact, effect-preflight, affected-closure and sealed
DataProposal APIs are dependencies. This document does not redefine them.
R13D owns history/index ports and legacy mapping. R13 owns client transport.
R15 owns physical source/archive capture. Those adapters may carry or persist
validated results; they cannot supply missing domain lineage, authority,
contract, debt, capability, source, rule or fork facts.

Bundle export, return import and reassessment are deliberately absent. They are
the next separate design unit.

## Fixed outcomes

1. `planning.lowering-applied` is the sole live graph-materialization route.
   Its product `ChangeSet` creates or exactly binds strategy, lowering, work,
   contracts, obligation coverage, stage debt and origin links atomically.
2. The old `domain.plan-lowered` event remains decodable only in the frozen
   schema-1 replay registry. Live execution rejects it before authority or
   mutation, so it cannot create an orphan executable graph.
3. Successor, inapplicability, stage discharge and deferral claims resolve from
   the current charter/outcome and `CurrentProofSet`. A parseable ID or
   authorization reference is never evidence by itself.
4. A packet is executable only when core derives a complete current resolution
   covering identity, contract, debt, role, capability, sources, rules, fork
   projection and typed candidate-only result protocol.
5. `PacketResolutionProvider` is composed in the app because it joins domain,
   runtime/capability and source/fork adapters. Callers submit only a packet ID;
   the service command adds only freshly generated job/attempt/dispatch/effect
   IDs. They cannot resubmit or override producer, work, contract, role or other
   semantic identity labels.
6. Legacy-imported draft/unlowered work and contract rows remain readable for
   recovery. They are unexecutable until a current lowering and packet
   resolution binds their exact origin and versions.

## Atomic lowering payload and records

`planning.lowering-applied` advances its payload schema and becomes the one
live graph-materialization command:

```rust
schema_tag!(LoweringAppliedSchema, "zap-planning/lowering-applied/2");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoweringApplied {
    pub schema: LoweringAppliedSchema,
    pub strategy_id: StrategicRevisionId,
    pub expected_strategy_revision: Revision,
    pub lowering: LoweringRecord,
    pub graph: LoweredGraph,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoweredGraph {
    pub parent_id: WorkId,
    pub nodes: Vec<WorkRecord>,
    pub coverage: Vec<ObligationAssignment>,
    pub contracts: Vec<TaskContractRecord>,
    pub integration_owner: WorkId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningRevisionState {
    Candidate,
    Current,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSourceBinding {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoweredWorkBinding {
    pub work_id: WorkId,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub execution: LoweredNodeExecution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoweredNodeExecution {
    Container,
    Executable {
        contract_id: ContractId,
        contract_version: Revision,
        contract_digest: ContractDigest,
        validation_generation: u64,
        resource_claims: Vec<ResourceClaim>,
        verification: Vec<VerificationPlan>,
        rules: Vec<RuleSourceBinding>,
        candidate_result: CandidateResultTemplate,
    },
}
```

`StrategicPlanRecord` and `LoweringRecord` add
`state: PlanningRevisionState`. `StrategyProposedCell` accepts only
`Candidate`. `LoweringRecord.work` becomes `Vec<LoweredWorkBinding>`; the
executable contract body lives in the current `TaskContractRecord`, so it is
not duplicated inside the lowering.

The binding is the origin link. Its containing lowering supplies lowering ID,
strategy ID and semantic digest; the current strategy supplies outcome and
intent. Its parent/dependency fields must equal the current `WorkRecord`.
An executable leaf's contract/version/digest and validation generation must
equal the current `TaskContractRecord` and work exactly. A work row
not named by an exact binding in a `Current` lowering whose strategy is also
`Current` has no executable origin.

`LoweredGraph` is canonicalized by work ID, obligation ID and contract ID.
Duplicate rows refuse. `parent_id` must equal `lowering.target`. Every node is
`Planned`, has no active job, has a parent in the submitted graph or the exact
parent, and depends only on an earlier acyclic submitted node or an already
executable current work item. Every leaf has exactly one active contract and an
`Executable` binding; non-leaves have no contract and use `Container`. Each
binding matches one node, and each node has exactly one binding.

Executable resource claims are sorted, positive and cover the task contract's
resource IDs exactly once; unit counts are lowering semantics and therefore
part of the R07-admitted effect rather than packet-render caller data.

The operation supports materialization and exact binding without a caller mode:

- an absent node/contract is inserted with the command's next revision and the
  supplied semantic contents;
- an existing row with no current origin is bindable only when it is still
  `Planned`, has no active job, and every semantic field plus its actual
  revision/version equals the submitted row;
- ordinary re-lowering may retain the same exact rows when their sole current
  origin is the exact `lowering.previous` for this strategy/target, that
  predecessor is superseded in this transaction, the rows remain `Planned`
  with no active job/candidate, and every semantic/version/contract binding is
  unchanged; and
- a conflicting, partially matching, active, unresolved-current-candidate or
  foreign-origin row refuses unless the R09 amendment below applies. The kernel
  never patches an imported draft until it happens to fit and never steals a
  Work ID from another current lowering.

An imported inactive task contract has a separate lawful activation path. The
kernel never treats `active: false` as executable and never parses legacy check
strings into argv. When no active contract exists for the work, the lowering
may either:

- compare-and-replace the exact inactive contract ID/version with a supplied
  `active: true` canonical contract for the same work, requiring the new version
  to be the checked successor and recomputing `contract_digest` from the new
  `TaskContract`; or
- insert a new active contract identity/version for the same work and retain
  the inactive legacy contract as evidence.

In both cases the new contract must pass the full current contract validator,
the `LoweredNodeExecution::Executable` binding must name its exact new
ID/version/digest, and its structured verification plans, rule/source bindings
and candidate template are supplied as admitted lowering semantics. The
`LegacyTaskConstraintRecord` and raw source/constraint bytes remain unchanged
and queryable; they may be cited as sources but cannot supply executable argv or
authority. If no truthful current contract can be authored, the draft stays
unexecutable or the lowering uses a replacement Work ID.

This lets R13D preserve legacy-mapped draft rows without inventing origin. They
remain queryable by the existing record/query paths. Readiness adds the typed
blocker `ReadinessBlocker::MissingLoweringOrigin { work_id }`, and packet/job
resolution refuses them. A later authorized lowering can bind an exact draft or
materialize a replacement through this one atomic operation.

## R09 amendment: causal semantic re-lowering of existing Work IDs

The unchanged-row rule above governs ordinary re-lowering. A return-caused,
R07-admitted semantic lowering may lawfully change the same Work IDs under the
exact current predecessor when it carries R09's applied reassessment and exact
prestate CAS:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReloweringWorkPrecondition {
    pub work_id: WorkId,
    pub expected_work_revision: Revision,
    pub expected_state: WorkState,
    pub expected_validation_generation: u64,
    pub expected_contract_id: Option<ContractId>,
    pub expected_contract_version: Option<Revision>,
    pub expected_contract_digest: Option<ContractDigest>,
    pub current_job_id: Option<JobId>,
    pub current_candidate_id: Option<CandidateId>,
}
```

The R09 reassessment sidecar owns the sorted precondition set; the lowering's
`ReassessmentBinding` hashes and resolves that sidecar. For each changed work,
the kernel requires the exact predecessor lowering origin and every precondition
to match current state. The applied `AdaptiveReviewRecord` must contain the
corresponding `ReviewWorkChange` and, for every current/recent job, an exact
`JobReconciliationPlan` whose action and observed safe/effect state resolve the
candidate or live effect.

A resolved semantic update keeps the Work ID, writes a checked-next work
revision, clears `active_job`, returns the new executable version to `Planned`,
and increments `validation_generation` exactly once. A changed contract either
compare-and-replaces the exact active contract at checked-next version or marks
it inactive and inserts the explicitly bound replacement; its digest is
recomputed from the canonical new contract. R07 effect simulation, actual
mutation comparison, relevant after-basis and affected/dependent closure cover
this entire batch.

Prior `RuntimeJobRecord`, `CandidateResultRecord`,
`CandidateProvenanceRecord`, evidence, packets and acceptances are never deleted
or rewritten. The generation/contract/basis change makes old proof inapplicable
to the new execution version through the existing `CurrentProofSet` rules while
preserving the historical fact. A current candidate is allowed only when the
review explicitly preserves it as historical input, requests revalidation, or
supersedes/drops its old route. A starting/running/unknown effect must first be
finished compatibly, drained, preserved at a safe boundary, or reconciled with
affirmative no-effect evidence exactly as the review says.

An unresolved current candidate, missing reconciliation row, unsafe/unknown
effect, CAS mismatch, out-of-scope semantic change, or origin other than the
exact predecessor refuses. Historical candidates alone do not block future
lowering and a provider/model retry does not create a new approach or reset a
counter.

Amendment acceptance uses a real predecessor lowering, packet, R11 job and
candidate, then applies an R09 review that explicitly revalidates that work and
reconciles its terminal effect. The second lowering names the predecessor,
reuses the same Work ID, supplies exact work/contract/job/candidate CAS, changes
the affected contract semantics, bumps work revision/contract version/
validation generation once, and commits through R07 preflight. The old job,
candidate/provenance and proof remain readable but are not current for the new
generation. Sibling cases with a stale CAS, foreign origin, missing candidate
disposition, unresolved running/unknown effect, no generation bump or changed
out-of-scope row refuse with zero mutation.

## Authoritative conservation and stage debt

Lowering retains caller-visible traces, but replaces unresolvable labels with
typed current-state bindings:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeDispositionBinding {
    pub charter_id: CharterId,
    pub charter_revision: Revision,
    pub charter_digest: PayloadDigest,
    pub outcome_id: OutcomeId,
    pub outcome_revision: Revision,
    pub disposition: ObligationDisposition,
    pub successor_ids: Vec<ObligationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObligationRoute {
    Active,
    Successor {
        binding: OutcomeDispositionBinding,
    },
    Inapplicable {
        binding: OutcomeDispositionBinding,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageDebtDisposition {
    Required,
    Accepted {
        acceptance_id: StageAcceptanceId,
    },
    Deferred {
        deferral_id: DeferralId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeferralRoute {
    Retained,
    Transferred {
        work_ids: Vec<WorkId>,
    },
    Inapplicable {
        outcome_id: OutcomeId,
        deferral_revision: Revision,
    },
}
```

The kernel loads exactly one active charter, its active intent and outcome, and
requires the strategy to name that intent/outcome. It derives the complete
strategy-obligation denominator from current `ObligationRecord`s plus the
active outcome's disposition rows. Every strategy obligation appears once:

- `Active` requires a current active obligation for this outcome and nonempty
  implementation, verification and integration owners in `LoweredGraph`;
- `Successor` requires the current outcome disposition, predecessor record and
  `binding.successor_ids` to agree exactly, and every successor to exist,
  remain active for the same current outcome and be allowed by the current
  charter; and
- `Inapplicable` requires an exact current `Excluded` or `Unattainable`
  disposition in both outcome and obligation records, permitted by the current
  charter's mutable-obligation and allowed-disposition sets. A free
  `AuthorizationRef` is removed from this path.

Lowering cannot create a successor or inapplicability decision. If current
records do not already carry the R07-admitted semantic disposition, lowering
refuses. For active obligations the kernel replaces owners with the exact
canonical coverage roles in the same product `ChangeSet`; it never silently
drops an owner or obligation.

For every executable work delivery stage, `Required` represents work still to
perform. `Accepted` loads the exact `StageAcceptanceRecord`, requires matching
work, validation generation, stage, active outcome and covered obligations, and
requires every evidence ID in that acceptance to be present in the transaction's
`CurrentProofSet`. `Deferred` loads an exact open `DeferralRecord` for the active
outcome whose work/obligation scope covers that stage's work and obligations.
An `EvidenceId`, `StageAcceptanceId` or `DeferralId` that merely parses has no
effect.

The deferral denominator contains exactly the current outcome's records whose
work or obligation scope intersects this lowering. `Retained`, `Transferred`
and `Inapplicable` must match the current record status, revision and exact
work/obligation scope. The lowering does not synthesize a transfer or
inapplicability from prose; any required deferral mutation precedes it through
the existing R07-admitted control route.

## One lowering kernel and one commit

Both the authorized transition and the accepted R07 effect contract call one
pure kernel:

```rust
pub(crate) struct LoweringKernelContext {
    pub next_revision: Revision,
    pub relevant_basis: RelevantBasisDigest,
}

pub(crate) fn apply_lowering_kernel(
    state: &dyn StateReader,
    context: &LoweringKernelContext,
    payload: &LoweringApplied,
    current_proofs: &CurrentProofSet,
    changes: &mut ChangeSet,
) -> Result<(), ZapError>;
```

`LoweringAppliedCell` registers through the accepted R07 builder with
`LoweringBasisScope`, `LoweringActionImpact` and its `EffectContract`.
`LoweringActionImpact` emits
`InitialBaselineOrSemantic { baseline_subject:
SubjectRef::Work(lowering.target.clone()) }`; the
accepted state provider may derive `InitialBaseline` only for the exact first
charter-bound baseline lowering, and otherwise derives `SemanticChange`.
Baseline lowering therefore uses R07 `Exempt` while still enforcing control;
all later lowering requires the exact economics-selected effect. Its
descriptor declares `StrategicPlanRecord`, `LoweringRecord`, `WorkRecord`,
`TaskContractRecord`, `ObligationRecord` and `WorkerPacketRecord`. Simulation and authorized apply
therefore cover exactly the same graph/status/coverage batch and are compared
by R07's current-transaction `EffectMutationDigest` and after-basis check.

The kernel performs these ordered changes in its one `ChangeSet`:

1. Validate the exact current charter/intent/outcome, relevant basis,
   `strategy_id`, expected strategy revision, lowering lineage, graph, current
   proof set, dispositions, stage debt and outcome-scoped deferrals.
2. If the strategy is `Candidate`, promote it to `Current`, supersede the prior
   current strategy for that outcome and supersede all current lowerings tied
   to that prior strategy. If it is already `Current`, require exact identity;
   `Superseded` refuses.
3. Insert absent graph rows, bind exact unoriginated drafts, rebind unchanged
   predecessor-owned rows, or apply the R09 causal/CAS semantic update above;
   derive the canonical `LoweredWorkBinding`s and replace every active
   obligation's owners with the exact coverage roles. Supersede affected
   predecessor packets. An active or current-candidate row without the exact
   applied review reconciliation cannot be rebound.
4. Supersede the prior current lowering for the same strategy/target, require
   `lowering.previous` to name it exactly, and insert the finalized lowering as
   `Current` with those bindings.

Any read, duplicate-key, version, proof, scope, handler simulation or product
failure discards the R07 admission hook and this complete product set. There is
no intermediate state containing a current lowering without its executable
graph or a graph created without its lowering origin.

## Retire the graph-only live route

`domain.plan-lowered` is removed from `zap_domain::cell_set()` and its live
`RouteRegistry`. Its exact current payload, decoder and reducer move unchanged
under the frozen schema-1 replay registration consumed by R07/R13D:

```rust
pub(crate) fn schema1_control_cell_set() -> Result<CellSet, ZapError>;
```

The schema-1 set registers `PlanLowered` and `LowerPlan` only for
`ReplayContext`'s schema-1 branch. The live set has no cell or route for that
`EventKind`; `CommitService::execute` returns `UnsupportedOperation` before
authority admission or mutation. There is no compatibility forwarder that
would repackage the old graph as `planning.lowering-applied`, because it lacks
strategy, disposition, proof, debt and origin facts.

Historical replay may recreate the old rows exactly. Such rows remain
unlowered drafts unless a later live `planning.lowering-applied/2` transaction
binds them by exact current identity and contents. R13D owns the history/index
assembly and legacy import mapping; R08 owns only this live-registration split
and the executable-origin rule.

## Typed candidate-only result contract

`zap-core/src/execution_views/packet.rs` owns the result boundary used by both
packet resolution and runtime candidate ingress:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandidateEffectPolicy {
    NoExternalEffect,
    Reported {
        allowed: Vec<CandidateEffectState>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateResultTemplate {
    pub required_criteria: Vec<AcceptanceCriterion>,
    pub required_checks: Vec<VerificationId>,
    pub required_artifact_kinds: Vec<ArtifactKind>,
    pub effect: CandidateEffectPolicy,
    pub safe_stop: SafeStopContract,
    pub digest: PayloadDigest,
}

impl CandidateResultTemplate {
    pub fn new(
        required_criteria: Vec<AcceptanceCriterion>,
        required_checks: Vec<VerificationId>,
        required_artifact_kinds: Vec<ArtifactKind>,
        effect: CandidateEffectPolicy,
        safe_stop: SafeStopContract,
    ) -> Result<Self, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateResultContract {
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub template: CandidateResultTemplate,
    pub digest: PayloadDigest,
}

impl CandidateResultContract {
    pub fn bind(
        work_id: WorkId,
        contract_id: ContractId,
        contract_digest: ContractDigest,
        relevant_basis: RelevantBasisDigest,
        template: CandidateResultTemplate,
    ) -> Result<Self, ZapError>;

    pub fn validate_candidate(
        &self,
        candidate: &CandidateResult,
    ) -> Result<(), ZapError>;
}
```

The template constructor sorts and rejects duplicate typed sets and hashes every
field except `digest`. Lowering validation requires criteria statements to
cover the task contract's acceptance statements exactly, required check IDs to
equal the lowered verification plans, and safe stop to equal the task contract.
Packet resolution binds the template to the current work, contract and
transaction-derived execution basis. Result validation requires those exact
work/contract/basis values from the runtime job,
exactly one result per required criterion/check, all required artifact kinds,
an allowed effect state and the exact safe boundary. A criterion may be
`Unsatisfied`; the output remains a candidate. This type contains no accepted,
approved, authority or state-mutation disposition.

## Authoritative stored packet

Packet rendering advances to a minimal request. The caller selects a new packet
identity and lineage, but cannot provide the packet's work meaning:

```rust
schema_tag!(PacketRenderedSchema, "zap-planning/packet-rendered/2");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketRendered {
    pub schema: PacketRenderedSchema,
    pub packet_id: PacketId,
    pub work_id: WorkId,
    pub parent_packet_id: Option<PacketId>,
    pub supersedes: Option<PacketId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PacketState {
    Current,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkBinding {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerPacketRecord {
    pub packet_id: PacketId,
    pub parent_packet_id: Option<PacketId>,
    pub supersedes: Option<PacketId>,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
    pub work_id: WorkId,
    pub work_revision: Revision,
    pub work_semantic_digest: PayloadDigest,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub contract_id: ContractId,
    pub contract_version: Revision,
    pub contract_digest: ContractDigest,
    pub validation_generation: u64,
    pub render_basis: RelevantBasisDigest,
    pub obligation_ids: Vec<ObligationId>,
    pub stage_debt: Vec<StageDebt>,
    pub role: WorkerRole,
    pub source_captures: Vec<SourceCapture>,
    pub rules: Vec<RuleSourceBinding>,
    pub forks: Vec<ForkBinding>,
    pub candidate_result: CandidateResultTemplate,
    pub state: PacketState,
    pub packet_digest: PacketDigest,
    pub revision: Revision,
}
```

`PacketRenderedCell` resolves exactly one `Current` lowering containing the
work, requires its strategy to be `Current`, then reloads the exact work,
active contract, obligation coverage, stage debt, source captures, rules and
full prepared forks. It recomputes every binding and the packet digest. Missing
or duplicate current origins refuse. Parent/superseded packets must name the
same work and current lowering; superseding atomically marks the prior packet
`Superseded`. There is at most one current packet per lowering/work.

`work_semantic_digest` hashes the work fields other than lifecycle state,
active job and record revision. `render_basis` records the basis at packet
creation. The later privileged `WorkDispatched` progress transition may change
state/active-job/revision without invalidating the packet; resolution recomputes
the current execution basis and requires the semantic digest, contract,
validation generation and lowering origin to remain unchanged. Any semantic
work or basis dependency change requires a new packet.

The cell registers `PacketBasisScope`, which derives a
`BasisPurpose::Lowering` request from the payload's work root only. The domain
basis provider expands that root through its current contract, obligations,
dependencies and sources; the caller no longer supplies basis roots. A changed
source/contract/proof therefore makes the render frame stale before the packet
record is written.

Role is derived, never copied from `PacketRendered`. The cell builds the
existing `RoleAssessment` from current typed facts: work kind/type, essential
obligation coverage, read/write subjects, dependency count, verification
plans, source/rule/fork count, unresolved horizons and required maturity. It
then calls `route_role`. The mapping is a single tested function; unknown or
high-consequence facts route conservatively, and an algorithmic result refuses
worker-packet rendering. Provider/model/effort and host are not chosen here.

```rust
pub fn derive_packet_role(
    work: &WorkRecord,
    contract: &TaskContractRecord,
    binding: &LoweredWorkBinding,
    obligations: &[ObligationRecord],
    lowering: &LoweringRecord,
) -> Result<RouteDecision, ZapError>;
```

For this worker-packet path `deterministic` is false. `architectural` is true
for campaign/phase/workstream or decision/integration work; `novel` is true
when an applicable fork or unresolved horizon intersects the work subjects;
consequence is high for any essential obligation, medium for a nonempty write
set or change work, and low otherwise; reversible means an empty write set.
Context fragments equal the two mandatory protocol/assignment components plus
the exact source, rule and fork counts. Verification cost is cheap for one plan
with at most eight cases, expensive above eight plans or thirty-two total
cases, and moderate otherwise. Missing facts take the higher category. These
derived values feed the existing `route_role` table unchanged.

The packet source set is the exact union of active-contract source handles,
lowering captures and verification-plan sources, with equal current digests.
The rule set is the exact union of verification cases, lowering negative cases
and candidate criteria. Each rule has one exact current source/digest binding;
a bare `RequirementRef` is insufficient. Fork bindings cover the full applicable prepared fork
objects from the current strategy/lowering, not caller IDs. Any missing source,
rule, fork, contract, debt or obligation fact prevents a current packet.

## Core packet-resolution API

R08 adds `PacketResolutionDigest` to the existing `zap-wire` digest table.
`zap-core/src/execution_views/packet.rs` owns the cross-sibling request and
result types:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketResolutionRequest {
    packet_id: PacketId,
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    effect_id: EffectId,
    request_digest: PayloadDigest,
}

impl PacketResolutionRequest {
    pub fn new(
        packet_id: PacketId,
        job_id: JobId,
        attempt_id: AttemptId,
        dispatch_id: DispatchId,
        effect_id: EffectId,
    ) -> Result<Self, ZapError>;
    pub fn packet_id(&self) -> &PacketId;
    pub fn job_id(&self) -> &JobId;
    pub fn attempt_id(&self) -> &AttemptId;
    pub fn dispatch_id(&self) -> &DispatchId;
    pub fn effect_id(&self) -> &EffectId;
    pub const fn request_digest(&self) -> PayloadDigest;
}

pub trait PayloadPacketResolution<P: CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P)
        -> Result<PacketResolutionRequest, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedPacketMaterial {
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
    pub token_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPacketSource {
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPacketRule {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPacketFork {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedPacketWorkspace {
    pub binding: WorkspaceBinding,
    pub manifest_artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedStageDebtDisposition {
    Required,
    Accepted {
        acceptance_id: StageAcceptanceId,
    },
    Deferred {
        deferral_id: DeferralId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedStageDebt {
    pub work_id: WorkId,
    pub stage: MaturityStage,
    pub disposition: ResolvedStageDebtDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPacketIdentity {
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedProducer {
    pub principal_id: PrincipalId,
    pub harness_id: HarnessId,
    pub role: WorkerRole,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeJobClaimRecord {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
    pub identity: ResolvedPacketIdentity,
    pub work: WorkExecutionView,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub role: WorkerRole,
    pub resolved_profile: ResolvedProfile,
    pub expected_producer: ExpectedProducer,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
    pub workspace: CapturedPacketWorkspace,
    pub stage_debt: Vec<ResolvedStageDebt>,
    pub sources: Vec<ResolvedPacketSource>,
    pub rules: Vec<ResolvedPacketRule>,
    pub forks: Vec<ResolvedPacketFork>,
    pub candidate_result: CandidateResultContract,
    pub eligibility: DispatchEligibilityRequest,
    pub digest: PacketResolutionDigest,
}

#[derive(Clone)]
pub struct RuntimeJobClaim {
    record: RuntimeJobClaimRecord,
    transaction_seal: Arc<()>,
    service_seal: Arc<()>,
}

impl RuntimeJobClaim {
    pub fn record(&self) -> &RuntimeJobClaimRecord;
}

pub struct PacketResolutionContext<'a> {
    transaction_seal: &'a Arc<()>,
    service_seal: &'a Arc<()>,
}

impl PacketResolutionContext<'_> {
    pub fn seal(
        &self,
        record: RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError>;
}

pub trait PacketResolutionProvider: Send + Sync + 'static {
    fn resolve_live(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaim, ZapError>;

    fn replay_captured(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError>;
}
```

`RuntimeJobClaimRecord` uses a validated custom `Deserialize` that recomputes
nested invariants and its digest. The service-sealed `RuntimeJobClaim` has no
public constructor or serde implementation. Its digest covers all record fields
except `observed_revision` and `digest`; it therefore survives unrelated head
churn while binding every packet/work/contract/capability/material value.
Domain `Prototype`, `Functional`, and `Productized` stages map respectively to
core `Draft`, `Checked`, and `Integrated`; core `Accepted` remains a later
central-acceptance state and is never manufactured by lowering.

The provider requires a `Current` packet, lowering and strategy; exact current
work and active contract versions/digests; matching lowering origin; complete
obligation and stage debt; the role derived in the packet; and unchanged
semantic basis inputs, with only the exact intervening work-dispatch progress
transition tolerated. It derives the current execution basis and constructs `WorkExecutionView` and
`DispatchEligibilityRequest` from those records. A current packet label alone
cannot make unlowered work executable.

It then resolves the role through a fixed composition profile and one exact
`CapabilityCurrentRecord::Current` pointer plus its
`CapabilityObservationRecord`. The selected observation must support the exact
provider/model/effort, structured candidate results, required instruction
isolation, liveness/cancellation needs, context size and execution route. The
resulting `ResolvedProfile` must be exact and its observation/host/digest must
match the claim.

Each required source and rule must still have the stored current logical
digest. A typed retrieval handle may be used during live assembly, but before
the claim is sealed every source, rule and full prepared fork is materialized
as a nonempty immutable captured artifact whose bytes were checked by the
physical adapter. The workspace likewise carries a content-addressed manifest
artifact. Unavailable or unknown required material refuses. No fragment
supplied by the job caller is consulted.

## App composition and adapter limits

Core exposes two narrow non-authorizing material/environment ports. They can
resolve only identities already derived from stored packet meaning:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PacketMaterialSubject {
    Source {
        source_id: SourceId,
        source_digest: SourceDigest,
    },
    Rule {
        requirement: RequirementRef,
        source_id: SourceId,
        source_digest: SourceDigest,
    },
    Fork {
        fork_id: ForkId,
        semantic_digest: PayloadDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketMaterialRequest {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub subject: PacketMaterialSubject,
}

pub trait PacketMaterialProvider: Send + Sync + 'static {
    fn capture_live(
        &self,
        request: &PacketMaterialRequest,
    ) -> Result<CapturedPacketMaterial, ZapError>;

    fn verify_captured(
        &self,
        request: &PacketMaterialRequest,
        captured: &CapturedPacketMaterial,
    ) -> Result<(), ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketWorkspaceRequest {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub packet_id: PacketId,
    pub lowering_id: LoweringId,
    pub work_id: WorkId,
    pub harness_id: HarnessId,
}

pub trait PacketWorkspaceProvider: Send + Sync + 'static {
    fn capture_live(
        &self,
        request: &PacketWorkspaceRequest,
    ) -> Result<CapturedPacketWorkspace, ZapError>;

    fn verify_captured(
        &self,
        request: &PacketWorkspaceRequest,
        captured: &CapturedPacketWorkspace,
    ) -> Result<(), ZapError>;
}
```

`zap-app/src/packet_resolution.rs` owns the one cross-sibling implementation:

```rust
#[derive(Clone, Debug)]
pub struct WorkerProfileBinding {
    pub principal_id: PrincipalId,
    pub desired: DesiredProfile,
}

#[derive(Clone, Debug)]
pub struct WorkerProfilePolicy {
    pub senior: WorkerProfileBinding,
    pub middle: WorkerProfileBinding,
    pub junior: WorkerProfileBinding,
}

impl WorkerProfilePolicy {
    pub fn for_role(&self, role: WorkerRole) -> &WorkerProfileBinding;
}

pub struct ApplicationPacketResolutionProvider {
    profiles: WorkerProfilePolicy,
    materials: Arc<dyn PacketMaterialProvider>,
    workspaces: Arc<dyn PacketWorkspaceProvider>,
}

impl ApplicationPacketResolutionProvider {
    pub fn new(
        profiles: WorkerProfilePolicy,
        materials: Arc<dyn PacketMaterialProvider>,
        workspaces: Arc<dyn PacketWorkspaceProvider>,
    ) -> Result<Self, ZapError>;
}

impl PacketResolutionProvider for ApplicationPacketResolutionProvider {
    fn resolve_live(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaim, ZapError>;

    fn replay_captured(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError>;
}
```

The app implementation may import zap-domain packet/lowering/control/knowledge
records and zap-runtime capability records; neither sibling imports the other.
`FoundationComposition` gains
`packet_resolution_provider: Arc<dyn PacketResolutionProvider>` and fixes it on
the mutation service.

R15 implements physical material lookup/capture. `capture_live` may follow a
typed retrieval handle internally, but returns a content-addressed immutable
capture; the handle is not claim evidence. It cannot choose the required
source/rule/fork set or change its digests. R13 may transport the request and
provide a workspace adapter; it cannot create packet identity, role, capability
or contract facts. Captures are checked against the request: byte/token counts
are positive, artifact bytes hash exactly, and workspace bindings bind
store/base and the resolved harness policy. Missing adapters leave the packet
honestly unexecutable.

## Core registration, commit and replay integration

The accepted typed registration builder gains one composable adapter; no
`single_with_*` constructor is added:

```rust
impl<C: TransitionCell> CellRegistrationBuilder<C> {
    pub fn packet_resolution<R>(self, adapter: R)
        -> Result<Self, ZapError>
    where
        R: PayloadPacketResolution<C::Payload>;
}

impl<S: TransactionStore> CommitServiceBuilder<S> {
    pub fn packet_resolution_provider(
        self,
        provider: Arc<dyn PacketResolutionProvider>,
    ) -> Self;
}

impl<P: CommandPayload> ValidatedCommand<P> {
    pub fn packet_resolution(&self) -> Option<&RuntimeJobClaim>;
}
```

The erased cell adapter exposes
`packet_resolution_request(&dyn ErasedCommandPayload)`. `CellSet` declares this
dependency so service build requires the fixed provider whenever any live cell
uses it. A duplicate adapter refuses. `CellDescriptor` gains the private
registration bit plus `pub const fn requires_packet_resolution(&self) -> bool`;
the builder sets it when this adapter is present. For that cell, dispatch
eligibility comes from the resolved claim, so no caller-facing
`PayloadDispatchEligibility` adapter is registered.

R07's schema-2 `CommandPreflightRecord` reserves the additive evidence slot:

```rust
pub struct CommandPreflightRecord {
    // Accepted R07 fields unchanged.
    pub packet_resolution: Option<RuntimeJobClaimRecord>,
}
```

Core resolves the request inside the current transaction after strict payload,
identity, service-permit and basis checks and before dispatch eligibility. It
constructs a private `PacketResolutionContext`, invokes the fixed provider,
then verifies request digest, operational IDs, store/campaign/base, observed
revision, every nested invariant and `PacketResolutionDigest`. It evaluates the
claim's derived `DispatchEligibilityRequest` through the existing fixed
provider and requires an exact current eligible view. Only then does it expose
the sealed claim to the product cell.

Schema-2 replay/audit reruns packet resolution against the historical pre-state,
compares the derived record with `command_preflight.packet_resolution`, reruns
dispatch eligibility and the product cell, and compares mutations/output. The
accepted `ReplayProviders` gains
`packet_resolution: Option<&dyn PacketResolutionProvider>`; schema-1 event DTOs
and replay semantics do not change. R13D continues to own history traversal and
passes the already composed provider into this narrow hook.

Replay calls `replay_captured`, never `resolve_live`, `capture_live`, a current
query handle, capability probe or current workspace. It derives logical packet,
strategy, lowering, work, contract, proof/debt and capability facts from the
historical `StateReader`; the capability observation is the immutable record
named by the captured claim, not today's host environment. It then calls only
`verify_captured` for every stored source/rule/fork artifact and the stored
workspace manifest and requires byte length/digest equality before sealing the
replayed claim.

If a named immutable artifact or workspace manifest is unavailable, replay
returns `ErrorCode::Unavailable` with `FixSurface::SourceCapture` (or Adapter
for the workspace), produces no replayed mutations and never substitutes
current bytes or re-runs a live retrieval. R13D may still expose the immutable
event bytes and mark audit evidence unavailable; absence of the artifact is an
honest verification boundary, not permission to reinterpret history.

## Runtime claim consumes only resolved packet meaning

The caller-built job payload is retired:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobClaimPayload {
    pub packet_id: PacketId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
}

pub struct JobClaimPacketResolution;

impl PayloadPacketResolution<JobClaimPayload>
    for JobClaimPacketResolution
{
    fn request(
        &self,
        payload: &JobClaimPayload,
    ) -> Result<PacketResolutionRequest, ZapError>;
}
```

The five fields are identities generated by the trusted runtime command
factory; packet ID is the only submitted semantic identity. There is no job,
work, producer, contract, basis, role, profile, capability, subject, resource,
source, rule, fork, result-contract or eligibility object in the payload.

```rust
pub trait RuntimeCommandFactory: Send + Sync {
    fn claim(
        &self,
        request: &PacketResolutionRequest,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    // Existing receipt/observation/reconciliation methods remain.
}
```

The old `claim(&WorkExecutionView, &AgentCapabilities) ->
(RuntimeJobRecord, CanonicalCommandFrame)` signature is removed. The factory
formats a command; it does not resolve packet meaning or construct the record.

`runtime.job-claimed` becomes `RouteClass::ServiceInternal`. The preceding
`domain.work-dispatched` command remains the privileged `work.dispatch` gate:
it executes R07 authentication, progress classification, Owner pause/exception,
hold/independence and current readiness checks and atomically sets the chosen
job ID on the work. The service-internal claim frame is then authorized by the
existing exact-frame internal-protocol handle. Packet resolution requires that
same current work to be `Active` with `active_job == request.job_id`; it cannot
bypass or repeat the privileged dispatch decision.

`JobClaimCell::apply` requires
`command.packet_resolution()`, compares all five payload IDs with the sealed
record, and constructs `DispatchIntent`, `RuntimeJobRecord` and
`WorkExecutionObservationRecord` solely from it. `RuntimeJobRecord` adds:

```rust
pub packet_resolution_digest: PacketResolutionDigest;
pub expected_producer: ExpectedProducer;
pub candidate_result_contract: CandidateResultContract;
```

The existing `RuntimeJobRecord.producer` is constructed by the cell, never the
payload: `ExpectedProducer.principal_id`, role `Worker`, and the exact attempt,
job and packet form its `ProducerRef`. The trusted app profile binding supplies
that principal alongside the desired role/profile, and capability resolution
binds its harness. `CandidateRecordedCell` requires the candidate producer to
equal the stored value and calls
`candidate_result_contract.validate_candidate` before writing provenance.
Producer labels in a candidate payload can only be compared; they never
establish or replace the binding.

The job begins in the current unlaunched state (`DispatchPending`,
`IntentCommitted`, `NotStarted`) and stores the resolved packet/contract/basis,
subjects, resources, integration owner, profile, workspace and typed result
contract. A product failure rolls back both job and execution observation; a
retry cannot resolve the packet to different meaning under the same committed
command.

## Ordered implementation units

1. **Core packet types.** Add `PacketResolutionDigest`; add
   `execution_views/packet.rs` with the candidate template/contract, resolution
   DTOs, sealed claim/context and material/workspace/provider traits; export
   them from core.
2. **R07 preflight extension.** Add only the builder's `packet_resolution`
   adapter, erased request method, fixed provider slot, validated-command
   accessor, exact provider checks and `CommandPreflightRecord` optional field.
   Include the provider in schema-2 replay before the first schema-2 write.
   Do not alter R07 impact/effect/closure semantics or schema-1 bytes.
3. **Domain record shapes.** Advance only the lowering-applied and
   packet-rendered payload schemas; add planning/packet status,
   `LoweredWorkBinding`, authoritative disposition/debt bindings,
   `RuleSourceBinding`, `ForkBinding` and the derived packet record. Register
   the new shapes.
4. **Atomic lowering kernel.** Extend the existing graph/contract validators,
   add current charter/outcome/proof/deferral resolution, implement one kernel,
   and register the lowering cell plus R07 effect contract against the complete
   record family set.
5. **Close the old route.** Remove `LowerPlan` from live domain cells/routes and
   hand its unchanged registration to R13D's schema-1 replay set. Preserve all
   additive legacy-projection registrations already landed by R14.
6. **Derived packet record.** Replace caller `PacketAssembly` on the live render
   path with the minimal payload, current-origin lookup, exact role/source/rule/
   fork/debt derivation and one-current-packet lineage. Leave bundle/return
   types and cells untouched for the next design unit.
7. **App resolver.** Add `ApplicationPacketResolutionProvider`, fixed profile
   bindings and adapter injection; compose it without moving domain records into
   runtime or runtime records into domain.
8. **Runtime claim.** Narrow `JobClaimPayload`, change it to the internal route,
   register its packet adapter, build the job from the sealed claim, retain
   caller-free producer identity, and validate candidates against the stored
   typed contract.
9. **Focused service journey.** Exercise all of the following sequence in one
   real redb store before broader gates.

Units 1-2 are the first implementation packet and can land while domain kernel
work is prepared. Units 3-6 form one atomic domain migration. Units 7-8 can
then proceed in parallel on app/runtime files, meeting at the service journey.

## Exact ownership

R08 owns `zap-domain/src/lowering/{payloads,model,records,transitions,cells,
packets}.rs`, the narrow control validation/live-registration removal,
`zap-core/src/execution_views/packet.rs`, the packet additions to the accepted
registration/preflight seams, `zap-app/src/packet_resolution.rs` and the
runtime job-claim/factory/record changes described here.

The active R07 implementation owner controls overlapping `zap-core`
admission/effect/commit files until its accepted contract lands; R08 adds only
the named packet extension and coordinates the first schema-2 writer. R13D owns
replay traversal, indexes, legacy projection and schema-1 registry assembly.
R14's imported rows remain additive evidence and are not rewritten by R08.
R15 owns material byte capture/archive safety and R13 owns transport/workspace
adapters. Neither may construct `WorkerPacketRecord` or `RuntimeJobClaimRecord`.

No R08 unit in this document edits bundle export, encounter recording, return
import, reassessment, archive publication or client protocol semantics.

## One required real journey

The acceptance test uses the composed production `RecordSet`, live `CellSet`,
routes, R07 providers, `ApplicationPacketResolutionProvider`, runtime
eligibility provider and a real redb store. The material/workspace test adapters
implement the exact production ports and verify requested digests; they do not
seed derived packet or job records.

1. Establish the active charter, intent, outcome, obligations, one current
   proof-backed stage acceptance, one outcome-scoped deferral, current sources
   and current runtime capability records through their real routes. Use the
   registrar-issued sealed DataProposal handle to submit a candidate strategy.
2. Keep one R14-shaped imported `Planned`/`Prototype` work row and its
   `active: false` contract plus raw legacy constraint/source records, all with
   no lowering origin; leave another proposed node absent. Query both: the
   imported row is visible but reports `MissingLoweringOrigin`; runtime claim
   by any packet ID refuses. No fixture fabricates executable argv from the
   legacy check string.
3. At the exact first charter-bound lowering boundary, submit
   `planning.lowering-applied/2` without an economics admission and prove R07
   derives `InitialBaseline`/`Exempt`. The one commit promotes the strategy,
   binds the exact imported work, activates or replaces its inactive contract
   with a separately authored canonical current contract, inserts the absent
   graph rows/contracts, leaves the raw legacy records unchanged, updates exact
   obligation owners and stores a current lowering with matching bindings.
   Reopen the store and prove there is no lowering-only or graph-only
   intermediate state and the inactive legacy contract never became executable
   by label alone.
4. Prepare the real R07 economics admission for a second lowering of the same
   strategy/target. Name the first lowering as the exact predecessor and reuse
   its still-Planned, unchanged work/contract IDs. The transaction derives
   `SemanticChange`, supersedes the first lowering and any current packets, and
   rebinds those exact rows to the second. A sibling attempt to reuse a Work ID
   whose current origin belongs to another lowering/strategy/target refuses
   without mutation.
5. From the same precondition, run four refusing sibling cases: nonexistent or
   foreign-outcome successor, inapplicability not present in current
   outcome/charter, stage acceptance whose evidence is absent from
   `CurrentProofSet`, and unrelated/closed deferral. Each leaves strategy,
   lowering, work, contracts, obligations and event head unchanged.
6. Submit the old canonical `domain.plan-lowered` frame to the live service and
   receive `UnsupportedOperation` with no admission/hook mutation. Replay a
   fixed schema-1 instance only through R13D's frozen cell set and preserve its
   historical rows as unlowered drafts.
7. Render a packet using only packet ID, work ID and valid lineage. Assert the
   stored current packet contains the exact current strategy/lowering/work/
   contract/debt role, source/rule/fork and candidate-template closure. Unknown
   payload fields attempting to choose Junior, replace a contract, omit a
   source or rewrite a result contract fail strict decode.
8. Make one required source stale, one rule material unavailable, one fork
   material mismatched and the capability current pointer unknown in separate
   sibling attempts. Packet resolution refuses each. Restore exact current
   evidence; resolution produces one sealed claim with a digest covering the
   complete packet and a current eligible dispatch request.
9. Execute the privileged `domain.work-dispatched` command for that work/job,
   proving R07 pause/hold controls. Then submit the service-internal
   `runtime.job-claimed` frame containing only packet plus operational IDs.
   The job and execution observation commit atomically and exactly match the
   resolved packet, profile/capability, workspace, subjects/resources,
   integration owner and candidate result contract. A frame containing the old
   caller-built job/producer/eligibility fields fails strict decode.
10. Submit one candidate with the wrong producer, contract, criterion/check set,
   effect state or safe boundary in sibling attempts; each refuses before
   provenance. Submit a matching candidate and prove it is stored only as a
   candidate, with no acceptance or mutation authority activated.
11. Exact-retry the claim, close/reopen the store, reconcile it and run
    schema-2 audit replay with packet resolution rederived from the historical
    pre-state and stored immutable captures. Change the current machine
    capability, live query result and workspace after the commit; replay still
    produces the original claim without consulting them. The job,
    packet-resolution digest, event output and mutations agree and no adapter
    performs an external launch. In a sibling audit with one captured artifact
    deliberately unavailable, replay returns `Unavailable`, produces no
    mutations and never substitutes current bytes.

Focused crate tests cover core packet types, domain lowering/packet derivation,
runtime claim/candidate validation and app composition. Root acceptance also
requires the repository's normal Rust floor after those focused gates. Bundle
or return tests are neither required nor claimed by this boundary.
