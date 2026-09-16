# ZAP change economics design API

This document freezes a proposed implementation contract for review. It does
not add handlers, schema registries, gates, hooks, or commands.

## Policy

The lazy default is `zap-change-policy/1`:

```json
{
  "schema": "zap-change-policy/1",
  "policy_id": "change-policy:default",
  "revision": 1,
  "parent_sha256": null,
  "threshold": {
    "metric": "expected_elapsed_hours",
    "comparison": "strictly_greater_than",
    "hours": 4.0
  },
  "unknown_handling": {
    "cost": "owner_if_threshold_possible",
    "impact": "hold_unproven_independent"
  },
  "estimation_budget": {"elapsed_hours": 0.5, "agent_hours": 1.0},
  "cost_band_model": {
    "low_max_threshold_ratio": 0.25,
    "moderate_max_threshold_ratio": 1.0,
    "high_max_threshold_ratio": 2.0
  },
  "utility_model": {"mode": "qualitative_default", "weights": null}
}
```

The 4-hour threshold is Owner-confirmed. The estimation budget is the proposed
bounded implementation default. Exactly 4.0 is not above the threshold;
4.0001 is. Hours are nonnegative finite values and every numeric interval
satisfies `low <= expected <= high`. Elapsed hours and agent-hours remain
separate.

Cost handling is `owner_if_threshold_possible`,
`owner_if_expected_unknown`, or `owner_if_unbounded`. Impact handling is
`hold_unproven_independent` or `hold_all_starts`. No mode treats unknown as
below threshold.

The default `owner_if_threshold_possible` requires Owner when interval high is
null or greater than `T`; a bounded interval wholly at/below `T` may use its
conservative high even when expected is null. `owner_if_expected_unknown`
requires Owner whenever expected is null and otherwise compares expected to
`T`. `owner_if_unbounded` requires Owner for null high; a bounded interval uses
expected when present and otherwise its conservative high. These modes change
only how explicit bounded uncertainty is compared; missing structural evidence
remains blocked. `hold_unproven_independent` blocks the known closure and starts
without complete independence proof; `hold_all_starts` blocks every new start.

The utility mode is `qualitative_default` or `owner_calibrated`. Calibrated
weights are exactly `{owner_benefit,risk_reduction,urgency,
strategic_optionality,reversibility,confidence}`, each an integer 0–5 with at
least one nonzero value. They rank dimensionless utility bands and are never
subtracted from time.

No further Owner choice is needed to implement the built-in defaults: the
confirmed threshold is `T=4.0`; unknown handling is
`owner_if_threshold_possible/hold_unproven_independent`; estimation stops at a
maximum 0.5 elapsed hour or 1.0 agent-hour; cost ratios are 0.25/1.0/2.0; and
utility uses the unweighted total table below. These are versioned policy
defaults, not facts inferred from a provider. A different value requires exact
Owner policy activation. Team capacity has no invented fallback: missing
configured/evidenced nominal parallelism or capability/resource assumptions
makes the estimate explicitly unknown under policy.

The implicit default does not mutate legacy state. Configuration uses a
data-only `economics.change-policy-proposed` event followed by Owner-only
`control.change-policy-activated`, binding the full policy hash, next revision,
and parent hash. Both policy activation and an economics Owner decision are
credential-channel-only: even a configured owner host principal cannot create
them. `submit_control` authenticates the campaign-matching Owner credential.

## Assessment and relevant basis

`zap-change-assessment/1` is exact:

```text
assessment_id, change_id, summary, necessity, baseline, affected_scope, relevant_basis,
alternatives, recommended_alternative_id, recommendation,
admission_disposition, comparison_reasons, estimation
```

`necessity` is exact
`{class,obligation_ids,constraint_refs,problem,basis,evidence_refs}`. Class is
`mandatory_problem`, `obligatory_safeguard`, or `optional_improvement`.
Mandatory/safeguard requires a current obligation or constraint and evidence.

`baseline` is audit provenance, exactly:

```text
baseline_id, base_sha256, committed_prefix_sha256, committed_seq,
observed_zap_revision, observed_domain_revision, active_charter_sha256,
active_intent_id, active_outcome_id, active_outcome_sha256,
observed_plan_sha256, change_policy_revision
```

Global revisions and whole-plan hash describe where estimation occurred; they
are not admission-equality keys. The service creates one internal
`economics.baseline-established` boundary on the first supported estimation or
semantic-change attempt after the exact initial charter-bound intent/outcome
exists. Reads, service construction, and charter activation alone do not write
that boundary. Events at or before its captured committed prefix are historical
baseline; the attempted new effect is excluded and requires economics admission.

`affected_scope` is the exact `zap-change-affected-scope/1` returned by
`affected_change_scope` below and covers the union needed to compare all
executable alternatives.

`relevant_basis` is `zap-change-relevant-basis/1`, exactly:

```text
change_policy_sha256, active_charter_sha256, active_intent_sha256,
active_outcome_sha256, affected_scope_sha256, subject_fingerprints,
source_captures, contract_captures, dependency_fingerprints,
consumer_fingerprints, proof_fingerprints, knowledge_fingerprints,
team_model, team_model_sha256, basis_sha256
```

Subject/dependency/consumer/proof rows are exact `{kind,id,sha256}`; contract
rows are `{work_id,version,sha256}` and source rows `{source_id,sha256}`. Lists
are unique and sorted. `basis_sha256` hashes this exact object without itself.
Knowledge fingerprints bind relevant applicability, closure, fog-region, native
fact, and invalidation rows; unrelated regions are omitted.
Economics events, hold/transport receipts, global revision counters, and
unrelated progress are projected out.

`team_model` is exact:

```text
model_id, profile_sha256, executor_classes, nominal_parallelism,
resource_capacities, scheduling_assumptions, evidence_refs
```

Executor rows are `{class_id,capability_ids,nominal_capacity}`. The model binds
configured/evidenced capacity assumptions used by the estimate, not current
free slots, queue occupancy, quota, provider overload, or transient jobs.
Ordinary churn does not stale approval or inflate engineering burden. A changed
configured team model does.

Every alternative is exact:

```text
alternative_id, kind, summary, solves_mandatory_problem,
preserved_obligation_ids, sacrificed_obligation_ids, utility,
incremental_cost, feasibility, effect_bundle, basis, evidence_refs
```

Kind is `proposal`, `cheaper_alternative`, or `no_op`; feasibility is
`feasible`, `infeasible`, or `unknown`. Proposal and no-op are mandatory. Every
feasible executable alternative carries actual effects; no-op has `[]`.

An effect row is exact:

```text
effect_id, index, kind, payload, payload_sha256, subject_ids,
predecessor_effect_ids, relevant_before_sha256, relevant_after_sha256,
product_event_id
```

`payload` is the detached immutable event payload and is validated against the
real handler schema before estimation. Its canonical hash must match.
`product_event_id` is prebound for idempotent crash recovery. Index is
zero-based and consecutive. Predecessors name earlier effects. Pure ordered
simulation computes each relevant-after basis, which becomes the next causal
precondition. An alternative that cannot supply validated payloads remains
unknown rather than a factual cheap option.

`effect_fingerprint` is the SHA-256 of the entire canonical effect row,
including payload, subject/predecessor IDs, before/after hashes, index, and
product event ID. A payload hash alone is never an approval fingerprint.

`incremental_cost` is exact:

```text
basis, expected_elapsed_hours, elapsed_interval_hours,
expected_passive_wait_hours, passive_wait_interval, total_agent_hours,
agent_hours_interval, precision, elapsed_band, engineering_burden_band,
consequence_band, uncertainty_band, overall_cost_band, categories,
unknowns, excluded_costs, attribution_summary
```

Basis is `incremental_attributable/1`; precision is `measured`,
`bounded_estimate`, or `order_of_magnitude`. Intervals are `{low,high}` and
contain expected values. Nullable expected/high requires an explicit unbounded
unknown. Elapsed includes passive waits on the critical path. Passive wait and
agent-hours remain separate.

Categories contain exactly:

```text
implementation, verification, migration, documentation,
proof_revalidation, dependencies_consumers, operations_maintenance,
passive_wait_external, fog_uncertainty
```

Each row is exact `{category,applicability,agent_hours_interval,
elapsed_contribution_interval,consequence,basis,evidence_refs}`.
Applicability is `included`, `not_applicable`, or `unknown`; consequence is
`negligible`, `low`, `moderate`, `high`, or `critical`. Overlapping elapsed
contributions are not summed. Passive wait affects time to verified result but
does not become engineering effort.

Unknown rows are exact `{unknown_id,category,question,lower_bound_hours,
upper_bound_hours,materiality,resolution_action,evidence_refs}`. Excluded-cost
rows are `{kind,summary,basis}` with kind `retained_approved_baseline` or
`sunk_cost`. The same attributable item cannot be included and excluded.

Utility is exact `{overall_utility,dimensions,beneficiaries,time_window,
confidence,basis,evidence_refs}`. Dimensions are `{owner_benefit,
risk_reduction,urgency,strategic_optionality,reversibility}`, each
`{band,basis}`. Utility uses `negligible|low|moderate|high|critical`; confidence
uses `unknown|low|moderate|high`.

### Cost bands

Let `T` be the active elapsed threshold and `P` the team model nominal
parallelism. Elapsed bands are: negligible at zero; low in `(0,T/4]`; moderate
in `(T/4,T]`; high in `(T,2T]`; critical above `2T` or unbounded. Engineering
burden applies the same ratios to agent-hours against `T*P`. Consequence is the
maximum included category consequence. Uncertainty is negligible with none,
low when bounded within one below-threshold band, moderate across adjacent
below-threshold bands, high when an upper bound crosses `T`, and critical when
unbounded or materially unknowable. Overall qualitative cost is the maximum of
engineering burden, consequence, and uncertainty. Elapsed band remains
separate because only elapsed controls the Owner threshold.

### Total decision matrix

Structurally invalid payloads or evidence too incomplete to identify a
defensible executable alternative yield recommendation `investigate_unknown`
and disposition `blocked`. A structurally valid selected alternative with
explicit unknown/unbounded cost follows active conservative unknown policy; by
default it retains the substantive recommendation, becomes
`owner_decision_required`, and creates its atomic unknown-boundary hold. For
optional work with bounded evidence:

| Utility | Bounded qualitative cost eligible for take |
| --- | --- |
| negligible | none; reject |
| low | negligible or low |
| moderate | negligible, low, or moderate |
| high or critical | any bounded cost when undominated; elapsed threshold is applied only to admission |

An eligible optional alternative is taken only when undominated; otherwise the
recommendation is `continue_baseline`. Mandatory/safeguard selects the
least-cost feasible alternative that preserves the same obligations. If the
proposal is the only evidenced feasible solver, recommend it. If no evidenced
solver is feasible, investigate rather than pretend necessity proves
feasibility.

Recommendation is exactly `take_proposal`, `prefer_alternative`,
`continue_baseline`, or `investigate_unknown`. Admission disposition is
separate: `automatic`, `owner_decision_required`, or `blocked`. Every selected
executable recommendation above `T`, or unknown under active policy, becomes
`owner_decision_required`; this does not replace the recommendation. Low-value
optional rejection and structurally incomplete needs-evidence are blocked and
create no hold because no executable alternative was selected.

The combinations are closed: `take_proposal|prefer_alternative` requires a
non-null executable `recommended_alternative_id` and disposition
`automatic|owner_decision_required`; `continue_baseline` requires the exact
no-op alternative ID and `blocked`; `investigate_unknown` requires null
recommendation ID and `blocked`. `automatic` requires bounded
elapsed at/below `T` and null hold/Owner decision. `owner_decision_required`
requires the atomic non-null hold. `comparison_reasons` is always nonempty, so
threshold conversion retains the substantive reasons shown to Owner.

`estimation` is exact `{elapsed_hours,agent_hours,stopped_because,assumptions,
evidence_refs}` with `sufficient`, `budget_reached`, or
`evidence_unavailable`. The configured budget is a maximum, never a target or
minimum. When enough evidence exists earlier, estimation stops earlier.

### Forecast refresh

`zap-change-cost-forecast/1` is exact:

```text
forecast_id, assessment_id, previous_forecast_id, trigger,
original_baseline_id, completed_effect_ids, team_model_sha256,
cumulative_actual, remaining_estimate, total_to_verified_estimate,
changed_relevant_basis, unknowns, evidence_refs
```

Actual, remaining, and total rows each preserve elapsed, passive-wait, and
agent-hour values/intervals as exact `{elapsed_hours,elapsed_interval_hours,
passive_wait_hours,passive_wait_interval,agent_hours,agent_hours_interval}`.
Trigger is `effect_completed|relevant_input_changed|scope_changed|
team_model_changed|estimate_corrected|proof_invalidated`.
`changed_relevant_basis` is null for an effect completion that preserves basis,
otherwise exact `{before_sha256,after_sha256,changed_fingerprints}`. Total-to-
verified is cumulative attributable actual plus remaining critical path against
the original baseline. Refresh never
relabels completed cost as sunk to reset remaining hours. A material team,
scope, input, or estimate change re-adjudicates the next unconsumed effect. If
the total forecast crosses the threshold, work holds before that effect and a
new exact Owner decision is required. Routine state progress alone does not
refresh or stale the forecast.

Raw wall-clock passage, Owner deliberation, queue occupancy, quota exhaustion,
and transient provider overload are operational waits. They remain observable
but do not increase engineering burden, become architecture failure, or by
themselves trigger reapproval. Proposal-intrinsic external latency remains a
separately estimated passive-wait component of elapsed time.

### Estimator adapter

Estimation starts from exact `zap-change-estimation-input/1`:

```text
proposal_id, change_id, summary, necessity, baseline,
alternative_seeds, affected_scope, relevant_basis, team_model,
source_captures, evidence_refs
```

An alternative seed is exact `{alternative_id,kind,summary,
solves_mandatory_problem,preserved_obligation_ids,
sacrificed_obligation_ids,feasibility,effect_bundle,basis,evidence_refs}`. It
contains real validated effect payloads but no invented cost or utility.
Proposal and no-op seeds are required. The service recomputes affected scope and
relevant basis before recording a request.

`zap-change-estimation-request/1` is exact:

```text
estimation_id, request_id, input, policy, policy_sha256,
estimation_budget, request_sha256
```

`request_sha256` hashes the request without itself. The budget is copied from
the active policy and is a maximum for elapsed and agent effort. The exact
`zap-change-estimation-response/1` is:

```text
request_id, request_sha256, captured_basis_sha256,
assessment, observations, usage
```

`assessment` is a complete `zap-change-assessment/1`. Observation rows are
`{observation_id,category,claim,result,evidence_refs,source_captures}` and grant
no acceptance or authority. Usage is exact `{elapsed_hours,agent_hours,
stopped_because}` with `sufficient|budget_reached|evidence_unavailable`; it may
not exceed the request budget. Exhaustion produces explicit unknowns rather
than another hidden estimator call.

The public nonblocking protocol is:

```python
ChangeEstimatorAdapter.submit(request_id, request) -> transport receipt
ChangeEstimatorAdapter.poll(request_id, request) -> {ready, ok, response?, diagnostic?}
ChangeEstimatorAdapter.request_stop(request_id, stop_id) -> stop receipt
ChangeEstimatorAdapter.profile_sha256() -> sha256

request_change_estimation(service, estimation_input, estimator) -> request receipt
poll_change_estimation(service, estimation_id, estimator) -> status
```

Request, submission, result, staleness, waits, wait release, and stop receipts
are durable and idempotent. Result application revalidates request hash,
relevant basis, policy/team model, evidence/source captures, and usage before it
records the returned assessment as a proposal. The estimator cannot adjudicate,
place a hold, create Owner authority, or admit an effect. Quota, provider, and
Owner waits remain resource/decision waits and never add engineering cost.
Transient retry uses one recorded boundary and server hint; deterministic
configuration failure waits for input/profile change. Invalid output receives
bounded validator feedback under the same one-repair rule as the coordinator.
No nested uncontrolled CLI retry is added.


## Events and routes

| Event | Exact payload fields after `schema` | Route |
| --- | --- | --- |
| `economics.baseline-established` | `baseline` | service internal |
| `economics.change-policy-proposed` | `policy` | agent data |
| `control.change-policy-activated` | `policy_id, policy_revision, policy_sha256, parent_sha256` | credentialed Owner control only |
| `economics.change-assessment-proposed` | `assessment` | agent data |
| `economics.estimation-requested` | `request` | service internal |
| `economics.estimation-submitted` | `estimation_id, request_sha256, receipt` | trusted observation |
| `economics.estimation-result-recorded` | `estimation_id, request_sha256, response, response_sha256, transport` | trusted observation |
| `economics.estimation-stale` | `estimation_id, request_sha256, captured_basis_sha256, current_basis_sha256, reason` | service internal |
| `economics.estimation-wait-recorded` | `wait_id, estimation_id, request_sha256, classification, observed_at_ns, next_retry_ns, diagnostic` | trusted observation |
| `economics.estimation-wait-released` | `estimation_id, request_sha256, wait_id, observed_at_ns, release_basis_sha256` | service internal |
| `economics.estimation-stop-observed` | `estimation_id, request_sha256, stop_id, receipt` | trusted observation |
| `economics.change-assessment-adjudicated` | `assessment_id, assessment_sha256, policy_id, policy_revision, recommendation, admission_disposition, recommended_alternative_id, comparison_reasons, hold` | service internal |
| `economics.cost-forecast-refreshed` | `forecast` | trusted observation |
| `economics.cost-forecast-adjudicated` | `forecast_id, forecast_sha256, recommendation, admission_disposition, comparison_reasons, hold_update` | service internal |
| `control.change-decision-recorded` | `decision_id, assessment_id, assessment_sha256, forecast_id, forecast_sha256, policy_id, policy_revision, recommended_alternative_id, choice, reason, effect_fingerprints` | credentialed Owner control only |
| `economics.change-admitted` | `admission_id, assessment_id, assessment_sha256, forecast_id, forecast_sha256, decision_id, alternative_id, effect_index, effect_fingerprint, product_event_id, logical_command_sha256, action_sha256, action_assessment_sha256, relevant_before_sha256` | service internal |
| `economics.change-command-applied` | `assessment_id, admission_id, alternative_id, effect_index, product_event_id, logical_command_sha256, relevant_after_sha256` | service internal or crash-derived consumption |
| `runtime.change-hold-stop-observed` | `hold_id, job_id, request_id, receipt` | trusted observation |
| `runtime.change-hold-safe-state-observed` | `hold_id, job_id, state, receipt_sha256, receipt` | trusted observation |
| `runtime.change-hold-work-released` | `hold_id, job_id, work_id, disposition, expected_state` | action `plan.lower` |
| `economics.change-hold-resolved` | `hold_id, assessment_id, resolution, successor_hold_id, product_event_ids, released_work_ids, summary` | service internal |

The adjudication reducer creates its nested hold in the same event when
`admission_disposition=owner_decision_required`; otherwise `hold` is null. The
hold is exact `{hold_id,alternative_id,affected_work_ids,dependent_work_ids,
consumer_ids,unknown_boundary,drain_job_ids,reason}`. This removes the crash
window between deciding that Owner approval is required and blocking affected
work. A forecast adjudication can retain, expand, or supersede that hold but
cannot silently shrink it.

`hold_update` is null when disposition remains automatic/blocked without an
existing hold, otherwise exact `{operation,prior_hold_id,hold}` with operation
`retain|expand|supersede`. `hold` is the same exact hold shape above. `expand`
must be a set superset of prior affected/dependent/consumer/unknown scope;
`supersede` keeps the prior hold linked and unresolved until the successor is
stored. There is no shrink operation.

Recommendation is `take_proposal`, `prefer_alternative`,
`continue_baseline`, or `investigate_unknown`. Admission disposition is
`automatic`, `owner_decision_required`, or `blocked`. Only a selected executable
alternative with Owner-required disposition creates a hold; expensive options
that were merely compared do not.

Owner choice is `approve_exact_proposal`, `reject_and_continue_baseline`,
`request_revision_and_hold`, or `defer_decision`. Approval carries the complete
ordered fingerprints of the selected alternative. Other choices carry `[]`.
`forecast_id` and hash are null when no refresh supersedes the original
assessment and otherwise bind the latest adjudicated forecast.

Hold resolution is `approved_applied`, `rejected_baseline`, or
`superseded_by_revision`; successor hold is required only for supersession. A
stale or partially applied hold is not resolved. Safe-state is `safe`,
`completed`, `not_started`, or `unknown_effect`. Work release is `retry_ready`,
`preserved_candidate`, `baseline_unchanged`, or `not_started`. Positive safe
state requires an exact receipt; process exit alone is insufficient.

The eleven existing action strings remain unchanged. Internal economics events
use a new `internal_kinds` service registry and private `_record_internal`
method. They are neither host observations nor public actions. Service
construction refuses overlap or an unclassified handler and route descriptors
publish `service_internal`. Only service methods can call `_record_internal`.

The constructor extension is exact:

```python
ApplicationService(
    ...,
    internal_kinds=(),
    change_event_classification=None,
    change_gate=None,
)

_record_internal(command) -> record receipt
```

`internal_kinds` is disjoint from control, data, action, and observation kinds.
`change_event_classification` must cover every composed product handler or
construction refuses. `change_gate` implements the pure/publicly testable
`classify(kind,payload)`, `preflight(state,events,command,action,assessment)`,
and `reconcile(state,events,product_event_id)` methods. No caller receives
`_record_internal` or a service-internal submission route.

`preflight` returns null for progress/baseline-exempt commands or exact
`zap-change-admission-plan/1` with `{classification,baseline_id,change_ref,
assessment_id,decision_id,alternative_id,effect_index,effect_fingerprint,
relevant_before_sha256,hold_id}`. `decision_id` and `hold_id` are nullable only
for automatic below-threshold admission. `reconcile` returns exact
`{state,admission_id,product_event_id,next_missing_step}` with state
`absent|reserved|product_committed|consumed|stale|held`; it performs no write.

Policy activation and Owner change decision are added to a
`credential_owner_only` control registry. `submit_host` refuses them even when
its configured host principal has role owner. `submit_control` must authenticate
an opaque campaign-matching Owner credential. Coordinator, runtime, actor text,
and agent data can never create those decisions.

## Semantic-change classification

`CHANGE_EVENT_CLASSIFICATION` classifies exact handler/payload variants and is
required at service construction.

Each value is immutable `ChangeClassSpec(kind, classify)`, where
`classify(state,payload) -> semantic_change|progress_exempt|baseline_exempt` is
a named pure function. It first runs the real payload validator and may inspect
only typed fields/current baseline. The registry covers every service
`action_kind` plus conditional `plan.refined`; data, observation, control, and
service-internal registries retain their owning route and do not enter this map.
There is no default classification, name-prefix inference, or prose matching.

| Event or variant | Class |
| --- | --- |
| `plan.refined`, `domain.plan-lowered` | semantic change |
| `domain.outcome-adopted` | initial exact charter baseline only before the baseline boundary; otherwise semantic change |
| `domain.review-applied` | semantic for outcome, obligation, ownership, priority/work, deferral, or nontrivial job-disposition changes; pure recorded keep-route is exempt |
| `domain.task-contract-replaced` | semantic change; an exact effect in the current envelope consumes that admission without a new assessment |
| `domain.deferral-created`, `.transferred`, `.inapplicable` | semantic change |
| `domain.work-transitioned` to `dropped`, `superseded`, or `deferred` | semantic change |
| display rename and ordinary progress among planned/ready/active/candidate/accepted/blocked | progress exempt |
| dispatch and `domain.work-revalidation-readied` | execution/reconciliation exempt after their owning admitted plan |
| proof, acceptance, promotion, source/knowledge, transport, wait, stop, verification, and reconciliation records | proof/progress/observation exempt |
| economics and control proposals/decisions | owning route; not recursively assessed |

The classification uses typed fields, not keywords. Future product handlers
declare a class before service construction succeeds. `DOMAIN_HANDLERS`,
`DOMAIN_EVENT_SCHEMAS`, `DOMAIN_DATA_KINDS`, and `DOMAIN_ACTION_KINDS` do not
change. `DOMAIN_CAPABILITIES` may expose an additive reference to the external
classification registry but does not duplicate its rules.

## Hard admission and causal consumption

The command reason gains optional `change_ref`; `decision_ref` keeps its current
core-decision meaning. `records.validate_command_envelope` validates
`change_ref` as an identity but does not treat it as authority. The application
service resolves it to the stored adjudication or credentialed Owner decision
and the selected next effect. Old envelopes without the field replay unchanged.

For automatic admission `change_ref` names the exact adjudication ID; for
Owner-required admission it names the exact credentialed decision ID. A blocked,
rejected, deferred, stale, differently scoped, or differently hashed record is
not a valid reference. `ApplicationService._command_shape` and
`records.validate_command_envelope` both allow exactly the same optional reason
fields `{evidence_refs,decision_ref,change_ref}`. The reference is only a lookup
key; service-side assessment/effect/basis matching supplies authority.

The service compares only `zap-change-relevant-basis/1` and the next effect's
`relevant_before_sha256`. Global revision movement, economics/hold records,
prior causal effects, and unrelated progress cannot stale it. A changed affected
subject, source, contract, dependency, consumer, proof, policy, charter,
intent/outcome, team model, or effect order does.

Admission extends the existing durable action-reservation pattern. It prebinds
product event ID, full logical command hash, action/action-assessment hashes,
alternative/effect index, and relevant basis. Retry revalidates scoped basis,
finds the same reservation, and applies the product event once. If a crash
occurs after product commit but before `change-command-applied`, the service
finds the prebound committed event and appends or derives causal consumption.
It never launches or applies the product effect twice.

For one semantic action the service sequence is fixed:

1. Load under the configured projection source, validate command/action/current
   control assessment, resolve `change_ref`, and recompute scoped basis/hold.
2. Reuse or durably record the exact `control.action-assessed` result. Its event
   and assessment IDs derive from the product event/action hashes; retry finds
   the exact row rather than creating a conflicting duplicate.
3. Durably record `economics.change-admitted`, prebinding the product event,
   logical command, action, assessment, selected alternative/effect, and
   relevant-before hash. This is a reservation, not the product effect.
4. Durably record/reuse the ordinary `control.action-admitted` reservation so
   the existing product handler retains all charter/stop/action checks.
5. Append the original typed product event exactly once through the ordinary
   recorder.
6. Append or crash-derive `economics.change-command-applied`, verify the
   simulated relevant-after hash, and advance the next causal effect.

Every step is idempotent by deterministic identity and exact hashes. A crash
after any prefix resumes at the next missing step. A relevant change before the
product event stales the economics reservation and retains/updates its hold; an
unrelated event permits explicit scoped rebind. A committed prebound product
event is never replayed as a second effect even when its consumption receipt is
missing. No new multi-record atomic storage primitive is required. The one
decision/hold invariant is atomic inside
`economics.change-assessment-adjudicated`: the nested hold and adjudication
appear in the same reducer event or neither appears.

Supported mutable routes—`ApplicationService`, Engine, CLI, HTTP/backend, and
semantic-runtime application—pass this gate. `storage.record` remains the
low-level append primitive used by the service and explicit recovery tools;
calling it directly is outside the supported application-authority boundary and
is not claimed to be prevented. `apply_command` remains a pure in-memory reducer
seam for replay and tests. Direct append/reducer access proves no economics,
control, or OS-process authority. The threat boundary is the documented
service/credential boundary and makes no raw-file or OS-isolation claim.

## Pure operations

`CHANGE_ECONOMICS_OPERATIONS` exposes:

```text
affected_change_scope(state, effect_bundle) -> zap-change-affected-scope/1
relevant_change_basis(state, affected_scope, team_model) -> zap-change-relevant-basis/1
simulate_change_alternative(state, relevant_basis, effect_bundle,
                            simulators=CHANGE_SIMULATORS) -> zap-change-simulation/1
validate_change_assessment(state, assessment) -> detached assessment
evaluate_change_assessment(state, assessment, policy) -> adjudication payload
change_policy(state) -> detached active/default policy
change_assessment(state, assessment_id) -> detached record
change_hold_status(state, hold_id) -> detached record
change_holds_for(state, work_id) -> detached matching holds
change_hold_guard(state, work_id, read_subjects=(), write_subjects=()) -> guard result
```

`affected_change_scope` returns exact `zap-change-affected-scope/1`:

```text
subject_fingerprints, work_ids, obligation_ids, dependent_work_ids,
consumer_fingerprints, source_ids, dependency_fingerprints,
closure_status, unknown_boundary, basis_sha256
```

It unions the typed subjects declared by every effect with domain ownership,
work dependencies, knowledge dependency edges, evidence/source consumers, and
integration consumers, then walks the relevant transitive closure iteratively.
`closure_status=complete` means this explicitly bounded affected closure is
complete; it never claims the whole campaign/fog is known. `unknown_boundary`
contains exact unresolved endpoint rows `{kind,id,reason}`. Callers cannot omit
the kernel-derived closure or supply a smaller scope. Unrelated regions remain
outside the result.

The assessment basis covers the union of inputs needed to compare every
executable alternative, so a changed cheaper-alternative premise can stale the
choice. Effect simulation and admission project that same captured union to the
selected alternative; unrelated campaign state remains excluded.

`relevant_change_basis` recomputes all rows named by that exact scope plus the
active policy/charter/intent/outcome and configured team model. Its
`basis_sha256` excludes itself, audit-only global revisions, economics records,
holds, transport receipts, and unrelated progress.

`change_hold_guard` returns exact `{status,hold_ids,matched_work_ids,
matched_subject_ids,independence_basis_sha256}` with status
`clear|blocked|unproven`. `clear` under unknown-boundary policy requires a
complete captured independence basis; absence of a known edge is not proof.
Service and runtime use this same pure result.

Simulation returns exact `zap-change-simulation/1`:

```text
initial_basis_sha256, effects, final_basis_sha256,
affected_scope, simulated_state_sha256
```

Each returned effect is `{effect_id,index,kind,payload_sha256,
subject_ids,relevant_before_sha256,relevant_after_sha256,
simulated_state_sha256}`. The next
effect's before hash must equal the previous after hash. The final state hash is
diagnostic; admission compares scoped basis, never the whole simulated-state
hash.

Simulation validates every payload with the actual handler validator and
uses immutable `CHANGE_SIMULATORS[kind]` entries. Each entry is exact
`SimulationSpec(kind, validate_payload, subjects, transition)`; `subjects`
derives typed subject IDs from state/payload so an effect cannot shrink its own
impact declaration, and `transition` is the pure
state transition factored out of the corresponding authority-checking handler.
The simulation registry is explicit and complete for every semantic class—no
dynamic import, arbitrary handler execution, or fallback to `HandlerSpec.apply`
is allowed. Construction refuses a semantic classification without a simulator.
Simulation applies transitions to a detached copy, validates the global graph
after every effect, and returns each exact relevant-before/after hash without
service admission, journal write, credential, clock, model, or external effect.
Characterization tests require simulated state to equal the same transition
through its authorized product handler. Simulation is never a mutation endpoint
or authorization receipt.

## Holds and runtime reconciliation

Only an atomic adjudication with selected executable Owner-required work creates
a hold. It covers affected work, transitive dependents, consumers, and the
configured unknown boundary. Under `hold_unproven_independent`, work continues
only when complete captured closure proves independence; `hold_all_starts`
blocks all starts. A change hold remains distinct from Owner pause, resource
wait, and campaign closure.

Runtime checks holds before selection, claim, submit, verification, semantic
plan application, and readiness. The service repeats the check. It reuses the
G1 reconciliation implementation for cooperative stop, transport identity,
safe-state proof, restart, peer isolation, and explicit release. The runtime
hold events bind the economics hold identity while sharing transport and safe
proof validators with G1.

G1 gains one additive typed source seam:

```python
ReconciliationSource(
    source_kind, source_id, source_event_id, outcome_id, items
)

plan_reconciliation(coordinator, source, actions) -> reconciliation_id
reconciliation_blocks_progress(state, job_id) -> bool
```

`source_kind` is `domain_review|change_hold`; no arbitrary provider is loaded.
The economics adapter derives change-hold items only from the stored atomic hold
and exact captured runtime jobs. A hold item is
`{job_id,work_id,attempt_id,action,safe_boundary,reason}` with action
`drain|preserve_candidate|revalidate|continue|finish_compatible`. G1 owns all
transport stop/delivery/safe-state/release events; economics stores only the
source/hold binding and final reconciliation IDs. A process terminal receipt is
never promoted to safe proof.

`drain_job_ids` is the exact active-job set observed at adjudication. Runtime
also scans durable reservations and adds a reconciliation item for any affected
job that committed before the atomic hold but was not visible in that captured
snapshot. Service hold checks prevent a new affected claim after the hold. This
race recovery never broadens to an independent job.

Campaign and matching scoped Owner pauses dominate every hold action and
release. Approval retains the hold until all effects apply and live jobs are
reconciled. Rejection explicitly returns safely stopped work to the unchanged
baseline while preserving attempts and stops. Revision or deferral keeps the
hold. Partial or stale application never releases it silently.

## Projection, registries, and implementation split

The lazy `extensions.change_economics` projection is
`zap-change-economics/1`:

```text
baseline, policy_revisions, active_policy_id, assessments, adjudications,
estimation_requests, estimation_waits, forecasts, owner_decisions, admissions,
holds, applied_effects, history
```

Legacy replay has no namespace, and service construction/read remains pure. On
the first supported semantic mutation or estimation request after an upgrade,
`ensure_change_baseline(service) -> receipt` records the exact already-committed
accepted prefix when an active charter-bound outcome exists, before considering
the new proposal. Before that outcome exists, only the exact first
charter-bound intent/outcome adoption variants are baseline-exempt; another
semantic kind refuses `BASELINE_PENDING`. A new campaign records the boundary
after that initial envelope; a legacy active campaign records its current
committed prefix. The attempted new effect is never included. The default policy
is a detached view until this boundary or an explicit policy activation is
stored. Reads never establish the boundary.

Proposed public maps are `ECONOMICS_HANDLERS`,
`ECONOMICS_EVENT_SCHEMAS`, `ECONOMICS_EVENT_ROUTES`,
`ECONOMICS_DATA_KINDS`, `ECONOMICS_OBSERVATION_KINDS`,
`ECONOMICS_INTERNAL_KINDS`, `ECONOMICS_ACTION_KINDS`,
`CHANGE_EVENT_CLASSIFICATION`, and `CHANGE_ECONOMICS_OPERATIONS`.
Control maps gain the two credential-only Owner events. Runtime maps gain the
three hold receipt/release events. Engine/service/capabilities compose them.
Domain handler/schema/action/data maps remain unchanged.

The implementation is split across three coding workers with disjoint primary
write ownership:

1. **CE-A — pure model, decision, dependency, and simulation.** Owns new
   `change_economics_model.py`, `change_economics.py`,
   `change_economics_simulation.py`, and `test_change_economics*.py`. It may make
   only the minimal transition-extraction edits in `domain_graph.py`,
   `domain_work.py`, `domain_deferrals.py`, `domain_adaptive.py`, and `domain.py`
   needed for explicit `CHANGE_SIMULATORS`; authority-checking public handlers
   and event behavior must remain byte-compatible by characterization. It
   delivers schemas/routes, projection, relevant/affected fingerprints, team
   model, cost bands, total decision matrix, forecasts, pure queries, and real
   ordered simulation. It does not edit service, control, runtime, estimator,
   CLI, Engine, or backend files.
2. **CE-B — credentialed authority and durable service sequencing.** Owns
   `records.py`, `service.py`, the necessary `control*.py` modules, new
   `change_economics_service.py`, and `test_change_economics_service*.py` /
   `test_change_economics_control*.py`. It implements optional `change_ref`,
   `internal_kinds`, credential-owner-only events, baseline establishment,
   atomic adjudication/hold, scoped preflight, the six-step durable admission
   sequence, crash reconciliation, and defense-in-depth hold checks. It does not
   edit CE-A or runtime/estimator/integration files.
3. **CE-C — estimator and runtime holds.** Owns new `change_estimator.py`,
   `runtime_change_holds.py`, their focused tests, and only the necessary hook
   edits in `runtime.py`, `runtime_loop.py`, `runtime_reconciliation.py`, and
   `runtime_reconciliation_model.py`. It implements durable nonblocking
   estimation, bounded retry/stop, pre-selection/claim/submit/verification/
   readiness hold guards, and a typed economics source adapter over G1 drain,
   safe-state, restart, and release machinery. It does not edit CE-A, service,
   control, CLI, Engine, or backend files.

After those candidates are accepted, the F release integrator alone owns
Engine/CLI/backend/capability composition, generated payload descriptors,
public proposal/credentialed-decision/status/explanation routes, portable
profiles, cross-module fixtures, and final documentation/manifests. Domain and
knowledge only supply the read-only fingerprints/closure and extracted pure
transitions above; reducers never run estimation, provider transport, or
external effects.

## Acceptance matrix

| Property | Positive and negative cases |
| --- | --- |
| Baseline boundary | Initial charter/legacy prefix establishes once; a later mutation cannot masquerade as history |
| Full effects | Actual payload validates before estimation; hash-only, invalid, reordered, or changed payload refuses |
| Scoped basis | Economics events, prior causal effects, and unrelated progress do not stale; changed affected input does |
| Causal bundle | Before/after hashes chain in ordered simulation; crash after product commit consumes prebound ID once |
| Team model | Configured nominal capacity changes stale; free-slot/job/quota churn does not |
| Cost bands | Exact T-relative elapsed/engineering bands, max consequence/uncertainty, and separate passive wait |
| Threshold | 3.9 and 4.0 may proceed when eligible; 4.0001 cannot execute without Owner approval |
| Optional matrix | Every utility/cost band pair yields take or continue-baseline; structurally incomplete evidence yields investigate_unknown + blocked, while a valid conservative unknown follows Owner policy |
| High-value threshold split | Optional high-value 6-hour proposal yields take_proposal + owner_decision_required, not continue_baseline; high-value at/below 4 hours may be automatic |
| Mandatory work | Least-cost same-obligation solver wins; only feasible solver is recommended; above threshold still waits Owner |
| Mandatory threshold split | Mandatory 6-hour sole feasible solver remains recommended but owner_decision_required; necessity never auto-admits it |
| Costly low value | Optional low-value costly work selects continue-baseline and creates no execution hold merely because an unselected option exceeds threshold |
| Alternatives | Proposal, factual cheaper option, and no-op share basis; unevaluable or obligation-losing cheap option is not preferred |
| Attribution | New/redo/consequences included; retained baseline and sunk costs excluded once; omitted consumers/proof/maintenance refuses |
| Forecast | Cumulative actual plus remaining never resets; threshold-crossing total holds before next effect |
| Estimation bound | Sufficient evidence stops early; maximum budget stops analysis with explicit unknowns |
| Classification | Drop/supersede/defer are semantic; rename/progress/observation/current proof reuse are exempt |
| Public gate | Every supported service/Engine/CLI/HTTP/model mutation path refuses missing/stale change_ref; low-level append is outside the application authority boundary; pure simulation and replay remain available |
| Internal route | Coordinator cannot submit adjudication/admission/hold; service-internal events are private and classified |
| Owner authority | Host owner/coordinator/actor labels refuse policy/decision; campaign-matching Owner credential succeeds |
| Atomic hold | Owner-required adjudication and hold appear together or neither appears |
| Live hold | Affected/dependent starts block and drain safely; proved independent work continues; Owner pause dominates |
| Resolution | Exact approve applies; reject restores baseline; revise/defer holds; partial application remains held |
| Legacy/large plan | Old MUP/zap/1 replays unchanged; a local NEXT-sized change charges attributable work, not retained 1,292 obligations |

Tests split into pure economics, real handler simulation, control/service
authority and crash recovery, runtime hold/drain/restart, backend/CLI contracts,
legacy replay, and isolated large-plan integration. No test starts NEXT.
