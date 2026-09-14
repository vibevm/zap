# Root acceptance of the complete Cargo gate

Root accepts the complete package test denominator plus the exact corrected
targets. The complete no-fail-fast log has SHA-256
aae6970b0c32a70d49654497f6a18f4537c6cbddc1a7f57647628d82adbf6864:
169 unit/integration passes, seven failures, sixteen ignored, and all185
doctests passing. The seven failures are now repaired and verified, yielding
176 supported unit/integration cases plus185doctests at the accepted boundary.
This combines unchanged passing targets with focused repairs; it does not
misrepresent the failed full invocation as exit0 or double-count repeated tests.

Root compared every changed fixture against the frozen source artifact.
read_server initializes its registered Genesis index catalog before reopening
the read-only server; its exact failed case passes1/1. runtime_contracts expects
the actual17families and explicitly checks NativeSpawnRecoveryRecord; its exact
failed case passes1/1. All other assertions and production behavior are intact.

Dreamer's fixture initializes the same domain/core/runtime catalog. Its two
test-only mutation descriptors derive exact index scopes from their unchanged
record families; no production authority is widened. dreamer_service passes4/4.
lowering_contracts passes5/5 after correcting three v2 record family names and
the domain-owned archive-publication/reassessment cells. Root separately read
ApplicationBundleExportedCell, ApplicationReturnImportedCell and
cross_domain_cell_set: application composition deliberately owns verified
export/import, and return atomically stores resolved encounter rows. Restoring
obsolete domain-only cell expectations would contradict that accepted boundary.

All-nine strict all-target clippy and formatting passed before these repairs.
Changed app/runtime all-target clippy and formatting, and both changed domain
test-target clippy/format checks pass afterward. Source-only conformance remains
zero; specmap --check still matches608units1260items1479edges with zero orphan,
warning or suspect. No production source changed after the accepted import
catalog repair. No new ignored/live test was run; conditional Vibe adapter
early-return is not claimed as actual Vibe integration. Installed-source gates
and publication remain separate required receipts.
