# R05 coordinator review

Status: the four findings below are repaired and root-reviewed. R05's
registered domain foundation is accepted with eight focused tests, scoped
clippy and formatting evidence. Shared control/source/runtime integration
remains mapped downstream; this does not accept full campaign execution.

## Control provider falsely reports clearance

queries.rs registers ControlCompletionProvider, whose blockers method ignores
state and returns an empty vector. A required control provider cannot be a
success stub. Implement its real evidence or omit/refuse it explicitly until
the control phase supplies that evidence; production readiness and direct
close must report the missing required provider.

## NoDuty reference binding is not permission

CompletionDutyDisposition::authorized_by compares only the public charter ID,
revision and digest. CharterRecord currently contains no policy authorizing
the claimed absence of final-gate or promotion duties. Bind the disposition to
an explicit Owner-controlled charter duty policy/scope and validate it. Merely
copying a current reference must not waive an obligation.

## Completion proof must be current and successful

The final-gate set in domain_blockers accepts any centrally accepted evidence
for the outcome; it does not require observed pass or current applicable
generation/source basis. The obligation coverage loop similarly considers
acceptance rows before checking current work ownership/generation. Reuse the
actual proof-applicability predicates; accepted historical evidence and current
satisfaction are separate facts.

## Authorized empty duties must close consistently

close_campaign requires a nonempty final_gate_evidence_ids list even when an
authorized NoDuty declared an empty set. Compare the exact expected duties and
their disposition consistently rather than imposing contradictory nonemptiness.

These are direct source-review findings. Root did not run additional product
tests during this review. R05 should add only the cases needed to distinguish
the incorrect and required behaviors, then report actual evidence. The broader
knowledge invalidation, released-job and promotion integrations remain mapped
to R06/R11/R15 and do not excuse false clearance in the present provider.

Performance follow-up remains R17: current completion scans must not become an
ordinary full-graph cost on every scheduler tick or UI query; use the planned
committed indexes and scoped invalidation before scale acceptance.
