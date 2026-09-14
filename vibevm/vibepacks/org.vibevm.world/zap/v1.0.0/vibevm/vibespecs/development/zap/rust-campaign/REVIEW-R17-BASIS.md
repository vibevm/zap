# Indexed relevant-basis boundary review

Root accepts the bounded basis/overlay implementation described by
REPORT-R17-LOCALITY.md. This does not accept whole ordinary-command locality,
the full R17 phase, production closure or the package release.

Reviewed the transaction-readable StateReader port, indexed subject selection,
record contributions, cumulative row bound, family fingerprints, retained scan
reference and real-store counter fixture. Root review identified and the
candidate corrected inactive obligation inclusion, duplicate active charter
handling, missing algorithm checks and the unsupported ChangeSetOverlay path.
The overlay now uses shared immutable partition storage, validates complete
cursor identity and applies prior/new contribution deltas without record scans.

The reported scoped checks cover ten valid scenarios across all nine
BasisPurpose variants, complete basis field/digest agreement after relevant
mutations, duplicate-charter refusal,
six missing/stale/capability/catalog cases, real adaptive transaction success,
and two overlay insertion/replacement/removal/cursor/collision cases. The
4,101-unrelated-Work fixture proves zero full-record scans and at most four
exact reads for its one-Work basis. Its unrelated semantic generation update
does not change that basis digest. Empty relevant partitions return zero rows;
this is not a timing or whole-admission benchmark.

Strict affected production lint and formatting are reported green. Test-target
lint used two explicitly recorded allowances for existing helper warnings;
those are not a strict all-test lint pass and remain in the production tail.
Preserve the precise final consumer checks for later shared catalog changes.
Do not repeat unaffected algorithm evidence merely because a new task starts.

The actual admission path still requests a scope provider that scans five
complete families, and the runtime job provider still applies an unrelated
4,096-record prefix before filtering. Those source-confirmed limitations are
selected in packets/R17-ADMISSION-locality.md. New admission work must preserve
this accepted basis behavior and separately prove its complete service path.

The final oracle refinement asserts successful results for every intended valid
scenario and after-update comparison, includes a stored SemanticAssessment
success, and checks the missing-assessment refusal separately. Equality of two
unexpected errors cannot satisfy the valid-scenario proof. The two focused
oracle tests pass after that refinement.
