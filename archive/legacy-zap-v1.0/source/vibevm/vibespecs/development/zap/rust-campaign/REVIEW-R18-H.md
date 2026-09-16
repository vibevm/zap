# Root review of legacy and API usage bindings

Accepted bounded H documentation unit, 2026-09-14. Root reviewed both usage
guides and their exact symbol/section catalogs, REPORT-R18-H.md and the mapping
receipt. All79 symbol-to-guide edges resolve to real guide units without false
revision pins:28 legacy declarations and51 API declarations.

Root independently compared both complete source directories against the
09:20 source capture, ignoring line-ending-only differences. Every source
change was a spec attribute, the same implementation/scope URI reformatted,
or an import of the inert specmark attribute. There was no behavior, type,
visibility or serialization change. The initial unnormalized diff displayed
whole-file line-ending changes; that was not interpreted as product deletion.

Actual scoped health/conformance reports zero public-type gaps for both
crates, and their type gates are enabled without changing the empty baseline.
Check, strict library lint and formatting passed. The existing API doctest
ran1/1; legacy has zero doctests and is covered by the reviewed documentary
bindings, not claimed execution.

The H source boundary had535 indexed units,0 warnings and0 suspects; its
deterministic --check passed. The remaining3 domain viewer-index helpers were
outside H and are assigned to the runtime/index continuation correction.
Subsequent J/I/domain edits require a final coherent index regeneration; the
earlier H receipt does not claim the changing package is already frozen.
