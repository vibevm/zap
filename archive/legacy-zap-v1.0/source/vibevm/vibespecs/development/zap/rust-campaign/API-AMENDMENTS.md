# Accepted implementation refinements after the architecture handoff

These are coordinator-approved implementation clarifications within the accepted
Rust MVP. They preserve legacy zap/1 as history while making required zap/2
semantics implementable. Read alongside RUST-API.md; the foundation integrator
updates that document and notifies consumers. This file is a durable record,
not a second independent public API.

## Revision 8: descriptors and provenance identities

StoredRecord::descriptor and TransitionCell::descriptor return Result. Registry
construction validates and caches descriptors; no unwrap, hidden invalid state,
or debug-only invariant escape is necessary for static registration.
AuthorizationRef and ObservationRef are distinct validated wire identifier
newtypes. They name public provenance records, never credentials or authority.
Admission checks their existence and applicable scope.

## Revision 9: shared runtime ownership and candidate provenance

R11 additionally owns zap-core/src/agent/** and zap-core/src/execution_views/**
for the shared agent, profile, goal, dispatch/result, frontier/work and resume
DTOs/ports. R03 retains generic core, authority, record/storage/query machinery,
all crate lib.rs exports, manifests and initial composition. R11 must reuse
ActorRef, ProducerRef, AdmittedAuthority and CompletionEvaluator from R03.
No sibling implementation crate imports another or invents substitute types.

R03 owns zap-core/src/candidate.rs and the registered core record family
zap.core.candidate_provenance. CandidateProvenanceRecord contains candidate_id,
producer, subjects, contract_id, contract_digest, relevant_basis, artifacts,
observation and revision using the corresponding typed IDs/digests/references.
The constructor checks required values, sorted unique collections and exact
reference shape. Only a trusted collection or explicitly adjudicated import
transition populates it from bound job/receipt evidence. A data proposal or
acceptance payload cannot set its producer authority.

Zap/2 work, evidence, stage and integration acceptance inputs carry CandidateId.
The domain acceptance cell reads core provenance, checks applicable subject,
contract and basis, and compares the stored producer with the service-admitted
acceptor. It does not import runtime records or trust a payload ProducerRef.

## Revision 9: authoritative completion duties

R05's OutcomeProposed/OutcomeRecord declare required_final_gate_evidence_ids
and required_promotions before adoption. These are planned obligations, not
claims that evidence or promotion already exists. The close request cannot
define or shrink them. Each vector has an explicit disposition: Required, or
NoDuty with existing typed charter reference/revision and reason. Empty lists
require a current charter-authorized NoDuty; missing fields are not defaults.
The exact owned type spelling may be chosen by R05 and recorded in its report.
Later changes follow ordinary admitted semantic revision.

## Decision ownership

Middle implementers may select internal typed field names, module structure,
and constructors within their assigned surfaces and normative invariants.
Routine missing shared IDs are coordinated through R03 with a recorded API
revision and consumer notice. Changes to meaning, authority, required behavior,
or cross-track ownership return to the coordinator. Do not block all code work
on an extra Senior naming pass or treat old Python payload fields as immutable
constraints on the explicitly new Rust epoch.

## Revision 10: shared relevant-basis implementation ownership

The coordinator assigned the accepted section 9 `RelevantBasis` DTO and
`BasisProvider` surface to R06 under `zap-core/src/basis/**`; R04 retains the
generic commit/store service and the `zap-core` entrypoint. The object-safe
provider computes a current typed basis from `StateReader` for one
`BasisPurpose` and validates that a proposed subject scope is no narrower than
the kernel-derived closure. `RelevantBasis::new` canonicalizes set-like fields
and computes its digest without `observed_revision`, so unrelated journal
progress may rebind while semantic input changes cannot. The implementation
reuses the existing shared `SourceFingerprint` rather than introducing a
second same-named core type. R04 wires the exported provider into transactional
admission after the R06 source lands.

## Open review findings

R06 additionally owns shared zap-core/src/basis/** for the accepted RelevantBasis
DTOs and BasisProvider trait. R04 retains lib.rs exports and generic admission
integration. This follows the same disjoint shared-type ownership as R11;
domain code must not invent a local incompatible basis.

R11/R12 is under repair per REVIEW-R11.md. Current transaction-bound eligibility
and a fenced pre-effect admission must guard actual dispatch. Native mailbox
absence is unknown, not NotStarted; trusted driver ingress and every required
lifecycle transition must persist through the store. A distinct
DispatchEligibilityDigest is a routine approved domain digest for the exact
evaluated request/view, coordinated through R04's wire declaration table.

R03: typed serialization must reject nested non-finite floats before serde_json
can turn them into null; strict tagged input must reject unknown members.
LegacyIdentity deserialization must preserve constructor invariants. These are
wire-scope checks, not a reason to rerun an unrelated full panel.
