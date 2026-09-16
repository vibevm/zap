# Selected navigation unit review

Accepted bounded navigation implementation, 2026-09-14. Full R17 remains open
for the required ordinary-command basis locality refactor and its evidence.

Current deterministic evidence covers exact late-key lookup; stable deduplicated
adjacency pages across Work and Knowledge families; selective and short-query
search; empty filtered Frontier pages with continuation; and durable cyclic
traversal. The traversal fixture crosses its initial four-node state quota,
resumes at 64 and returns all 20 generated nodes after reopen. Each generation
asserts no more than eight actual index rows across its processed nodes.

Reviewed per-entry state avoids serializing the entire visited/frontier set on
each page. Exact retry, conflicting generation retry, stale revision/catalog,
logical cancellation and bounded cleanup have scoped receipts. The public
ApplicationService/HTTP routes exercise Owner-only rebuild and traversal;
the compiled CLI exercises maintenance success and typed missing-session
refusal. A 4,101-record fixture covers lookup beyond the former prefix and
filtered-frontier continuation. Strict affected production lint passed.

The recorded 10k speed and integrity measurements belong to the exact earlier
index-unit source identity in REPORT-R17.md. They are retained observations,
not fresh latency claims for the subsequently changed merge/search/traversal
algorithms. No repeat scale run is required solely for this review.

R17-BASIS-LOCALITY-DESIGN.md is selected next. Production local basis requests
must stop scanning complete domain families while preserving the old algorithm's
RelevantBasis fields/digest and closure semantics. The old full-scan algorithm
becomes a test-only oracle; generated cases and exact read counters establish
the refactor. Global Completion and genuinely Project-scoped sources remain
explicitly global/relevant rather than being omitted for performance.

R18-D owns inherited store structural floor work; the locality worker owns its
new transaction index port, contribution and basis paths. Final post-locality
traceability and package acceptance remain separate.
