# ZAP adaptive job reconciliation API

Adaptive review commits a job disposition but performs no process effect in
the pure domain reducer. This module is the durable effect adapter that applies
those dispositions without converting process exit into a claim about task
safety.

## Public seam

`zaplib.runtime_reconciliation` exports:

- `reconcile_applied_reviews(coordinator, actions) -> None`: materializes every
  unapplied job plan from an applied domain review, performs its idempotent
  transport/runtime step, and appends public action summaries to `actions`.
- `reconciliation_blocks_progress(state, job_id) -> bool`: true while an
  applied drain or revalidation plan has not reached a safe terminal state.
  The coordinator calls this before submitting or advancing that job, closing
  the one-tick gap between review application and plan materialization.
- `RECONCILIATION_HANDLERS`, `RECONCILIATION_EVENT_SCHEMAS`,
  `RECONCILIATION_EVENT_ROUTES`, `RECONCILIATION_ACTION_KINDS`,
  `RECONCILIATION_OBSERVATION_KINDS`, and
  `RECONCILIATION_DATA_KINDS`. The last collection is empty.

The coordinator composes the handlers and service classifications, checks
`reconciliation_blocks_progress` before job submit/result advancement, and
calls `reconcile_applied_reviews` after transport/status and pause delivery,
before result advancement, verification or dispatch scheduling.

## Events and routes

| Event | Exact payload fields after `schema` | Route |
| --- | --- | --- |
| `runtime.reconciliation-planned` | `review_id, review_event_id, outcome_id, items` | trusted observation |
| `runtime.reconciliation-progress-observed` | `review_id, job_id, phase, compatibility, transport, safe_state, detail` | trusted observation |
| `runtime.reconciliation-revalidation-released` | `review_id, job_id, work_id, expected_job_state, expected_work_state, from_generation` | action `plan.lower` |

Schemas are `zap-runtime/reconciliation-planned/1`,
`zap-runtime/reconciliation-progress/1`, and
`zap-runtime/reconciliation-revalidation-released/1` respectively.

Each planned item is exactly
`{job_id, work_id, attempt_id, action, safe_boundary, reason, captured_state,
contract_version, contract_sha256, source_captures}`. The reducer derives and
checks it against the applied review, review capture and runtime job. It cannot
introduce or omit a reviewed job.

Compatibility is exactly
`{status, contract_version, contract_sha256, source_status, obligation_ids,
reasons}`. Status is `compatible`, `incompatible` or `unknown`. It binds the
current active task contract, exact owned-obligation set, current source
captures and effective work state to the job's immutable captures.

Safe state is exactly `{status, receipt_sha256, receipt}`. Status is `unknown`,
`needs_reconcile`, `safe`, `completed` or `not_started`. A positive state
requires a receipt whose canonical hash matches `receipt_sha256`. A transport
`prepared` result with no live process proves `not_started`; other positive
states require the configured task/postcondition verifier. Cooperative stop,
PID exit or a terminal transport result alone never proves safety or reconciled
external effects.

## Action semantics

- `continue` and `finish_compatible` complete only while contract, sources,
  owned obligations and work state remain compatible. They do not stop or
  relaunch the job.
- `preserve_candidate` keeps the same attempt. A natural terminal result may
  move through the existing privileged candidate event; stopped,
  interrupted, failed, blocked, invalid, stale and unknown-effect results
  cannot become candidates. Their exact result remains in the reconciliation
  history as a rework artifact and the item stays `needs_reconcile`.
- `drain` always uses transport cooperative stop. Completion requires an
  independent safe/completed/not-started receipt. Without it the item remains
  `needs_reconcile`.
- `revalidate` uses the same stop and safe-state boundary, then releases the
  prior runtime attempt. The dedicated privileged
  `domain.work-revalidation-readied` event advances the work validation
  generation and readies the same work/problem identity. Evidence, stage,
  integration and work acceptance bind their validation generation; prior
  generation proof stays historical and cannot accept the fresh attempt.

Campaign or applicable scoped owner pause takes precedence before every item
effect, including the final domain readiness event after a durable runtime
release. The adapter does no item effect until that pause is resolved. A
pending/unknown asynchronous action assessment likewise leaves the durable
item at its prior phase for a later tick instead of aborting peer progress. A
plan names only its captured jobs, so unrelated jobs continue.

All stop request IDs and event IDs derive from stable review/job identity.
Replaying or restarting resumes the existing plan and transport request; it
does not issue a second process launch or turn an unknown effect into a retry.
