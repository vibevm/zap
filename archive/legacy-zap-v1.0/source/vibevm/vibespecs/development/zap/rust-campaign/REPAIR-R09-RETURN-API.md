# R09 offline bundle, return, and causal reassessment contract

Status: bounded architecture contract for root review.

## Ownership boundary

R09 owns the semantic manifest for an offline execution envelope, durable
encounter/return lineage, bounded imported delta, and the causal handoff from
an affected reassessment to a later R08 lowering. It consumes the accepted R07
admission/affected-closure APIs and the accepted R08 executable packet and
`RuntimeJobClaimRecord`; it does not redefine them.

R15 owns physical archive creation, byte rehashing, path/symlink/no-clobber
safety and immutable artifact availability. R13 owns transport. These adapters
may carry the exact R09 manifest and captures but cannot supply missing
strategy, lowering, packet, job, producer, permission, capability, stop-rule,
candidate or reassessment lineage.

## Fixed outcomes

1. A bundle manifest is derived from exact current strategy, lowering and
   packet records plus immutable R08 captures. It closes every required source,
   rule, prepared fork, capability observation, permission and typed
   `StopRuleId`; no caller list establishes closure.
2. Offline encounter rows form an ordered bounded delta with stable identities,
   causal predecessors and exact packet/job/attempt/producer bindings. They are
   observations and proposals only.
3. Return import binds one exported bundle and its actual runtime attempts,
   verifies immutable artifacts and exact delta range, and is idempotent.
   Candidate-less material is never classified as an applicable candidate.
4. Applicable candidates require the current stored `CandidateResultRecord`
   and `CandidateProvenanceRecord` to agree with the job, attempt, producer,
   packet, contract, basis and artifacts. Actor labels in returned bytes grant
   nothing.
5. Reassessment consumes one stored bounded return delta, derives its affected
   closure, preserves problem/approach counters and all prior history, and emits
   either an exact no-change result or a causal input to one later R08 lowering.
   Temporal adjacency is not causation.

## Wire additions and minimal export command

R09 adds `EncounterDeltaDigest` and `ReassessmentDigest` through the existing
`zap-wire` digest declaration table. Existing `BundleDigest` and
`ReturnBundleDigest` keep their current domains.

The live export payload advances to a distinct schema and stops accepting a
manifest assembled by its caller:

```rust
schema_tag!(BundleExportedSchema, "zap-planning/bundle-exported/2");
```

The request and captured payload shape are specified with the app-owned seam
below. The caller chooses a bounded export scope only. Packet IDs are sorted, unique
and nonempty; all three bounds are positive and are stored in the manifest.
There is no source, capability, permission, stop, entry or digest list in this
payload.

## Exact semantic bundle closure

The derived manifest types live in `zap-domain/src/lowering/offline.rs` and use
typed bindings rather than parallel ID lists. The app-owned cell can read both
domain and runtime records without making either sibling depend on the other:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleStrategyBinding {
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePacketBinding {
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_version: Revision,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleAttemptBinding {
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
    pub packet_id: PacketId,
    pub packet_resolution_digest: PacketResolutionDigest,
    pub dispatch_intent_digest: DispatchIntentDigest,
    pub producer: ProducerRef,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
    pub workspace_manifest: ArtifactDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSourceBinding {
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleRuleBinding {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleForkBinding {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharterPermissionBinding {
    pub charter_id: CharterId,
    pub charter_revision: Revision,
    pub charter_digest: PayloadDigest,
    pub action: ActionClass,
    pub packet_ids: Vec<PacketId>,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRuleBinding {
    pub stop_rule_id: StopRuleId,
    pub revision: Revision,
    pub rule_digest: PayloadDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleEntryKind {
    Packet,
    Assignment,
    Source,
    Rule,
    Fork,
    Capability,
    Permission,
    StopRule,
    Workspace,
    ResultSchema,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleEntryBinding {
    pub kind: BundleEntryKind,
    pub path: BoundedText<4096>,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeakBundleManifest {
    pub bundle_id: BundleId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub export_revision: Revision,
    pub binding: BundleStrategyBinding,
    pub packets: Vec<BundlePacketBinding>,
    pub attempts: Vec<BundleAttemptBinding>,
    pub sources: Vec<BundleSourceBinding>,
    pub rules: Vec<BundleRuleBinding>,
    pub forks: Vec<BundleForkBinding>,
    pub capabilities: Vec<CapabilityObservationId>,
    pub permissions: Vec<CharterPermissionBinding>,
    pub stop_rules: Vec<StopRuleBinding>,
    pub entries: Vec<BundleEntryBinding>,
    pub maximum_archive_bytes: u64,
    pub maximum_encounters: u32,
    pub maximum_return_bytes: u64,
    pub encounter_genesis: PayloadDigest,
    pub simulated: bool,
    pub digest: BundleDigest,
}
```

The manifest constructor recomputes `digest` over every preceding field,
canonicalizes set-valued rows by their typed keys and rejects conflicting
duplicates. Paths are derived from typed identities and entry kind; they are
not accepted from `BundleExported`.

`encounter_genesis` is a domain-separated hash of bundle ID, exact
strategy/lowering binding and ordered packet/attempt identities. It does not
depend on the final manifest digest, so the manifest and encounter chain are
not circular.

All packets must be `Current`, belong to the exact current lowering and current
strategy, and retain their exact R08 work/contract/basis closure. Each packet
must have exactly one claimed, not-yet-launched `RuntimeJobRecord`; its attempt,
producer, dispatch intent, capability and workspace capture form the attempt
binding. A packet ID with no actual job/attempt cannot enter an executable
offline bundle.

R08's claim cell copies
`RuntimeJobClaimRecord.workspace.manifest_artifact` into the additive
`RuntimeJobRecord.workspace_manifest: ArtifactDigest` field. Bundle closure
uses that stored value; it never guesses the job's workspace capture from a
current path or recaptures a different workspace under the same packet.

For source/rule/fork material, the app provider rederives the R08 packet claim
for the job's exact operational IDs, requires its full
`PacketResolutionDigest` to equal the stored job value, and then copies those
immutable captures. Any changed material produces a different resolution and
refuses export.

The source/rule/fork rows are the exact union of the selected packets' R08
closures and immutable captured material. Capability entries contain the full
captured `AgentCapabilities` value named by each attempt, not a fresh probe.
Required assignment, packet, result-schema and workspace artifacts also have
one entry. The canonical manifest is written at a fixed archive path after its
digest is complete and is bound separately by `BundleArchiveReceipt`, avoiding
a self-referential entry digest. The entry set is complete in both directions:
no required binding lacks an artifact and no archive entry is unrelated to a
binding.

`PreparedFork` gains one semantic field before R08 first writes it:

```rust
pub selection_action: ActionClass;
```

Its action and delegated alternatives are part of the fork semantic digest.
The permission set is derived from the active charter's exact revision/digest
and the actions actually needed by selected packets and delegated forks. A
permission binding is an offline envelope fact, not a credential and not live
mutation authority. An action absent from the current charter refuses export;
no caller `AuthorizationRef` substitutes for it.

The stop-rule set is every active `StopRuleRecord` for the campaign, identified
by `StopRuleId`, revision and content digest, plus each packet's safe-stop
contract. `ConditionId` is not accepted as a stop identity. Each rule artifact
contains the exact supported expression and reason needed by the offline
deterministic evaluator. Unknown evidence remains unknown and stops the
affected action according to the rule contract.

## App-owned bundle preparation and pure cell verification

Bundle closure crosses domain packet records and runtime attempt/capability
records. It therefore stays at the existing composition layer rather than
adding another universal core preflight slot or changing R07's schema-2 event
shape.

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleClosureRequest {
    pub bundle_id: BundleId,
    pub lowering_id: LoweringId,
    pub packet_ids: Vec<PacketId>,
    pub maximum_archive_bytes: u64,
    pub maximum_encounters: u32,
    pub maximum_return_bytes: u64,
    pub simulated: bool,
    pub request_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleClosureRecord {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub manifest: WeakBundleManifest,
}

pub trait BundleClosureProvider: Send + Sync + 'static {
    /// App-side preparation; may ask R15 to capture the already derived set.
    fn prepare(
        &self,
        state: &dyn StateReader,
        request: &BundleClosureRequest,
    ) -> Result<BundleClosureRecord, ZapError>;

    /// Pure validation used by the transition and replay; performs no I/O.
    fn verify_captured(
        &self,
        state: &dyn StateReader,
        request: &BundleClosureRequest,
        captured: &BundleClosureRecord,
    ) -> Result<(), ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleExported {
    pub schema: BundleExportedSchema,
    pub request: BundleClosureRequest,
    pub closure: BundleClosureRecord,
}

pub struct ApplicationBundleExportedCell {
    verifier: Arc<dyn BundleClosureProvider>,
}

impl ApplicationBundleExportedCell {
    pub fn new(verifier: Arc<dyn BundleClosureProvider>) -> Self;
}

impl TransitionCell for ApplicationBundleExportedCell {
    type Payload = BundleExported;
    type Output = DomainMutation;
    // ServiceInternal; writes only WeakBundleRecord.
}

pub struct BundleExportArtifacts;

impl PayloadArtifacts<BundleExported> for BundleExportArtifacts {
    fn artifacts(
        &self,
        payload: &BundleExported,
    ) -> Result<Vec<ArtifactDigest>, ZapError>;
}
```

The app command factory builds `BundleClosureRequest`, reads one current
snapshot, calls `prepare`, and places the resulting captured record in the
strict payload. `prepare` first derives the required logical closure, then asks
R15 only for those immutable artifacts. The service-internal permit binds the
exact frame. `ApplicationBundleExportedCell` calls only `verify_captured`, which
rederives strategy/lowering/packet/attempt/capability/permission/stop identities
from its transaction `StateReader`, compares the captured record exactly, and
writes the domain bundle record. The payload artifact adapter causes the normal
core artifact witness to cover every referenced immutable artifact.

Schema-2 replay decodes the same `/bundle-exported/2` payload and calls the same
pure `verify_captured` against the historical pre-state; it does not recapture,
probe the host, follow a live query handle or rebuild a workspace. Physical
artifact availability is a separate R15 audit/execution gate: unavailable bytes
return `Unavailable` and prevent offline use, but replay never substitutes
current bytes or changes the historical mutation.

The old `/bundle-exported/1` payload/cell remains only in the frozen schema-1
replay set. New schema-2 events use the strict v2 payload above. No existing v2
event DTO is retroactively changed, and R07 can finish its coherent protocol
without waiting for R09 types.

The live records use new registered families
`zap.planning.bundle.v2`, `zap.planning.encounter.v2` and
`zap.planning.return_import.v2`; the sidecar families are new. Frozen schema-1
record adapters retain the old family/value shapes for replay and historical
reads. Raw v1 values are never deserialized as the new structs or rewritten in
place; an explicit later migration may project them as non-executable history.

## Semantic preparation and R15 archive receipt

Physical publication remains two-phase and never occurs inside the domain
transaction:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    Prepared,
    Ready,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleArchiveReceipt {
    pub bundle_id: BundleId,
    pub manifest_digest: BundleDigest,
    pub entries_digest: PayloadDigest,
    pub archive_artifact: ArtifactDigest,
    pub byte_len: u64,
    pub harness_id: HarnessId,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeakBundleRecord {
    pub bundle_id: BundleId,
    pub manifest: WeakBundleManifest,
    pub manifest_digest: BundleDigest,
    pub status: BundleStatus,
    pub archive: Option<BundleArchiveReceipt>,
    pub revision: Revision,
}

schema_tag!(BundleArchivePublishedSchema,
    "zap-planning/bundle-archive-published/1");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleArchivePublished {
    pub schema: BundleArchivePublishedSchema,
    pub receipt: BundleArchiveReceipt,
}
```

`ApplicationBundleExportedCell` is `ServiceInternal`, requires the exact
captured and rederived bundle closure and inserts `Prepared` with no archive.
It does not accept archive paths
or claim a file exists. R15 writes the archive from the derived entry plan,
rehashes every entry and the final artifact, enforces its physical safety rules,
then submits `BundleArchivePublished` as a trusted observation with an artifact
witness. That cell checks bundle/manifest/ordered-entry digests and positive
length, requires the admitted trusted-observation harness/source to equal the
receipt, and transitions only `Prepared -> Ready`. Return import and offline
execution require `Ready`. A physical failure leaves the semantic preparation
inspectable and unexecutable.

## Ordered bounded offline encounter delta

The portable journal format lives beside the manifest DTOs in
`zap-domain/src/lowering/offline.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterKind {
    AttemptStarted,
    ForkSelected,
    CandidateProduced,
    Failure,
    Contradiction,
    Observation,
    MissingCapability,
    UnresolvedQuestion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterApproachBinding {
    pub problem_id: ProblemId,
    pub epoch: u32,
    pub approach_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineEncounter {
    pub encounter_id: EncounterId,
    pub sequence: u32,
    pub previous_digest: PayloadDigest,
    pub causes: Vec<EncounterId>,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub producer: ProducerRef,
    pub kind: EncounterKind,
    pub approach: Option<EncounterApproachBinding>,
    pub selected_fork: Option<ForkId>,
    pub candidate_id: Option<CandidateId>,
    pub detail: BoundedText<4096>,
    pub artifacts: Vec<ArtifactDigest>,
    pub evidence_ids: Vec<EvidenceId>,
    pub effect_state: CandidateEffectState,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterDelta {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub first_sequence: u32,
    pub previous_digest: PayloadDigest,
    pub encounters: Vec<OfflineEncounter>,
    pub final_digest: PayloadDigest,
    pub encoded_bytes: u64,
    pub digest: EncounterDeltaDigest,
}

pub struct OfflineEncounterJournal {
    // Manifest-bound builder; no authority or I/O.
}

impl OfflineEncounterJournal {
    pub fn new(manifest: &WeakBundleManifest) -> Result<Self, ZapError>;
    pub fn append(&mut self, encounter: OfflineEncounter)
        -> Result<(), ZapError>;
    pub fn finish(self) -> Result<EncounterDelta, ZapError>;
}
```

Sequence starts at zero, `previous_digest` starts at the manifest's
`encounter_genesis`, and every row increments by one and hashes all fields
except its digest. Causes name only earlier rows. The delta's first/previous/
final values must describe one contiguous prefix and its digest includes the
ordered row digests and exact encoded byte count.

Every encounter must match one `BundleAttemptBinding` exactly. Fork selection
must name a fork in that attempt's packet and an allowed delegated alternative.
`CandidateProduced` requires exactly one candidate ID; all other kinds require
none. Failure requires an approach binding; retry/observation rows may refer to
the same approach but do not increment its count. Artifacts/evidence are sorted
and unique and must be included in or newly attached to the return archive.

The journal refuses before exceeding either manifest bound. There is no global
encounter or byte limit: the exact export stores the selected limits. An empty
delta is valid only as an explicit no-observation return with first sequence
zero and both chain digests equal to genesis.

## Return DTO, exact provenance, and classification

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnArchiveReceipt {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub entries_digest: PayloadDigest,
    pub archive_artifact: ArtifactDigest,
    pub byte_len: u64,
    pub harness_id: HarnessId,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnBundleInput {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub base_id: BaseId,
    pub binding: BundleStrategyBinding,
    pub delta: EncounterDelta,
    pub archive: ReturnArchiveReceipt,
    pub digest: ReturnBundleDigest,
}

schema_tag!(ReturnImportedSchema, "zap-planning/return-imported/2");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnImported {
    pub schema: ReturnImportedSchema,
    pub expected_import_revision: Revision,
    pub input: ReturnBundleInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReturnClassification {
    ApplicableCandidate { candidate_id: CandidateId },
    ApplicableObservation,
    StaleReviewInput,
    Contradiction,
    Failure,
    NewUnknown,
}
```

`ReturnBundleDigest` covers the source manifest, exact strategy/lowering
binding, ordered delta digest and immutable return-archive receipt. Return
artifacts are declared through `PayloadArtifacts<ReturnImported>` and R15 owns
their physical verification. Import requires the source `WeakBundleRecord` to
be `Ready` and every bundle/delta/archive identity and bound to match exactly.

Classification is a pure function of the validated row and transaction state:

- `CandidateProduced` can become `ApplicableCandidate` only when its candidate
  ID resolves to exactly one current `CandidateResultRecord` and one
  `CandidateProvenanceRecord`; both must match the bundle attempt's job,
  attempt, producer and packet, the R08 contract/basis/result contract, and the
  exact result artifact set. The job must be terminal, `CandidateRecorded`,
  point to that candidate ID and remain unreviewed. Current strategy/lowering/
  packet applicability is then checked independently.
- A valid attempt, fork selection or ordinary observation with no candidate is
  `ApplicableObservation` while current, or `StaleReviewInput` after semantic
  drift. It can never be `ApplicableCandidate`.
- Contradiction and failure keep those classifications regardless of current
  plan tip. Missing capability, unresolved question or unknown external effect
  is `NewUnknown`. A stale candidate remains `StaleReviewInput`; it is not
  discarded or accepted.

The returned `ProducerRef` is comparison data only. It must equal the producer
already derived into the actual runtime job and candidate provenance; changing
the actor label, model, host or role cannot create provenance or authority.

## Durable bounded import

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterRecord {
    pub encounter_id: EncounterId,
    pub source_bundle_id: BundleId,
    pub delta_digest: EncounterDeltaDigest,
    pub sequence: u32,
    pub encounter: OfflineEncounter,
    pub classification: ReturnClassification,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnResolutionState {
    AwaitingReassessment,
    NoChange,
    ReloweringRequired,
    ReloweringApplied,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnImportRecord {
    pub source_bundle_id: BundleId,
    pub return_digest: ReturnBundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub first_sequence: u32,
    pub final_sequence: Option<u32>,
    pub final_digest: PayloadDigest,
    pub imported_encounters: Vec<EncounterId>,
    pub classifications: Vec<(EncounterId, ReturnClassification)>,
    pub affected_scope: AffectedScopeDigest,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub reassessment_review_id: Option<ReviewId>,
    pub resolved_lowering_id: Option<LoweringId>,
    pub resolution: ReturnResolutionState,
    pub revision: Revision,
}
```

`ApplicationReturnImportedCell` is `TrustedObservation`, requires its admitted
harness/source to equal `ReturnArchiveReceipt`, and registers the accepted R07
state-capable `PayloadAffectedScope`. The extractor shares the exact bundle/
delta/provenance validator, derives roots from the actual attempt packets,
work/contracts, changed sources/evidence/forks and candidate provenance, and
asks core for the authoritative affected/dependent closure. Caller-provided
affected lists do not exist. The cell requires that exact current view and
stores its digest and canonical work/subject projection.

The cell performs at most the manifest's encounter count and encoded-byte
bounds plus direct keyed reads for the named jobs, candidates, provenance and
existing encounter IDs. It does not `scan_all` the encounter family. New rows
are inserted in sequence order in the same transaction as one import record.
An exact existing bundle/return digest is an idempotent no-change result;
conflicting reuse of the bundle, return or encounter identity refuses. One
bundle admits one contiguous return delta; another cycle exports another bundle.

The `zap.planning.bundle` query performs direct keyed reads of the bundle and
its optional import, then reads only `imported_encounters` by ID. It never scans
the encounter family. Returned rows and encoded bytes are bounded by the
manifest limits and normal query page limits; incomplete paging returns a
cursor rather than materializing the campaign journal.

## Approach history and counters survive the cycle

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FailedApproachKey {
    pub problem_id: ProblemId,
    pub epoch: u32,
    pub approach_digest: PayloadDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterDisposition {
    Counted,
    PendingOwnerEpoch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedApproachRecord {
    pub key: FailedApproachKey,
    pub source_bundle_id: BundleId,
    pub encounter_id: EncounterId,
    pub attempt_id: AttemptId,
    pub disposition: CounterDisposition,
    pub revision: Revision,
}
```

The composite key has the canonical `RecordKey` encoding and makes one semantic
approach count at most once per problem epoch. On a failure, import compares the
binding with the current `ApproachEpochRecord`. An exact existing epoch inserts
one `Counted` record and increments `failed_approaches` once in the same
transaction. A retry, provider change, renamed task, repeated failure row or
new bundle carrying the same key cannot increment it again.

If the offline result identifies a genuinely new problem with no Owner-created
epoch, import preserves the encounter and a `PendingOwnerEpoch` history row but
does not create/advance an epoch or alter a stop counter. Only the existing
Owner-control route may establish the epoch. Reassessment can surface that
pending problem, never relabel an old failed approach to escape its counter.

## Causal bounded reassessment

R09 extends the existing adaptive review rather than creating a second review
engine:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnDeltaBinding {
    pub source_bundle_id: BundleId,
    pub return_digest: ReturnBundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub prior_strategy_id: StrategicRevisionId,
    pub prior_lowering_id: LoweringId,
    pub affected_scope: AffectedScopeDigest,
    pub import_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReturnReassessmentOutcome {
    NoChange {
        reason: BoundedText<4096>,
    },
    Relower {
        target: WorkId,
        changed_work_ids: Vec<WorkId>,
        changed_subjects: Vec<SubjectRef>,
        preconditions: Vec<ReloweringWorkPrecondition>,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnReassessmentStatus {
    Proposed,
    Applied,
    Consumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnReassessmentRecord {
    pub review_id: ReviewId,
    pub binding: ReturnDeltaBinding,
    pub outcome: ReturnReassessmentOutcome,
    pub digest: ReassessmentDigest,
    pub status: ReturnReassessmentStatus,
    pub revision: Revision,
}

schema_tag!(ReturnReassessmentProposedSchema,
    "zap-planning/return-reassessment-proposed/1");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnReassessmentProposed {
    pub schema: ReturnReassessmentProposedSchema,
    pub review: AdaptiveReviewRecord,
    pub binding: ReturnDeltaBinding,
    pub outcome: ReturnReassessmentOutcome,
}
```

The new DataProposal cell validates one ordinary proposed
`AdaptiveReviewRecord` and atomically inserts it with a proposed sidecar keyed by
the same `ReviewId`. Existing `AdaptiveReviewRecord`, `ReviewProposed` and their
wire history remain unchanged. The sidecar's reassessment digest covers the
exact review, import, prior strategy/lowering, affected scope and outcome
fields.

`ReturnReassessmentProposedCell` loads the one `ReturnImportRecord`, requires
`AwaitingReassessment`, exact import revision/digests/scope, and bounds the
review's signals, captured sources, alternatives and work/job changes to the
stored delta and authoritative affected/dependent closure. It may preserve
applicable proof but must revalidate it through `CurrentProofSet`. It cannot
scan unrelated history or treat absence from the delta as proof of no impact.

`NoChange` requires `ReviewDecision::KeepRoute`, no outcome/obligation/work/
deferral mutation and no relowering target. Applying that review atomically
marks the review and sidecar `Applied`, links its ID from the import and sets
the import to `NoChange`. Candidate evidence remains candidate evidence for the
ordinary acceptance path; the no-change decision grants no acceptance.

`NoChange` is unavailable while the stored affected closure has an unknown
boundary or the delta contains a contradiction, failure, new unknown or stale
material whose impact is unresolved. Those cases require a bounded relowering
target, which may be diagnostic/research work rather than invented product
steps.

`Relower` requires nonempty sorted changed work/subject/precondition sets
contained in the stored affected/dependent closure and a target containing that
change. Its
existing `ReviewTransition` carries any semantic work, obligation, ownership,
deferral and live-job reconciliation. `ReviewApplied` remains the R07-admitted
semantic transition and requires the complete actual affected-job view. It
marks the sidecar `Applied` and import `ReloweringRequired`; it does not create
a lowering itself. Reviews without a return sidecar keep their current behavior.

## Exact causal handoff to the next R08 lowering

R08's v2 lowering payload/record reserves the return-specific optional binding:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReassessmentBinding {
    pub review_id: ReviewId,
    pub review_revision: Revision,
    pub reassessment_digest: ReassessmentDigest,
    pub return_digest: ReturnBundleDigest,
    pub affected_scope: AffectedScopeDigest,
    pub previous_lowering_id: LoweringId,
    pub preconditions_digest: PayloadDigest,
}

// Add to LoweringApplied and LoweringRecord:
pub reassessment: Option<ReassessmentBinding>;
```

The next lowering may consume this binding only when the review is applied, its
sidecar is `Applied`, its return import is `ReloweringRequired`, the previous lowering is the exact
current predecessor, `preconditions_digest` equals the sidecar's canonical
precondition set, and the target/change sets equal the reassessment. The R08
kernel compares predecessor and proposed graphs: every semantic change must be
inside the affected/dependent closure or a necessary integration edge from it;
all rows outside that set remain byte-for-byte semantically equal. At least one
bounded semantic change is required. Unrelated current lowerings and packets
remain current.

The lowering's one product `ChangeSet` inserts the new causal lowering,
preserves the predecessor and every prior packet/attempt/encounter as history,
and changes the import from `ReloweringRequired` to `ReloweringApplied` with the
new lowering ID while marking the sidecar `Consumed`. A binding cannot be
consumed twice or by another return.
Approach epoch/history records are neither reset nor renamed. A lowering that
merely follows the return in time but lacks this exact binding refuses.

Reassessment proposal generation may occur in a strong planning environment,
but enters only through the sealed DataProposal issuer. Reducers, affected
closure, review apply and lowering remain deterministic and perform no model or
external call.

## Live cell and provider ownership

The two cross-sibling mutations are registered by the app composition root:

```rust
pub trait ReturnResolutionProvider: Send + Sync + 'static {
    fn affected_request(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
    ) -> Result<AffectedScopeRequest, ZapError>;

    fn resolve(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
        affected: &AffectedScopeView,
    ) -> Result<ResolvedReturnImport, ZapError>;
}

pub struct ResolvedReturnImport {
    pub encounters: Vec<EncounterRecord>,
    pub failed_approaches: Vec<FailedApproachRecord>,
    pub counter_updates: Vec<ApproachEpochRecord>,
    pub import: ReturnImportRecord,
}

pub struct ApplicationReturnImportedCell {
    resolver: Arc<dyn ReturnResolutionProvider>,
}

pub struct ApplicationReturnAffectedScope {
    resolver: Arc<dyn ReturnResolutionProvider>,
}

impl ApplicationReturnImportedCell {
    pub fn new(resolver: Arc<dyn ReturnResolutionProvider>) -> Self;
}

impl ApplicationReturnAffectedScope {
    pub fn new(resolver: Arc<dyn ReturnResolutionProvider>) -> Self;
}

impl TransitionCell for ApplicationReturnImportedCell {
    type Payload = ReturnImported;
    type Output = DomainMutation;
    // TrustedObservation; writes the declared R09 import/history families.
}

impl PayloadAffectedScope<ReturnImported>
    for ApplicationReturnAffectedScope
{
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &ReturnImported,
    ) -> Result<AffectedScopeRequest, ZapError>;
}
```

`ApplicationReturnImportedCell` is `TrustedObservation`, calls `resolve` with the
exact R07 transaction-derived scope, and writes only the declared R09 domain
record families. The resolver is pure and fixed at composition. Both app cells
are included by a new `zap_app::cross_domain_cell_set()` which
`foundation_composition()` composes with domain/runtime/legacy cells.

Live zap-domain registration removes the old caller-manifest
`BundleExportedCell`, standalone `EncounterRecordedCell`, and old
`ReturnImportedCell`. Their exact v1 payloads/reducers remain in the frozen
schema-1 replay registry. Live v2 uses the app-owned bundle/return cells,
`BundleArchivePublishedCell`, and the existing domain review/lowering cells.
There is one validation path for an encounter: the ordered return resolver.

Zap-domain registers `ReturnReassessmentRecord` and the new
`ReturnReassessmentProposedCell` as a sealed-issuer `DataProposal` route. The
existing `ApplyReview` descriptor statically adds the sidecar/import families;
the cell reads or mutates them only when the review ID resolves an exact
sidecar, updating them atomically with the review. Reviews without a sidecar
retain their existing behavior.

App cells follow the ordinary one-transaction rules: their provider outputs are
validated, record families are declared, duplicate keys conflict, and replay
reruns the pure resolver against historical state. No cross-sibling record type
is moved into core and no new universal commit/preflight extension is created.

## Ordered implementation units

1. Add the two wire digests and the R09 offline DTO/record types under
   `zap-domain/src/lowering/offline.rs`; add `selection_action` to the R08 fork
   semantic shape before its first live write.
2. Add bundle status/archive receipt and the app `BundleClosureProvider`,
   captured v2 payload/cell, artifact extractor and composition registration.
3. Add the offline journal builder and bounded delta validator, sharing it with
   return resolution.
4. Add return/counter records, `ApplicationReturnAffectedScope`, pure return
   resolver and app-owned import cell; retire the three bypassing v1 live cells
   into schema-1 replay only.
5. Add the return reassessment sidecar/proposal cell and narrow sidecar handling
   to adaptive review apply, with exact no-change versus relower validation and
   existing proof/affected-job gates.
6. Add `ReassessmentBinding` to R08 lowering before its v2 first write and make
   the lowering kernel consume/update the return record atomically.
7. Exercise the focused real service roundtrip below, then run the affected
   core/domain/runtime/app/store gates and the normal Rust floor.

R09 owns the new lowering offline module/records, return review sidecar,
app cross-domain provider/cells and their tests. R08 owns the shared fork and
lowering binding edits and coordinates them before first v2 write. R15 owns all
physical archive implementation. R13D owns schema-1/history assembly and R13
owns transport; their additive registrations are preserved.

## Focused real service roundtrip

Acceptance uses one real redb store, the production composition and exact
registered routes. A deterministic fixture weak executor and R15 artifact
adapter are explicitly marked simulated; they exercise protocol mechanics and
never invoke a model, external launcher or upload.

1. Through the accepted R07/R08 service journey, create an active charter,
   current strategy/lowering, two current packets, current sources/rules/forks/
   stop rules/capabilities, and two actual `RuntimeJobRecord`s with exact
   attempts and derived producers. Leave both jobs claimed and unlaunched.
2. Ask the app bundle provider to prepare a two-packet closure. Verify the v2
   payload contains the exact current strategy/lowering, packet contracts and
   bases, both job/attempt/producer/dispatch bindings, full immutable source/
   rule/fork/capability/workspace/result-schema entries, charter-derived
   permissions and every active typed `StopRuleId`. A caller cannot add or omit
   any of those lists because they are absent from the request.
3. Submit the captured app-owned bundle command and commit `Prepared`. Have the
   R15 fixture publish the exact archive receipt through trusted observation;
   commit `Ready`. Missing entry bytes, wrong entry/manifest digest, a
   `ConditionId` substituted for a stop rule, a foreign capability, unallowed
   fork action, credential-like extra entry, traversal path or oversized plan
   refuses before ready. Physical no-clobber/path/symlink behavior remains R15's
   own acceptance suite.
4. Drive both offline attempts through the real R11 lifecycle, using the
   simulated bridge only as the configured host adapter: commit pre-effect
   authorization, consume the exact dispatch intent once, obtain and persist the
   dispatch receipt/handle, reconcile any uncertain handoff, and ingest
   running/terminal observations with matching trusted driver provenance. Do not
   inject a synthetic terminal row directly. For the first terminal job, submit
   `runtime.candidate-recorded` before return import, producing the real
   `CandidateResultRecord` and `CandidateProvenanceRecord`. The second job
   produces a candidate-less observation and one failure bound to an existing
   problem/epoch/approach digest. No caller or import cell seeds either candidate
   record.
5. Build the offline digest-chained journal with `AttemptStarted`, a permitted
   `ForkSelected`, `CandidateProduced`, the candidate-less `Observation`, and
   `Failure`. Prove sequence/causes/digests and encoded size stay within the
   manifest bounds. R15 captures the return archive and the service's artifact
   gate verifies every new artifact.
6. Submit `ReturnImported/2` through the exact trusted archive observation. The
   one transaction inserts ordered encounters,
   the return import, one failed-approach history row, and exactly one increment
   of the existing approach counter. The candidate is
   `ApplicableCandidate`; the candidate-less row is
   `ApplicableObservation`; the failure remains `Failure`. Replaying the same
   failure in another command/bundle does not increment again.
7. Exact-retry the import and then try conflicting reuse of bundle, return,
   encounter, sequence and candidate identities, a foreign job/attempt/producer,
   a candidate ID with no provenance, and a candidate whose artifact/contract/
   basis differs. Every conflict refuses with the prior return and history
   unchanged. An otherwise valid `CandidateProduced` row with no candidate ID
   fails delta validation; an ordinary candidate-less observation never becomes
   applicable candidate.
8. Propose a return-triggered adaptive review through the sealed DataProposal
   issuer. Its stored delta binding and changed sets must equal the import's
   authoritative affected/dependent closure. Apply the review through the real
   R07 semantic/affected-job path and obtain `ReloweringRequired` without yet
   changing lowering.
9. Submit the second R08 lowering with the exact `ReassessmentBinding`. Change
   the affected contract while retaining its Work ID under the exact predecessor
   and CAS precondition. The review explicitly revalidates/preserves the terminal
   candidate and reconciles its effect; the new work/contract revisions and
   validation generation each advance exactly once. Preserve unrelated current
   lowerings/packets, and atomically mark the import `ReloweringApplied` with the
   new lowering ID. Prove the old strategy, lowering, packets, attempts,
   candidate/provenance, encounters, failed-approach row and counter remain
   queryable while old proof is non-current for the new generation. A temporally
   adjacent lowering with no binding, stale CAS, foreign origin, unresolved
   candidate/effect, wrong review/return/scope, reused binding, missing generation
   bump or out-of-closure change refuses.
10. In a sibling return with only an applicable candidate/observation and no
    material semantic delta, propose/apply `NoChange`. It sets the import to
    `NoChange`, creates no lowering and grants no acceptance; the candidate may
    proceed only through the ordinary current proof/acceptance routes.
11. Close/reopen and audit schema-2 events. Bundle replay uses the captured v2
    payload and historical state only; return replay revalidates the same bounded
    delta/provenance. Removing an immutable archive artifact makes R15's
    availability gate report `Unavailable` and blocks offline use without
    substituting current bytes or changing replayed semantic mutations.

Focused tests cover domain offline DTOs/journal, app bundle/return providers and
cells, runtime provenance integration, adaptive review causality and the R08
second-lowering handoff. Root acceptance also requires the affected crate gates
and normal Rust floor. Broad bundle-query indexing and physical archive mechanics
remain with their assigned owners.
