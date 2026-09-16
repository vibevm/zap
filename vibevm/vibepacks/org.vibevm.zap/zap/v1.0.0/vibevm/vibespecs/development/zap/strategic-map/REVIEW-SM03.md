# Root review of the descriptive assessment boundary

Root reviewed the new assessment model, payload/cell registration, source
fingerprint and freshness helpers, shared contract-selection extraction, and
the actual store/command journey and validation fixtures. The inspected design
preserves the declared boundary: one optional data record keyed by WorkId,
without a Work/contract/authority mutation.

The source fingerprint contains exact Work and the contract selected by the
existing complete last-active-in-index-order policy. The new helper validates
partition membership; it does not substitute highest version, first record or
blanket multiple-active refusal. The multiple-active test changes an earlier
contract without changing the fingerprint, then changes the selected contract
and establishes staleness. The assessment record itself and unrelated records
are excluded from its Work/contract source fingerprint.

The real CommitService/RedbStore journey proves initial insert, exact command
retry, record CAS, unchanged canonical Work, unrelated change stability, Work
and selected-contract staleness, update under current source, removed-Work
Unavailable, and byte-preserving cold reopen of the assessment. The validator
checks decoded intervals, independent grade requirements, retained partial
Unassessed context, nonblank text, bounded unique assumptions/unknowns/evidence,
definite passive-wait versus elapsed contradiction and evidence existence.
Evidence presence is not evidence acceptance; assessment confidence is declared
metadata and does not become a dispatch or approval rule.

Four focused tests pass at worker sequence3. Root requested one fixture
precision correction: the clock contradiction must use an individually valid
passive-wait interval above the elapsed upper bound, rather than accidentally
retesting an inverted interval. Its focused result is recorded by the worker.

This is bounded source/behavior review, not final feature acceptance. The
public card must expose the current fingerprint even before an assessment
exists, so the machine/HTTP/CLI write journey does not depend on a private
StateReader. Shared aggregate query budgets, public-use/strict gates and the
complete SM04 transport journey remain required before final acceptance.
