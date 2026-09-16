# Candidate acceptance repair report

Status: production candidate, pending the root-owned native executor receipt.
Date: 2026-09-14.

## Repaired boundary

Real runtime candidates keep the post-dispatch `Dispatch(work)` basis that
describes their execution. Acceptance no longer compares that producer digest
to unrelated Verification, Stage, Integration, or Work command headers. Core
continues to validate every command against its own registered current basis.

The domain now opens a durable, phase-stable `CandidateReviewRecord` during the
existing Active-to-Candidate transition. Opening the review validates the exact
candidate/job/attempt/packet/contract/generation lineage, terminal safe
execution, immutable packet digest, current source and rule captures, and the
original producer basis. The stored `CandidateReview(candidate_id)` request
binds meaningful Work, Outcome, contract, source, fact, and dependency inputs
while excluding evidence created during the review itself.

Evidence, stage, integration, work acceptance, and current-proof resolution
all use the same private applicability resolver. They recheck the immutable
producer lineage and current review basis while their command headers remain
independent. Evidence adjudication stores its actual Verification basis. The
artifact predicate follows the evidence/result contract and does not impose a
blanket nonempty-artifact rule.

The normal Candidate-to-Ready transition clears the completed active-job link.
After relevant input drift, a fresh execution, candidate, and review can restore
applicability without changing the old producer provenance. A freshly computed
command header by itself cannot revive the old review.

`CandidateReviewRecord` is in the domain record registry and is exposed through
the existing viewer detail and history surfaces. `WorkTransitioned` bytes are
unchanged; the basis purpose and record family are additive current-schema data.

## Source map

- `crates/zap-core/src/basis/mod.rs`: additive CandidateReview basis purpose.
- `crates/zap-domain/src/acceptance/applicability.rs`: review opening and shared
  candidate applicability resolver.
- `crates/zap-domain/src/acceptance/{cells,records,mod}.rs`: local command bases,
  durable record, and intended public basis-scope adapters.
- `crates/zap-domain/src/control/{cells,transitions,mod}.rs`: lawful
  Active-to-Candidate review opening and Candidate-to-Ready recovery.
- `crates/zap-domain/src/knowledge/{basis,basis_helpers,proof}.rs`: phase-stable
  fingerprints and proof conservation.
- `crates/zap-domain/src/registration.rs`: record registration.
- `crates/zap-domain/tests/knowledge_service/{support,proof}.rs`: focused drift,
  recovery, detail, and history evidence.
- `AMENDMENT-CANDIDATE-ACCEPTANCE.md`: exact lifecycle and compatibility
  contract.

R17 supplied the minimal CandidateReview mappings in its existing viewer
modules. R16 supplied the genuine runtime two-candidate fixture and compiled
the complete transition/evidence/stage/work tail against the public APIs.

## Evidence

1. `run-cargo.ps1 test -p zap-domain --test knowledge_service
   proof::registered_adjudications_reject_current_but_unrelated_evidence --
   --exact --nocapture` — exit 0, 1/1, 0.42s. It proves distinct producer and
   command bases, CandidateReview detail/history visibility, stale dependency
   refusal even with a fresh local header, and fresh candidate/review recovery.
2. `run-cargo.ps1 clippy -p zap-core -p zap-domain --lib --no-deps -- -D
   warnings` — exit 0.
3. R16-owned `native_campaign_probe --no-run` — exit 0. Its two real runtime
   candidates compile through WorkTransitioned, EvidenceAdjudicated,
   StageAccepted, and WorkAccepted with the repaired public basis scopes.

## Pending acceptance evidence

The two-candidate native probe has not been executed because its two model
receipts are root-owned. This report therefore does not claim end-to-end native
acceptance. The remaining action is to run that already-compiled R16 probe and
repair only a concrete runtime failure if one appears.
