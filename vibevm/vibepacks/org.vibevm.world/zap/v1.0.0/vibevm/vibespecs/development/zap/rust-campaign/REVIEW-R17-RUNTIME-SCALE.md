# R17 runtime scale acceptance

Root accepts the bounded implementation reported in REPORT-R17-RUNTIME-SCALE.md
at the worker's frozen sequence 8 boundary, 2026-09-14T11:50:36Z.

The root reviewed index contribution and descriptor composition, exact current
record validation, cursor ordering and query identity, aggregate scan/read
budget, coordinator snapshot binding, terminal-effect occupancy and completion,
and the application pause/hold guard. The four concrete findings in
REVIEW-R17-RUNTIME-SCALE-CANDIDATE.md are resolved. The follow-up same-page
capacity case now resolves packet availability before selection; the actual
Coordinator::step journey covers both one-page capacity 1 and page-limit 1
continuation. Unknown frontier boundaries refuse before selection.

Accepted focused evidence is runtime persistence 5/5, runtime library 6/6,
application server 4/4, frontier and numeric-order units 1/1 each, packet service
1/1 and deterministic two-job completion 1/1. Strict all-target clippy for the
three changed crates, formatting, scoped conform and the read-only orphan gate
pass. Final coherent index generation and package regression remain separate.

The deterministic completion marker change is test-only: it preserves native
probe create-new semantics while avoiding a shared temporary-parent marker when
the native probe environment variable is absent. Root inspected this branch and
the unchanged closure-record assertion.

The generated 4,101-record fixtures establish complete indexed discovery and
zero decorated record-family scans for the measured paths. They do not establish
global latency, memory or whole-command cost bounds. The runtime budget counts
its own index/read operations and external read-port calls; nested provider cost
retains its separate documented bounds. No live LLM launch was required or
claimed. Final source normalization, installation and publication remain pending.

## Final regression addendum

Root accepts the three-file import repair at worker sequence10. Comparison
against the frozen source artifact shows exactly one current-head
rebuild_indexes_v2 call and crate-private visibility for the two shared catalog
helpers and their module. No payload, event, receipt or authority contract
changed. The explicit import staging path initializes derived state before its
canonical import commit; ordinary missing-catalog reads still refuse. The exact
previously failing legacy_activation test passes1/1, legacy_import passes9/9,
and changed-app strict lint, formatting and conformance pass. Final package
test continuation remains separately required because its first run stopped
early. The earlier524-file artifact is superseded for publication.
