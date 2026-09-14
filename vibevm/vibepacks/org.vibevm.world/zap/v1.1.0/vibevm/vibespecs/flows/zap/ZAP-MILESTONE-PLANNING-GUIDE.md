# Milestone planning and selective lowering {#root}

`guide r1`

This guide describes the Rust API that adopts milestone-centered planning and
enforces it in the existing lowering path. It implements the planning and
materialization rules in
`spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root`.

## Plan records {#plan-records}

`guide r1`

`MilestonePlanProposalRecord` is an immutable semantic proposal. Its
`MilestonePlanKey` combines the active outcome with a consecutive plan
generation. The proposal binds one exact strategic record, outcome revision,
relevant basis, and the current revisions of every milestone it uses.

The plan stores one current focus and its unsatisfied milestone prerequisite
closure. Every other unsatisfied milestone is a `MilestonePlanHorizon` with an
explicit refinement trigger. Achieved milestones remain in the plan history,
but are neither frontier work nor unresolved horizons. When all retained
milestones are currently achieved, the focus is `None` and the frontier and
horizon lists are empty.

`obligation_coverage` is a deterministic inverse map from every current,
retained outcome obligation to the milestone revisions that require it. The
admission kernel recomputes the map from milestone definitions. A caller cannot
make a plan appear smaller by omitting an outcome obligation from a candidate
strategy.

Boundary rationales are semantic input. They identify a consumer outcome,
decision boundary, or independently provable capability and bind current source
captures. The kernel validates identities, source currency, coverage,
dependencies, focus and lineage. It does not claim to prove the producer's
domain judgment from labels.

## Adoption {#adoption}

`guide r1`

`MilestonePlanProposed` stores candidate data and grants no execution. Submit
`MilestonePlanAdopted` through the privileged `plan.lower` route to move the
outcome's `MilestonePlanStateRecord` pointer with exact CAS. Adoption uses the
normal relevant-basis, effect simulation and change-economics path. The first
plan for a candidate strategy participates in the initial-lowering baseline;
later plan generations are semantic changes.

The adoption payload contains the full immutable proposal, not only its hash.
The service compares it with the stored proposal and updates only the outcome's
adopted pointer. Older proposals remain readable. A reassessment may create new
canonical milestone boundaries and then adopt a successor plan that retains
unchanged milestone revision IDs. No Work is created by proposing or adopting a
plan.

## Refinement {#refinement}

`guide r1`

For an outcome without an adopted milestone plan, established lowering behavior
is unchanged. Once `MilestonePlanStateRecord` exists, every
`planning.lowering-applied` command must have an immutable
`RefinementPlanRecord` keyed by that lowering ID. Propose it with
`RefinementPlanProposed` before submitting the lowering.

The refinement record binds:

- the exact lowering semantic digest and canonical `LoweredGraph` digest;
- the adopted plan key, fingerprint and plan-state CAS revision;
- the exact strategy identity, record revision and semantic digest; and
- one `WorkMaterializationRationale` for each Work record that the lowering
  would create, including a new root container.

The mandatory check runs inside `apply_lowering_kernel` after ordinary graph,
obligation, proof, stage and deferral validation and before graph
materialization. The same kernel is used for economics effect simulation and
actual application. Review relowering and returned-bundle relowering enter the
same path. A new strategy revision cannot omit the plan: its exact binding will
fail until a successor milestone plan is adopted.

Each new Work rationale uses one typed cause:

- `AdvancesResult` binds a frontier milestone and exact covered obligations.
- `ResolvesBlocker` requires a current or submitted nonterminal blocker whose
  typed `depends_on` edge names the proposed Work.
- `SelectedInformation` binds a persisted information selection, its current
  opportunity and basis fingerprints, and the complete stop-rule snapshot.

The expected result must match the executable candidate contract's requirement
and artifact identities. Containers name an evidence requirement instead of
inventing an executable artifact. Decision relevance is always explicit.

The kernel derives comparison candidates from current obligation owners,
existing siblings, prior same-target lowerings, and currently applicable proof.
The proposal must account for every candidate. Reuse requires an actual
dependency, contract input, or `reused_evidence` binding. A
`DistinctContribution` or `Insufficient` explanation remains an attributed
semantic claim; the algorithm checks its presence and exact comparison scope,
not its truth from task names. Successor lineage can name only compared Work
IDs. Retries keep their existing Work identity and therefore do not appear as
new materialization.

An information opportunity creates Work only after a persisted selection. The
information service rechecks the exact candidate Work ID and Evidence or
Decision work type, source and decision basis, and acquisition reuse. A
contract-bound stop rule must equal the task contract's deterministic safe-stop
boundary; existing packet validation then carries the same boundary to the
candidate result contract. Decision guidance remains visible without being
reported as machine enforcement.

Every distant plan horizon is copied exactly into the lowering's
`unresolved_horizons`. It remains a diagnostic boundary and never becomes a
runnable task or completion blocker merely because it exists.

## Commands and registration {#commands}

`guide r1`

`MilestonePlanProposed` and `RefinementPlanProposed` are registered data
proposal commands. `MilestonePlanAdopted` is registered as a privileged
`plan.lower` semantic effect with the shared initial-milestone classification,
relevant-basis provider and deterministic simulation contract. The registered
record families are the immutable plan proposal, the outcome-keyed adopted
pointer and the lowering-keyed refinement plan.

## Queries {#queries}

`guide r1`

`MilestonePlanViewQuery` reads either an exact proposal key or the adopted plan
for an outcome. Callers provide `maximum_milestones`; zero, values above the
query profile, and plans beyond the requested bound are refused.

The result shows proposal/adoption state, focus, current achievement validity,
unsatisfied milestone IDs, distant horizons and binding or coverage gaps. A
missing adopted record is reported as a broken binding rather than as absence
of adoption. The query remains readable after focus achievement. It reports
`AllSatisfied` immediately when all retained milestone achievements are
current, even if the historical plan still names the just-achieved focus.

`query_cost` states that the whole plan record is decoded and reports the number
of milestone records and validity evaluations. It also discloses when validity
may perform one store-wide proof pass per evaluation plus recursive milestone
dependencies; it does not present that variable work as an exact scan count.
The view does not evaluate readiness, holds,
capacity or dispatch and therefore does not claim a next executable action.

```rust,no_run
use zap_domain::milestone_planning::{
    MilestonePlanViewInput, MilestonePlanViewQuery,
};
use zap_wire::OutcomeId;

let input = MilestonePlanViewInput {
    outcome_id: OutcomeId::parse("outcome.release")?,
    plan_key: None,
    maximum_milestones: 64,
};

// Execute `MilestonePlanViewQuery` through the registered query service.
let _query = MilestonePlanViewQuery;
let _input = input;
# Ok::<(), zap_wire::ZapError>(())
```
