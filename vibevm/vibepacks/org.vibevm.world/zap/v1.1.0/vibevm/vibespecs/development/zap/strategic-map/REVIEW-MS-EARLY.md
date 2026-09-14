# Early milestone integration review

Final disposition: all actionable findings below were repaired and verified.
The strategy record revision-increment assumption was withdrawn after source
inspection, as recorded below. Exact results and remaining deliberate scope
limits are in [MILESTONES-RESULT.md](MILESTONES-RESULT.md) and the worker reports.

Root review of incrementally written source; these are required fixes and test
cases, not an acceptance report. The sources are still under implementation.

- Bootstrap: StrategyProposed yields Candidate; the first lowering promotes the
  same strategy to Current while preserving its record revision. Milestone
  discovery/adoption must precede first materialization, and deterministic
  state promotion must not invalidate the adopted plan. Test the real
  first-campaign sequence, not only a fixture seeded as Current.
- Proof scope: full record/strategy revision is provenance, not by itself proof
  applicability. Contribution reshuffling or unrelated strategy changes must
  not erase established current achievement when the result, obligations,
  consumer and relevant proof basis survive; relevant changes must revalidate.
- Reference integrity: milestone dependency/contribution scope and current
  lifecycle must be checked, including missing/foreign references. Known
  strategic Work may legitimately not yet be materialized.
- Information cost: finite material CostUnknown entries cannot be ignored while
  deriving a positive benefit/cost verdict. Explain inclusion in totals or keep
  the recommendation unknown until the uncertainty is accounted for.
- Decisive observation: cheaper-decisive ranking needs validated coverage of the
  alternatives being distinguished, not any nonempty subset.
- Stop rules: research stop conditions must reach its real work/packet or the
  existing control boundary. Stored descriptive limits must not be reported as
  runtime-enforced limits.

All findings were sent to the owning gpt-5.6-sol/high workers during construction.
Final review must record their disposition against the resulting implementation.

Further source findings:

- Opportunity query limits must bound reading, not test family length after an
  unbounded scan. Selection/evidence joins and peer comparison need explicit
  bounds and current-record validation.
- AlreadySatisfied must check the current opportunity basis before current
  applicable retained proof; stored Accepted/Current flags alone are not proof
  freshness. Decisive alternatives need equivalent actual decision meaning.
  Equal opportunities must not mutually dominate each other.
- Direct milestone revision must obey the same contribution/dependency/proof
  conservation as typed transformations. Removed consumer/obligation successor
  records must be valid, active and in scope. A payload AuthorizationRef is not
  self-authenticating; unused authority fields cannot stand in for admission.

Transformation and final-state review:

- Split/merge must preserve or explicitly remap typed dependencies, including
  internal edges collapsed by a merge. Active proposed records must validate
  against a bounded atomic after-state; blanket rejection of internal batch
  references cannot implement normal dependent split/merge cases.
- Route change must expose exact added/removed/dormant contributions and proof
  consequences. Matching proof fingerprints alone does not justify a constant
  contributions_conserved=true result.
- Exact milestone queries must not flatten a missing linked receipt into no
  achievement. Prerequisite achievement must match its declared binding or a
  validated applicability-preserving transfer, not any new receipt by ID.
- Plan views must distinguish ordinary distant-horizon debt from invalid-plan
  triggers; otherwise every valid plan with distant work demands reassessment.
  A missing adopted/explicit proposal is not NoAdoptedPlan. Final achievement
  should display AllSatisfied without a fictitious new focus or status-only
  planning round. Query-specific reads need finite bounds and honest proof-cost
  disclosure.

Locality and causal refinement:

- Empty information-selection bindings must return without scanning opportunity
  families; otherwise many optional questions block ordinary lowering. Selected
  research should read bounded relevant decision/acquisition/selection partitions,
  with a locality test showing unrelated rows cannot exhaust its read budget.
- ResolvesBlocker needs an actual causal dependency/current blocker relation;
  merely naming any submitted Work is insufficient justification. Existing scoped
  blockers outside the submitted graph must not be rejected solely for absence
  from that one batch.

Correction verified against the actual lowering code: the root initially
assumed candidate-to-Current promotion increments StrategicPlanRecord.revision.
It changes state only and retains that record revision. No revision-increment or
promotion-normalization change is authorized or needed. The original bootstrap
failure was the Current-only acceptance restriction, not revision drift.
