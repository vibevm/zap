# Independent review of R06 repairs

Status: bounded repairs required before R06 acceptance. No build, test, Git, or
model command was run by this review.

## Disposition of the original findings

The repaired registered paths close the original region-authority,
dependency-direction, affected-job closure/action, exact revalidation,
non-pivot ownership, and promotion findings. Promotion is correctly absent
from `cell_set` until R15 supplies an exact durable repository-effect record.
Direct content-change invalidation works without a duplicate graph edge.
Closure, applicability, fact, and region truth changes now use payload-derived
transaction bases and private scoped-evidence witnesses. The seven public
`CommitService`/redb scenarios exercise real registered routes rather than the
old handpicked boolean helpers.

Original findings 1, 2, and 9 are only partly closed by the remaining defects
below.

## Open findings

1. **[P1] Current proof is recomputed only for knowledge witnesses, not for
   ordinary acceptance or completion.** `knowledge/proof.rs:94-100` correctly
   reconstructs the original `BasisPurpose::Verification` request and compares
   its current digest. In contrast, `acceptance/validation.rs:21-62` and
   `:78-132` trust the stored `ProofApplicability::Current`, outcome, generation,
   and provenance values without recomputing that basis. `queries.rs:95-118`
   uses the same weaker predicate to build `current_evidence`, and
   `queries.rs:262-270` therefore permits required final-gate evidence after a
   relevant contract or dependency fingerprint changed. Centralize the
   state-derived current-proof predicate and use it for collective acceptance,
   proof reuse, completion, and final-gate duties. Add a registered case where
   a proof is accepted, a relevant dependency or contract changes, and close or
   acceptance refuses until revalidation; an unrelated change must still reuse
   it.

2. **[P1] A byte-identical recapture can change source scope without any
   invalidation.** `knowledge/transitions.rs:117-118` defines `changed` using
   only digest and byte length, while `:133` replaces `SourceScope`.
   `current_applicability` at `:409-448` compares assessment digest and status
   but never assessment scope with the current source scope. Changing
   `Subjects(A)` to `Subjects(B)`, or Project to Subjects, with identical bytes
   leaves source applicability, facts, and evidence marked current. Include
   semantic scope change in invalidation and require the applicability record's
   scope to equal current source scope. Cover this through the registered
   recapture route.

3. **[P2] `knowledge-summary` still hides a missing Fact closure in one concrete
   state.** `knowledge/queries.rs:117-183` reports existing non-complete closure
   rows, then checks absent closure only for `KnowledgeEndpoint::from_subject`.
   If Work W has a Complete closure and Fact F about W has no
   `KnowledgeEndpoint::Fact(F)` closure, the query reports W complete. The
   provider correctly marks W unknown in `basis_helpers.rs:540-549`. Apply the
   same fact-to-subject propagation in both All and Subjects views. This is R06
   correctness for its registered summary, separate from R13 detail surfaces
   and R17 indexing.

4. **[P3] Equivalent recapture causes avoidable proof-reuse loss after the
   safety fix above.** `verification_subject_fingerprints` at
   `basis_helpers.rs:304-325` normalizes Work to generation but leaves Source as
   the full record fingerprint from `:261-265`. A same-byte, same-scope recapture
   changes revision/provenance and therefore the verification basis even though
   `SourceFingerprint` remains content-identical. Define a verification source
   value fingerprint containing semantic inputs such as identity, kind,
   locator, content, byte length, scope, and capture status while excluding
   observation history. This is bounded reuse efficiency debt, not a reason to
   weaken stale-proof rejection.

## Recommendation

Return R06 to a Middle repair for findings 1-3. Finding 4 may be fixed in the
same bounded change or carried explicitly as scoped reuse debt. Reuse the green
22-case receipt; add only the discriminating cases above and rerun the affected
service target. R13 query expansion, R17 indexing, R15 real capture/promotion,
and the separately accepted R11 strict-safe-state work remain their existing
campaign boundaries.
