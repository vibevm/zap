# SM03 durable map-work assessment report

Status: candidate complete for root review.

The candidate adds an optional revisioned assessment keyed by an existing
materialized WorkId. Its DataProposal transition records descriptive metadata
only. Source freshness is the canonical digest of the exact Work record and
the active contract selected by the existing complete contract-index policy.
The assessment itself and unrelated records do not contribute to that digest.

The public convenience basis helper owns one bounded index budget. The
crate-private source loader accepts a caller-shared budget so the strategic-map
query can reuse loaded Work/contract data across cards and account for aggregate
index reads. Existing admission callers retain the same 65,536-row default.

The record separates three optional `HoursInterval` estimates, each with its
own `CostPrecision`, source and assumptions. Complexity, executor-relative
difficulty and confidence are independent grades, with Unassessed distinct
from Low. Assessed difficulty requires a rationale plus executor and knowledge
assumptions; unassessed fields may retain partial nonblank context. Validation
also rejects inverted ranges, a definite passive-wait/elapsed contradiction,
whitespace-only required text, duplicate or excessive references and missing
declared evidence records.

The real RedbStore/CommitService target passed four cases. It covers initial
insert, exact command retry, exact record CAS, stale supplied source, Work and
selected-contract invalidation, unrelated-commit stability, unavailable
removed Work, cold reopen and the retained last-active-in-index-order contract
policy. The assessment cell descriptor names only the assessment record,
requires no completion check and retains the DataProposal route. The test
compares the Work before and after the metadata commit.

Verification:

- `cargo test -p zap-domain --test map_assessment --offline`: 4 passed.
- strict target Clippy with `-D warnings`: passed.
- scoped domain conformance: 0 findings, 0 frozen, 0 new.
- discipline health: `zap-domain` 476/476 syntax-visible public types covered,
  zero gaps; package file-length ceiling has zero violations.
- coordinated domain/app/CLI formatting and `--check`: passed.

The domain guide catalogs all 11 public assessment types. New assessment
source files are at most 279 lines and focused test files are at most 407
lines. No record, command, authority, completion, admission, dispatch or
runtime behavior from 1.0 was redefined.
