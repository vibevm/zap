# R18-A coordinator review

Accepted bounded batch, 2026-09-14: zap-wire's first real discipline gate and
the five assigned core module decompositions. This is not final R18 acceptance.

The wire policy gates the actual product crate with an empty baseline. Its
CanonicalEncode/CanonicalDecode examples accompany 15 passing runnable tests,
two passing doctests and scoped clippy/conformance receipts. The original
all-exempt zero is not used as evidence.

Core commit, transition, trust, preflight and effects responsibilities are now
separate modules below the configured line limit. Reviewed parent exports and
trust construction boundaries retain typed APIs and private/pub(super) fields;
the ValidatedCommitIntent constructor remains crate-private. The existing seven
core tests and wire/core strict library lint passed after the structural batch.
The active app/runtime/navigation integration checks remain separate evidence.

Core is correctly left ungated while 47 seam contracts and the owned packet
module remain unresolved. Registries preserve those debts rather than freezing
them into a conformance baseline. Vendor provenance is retained, and specmap's
nine explicit product roots remove the unnecessary nested dependency assumption.
The regenerated specmap and final production orphan dispositions are still due.

R18-B continues runnable core contracts and the next independent crate. No new
test panel was run solely for this review. REPORT-R18-A.md carries the original
counts, commands, limitations and the excluded unwrapped nextest-list attempt.
