# Root review of raw record continuation

Accepted as a bounded storage-port correction on 2026-09-14. Root inspected
the redb family/range reader, StateReaderExt validation, complete-family domain
and application loops, and the complete nested record-overlay algorithm and
focused tests. REPORT-R17-RECORD-PAGES.md and its checkpoint retain execution
receipts. Final source artifact and installed consumer acceptance remain separate.

The separate non-serialized RecordCompleteness enum fixes an accidental failure
after512 records. Redb observes an extra in-range row before returning More;
typed pages validate cardinality, range, strictly increasing keys and exact
last-key continuation. The domain/material/bundle loops continue with an
excluded matching key. UnknownBoundary remains explicit and cannot establish
complete-family knowledge.

The overlay fetches a bounded base prefix sufficient for the requested page,
pending in-range removals and one continuation decision. Merging pending
insert/replace/remove rows cannot promote a later pending key ahead of an
unseen earlier base row: enough unremoved base rows are retained before page
selection. Nested overlays preserve this argument. Malformed order, duplicate
keys, invalid boundaries and unknown input refuse instead of falling back to
an unbounded family materialization.

Evidence is deliberately separated:

- The real redb test reads513 records in exactly2 pages at limit512 through both
  a live write table and immutable snapshot, with exact ordered termination.
- The nested overlay fixture checks page-edge mutations, range bounds, full
  result order and values; its first2-row request fetches6 underlying rows from
  a40-row base, including both overlay removal allowances.
- The typed malformed fixture exercises missing/wrong/nonadvancing boundaries
  and an over-limit page. Typed Unknown is preserved; the overlay rejects
  Unknown and decreasing order. Root source inspection also verifies the other
  implemented order/cardinality checks without calling every combination a test.
- The existing economics_blockers consumer returns513 semantic ActiveHold
  blockers over2 raw pages and refuses Unknown. This is separate from the real
  redb reader proof, not a claim of one combined513-hold service benchmark.
- Relevant viewer, index rebuild/replacement, basis catalog, shipped WorkRenamed
  and application read/reopen/cursor regressions passed. All domain/runtime/app
  test targets compiled. Scoped formatting, strict lint and five conformance
  checks passed; nine product crates remain gated with no exemptions.

Public Completeness/PageCursor and their serialized shapes are unchanged.
SnapshotRead's generic Page retains UnknownBoundary on truncation because it
cannot invent a query identity. Runtime bounded guards retain their existing
refusal of incomplete raw pages. Global operations still cost work proportional
to their relevant families; the correction claims usable continuation, not
constant-time global behavior. No data epoch or historical byte format changed.
