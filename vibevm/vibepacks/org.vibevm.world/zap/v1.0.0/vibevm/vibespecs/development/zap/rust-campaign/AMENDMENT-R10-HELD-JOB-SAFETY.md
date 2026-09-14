# Accepted R10 amendment: held execution safety after semantic mutation

Root architecture decision, 2026-09-14. A semantic product may lawfully change
the Work, obligation, dependency or strategy records that defined its pre-state
hold scope. Requiring the post-product affected-scope digest to equal that
pre-state digest makes any such change with a live job impossible to release.

Current-schema economics holds persist `safe_job_mode = HeldExecutions` and the exact execution identities captured
when the hold is created: job, attempt, work, contract, contract digest and
validation generation. Hold expansion may add identities; reuse of one job ID
with changed identity refuses. The original affected-scope digest remains
immutable provenance and independence evidence.

`SafeJobRequest` and the durable hold explicitly distinguish two modes:

- `ExactScope` retains the prior exact-scope behavior for already versioned
  callers and replay evidence.
- `HeldExecutions` resolves every persisted execution identity directly from
  the transaction state and requires the shared safe predicate. Missing job,
  changed attempt/work/contract/generation, started or unknown effect, unsafe
  process state, or unknown safe state refuses.

The same request also derives the current post-product affected scope. Every
job currently returned for that scope must satisfy the same safe predicate, so
a newly affected unsafe job cannot be hidden by moving the original job out of
scope. The safe-job view returns the sorted union of held and currently affected
job IDs and binds its mode and held identities into the view digest. Opaque
transaction/service seals are unchanged.

This is a current zap/2 DTO and preflight amendment. Frozen zap/1 event,
authority and replay bytes are not reinterpreted. A deserialized prior hold or
safe-job view with no mode defaults explicitly to `ExactScope`, retaining its
original request and view digest formula; newly created holds select
`HeldExecutions`, including complete-empty held sets.
