# Accepted R08/R09 adaptive-review relowering amendment

Status: narrow implementation amendment for root acceptance (2026-09-13). It
supersedes only the causal-binding and CAS paragraphs in the R08/R09 contracts.

## Fixed resolution

An applied adaptive review is sufficient causal authority to prepare a later
semantic lowering whether it arose from ordinary live work or an offline
return. `ReturnDeltaBinding` is additional evidence when present; ordinary
review never invents one.

Review application and lowering observe two lawful states.
`reconcile_work_change` owns the first transition: it increments `WorkRecord`
revision and maps `Revalidate` to `Blocked`, while leaving
`validation_generation` unchanged. R08 lowering compares that actual
post-review state, then owns the single generation increment and semantic
contract/work update.

## Exact sidecar and lowering binding

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         Serialize, Deserialize)]
pub struct ReviewReloweringKey {
    pub review_id: ReviewId,
    pub previous_lowering_id: LoweringId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewWorkCas {
    pub work_id: WorkId,
    pub pre_review_revision: Revision,
    pub pre_review_state: WorkState,
    pub post_review_revision: Revision,
    pub post_review_state: WorkState,
    pub validation_generation: u64,
    pub active_job: Option<JobId>,
    pub contract_id: Option<ContractId>,
    pub contract_version: Option<Revision>,
    pub contract_digest: Option<ContractDigest>,
    pub candidate_basis: RelevantBasisDigest,
    pub current_candidate_ids: Vec<CandidateId>,
    pub affected_jobs_digest: PayloadDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewReloweringStatus {
    Pending,
    Consumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReloweringRecord {
    pub key: ReviewReloweringKey,
    pub applied_review_revision: Revision,
    pub affected_scope: AffectedScopeDigest,
    pub work: Vec<ReviewWorkCas>,
    pub return_cause: Option<ReturnDeltaBinding>,
    pub digest: ReassessmentDigest,
    pub status: ReviewReloweringStatus,
    pub consumed_by: Option<LoweringId>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReloweringBinding {
    pub key: ReviewReloweringKey,
    pub record_revision: Revision,
    pub digest: ReassessmentDigest,
}

// Add to LoweringApplied and LoweringRecord.
pub review_cause: Option<ReviewReloweringBinding>;
```

The key has canonical composite `RecordKey` encoding. One applied review may
create one sidecar per affected current predecessor lowering. Work rows and
candidate IDs are sorted and unique. The digest covers all causal/CAS fields,
excluding only digest, status and consumption bookkeeping.

`ApplyReview` constructs each row from the same before/after `WorkRecord`
values it sends to `ChangeSet`. For `Revalidate`, it records checked-next
revision, post-review `Blocked`, and unchanged generation. Contract CAS is the
exact active contract at that boundary. `affected_jobs_digest` covers the
complete transaction-derived job rows used by review reconciliation, never
caller IDs.

The sidecar is created for an ordinary applied review whenever its selected
transition requires semantic relowering of a current lowering target. If an
exact applied R09 return-reassessment sidecar exists, `return_cause` copies and
validates it; otherwise it is `None`. Both paths use identical R07 admission,
affected closure, job/candidate reconciliation and predecessor rules.

The trigger is deterministic: a current predecessor gets a sidecar when it
contains a work named by `Revalidate`, `Supersede`, or `Drop`, or when a
`Research`, `ReplaceMethod`, or `PivotOutcome` review names that predecessor's
target in its affected closure. Pure `KeepRoute`, `Reorder`, `Wait`, and
`OwnerProposal` reviews create none unless one of those work operations is
present. Same-ID semantic reuse is available only for `Revalidate`; superseded
or dropped work remains historical and its named successor is lowered instead.

## Post-review CAS and single generation owner

A lowering with `review_cause` requires a `Pending` sidecar, exact applied
review revision/digest, exact current predecessor origin, and current work,
contract, job and candidate values equal to the **post-review** CAS. Pre-review
fields are provenance and are never compared as current state.

For a changed same-ID work row, lowering writes:

- `revision = post_review_revision.checked_next()`;
- `state = Planned` and `active_job = None` after exact reconciliation;
- `validation_generation = prior_generation + 1`, exactly once; and
- either a checked-next replacement of the exact contract or a new explicitly
  bound active contract with canonical digest.

`ApplyReview` never increments validation generation or contract version.
Lowering refuses a missing or double increment. Changed generation, contract
and relevant basis make prior proof/candidates non-current through existing
applicability rules while preserving candidate, provenance, evidence, job,
packet and acceptance history.

A current candidate does not permanently reserve the Work ID. Reuse is lawful
when the review explicitly preserves it as history, revalidates it, or
supersedes its route and every affected job has exact reconciliation. A
starting/running/unknown effect, missing candidate disposition, unsafe job,
post-review CAS drift or foreign predecessor refuses. Successful lowering marks
the sidecar `Consumed`, records `consumed_by`, and cannot consume it twice.

The packet's `render_basis` and a returned candidate's execution basis identify
two lawful `Dispatch(work)` states. Packet rendering seals the Ready pre-dispatch
state. R08 packet resolution validates that state, permits only the exact
Ready-to-Active `WorkDispatched` progress, and seals the runtime job and
candidate provenance with the resulting current execution basis. The two
digests are therefore not compared directly.

`ApplyReview` captures candidates on its pre-state by deriving the exact current
`Dispatch(work)` basis and requiring each candidate to match it, the Work
subject, active contract, validation generation, and its persisted producer
packet's identity. Packet state is not required, so a pure rerender does not
discard proof. After review the Work is Blocked and that pre-state basis cannot
be recomputed, so `ReviewWorkCas.candidate_basis` stores the exact captured
digest. Lowering requires every sealed candidate ID to retain that provenance
and rescans candidates by the same candidate basis and producer packet, Work,
contract, and generation lineage. The sorted set must equal the sealed set. A
stale sibling from another execution basis remains history but does not enter
the set; a new or removed candidate on the sealed basis is CAS drift and
refuses. Runtime candidate ingress already bound each such candidate to its
exact job execution basis, so this post-review check neither widens provenance
nor silently drops returned proof when review changes current Work facts.

## Exact ownership

- The adaptive-review owner adds the sidecar record/registration, makes
  `apply_work_changes` return its actual before/after rows, groups them by
  current predecessor and inserts sidecars in the `ApplyReview` transaction.
- The R08 owner adds `review_cause` to v2 lowering and implements post-review
  CAS, one generation bump, proof invalidation and sidecar consumption.
- The R09 owner supplies `return_cause: Some(...)` only for an exact applied
  return reassessment. Ordinary review uses `None`.
- R07 requires no core change. Its accepted effect-item versus bundle/suffix
  identity amendment stays unchanged.
- `PacketRendered` continues using `BasisPurpose::Dispatch(work_id)` with
  independent exact current lowering/strategy checks.

## Single-generation service sequence

1. Create current lowering L1 and work W at revision `w`, generation `g`, with
   contract C version `c`; produce a real terminal R11 candidate for W.
2. Apply an ordinary `ReplaceMethod`/`Revalidate` review with complete job
   reconciliation and no return record. W becomes `Blocked` at `w+1`, remains
   generation `g`, C remains `c`, and its sidecar has `return_cause == None`
   plus exact post-review CAS.
3. Submit L2 under exact predecessor L1 and that sidecar. Change W's admitted
   contract semantics with the same Work ID. R07 preflight proves the batch; W
   becomes `Planned` at `w+2`, generation `g+1`, contract version `c+1`, and
   the sidecar is consumed. Old candidate/provenance/proof remains readable but
   is non-current for `g+1`.
4. Repeat with an R09 return-caused review. The same path succeeds with exact
   `return_cause`; wrong return/delta/scope refuses. Ordinary review still needs
   no return.
5. Sibling cases using pre-review `w`, concurrent post-review drift, missing or
   double generation increment, missing candidate/job disposition,
   running/unknown effect, foreign predecessor or consumed sidecar all refuse
   with zero mutation.

This amendment adds no new authority path and does not alter the accepted R07
effect identity or R08 packet-basis decisions.

## Initial executable root amendment

`LoweredGraph` adds `root: Option<WorkRecord>`. `Some(root)` is permitted only
for the first lowering of an ordinary campaign when the exact `parent_id` does
not yet exist. The root has that identity, no parent or dependencies, no active
job, a container work kind, `Planned` state and the transaction revision. It is
not a contract leaf and is materialized atomically with the lowering and child
graph. `None` requires the exact external parent to exist and be a live
non-executing container; a dangling parent or a colliding root refuses.

For this bootstrap only, `LoweringBasisScope` uses the already existing
obligation and source subjects and does not name absent root or child Work
subjects. Normal lowering of an existing target includes that target Work
subject. The registered schema-2 lowering extractor emits
`ActionImpactRule::InitialLoweringOrSemantic { strategy_id, target }` from the
strictly decoded payload. The domain impact provider derives `InitialBaseline`
only when the active charter, intent, outcome and strategy match and no prior
`LoweringRecord` exists anywhere in the campaign; every later lowering is a
semantic change independent of economics-baseline record existence. The pure
lowering kernel remains responsible for validating and atomically creating the
root, so a malformed root cannot gain initial-baseline authority.
