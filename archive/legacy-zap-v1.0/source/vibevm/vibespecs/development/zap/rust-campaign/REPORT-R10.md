# R10 Dreamer implementation report

Status: review candidate. Root acceptance remains separate.

## Implemented boundary

`zap-domain/src/dreamer/**` implements a registered, durable Dreamer cell with
four distinct layers:

- `DreamExplorationStarted` records hypothetical exploration.
- `DreamScopeChangeRequested` records an explicit live add, remove, move or
  replace request. It remains detached until admitted.
- `DreamApplied` is the only live product transition. It is a registered
  `plan.lower` semantic effect and therefore uses the existing relevant-basis,
  affected-scope/job, economics, Owner, pause and causal-admission machinery.
- `DreamCombinedOwnerDecision` is the dedicated authenticated route for a
  scope change that needs both one exact charter expansion and the normal
  above-threshold economics decision. It applies those two existing kernels
  atomically and stores their common immutable binding.

A branch stores one exact base strategy identity/revision, an affected-scope
digest, typed attachment state, delta operations, assumptions, unknowns,
alternatives, grill transcript and optional economics assessment identity. It
does not copy the live strategy or Work graph. Strategy projection is computed
from the current state and delta by one pure kernel.

Attachment ambiguity has no exact attachment constructor. It persists as an
`Unresolved` attachment with a placement question and distinct candidates.
Only an authenticated Owner answer can select one saved candidate. Exact
attachments begin with `GrillState::Offered`; promotion remains unavailable
until the Owner declines the grill or every persisted question has a typed
answer and the service records a transcript digest.

Factual and Owner answers are distinct commands and authority routes.
`DreamFactAnswered` requires a trusted observation and exact current source
captures. `DreamOwnerAnswered` requires Owner control, an existing choice and,
for placement, the matching saved attachment candidate. Questions and answers
use stable IDs and record revisions, so restart or compaction resumes the
stored interview rather than starting it again.

## Projection, uncertainty and economics

`project_dream` and the registered `zap.planning.dream` query recalculate from
one snapshot without dispatch or live mutation. A saved recalculation is a
detached projection record. Global/economics churn rebinds when the exact
affected scope is unchanged; changed relevant scope marks the projection stale.

Projection exposes measured structural consequences:

- added and removed goal counts, affected obligations and retained proof;
- affected and dependent Work, proof revalidation and live-job counts;
- affected lowerings, stage debt, deferrals and retained artifacts.

These counts are structural burden evidence, not invented hours, utility or a
universal score. Actual value, elapsed time, agent effort, uncertainty and
recommendation remain in the selected `ChangeAssessmentRecord`. A live branch
names that assessment ID; `DreamApplied` carries the exact projection digest in
its immutable effect payload, and product application requires the adjudicated
assessment to contain that exact payload. The six-hour service case also
persists the bounded Dream uncertainty as a typed economics `CostUnknown`.

`DreamUnknownDisposition` distinguishes `AdmissionCritical` questions from
`BoundedForEconomics` uncertainty. Critical incompleteness blocks promotion.
Bounded fog remains visible in projection and the exact assessment, and may be
accepted through normal economics/Owner policy. Dreamer does not require the
entire knowledge graph to become complete.

Combined charter/economics preparation is the sole explicit policy-fingerprint
exception. The immutable Dream payload binds original and replacement charter
IDs, revisions and digests; its basis and assessment comparison mark the
generic policy fingerprint NotApplicable. All structural roots and other
semantic inputs remain exact, and the ordinary Owner decision kernel still
checks the independent active `ChangePolicyRecord`. This makes the projection,
payload, item digest, local basis and comparison basis identical across the
Owner charter commit without an intervening recalculation write.

## Live application and removal

The shared Dream effect/product kernel rechecks the saved/current projection,
branch revision, base scope, strategy revision/digest, exact basis, complete
affected-scope view and safe-job state. Simulation and execution use the same
strategy-delta reducer. Simulation receives no write or authority handle.

Application replaces only the current strategy version and explicitly
supersedes affected lowerings and packets. Additions are strategic nodes over
existing active obligations; a later checked lowering materializes executable
Work. Removal requires an exact `DreamRemovalPlan` covering every current:

- active obligation and successor Work;
- dependent Work and replacement prerequisite;
- applicable evidence and candidate artifact;
- affected lowering stage debt;
- deferral and successor Work;
- affected external job identity.

The kernel refuses an incomplete denominator before effect preparation.
Successful removal drops the selected Work, transfers obligation ownership,
rewires dependents and deferrals, retains evidence/candidate history, records
stage/artifact dispositions and supersedes the affected lowering. It never
deletes proof or history.

## Held execution repair discovered by R10

The first nonempty-job removal exposed a shared hold-release deadlock: the old
safe-job request required the post-product affected-scope digest to equal the
pre-product hold digest. Any authorized semantic mutation of Work, obligations
or dependencies therefore made release impossible.

The root-approved amendment is recorded in
`AMENDMENT-R10-HELD-JOB-SAFETY.md`. New holds persist
`safe_job_mode = HeldExecutions` and exact job/attempt/work/contract/generation
identities from the admitted pre-state. Hold release resolves every held
identity directly with the shared safe predicate and also evaluates every job
in the current post-product scope. A missing job, changed attempt or binding,
started/unknown effect, unsafe state, or newly affected unsafe job refuses.

Prior records and views default explicitly to `ExactScope`; the original
request/view digest formulas are retained for that mode. Frozen zap/1 bytes are
unchanged.

## Focused evidence

`run-cargo.ps1 test -p zap-domain --test dreamer_service` passed 4/4:

1. `hypothetical_grill_ambiguity_and_recalculation_stay_detached` persists an
   ambiguous placement, Owner placement answer, later factual question/trusted
   answer, completed transcript and saved/queryable recalculation. Unrelated
   economics churn rebinds the projection. Live strategy, Work, holds and
   application records remain unchanged.
2. `expensive_scope_add_uses_exact_owner_pause_and_effect_admission` binds one
   measured projection and bounded fog row into a six-hour assessment. Exact
   adjudication creates a hold, one Owner decision approves the envelope, a
   campaign pause blocks application without consuming it, resume/rebound
   applies it once, exact retry succeeds, and the hold releases.
3. `removal_requires_complete_dispositions_and_preserves_proof_history` refuses
   an omitted proof/job disposition, then applies the complete removal through
   an Owner hold with one NotStarted held job. Changed attempt, unsafe effect,
   missing held job and a newly affected unsafe job each refuse release without
   advancing head. Restoring exact safety releases the hold. The product exact
   retry and cold schema-2 audit both pass.
4. `one_owner_response_binds_charter_amendment_and_exact_dream_envelope`
   proves the old charter cannot apply the product, an active campaign pause
   blocks the combined response, and charter-only/economics-only credentials
   do not imply combined authority. One combined Owner event applies only the
   saved action/obligation expansion, records the normal economics decision and
   immutable common binding, and creates no recalculation record. After
   unrelated interruption, the identical payload/item/local/comparison bases
   rederive; a wrong replacement binding refuses, while the exact product,
   retry, hold release and cold audit pass without a second approval.

Additional receipts:

- `run-cargo.ps1 test -p zap-domain --test economics_contracts` — 5/5.
- `run-cargo.ps1 test -p zap-domain --test economics_service owner_decision_order_pause_and_completion_share_the_real_service_path -- --exact` — 1/1.
- `run-cargo.ps1 clippy -p zap-core -p zap-domain --lib --test dreamer_service --test economics_contracts --test economics_service --test knowledge_service --no-deps -- -D warnings` — exit 0.
- Focused `rustfmt --edition 2024 --check` over R10 Dreamer, held-job core,
  economics and test files — exit 0.
- Every Dreamer production source file is at most 511 lines.

## Explicit boundaries

R10 does not require produced-root comparison. Its one `DreamApplied` effect
reads existing active attachment, obligation, Work, contract, source and proof
roots; the accepted R07 limitation for roots created only by an earlier effect
remains recorded for R16.

Current live additions reuse existing active outcome obligations. When the
saved Dream requires a charter action/mutable-obligation expansion, the
dedicated combined Owner route binds and applies that exact expansion together
with the above-threshold economics decision. It cannot create a new Outcome or
Obligation record; those remain ordinary intent/outcome revision work rather
than an implicit Dreamer scope enlargement.

The R08 packet named an installed attribution-policy file that is absent from
this package. R10 performed no Git operation; root retains attribution and
commit responsibility. No model, local inference, NEXT execution, UI,
publication or external effect was invoked.
