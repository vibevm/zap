# R07 independent economics and Owner-control review

Status: repair required. The candidate has a sound atomic hook/store foundation,
but the registered R07 service path does not yet satisfy the cumulative forecast,
semantic-classification, affected-hold, and shared-completion hard gates. Root
should not accept R07 at this boundary.

The review treats R04/R05/R06/R11 as the root-accepted foundation and does not
reopen them. Missing mutable application injection and runtime drain orchestration
remain R13/R16 integration work. The findings below are defects in the R07
provider or registered R07 transitions themselves, before that downstream wiring.

## P1: the advertised provider gates every privileged action as an economic change

`CommitService` invokes its one `ActionAdmissionProvider` for every
`RouteClass::Privileged` command (`crates/zap-core/src/commit.rs:1188-1203`).
`ChangeControlAdmissionProvider::admit` has no semantic-change classification or
baseline/progress exemption: it always scans for an exact
`ChangeAdmissionRecord` by product command ID and refuses when none exists
(`crates/zap-domain/src/economics/admission.rs:48-65`). Passing this provider
directly to `CommitServiceBuilder::action_admission_provider`, as REPORT-R07
instructs, therefore makes initial baseline outcome adoption, acceptance,
verification, dispatch, and `campaign.close` require an economics effect record.
Those are expressly baseline/progress/proof routes under
`SEMANTIC-CHANGE-BOUNDARY` and the change-economics classification table.

This is not the deferred act of injecting the provider in `zap-app`; the provider
being injected cannot implement the declared classification. Add a replayable,
typed exemption/classification result (including the initial baseline boundary)
or an equivalent core/domain seam, and prove that semantic mutations are gated
while exempt privileged actions still use their ordinary authority and control
checks.

## P1: forecast history can fork and reset the cumulative authorization denominator

`CostForecastRefreshedCell` accepts `previous_forecast_id = None` even when a
forecast already exists, and a supplied predecessor is checked only for the same
assessment. It never requires the predecessor to be the latest forecast, compares
new cumulative actuals with prior cumulative actuals, or binds
`completed_effect_ids` to the persisted `ChangeAdmissionRecord` prefix
(`crates/zap-domain/src/economics/cells.rs:472-527`).
`validate_forecast` proves only arithmetic inside one record
(`crates/zap-domain/src/economics/decision.rs:277-318`). A later trusted forecast
can consequently report a smaller cumulative actual, a fresh remaining estimate,
and an at-threshold total, then be adjudicated automatic. That recreates the
forbidden fresh four-hour allowance.

The same cell family loses hold lineage: forecast adjudication searches only
`assessment.hold_id`, so a hold first created by an earlier forecast is not found
by the next forecast (`crates/zap-domain/src/economics/cells.rs:591-639`). It can
attempt a duplicate insert or create another active hold instead of retaining or
superseding the existing one. Require one linear latest-forecast chain, exact
current committed prefix, monotone attributable actuals, actual-plus-remaining
totals, and explicit carry-forward of the current hold.

## P1: resolving a hold can hide an unresolved latest forecast from completion

`ChangeHoldResolvedCell` checks decision, effect prefix, listed safe job IDs, and
unknown effects, then marks the assessment resolved; it does not require the
latest forecast to be adjudicated (`crates/zap-domain/src/economics/cells.rs:814-871`).
`economics_blockers` immediately skips every resolved assessment and checks
forecasts only inside the non-resolved assessment loop
(`crates/zap-domain/src/economics/completion.rs:35-58`). Thus an
`effect_completed` forecast can remain unadjudicated, the hold can be released,
and the common provider can return no economics blocker. This directly reopens
the recorded completion P1. Hold resolution must bind the latest adjudicated
forecast, and unresolved forecasts must block independently of the assessment's
resolved flag.

## P1: the live hold scope is accepted from proposal data rather than derived

`ChangeAssessmentProposed` is an agent-data route. Its cell verifies only that
direct effect subjects/work appear somewhere in caller-provided affected lists;
it does not derive the known dependent/consumer closure from current domain state
(`crates/zap-domain/src/economics/cells.rs:303-349`). The service-internal
adjudication then copies those lists and caller-provided independence fingerprints
straight into the active hold (`crates/zap-domain/src/economics/cells.rs:417-447`).
`change_hold_guard` clears an incomplete-closure hold when its caller supplies any
digest present in that list, without binding that proof to the requested work and
subjects (`crates/zap-domain/src/economics/holds.rs:31-73`). A selected change can
therefore omit a known dependent start or claim independence without an exact
current closure witness.

R13/R16 may transport stop and safe-state receipts, but they cannot make this
stored scope authoritative after the fact. Adjudication must consume a
transaction-bound affected/dependency view and an opaque, work/subject-bound
independence witness; the service and runtime may then share the resulting guard.

## P2: forecast adjudication ignores configured unknown-cost policy

Assessment evaluation implements the three `UnknownCostHandling` modes, but
forecast adjudication hard-codes the default threshold-possible rule and treats
any material unknown as Owner-required (`crates/zap-domain/src/economics/cells.rs:578-590`).
For an activated `OwnerIfExpectedUnknown` or `OwnerIfUnbounded` policy, bounded
forecasts can therefore receive the wrong decision and hold. Use the active
policy's exact mode for forecast totals and add all three boundary cases.

## P2: effect assessment validates canonical bytes, not the registered product transition

`ChangeEffect::validate` proves canonical JSON, digest, ordering, and predecessor
shape, but never decodes the payload with the named registered product cell or
derives its subjects and relevant-after basis
(`crates/zap-domain/src/economics/model.rs:415-432`). No R07 assessment or
adjudication path performs the specified handler-aware simulation. The eventual
product command decoder protects the mutation from malformed payload bytes, and
the next command's core basis check protects a later effect, but an invalid
assessment can still obtain an Owner decision/hold and a final effect's claimed
`relevant_after` is never checked. Bind assessments to a registered typed effect
validator/simulator and verify the causal after-basis before final resolution.

## Verified foundation and evidence limits

The core/store portion is good: hook and product `ChangeSet` batches are prepared
against their own declared record/index families, duplicate keys conflict, both
batches commit in one redb transaction, product failure discards the hook batch,
and audit replay calls the registered hook and product cell again rather than
trusting serialized prepared mutations (`crates/zap-core/src/commit.rs:335-479,
908-1095`; `crates/zap-store/src/history.rs:225-285`). Exact Owner route checks,
assessment/forecast/effect digest comparisons, ordered prefix advancement,
campaign-pause precedence, and one-use exception consumption are present in the
reviewed source. The real `zap.control` and `zap.economics` providers are
registered, `zap-app` composes them with runtime providers, and direct close is a
completion-gated cell. Mutable application construction and runtime hold/drain
consumption remain honestly downstream.

The reported `cargo test -p zap-domain` total contains four economics contract
tests and two economics service tests; the other tests exercise other domains.
Both R07 tests do use redb and `CommitService`, but the first inserts every
lifecycle record through a test-only `SeedCell`, and the second seeds its policy
proposal and assessment rather than executing the real assessment-proposal path
(`crates/zap-domain/tests/economics_service.rs:45-105, 479-663, 666-953`). Neither
test executes forecast refresh/adjudication, hold rejection/resolution, the real
exception-grant cell, a product-cell failure after hook mutation, reopen/exact
retry, or an actual close command. The generic zap-store test establishes the
hook transaction/replay mechanics, not these R07 semantics.

The checkpointed hashes still match all eighteen R07-owned/core/store/report
files. The two shared entrypoints (`zap-domain/src/registration.rs` and `lib.rs`)
have later additive hashes; current inspection confirms that all R07 records,
cells, routes, and completion providers remain registered.

## Acceptance recommendation

Repair R07 before acceptance. Focused evidence is sufficient: add real redb
journeys for semantic versus exempt privileged routes; a linear cumulative
forecast that crosses four hours and rejects reset/fork attempts; every unknown
policy mode; a known affected-to-dependent closure plus an unknown-boundary
independence witness; rejection before and after a committed prefix; unresolved
forecast versus direct close; and a product reducer failure after hook preparation
that leaves the admission, exception, hold, product, and head unchanged. Reopen
the store and exact-retry the same committed effect, then audit it through the
real provider. No broad campaign panel is needed for these repairs.
