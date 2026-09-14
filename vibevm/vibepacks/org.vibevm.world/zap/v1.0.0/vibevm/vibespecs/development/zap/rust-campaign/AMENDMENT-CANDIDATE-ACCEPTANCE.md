# Candidate acceptance basis and applicability amendment

Status: root-approved implementation contract, 2026-09-14. This amendment
repairs current Rust candidate acceptance. It does not rewrite producer
provenance, import runtime types into the domain, invoke a model, or grant
authority.

## Separate basis domains

`CandidateProvenanceRecord.relevant_basis` remains the exact post-dispatch
`Dispatch(work)` basis sealed by the real runtime job and candidate-result
contract. It describes the producer execution. It is never replaced by an
acceptance, evidence, stage, or work-command basis.

Each later command retains its registered local basis. Core validates that
header against the current transaction before the cell runs. Evidence
adjudication persists its actual `Verification(verification_id)` basis in
`EvidenceAdjudicationRecord.relevant_basis`; it no longer stores the producer
Dispatch digest under that field. Stage, integration and work acceptance keep
their independent registered mutation purposes.

## Phase-stable candidate review

The domain adds `BasisPurpose::CandidateReview(candidate_id)` and a durable
`CandidateReviewRecord`. Its basis normalizes Work to identity plus validation
generation and Outcome to phase-independent semantic content, while retaining
current contract, source, fact and dependency fingerprints. It excludes
evidence records so inserting the evidence being reviewed cannot stale its own
basis. Policy remains explicit and capacity is not guessed.

The existing `domain.work-transitioned` Active-to-Candidate Progress route is
the review-open transition. Before changing Work it derives the unique
candidate for the current active job and validates:

- exact candidate, job, attempt and producer packet identities;
- current Work active-job and validation generation;
- the active contract ID, version and digest;
- equality between candidate provenance and the trusted execution observation
  subject set;
- terminal execution, an actual safe state, and no started or unknown effect;
- producer `Dispatch(work)` basis against the current Active pre-state;
- current source and rule digests captured by the exact producer packet.

Later evidence adjudication also requires its actual observation artifact to
belong to that candidate when the evidence contract names one. Artifact
presence remains governed by the resolved result contract; artifact-free
result contracts do not acquire a blanket nonempty rule.

The transition stores the immutable candidate-review request and digest plus
the exact producer basis, provenance digest, job/attempt/packet,
packet digest and source/rule captures, contract/version/digest and generation.
Work then becomes Candidate through the ordinary typed state transition.

Evidence, stage, integration and work acceptance require that record, rederive
its phase-stable basis, and recheck the same current lineage and materials.
After final Work acceptance, current evidence additionally requires the exact
WorkAcceptanceRecord for that candidate/generation; Work has no active job, but
the immutable producer and review records remain auditable.

The first evidence for a current captured source does not require a fabricated
preexisting Applicable assessment. Evidence adjudication validates the current
SourceRecord bytes, assessed scope, exact candidate artifact and Verification
basis directly. If a SourceApplicabilityRecord already exists, it must remain
Applicable, complete and bound to those bytes. A later applicability assessment
may then cite the accepted evidence through its ordinary route.

## Drift and recovery

A relevant source, fact, dependency, contract, generation, packet-material or
job-state change makes the stored CandidateReview basis or lineage stale. A
caller cannot restore the old candidate by computing a fresh local command
header because local and producer/review bases are checked independently.

Recovery uses the ordinary verification/execution lifecycle: the lawful
Candidate-to-Ready transition clears the completed active-job link, current work
is rendered and dispatched again, and a fresh verified CandidateResult and
CandidateProvenance opens a new candidate review. The old provenance and review
record remain unchanged. The domain fixture shows the old candidate refusing
after dependency drift even with a fresh local header, then a new verified
candidate/review restoring evidence and stage acceptance. The R16 fixture owns
the corresponding real runtime-created candidate path.

`WorkTransitioned` payload bytes are unchanged. The CandidateReview purpose and
record are additive current-schema data. Frozen schema-1 event/authority/
admission readers retain their existing byte decoders; no old producer basis is
reinterpreted.
