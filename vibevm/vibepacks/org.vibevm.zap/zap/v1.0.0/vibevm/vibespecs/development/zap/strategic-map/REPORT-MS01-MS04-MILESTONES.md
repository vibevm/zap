# MS01–MS04 milestone implementation report

Date: 2026-09-14. Scope: canonical milestone identity, witnessed
achievement/current validity, typed transformations, persistence, queries, and
the milestone-to-planning proof read. This report records candidate evidence
for root acceptance; it does not activate a campaign or publish a package.

## Delivered behavior

The `zap-domain::milestones` module now owns a stable `MilestoneId` head,
immutable `MilestoneRevisionId` revisions, and immutable
`MilestoneAchievementId` receipts. Definitions bind the exact strategic and
outcome scope, observable result, consumers, obligations, contributions, and
typed preparation or achievement prerequisites. Milestones are independent of
`WorkRecord`; strategic-node Work contributions may exist before executable
Work is materialized.

Semantic and proof-applicability fingerprints are separate. Work decomposition,
preparation-route, and presentation changes alter semantic history without
automatically erasing an applicable achievement. Result, outcome, obligation,
consumer, source/evidence, or achievement-prerequisite drift requires
revalidation. Historical receipts remain immutable and queryable.

Registered create/revise commands use exact CAS, relevant basis, privileged
authority, effect simulation, and the existing admission service. Initial
creation uses `InitialMilestonePlanOrSemantic`; dynamic creation becomes a
semantic change after an adopted milestone plan or lowering. Direct revision is
additive and cannot remove commitments or pivot outcome. Achievement is a
separate proof command and reuses current evidence adjudication, including
candidate provenance, contract, validation generation, source captures,
applicability, observed pass, and exact basis.

Typed transform preview/apply supports split, merge, contribution move, route
change, and retirement. It validates an atomic proposed-state overlay, rewrites
all live incoming dependents, ignores immutable retired dependents, checks
cycles and exact bindings, conserves obligations/consumers/contributions,
records every removed dependency disposition, and limits auxiliary rows to
justified route rebasing. Retired history, dormant contributions, full edge
mapping, and before/after identities are persisted in the immutable
`MilestoneTransformRecord`.

Exact registered reads are available for current milestone view, any historical
revision, any achievement receipt, transform preview, and persisted transform
history. The guide documents that current-proof evaluation and dependent
discovery presently use store-wide scans; no bounded-cost claim is made for
those components.

## Verification evidence

All Cargo invocations used the required wrapper and shared offline target.

```text
& C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1 \
  -CargoArgs @('test','-p','zap-domain','--test','lowering_service',
    'milestones_core','--offline')
& C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1 \
  -CargoArgs @('test','-p','zap-domain','--test','knowledge_service',
    'milestones_achievement','--offline')
& C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1 \
  -CargoArgs @('test','-p','zap-domain','--test','knowledge_service',
    'milestones_transforms','--offline')
```

Result: 9 passed, 0 failed.

- `lowering_service::milestones_core`: 3 passed. Covers registry/query surface, semantic versus proof
  fingerprint behavior, authenticated initial create, exact retry, changed
  payload under reused command refusal, cold reopen, connected split preview,
  omitted economics refusal, unrelated auxiliary contribution-smuggling
  refusal, and unrelated-owner edge-remap refusal.
- `knowledge_service::milestones_achievement`: 1 passed. Uses an actual candidate/source/contract
  chain and the registered evidence and milestone acceptance commands. Covers
  current success, incomplete obligation and foreign outcome refusal, stale
  proof refusal, cold reopen, unrelated commit stability, contribution
  reshuffle stability, source drift to `NeedsRevalidation`, immutable receipt
  retention, and a planning `AllSatisfied` read with no unsatisfied milestones.
- `knowledge_service::milestones_transforms`: 5 passed. Positively applies route change, contribution
  move, retirement, connected split, and internal merge. Cases include an
  `A <- B <- C` route rebind, an external dependent rewritten during split, an
  internal merge-edge collapse, retirement into an existing successor without
  losing its independent commitments, and changing a live target after a
  retired historical edge remains.

The positive transform tests use the knowledge harness's `ExactAdmissions`
test provider. That provider prepares the registered exact effect bundle,
affected scope, basis, command, and simulated reducer result before granting the
test admission. It is not a claim that those tests exercised the production
economics assessment/decision records. The normal lowering-service harness uses
the production `ChangeControlAdmissionProvider`; its milestone test proves that
an unassessed semantic transform is refused and commits no milestone change.
The planning suite separately owns the production dynamic-creation economics
journey.

The final admission-specific journey closes that earlier evidence distinction:

```text
& C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1 \
  -CargoArgs @('test','-p','zap-domain','--test','lowering_service',
    'admitted_route_transform','--offline')
```

Result: 1 passed, 0 failed, 9 filtered out. The test starts with an adopted
milestone plan, establishes the production economics baseline, prepares the
exact `MilestoneTransformApplied` effect comparison, records and adjudicates a
real `ChangeAssessmentRecord`, prepares the exact `ChangeAdmissionRecord`, and
executes through the normal `ChangeControlAdmissionProvider`. The transform adds
a typed milestone prerequisite, atomically advances the milestone head, retains
the old revision, persists the full transform receipt, leaves the adopted plan
record unchanged for later reassessment, and creates or changes no `WorkRecord`.

```text
& C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1 \
  -CargoArgs @('clippy','--workspace','--all-targets','--offline','--','-D','warnings')
```

Result: passed for the complete workspace and every target with warnings denied.
The earlier information/map and planning findings were fixed before this rerun.

The achievement and transform scenarios now live as child modules of the
existing `knowledge_service` integration-test target, and core milestone tests
live under `lowering_service`. Each shared harness is compiled once in the test
target where its capabilities have real users; no dead-code allowance or fake
fixture call is used.

All milestone production Rust files are at most 600 lines after formatting; the
largest is 594 lines. The milestone test files are also at most 600 lines; the
largest is 595 lines.

## Review findings and disposition

Root and the independent information review raised the following material
findings. Each is closed in the candidate and covered where practical by the
focused tests.

- A full strategic revision/decomposition fingerprint would stale unrelated
  proof. Fixed with a separate proof-applicability fingerprint and explicit
  admitted strategy transfer.
- Candidate strategy bootstrap and future strategic Work were incorrectly
  treated like current executable Work. Fixed by accepting exact candidate or
  current strategy and validating a separately declared materialized affected
  Work subset.
- Current achievement initially followed prerequisite heads without checking
  the dependency's exact revision/fingerprint. Fixed; a changed prerequisite
  requires explicit parent rebasing and, when proof-relevant, revalidation.
- Current read paths could flatten missing/crossed head, revision, or receipt
  references. Fixed with explicit missing/conflict results and exact historical
  revision/operation reads.
- Early transform union checks omitted dependencies and route changes could
  discard contributions. Fixed with explicit owned-edge deltas, dormant
  contributions, and one disposition per removed dependency.
- A pre-state-only validator made connected split/merge impossible. Fixed with
  an in-memory after-state overlay and bounded changed-set validation.
- Incoming dependents could remain bound to retired revisions. Fixed by a
  store-wide live-dependent discovery pass and required atomic dependent
  rewrites; retired historical heads are excluded and remain untouched.
- Route change could not repair a connected chain. Fixed by permitting
  dependency-only auxiliary rewrites and testing `A <- B <- C` end to end.
- Retirement into an existing successor could overwrite that successor's prior
  commitments. Fixed by conserving the union of the retiring boundary and every
  changed existing successor.
- Auxiliary rows could smuggle unrelated Work/Evidence contribution changes,
  and an unrelated owner edge could be presented as a remap. Fixed by preserving
  auxiliary Work/Evidence identities, permitting only operation-mapped milestone
  reference rebasing, validating both remap endpoints, and adding negative
  preview cases.
- Transform history initially stored only IDs and a digest. Fixed by persisting
  the complete exact plan, dormant contributions, dependency dispositions, and
  exact before/after revision identities.

Public milestone schemas and query/result types carry `#[spec]` bindings to the
normative milestone contract and the usage guide. The guide includes a real
construction example and explains authority, retry, proof validity, historical
reads, transformation conservation, and scan-cost limits.
