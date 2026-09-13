# Root review of the bounded R14 archive preparation

Status: source/codec/archive candidate retained; full R14 is not accepted.
The exact legacy corpus and wire parsing receipts support the codec boundary.
The actual-source dry run preserves the425/212/64/1292 denominator and original
plan hash. No actual destination has been created or activated.

Implementation obligations remain in addition to the explicitly missing
usable current domain projection:

1. `snapshot.rs::LegacySnapshot::parse` validates snapshot-to-history transport
   bindings and computes a state digest, but does not derive that state by
   replaying legacy semantics from the base. A parsed state or self-computed
   digest is not distrustful cold replay. Full R14 must implement the supported
   old event semantics or explicitly reject unsupported histories, retain exact
   bytes, and compare the derived state with an offered snapshot. The actual
   genesis-only source is useful evidence but does not prove generic replay.
2. `import.rs::LegacyImportCell::apply` currently checks inactive flags,
   duplicates and raw lengths, but not the declared raw-body/line/base/prefix
   hashes, inventory/mapping consistency or complete lineage. The official
   import boundary must consume independently validated exact imported state
   or recompute these invariants. Merely submitting a ServiceInternal payload
   must not turn self-asserted digest/count fields into verified lineage.
3. `codec.rs::LegacyValue` represents integers only as i64/u64, with an f64
   fallback visitor. Full Python-integer compatibility is not established by
   the current corpus. Cover signed/unsigned boundary-crossing integer tokens
   and preserve their exact integer value/packed spelling; do not silently
   coerce an oversized integer to floating point. If an old value is genuinely
   unsupported, refuse explicitly while preserving raw bytes, not with a
   success claim for altered canonical semantics.

Retain these obligations for the resumed R14 implementation. The worker now
implements independent R07; do not confuse pausing R14 for stable domain
mapping with completion. Future normalized records must preserve hierarchy,
contracts, mandates and obligations as queryable current model entities with
draft authority, not only as raw archive bytes or inventory counts.

The accidental disposable build directory C:/Users/olegc/.vibe Subs zap remains
untouched after automatic recursive-cleanup rejection, exact reason
`blocked by policy`. Do not retry deletion by another tool/language/agent;
disclose the retained directory in the final report.
