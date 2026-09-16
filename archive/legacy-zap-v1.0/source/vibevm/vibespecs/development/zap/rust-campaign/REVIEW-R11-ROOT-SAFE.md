# Root review of runtime repair2: strict safe-proof scope

Current disposition at ROOT-0040: repaired and bounded R11 runtime accepted.
Root inspected safe_verification_proves and both positive-safe/retry consumers;
they require exact SafeBoundary/job/attempt/effect/verifier/boundary and a
persisted observed Passed receipt. NotStarted now requires persisted reconciled
EffectState::NotStarted. Actual service negatives and scoped clippy pass.
R12 lowered contracts and R16 real native/persisted-policy integration remain.

Original finding: repair required for this remaining boundary. Root reviewed the other
reported harness, producer, artifact, capability and teardown changes as
material repairs; their scoped receipts remain applicable.

`runtime_updates.rs::verification_scope_matches` returns true for General
verification. Positive SafeStateRecorded and terminal_retry_is_proven use it
without independently requiring SafeBoundary or matching receipt.job_id.
A Passed General receipt under the declared verifier ID can therefore be used
as safety proof; a receipt for another job can also pass that helper.

Use a strict transaction-derived safe-proof predicate or witness for BOTH
positive safe state and terminal retry. It must require the current exact job,
attempt, effect, declared verifier ID, SafeBoundary scope and declared boundary,
plus Passed trusted observation. General verification is valid only on general
verification paths. Prove refusal of a General receipt with the same declared
ID and a foreign-job receipt through the existing actual service scenario.

Also refuse a caller assertion of SafeState::NotStarted for an already Started
or Unknown effect without affirmative persisted NotStarted reconciliation.
Do not infer that no effect occurred merely from a parsed enum value.

This is a narrow correction to the existing safety contract, not a request for
a new architecture or another full test panel. Root assigned the repair back to
the same Middle worker and retains acceptance authority.
