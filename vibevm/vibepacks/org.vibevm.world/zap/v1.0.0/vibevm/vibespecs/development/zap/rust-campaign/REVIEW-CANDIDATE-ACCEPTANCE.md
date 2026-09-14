# Candidate acceptance review

Reviewed by the coordinator on 2026-09-14. This is a bounded source and
scoped-proof review; end-to-end acceptance remains pending R16.

The repair preserves the actual runtime Dispatch producer basis. Review opening
checks current execution, packet, contract, generation and captured materials,
then retains a separate phase-stable CandidateReview basis. Later commands keep
their registered local bases and cannot revive a stale review by refreshing only
the command header. Evidence retains its Verification basis. Accepted Work is
checked against its exact acceptance record after the active job link clears.

The existing domain proof now covers dependency drift, fresh candidate recovery,
and CandidateReview detail/history. Its latest receipt is 1/1 passed in 0.42s;
core/domain strict library clippy passed. R16's actual two-candidate runtime tail
compiles, but compilation is not execution evidence. The root-owned native
probe must still demonstrate both results reaching acceptance and the same
nonempty campaign reaching CampaignClosed.

Source reviewed: acceptance/applicability.rs, knowledge/basis_helpers.rs,
AMENDMENT-CANDIDATE-ACCEPTANCE.md and REPORT-CANDIDATE-ACCEPTANCE.md.
No additional test panel was run for this review.
