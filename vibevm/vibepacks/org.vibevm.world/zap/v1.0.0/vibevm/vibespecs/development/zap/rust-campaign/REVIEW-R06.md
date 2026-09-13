# R06 review: relevant basis and incremental behavior

Status: early source-review findings returned before candidate acceptance.
No additional root product tests were run for this review.

1. DomainBasisProvider calls dependency_fingerprints on every stored dependency
   even for Dispatch(work). An unrelated dependency edit therefore changes a
   supposedly scoped basis. Fingerprint only the actual relevant dependency
   closure and prove related versus unrelated change behavior.
2. subject_fingerprints silently skips selected subject kinds it does not
   implement and missing records. Missing material inputs need an explicit
   unknown/refusal, not silent omission from a complete basis.
3. Most BasisPurpose variants fall through to every work/obligation because the
   old purpose lacks an exact target scope. Add an implementable typed request
   or registered payload-to-scope extraction with the service; a caller must
   not choose a narrower scope than the actual effect. Conservative whole-scope
   fallback cannot be advertised as scoped unchanged-proof reuse.
4. Policy and capacity are always None and unassessed sources are dropped.
   Where these are material, retain an explicit unknown/required-provider
   boundary rather than omitting them. Known-graph traversal completeness does
   not establish that missing semantic edges have been assessed.

Full scans and index-based optimization remain R17 performance work, but
unrelated fingerprints and missing material scope are immediate correctness
and change-economics issues. Use bounded scenarios that distinguish the faulty
and required behavior instead of broad repeated test panels.

## Adaptive application review

ApplyReview rejects a nonempty proposed job_reconciliation list but passes a
literal true for live-job validity when that list is empty. An omitted plan
does not prove there are no actual affected jobs. Derive the current job set
from trusted runtime records/provider inside the applying transaction for both
empty and nonempty cases; missing complete observation remains unknown/refused.

apply_work_changes reads pre-state obligations after apply_pivot has queued
ownership changes. Work drop/supersede must be validated against the proposed
final ownership. Produce one coherent replacement per record instead of
accumulating contradictory/duplicate ChangeSet replacements. Cover ownership
transfer plus work removal and omitted actual live jobs in the scoped cases.
