# Atomic native recovery review

Accepted bounded recovery implementation, 2026-09-14. Final production and
whole-campaign evidence disposition remain separate gates.

Reviewed the trusted app route and NativeSpawnObservedCell. The caller supplies
stable observation identity, exact JobId/DispatchId, authorization revision and
observation time. One transaction stores outcome, capacity, history, job/shared
state, authorization, reconciliation and wait. Exact retries reuse the saved
observation; changed evidence under the same identity conflicts.

Current source also addresses the final two review findings. Repeated Unknown
observations supersede prior active waits while retaining their history.
Historical Started evidence after newer authorization quarantines the job as
Unknown and preserves the current receipt plus the historical handle evidence.
It cannot silently convert the prior refusal into another launch permission.

The focused deterministic recovery receipt is 1/1 in 0.25 seconds, including
same-revision atomic state, exact retry, early-release refusal, current Held
refusal, due observed-capacity release, Unknown preparation refusal, repeated
Unknown cleanup and late-start quarantine. Restored persisted handles accept
later observations without constructing or consuming a new launch ticket.
Runtime files are below 600 lines; its two doctests and strict lint passed.

This evidence proves the changed runtime/driver protocol boundary. It does not
claim actual native model execution, inference telemetry or slot release. Those
external claims retain the recorded real refusal evidence and the Owner's
AMENDMENT-FINAL-VALIDATION.md. Existing nonempty completion evidence is reused;
no unchanged panel was rerun for this review.
