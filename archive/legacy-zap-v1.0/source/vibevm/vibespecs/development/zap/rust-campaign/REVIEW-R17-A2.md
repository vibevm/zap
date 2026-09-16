# R17 A2 acceptance

Root accepts the bounded physical schema-2 implementation and measurements in
REPORT-R17-A2.md, including its final partial-cleanup correction. This closes
the selected A2 representation atom, not the full graph/history scaling task.

The checked record/history formats preserve logical values, versions, commands,
events and ordinary indexes. Frozen physical v1 remains readable, writable and
replayable. V2 names its own physical digest algorithm; the cross-format logical
digest is separate audit evidence. Root inspected the codec, history readers,
snapshot construction and rebuild/publication path. Physical and logical
snapshot values now come from one captured redb read transaction.

The exact accepted P0 import fixture was reused read-only in both profiles.
Record/history values fell from 39,993,249 to 8,847,677 bytes (22.12% retained).
Database length and verified NTFS allocation fell from 136,318,976 to 50,339,840
bytes (36.93% retained). Logical-row digest was equal. Warm read ratios were
30.07% debug and 19.39% release. These clear the selected A2 gates; they are not
measurements of the 425-node pilot, full-operation memory gain or cold disk.
The separate synthetic microbenchmark remains explicitly labeled.

Root's publication findings are resolved: fresh claims atomically reserve a
missing destination; recovered incomplete reservations require their exact
marker; the destination database is verified before the ready receipt is
linked. A verified completed publication is recognized even after its marker
was removed. Partially completed cleanup validates surviving owned bytes and
treats already-absent files as completed steps. Foreign or unverified leftovers
are preserved. The tiny crash window between directory creation and ownership
marker publication remains explicitly ambiguous and refuses adoption.

Accepted evidence comprises store tests, frozen-v1 replay, targeted corruption
and rebuild/crash tests, default-v2 import recovery, the same-fixture debug and
release measurements, scoped clippy and Rustfmt. Root reuses those applicable
receipts; no benchmark is repeated for this acceptance document.

B remains unselected. The actual NEXT source, 425-node import, active pointer
and execution authority are unchanged. R17 still needs declared graph/history
scale, latency, invalidation and resource evidence. R13C must expose the actual
physical schema/algorithm in the public surface. Publication remains R19.
