# Final R07 architecture and source review

Status: blocking repair remains. The current source supports transaction-derived
effect preparation, per-effect before/after validation over a shared overlay,
persisted no-op basis requests, ReviewApplied kernel reuse, the shared
economics completion provider, and schema-2 replay. The findings below prevent
acceptance of the complete R07 control/economics boundary.

## P1: scoped Owner pauses are bypassed by exempt progress and proof actions

`ChangeControlAdmissionProvider::needs` requests an affected-scope preflight for
a semantic change, or for an exempt action only when an economics hold already
exists (`crates/zap-domain/src/economics/admission.rs:70-101`). With no hold, a
progress/proof action reaches `admit` with no candidate scope
(`admission.rs:129-130`). `pause_matches_scope` then treats every work- or
subject-scoped pause as nonmatching because both branches require `Some(scope)`
(`admission.rs:541-552`).

Concrete counterexample: create an active `PauseScope::Work([W])`, keep the
economics hold set empty, then submit the registered privileged
`domain.work-renamed` action for W. That cell is classified `Progress`
(`crates/zap-domain/src/control/cells.rs:593-598`), so `needs` returns no scope,
the work pause is not seen, and the provider emits an exempt admission without
an exception. The same bypass applies to proof-classified privileged actions.
This violates sticky scoped Owner control; economics exemption must exempt only
the economics decision, not the pause.

Smallest fix: derive the registered affected scope for every privileged action
that can be matched by a scoped pause (the uniform and safer rule is every
privileged action), then require an exact unconsumed exception for each matching
pause. Add a real service case for `domain.work-renamed` and one proof action
under a work/subject pause with no economics hold. The existing scoped-pause
case uses a semantic fixture, while the other precedence case uses a campaign
pause, so neither distinguishes this path.

## P1: advancing a committed prefix freezes the prior effect's action and authority

When `ChangeAdmissionPreparedCell` advances from an applied effect to the next
effect, `advances_prefix` requires the new admission to retain the previous
record's `action`, `hold_id`, and `decision_id`
(`crates/zap-domain/src/economics/admission_cells.rs:144-153`). Those values
belong to the individual effect/current adjudication, not to the immutable
committed prefix.

Two required paths therefore dead-end:

- An approved heterogeneous envelope containing `domain.review-applied`
  (`adaptive.apply`) followed by `planning.lowering-applied` (`plan.lower`)
  cannot prepare its second admission because the registered action changes.
- An initially automatic multi-effect change that crosses the threshold after
  its first effect cannot continue after exact Owner approval: the next
  admission must add the new forecast hold and decision, but the consumed first
  admission has `hold_id = None` and `decision_id = None`.

The result is a permanently incomplete selected envelope/active hold even
though the new effect separately passes the current forecast, decision, item,
suffix, and payload checks. The evidence does not cover either transition: the
two-effect economics service journey uses the same fixture kind/action for both
effects (`crates/zap-domain/tests/economics_service.rs:1602`), the forecast
journey starts already Owner-required and ends in rejection
(`economics_service.rs:2178`, `economics_service.rs:2503`), and the ordinary
review-to-relowering journey performs two separate assessments/admissions
(`crates/zap-domain/tests/lowering_semantic_service.rs:29-30` and its review
helper).

Smallest fix: define prefix advancement by the same assessment/change,
alternative, exact applied prefix, and next index/predecessor closure. Validate
the next effect's registered action and its latest exact forecast/decision/hold
as current per-effect authority, without requiring those fields to equal the
consumed effect's values. Prove one approved review-to-lowering envelope through
first commit, churn, suffix preparation, second commit and retry, plus one
automatic-to-Owner-required forecast continuation.

## P2: the assessment comparison basis is forced to be the first effect's local basis

Public preparation derives a nonempty bundle's `initial_basis` from its first
effect's registered local basis (`crates/zap-core/src/preflight.rs:137-148`).
Assessment proposal then requires every feasible alternative's first
`relevant_before` to equal the single assessment basis
(`crates/zap-domain/src/economics/assessment_cells.rs:141-153`), and
adjudication reconstructs every bundle with that same value
(`crates/zap-domain/src/economics/preflight.rs:53-64`). Because
`RelevantBasisDigest` includes `BasisPurpose` in its canonical digest body
(`crates/zap-core/src/basis/mod.rs:256-278`), alternatives whose first effects
have different registered kinds necessarily have different local domains even
when they start from the same store state and affected union.

Concrete counterexample: compare a task-contract replacement proposal with a
factual deferral-based cheaper remedy for the same obligation. Their first
effects use different `Mutation(EventKind)` purposes. Both can be strictly
prepared, but no single `assessment.relevant_basis` can equal both digests, so
the assessment is refused before the decision table can compare them. This
prevents a required factual cheaper alternative from participating in the
economics gate.

Smallest fix: derive and persist an assessment-wide comparison basis from the
union of executable alternatives, while retaining each effect's registered
local before/after basis for simulation and admission. Do not equate the union
comparison digest with the first local effect digest. Add an adjudicated
multi-alternative case whose feasible alternatives start with different
registered semantic kinds; the current economics contract fixtures give every
executable alternative the same `domain.task-contract-replaced` kind.
