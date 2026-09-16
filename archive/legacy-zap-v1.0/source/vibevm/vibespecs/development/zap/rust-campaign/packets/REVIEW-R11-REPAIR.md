# Independent review of R11/R12 repairs

##subagent-quiet-clause

Senior gpt-5.6-sol/ultra. Review only; no production code, tests, full boot,
external runners, local inference, NEXT execution, Git or publication. Read
R11-runtime.md for its exact workspace/package and named standing rules; reuse
already-read material. Root alone accepts. Do not edit producer-owned reports
or source while the implementation worker is active.

Read ../REVIEW-R11.md, ../REPORT-R11.md, ../REPORT-R12.md,
../checkpoints/R11.json, ../RUST-API.md, ../API-AMENDMENTS.md and the R11/R12
requirement entries. Inspect changed runtime source and tests plus relevant
core/agent, core/execution_views, trust/commit and app composition ports.
Use exact canonical requirement sources named in the original packet as needed.

Decide each original finding separately. Trace official registered service
paths, not just helper signatures: durable claim and current launch consume,
late receipt upgrade, trusted observation/candidate provenance, independent
execution/collection/safe/acceptance states, reconciliation, stop delivery,
verification/retry/wait/release, capability/profile and goal lifecycle. Check
closed typed message families and actual current capability binding.

The producer added real redb/CommitService/Coordinator tests and a cold reopen.
Check that all original handles are dropped before open, unknown effect stays
unknown until observed, and receipt/retry state survives reconstruction. A
configured external boundary in a fixture is expected; an injected result that
skips the rule being tested is not evidence. R07/R16 still own final persisted
Owner pause/hold composition; R08 owns complete lowered packet contracts; R16
owns the real native collaboration probe. Keep these explicit and do not claim
them proven by this review or demand duplicate tests for stable source.

Pay particular attention to native mailbox enqueue versus actual launch:
single-use exact current authorization is consumed at trusted driver pickup,
and a lost post-consume outcome is reconciled rather than replayed. Do not
claim a database transaction makes a remote native effect atomic.

Write REVIEW-R11-REPAIR.md with concise resolved/open findings, exact source
references and meaningful evidence limits. Update your own separate
checkpoints/REVIEW-R11-REPAIR.json using actual clock, structured serialization,
parse-before-atomic-replacement and previous-valid backup. Checkpoint before
long review units and within five minutes active work. Send root a concrete
accept-or-repair recommendation; do not implement fixes.
