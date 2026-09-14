# R09 implementation report

Status: review candidate for root acceptance. This report does not publish or
activate NEXT.

## Implemented boundary

R09 now has a distinct schema-2 offline planning boundary. The caller selects
only a bundle ID, current lowering, sorted packet IDs, and positive archive,
encounter, and return bounds. `ApplicationBundleClosureProvider` derives the
strategy/lowering revisions and semantic digests, current packet contracts,
actual claimed runtime attempts, producers, dispatch-intent and packet-resolution
digests, immutable source/rule/fork/workspace captures, captured capabilities,
active-charter permissions, active typed stop rules, and the complete typed
artifact-entry set. The app-owned captured verifier rederives the closure during
commit and replay. The domain stores `Prepared`; a matching trusted archive
receipt with a witnessed artifact advances the local bundle record to `Ready`.

The offline journal is an ordered, bounded, digest-chained delta rooted at the
manifest genesis. It validates exact bundle attempt/producer identity, causal
references to earlier encounters, typed fork selection, candidate-ID presence,
failure approach binding, unique artifacts/evidence, encounter count, and
encoded bytes. Return import is an app-owned trusted observation with a
state-capable affected-scope extractor. It validates the Ready manifest, exact
return archive and delta, actual runtime jobs, candidates, provenance, result
artifacts, and current strategy/lowering/packet applicability. Candidate-less
rows cannot become `ApplicableCandidate`.

Return import inserts encounter history, the local-version import record, failed
approach history, and current owner-created approach-counter changes atomically.
The direct composite key counts one semantic approach at most once per problem
epoch. The service fixture repeats the same failure under another command and
the counter remains one. The bounded `zap.planning.bundle` query reads only the
named bundle, optional import, and imported encounter IDs and enforces both
manifest encounter and encoded-byte bounds.

`ReturnReassessmentProposed` is a sealed DataProposal that uses the ordinary
typed adaptive-review basis extractor. It requires the exact unresolved import,
Ready bundle strategy/lowering binding, active intent/outcome, current captured
sources, authoritative affected-scope containment, and matching review work
transition. `ReviewApplied` remains the single semantic transition. It marks a
no-change return `NoChange`, or copies the exact optional return evidence into
the generic review-relowering sidecar and marks the import
`ReloweringRequired`. A later ordinary R08 lowering consumes the generic sidecar
and atomically advances the import to `ReloweringApplied`, the reassessment to
`Consumed`, and the sidecar to `Consumed`.

All new bundle, encounter, import, failed-approach, reassessment, and generic
sidecar identities start at local revision one; replacements use checked local
successors. Global event revision remains event metadata. Effect assessment and
admission use public `prepare_effect_comparison`, the derived comparison
`BasisRequest`, exact proposal/adjudication basis headers, prepared effect-item
digests, and the persisted no-op basis. No approved payload is edited or
reapproved after unrelated commits.

## Real service evidence

`packet_resolution_service` uses one real redb store, production
domain/runtime/app composition, `ArtifactStore`, and registered authority routes.
Its host and archive adapter are deterministic and explicitly simulated; no
model, launcher, network, Git, upload, or external side effect runs.

The journey creates the R08 charter, intent/outcome, strategy, lowering,
Work/contract, packet and claimed job, then prepares two independent Ready
bundles before launch. It drives the actual job through `NativeBridge`: persisted
AwaitingHarness receipt, pre-effect authorization, one-time consume, Running
receipt, trusted terminal observation, verification, safe-state evidence, and
`runtime.candidate-recorded`. The runtime cells create the real
`CandidateResultRecord` and `CandidateProvenanceRecord`; the return resolver
classifies the resulting `CandidateProduced` encounter as
`ApplicableCandidate` and the candidate-less start as
`ApplicableObservation`.

One return takes the real no-change path through reassessment proposal, public
effect preparation, economics assessment/adjudication/admission, and
`ReviewApplied`; it creates no lowering. A second Ready bundle returns a failure
bound to an owner-created problem epoch and approach. Import classifies it as
`Failure`, inserts one counted failed-approach record, and increments the epoch
once. Repeating that return under a different command does not increment again.
The return-triggered ReplaceMethod/Revalidate review then creates an exact
`ReloweringRequired` generic sidecar containing the real candidate. A same-Work
second lowering changes the contract meaning, advances Work and contract local
versions and validation generation exactly once, clears the prior active job,
and consumes the import, reassessment, and sidecar. The prior lowering, packet,
job, candidate/provenance, encounters, failed-approach record, and epoch remain
queryable. Cold replay uses captured packet/bundle material and reproduces the
same mutation history without live recapture; disabling captured material
verification reports `Unavailable`.

The focused app fixture uses one selected packet per bundle because the stable
R08 base fixture has one executable leaf. The production closure algorithm is
set-valued and validates each requested packet/actual job independently, but a
two-packet union fixture and the full matrix of every malformed archive field are
not claimed by this report.

## Candidate execution-basis and drift rule

R08 packet rendering seals `packet.render_basis` over the Ready pre-dispatch
Work. Packet resolution validates that basis, permits only the exact
Ready-to-Active `WorkDispatched` progress, and seals the runtime job and
candidate provenance with the resulting current `Dispatch(work)` execution
basis. Comparing candidate basis directly to lowering basis or packet render
basis crosses semantic purposes or states and is invalid.

`ReviewApplied` therefore derives the current Dispatch basis on its pre-state
and seals only candidates that also match the Work subject, active contract,
validation generation, and persisted producer packet identity. Packet state is
not required, so pure rerender does not discard proof. Once review changes Work
to Blocked, the pre-state basis cannot be recomputed, so `ReviewWorkCas` stores
that exact candidate basis. Lowering verifies every sealed candidate still has
exact provenance, rescans candidates through the same basis and producer
packet/Work/contract/generation lineage, and requires exact sorted-set equality.
A stale sibling from another execution basis remains history but does not enter
the set; a new or removed candidate on the sealed basis is CAS drift and
refuses. Runtime ingress already bound each matching candidate to its exact job
execution basis. The rule is also recorded in
`REVIEW-R08-R09-AMENDMENT.md`.

Return classification separately rechecks current applicability. The bound
strategy/lowering must retain their exact local revisions and semantic digests;
the packet must still be Current with the captured identity; Work must remain
Active with the exact active job and validation generation; the active contract
must retain its version/digest; and `DomainBasisProvider` must derive the same
current `Dispatch(work)` execution basis. After causal relowering, the fixture
resolves the earlier return again without mutation and observes
`StaleReviewInput`, while the old candidate and provenance remain readable.

## Verification

- `run-cargo.ps1 test -p zap-app --test packet_resolution_service`: 1/1 green;
  final full causal run body 3.55 seconds, including repeated-failure counter
  protection, symmetric candidate-basis CAS and stale-return classification.
- `run-cargo.ps1 test -p zap-domain --test lowering_offline`: 1/1 green.
- `run-cargo.ps1 test -p zap-domain --test lowering_semantic_service`: 1/1
  green using public effect comparison and unchanged approved payloads.
- `run-cargo.ps1 clippy -p zap-app --tests --no-deps -- -D warnings`: green,
  16.02 seconds.
- `run-cargo.ps1 clippy -p zap-domain --test lowering_offline --test
  lowering_semantic_service --no-deps -- -D warnings`: green, 18.48 seconds.
- Scoped Rustfmt check covers every R09 source/test file and the narrow shared
  R08 review/relowering files.

## Remaining integration boundary

R15 still owns production physical source/archive construction, portable
archive layout, path/symlink/no-clobber mechanics, and installed-source adapters.
R09 proves protocol mechanics with real artifact publication and a clearly
simulated host/archive fixture; it does not claim that fixture as the production
portable archive product. No R09 code invokes an executor model, launcher,
network service, or publication path.
