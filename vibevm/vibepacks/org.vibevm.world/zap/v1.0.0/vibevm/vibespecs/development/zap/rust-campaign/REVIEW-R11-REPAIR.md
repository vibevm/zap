# Independent review of R11/R12 repairs

Recommendation: repair required before R11/R12 acceptance. This was a read-only
source/evidence review; no tests, build, Git operation, model, or native tool was
run. The repaired candidate materially closes five original findings, but four
runtime trust/durability defects remain. R07/R16 still own the final persisted
pause/hold composition and live native probe; R08 still owns closed semantic
contracts.

## Original finding disposition

| Original finding | Disposition |
| --- | --- |
| F1 transaction/current launch eligibility | Resolved in structure: claim, authorize, and single-use consume all require transaction-derived `DispatchEligibilityView`. The test provider remains an `AtomicBool`; R07/R16 must supply persisted charter/pause/hold policy. |
| F2 volatile/dead-end native launch | Resolved in source: `NativeDriverCoordinator`, consumed authorization, receipt upgrade, persisted reconciliation, and conservative missing-mailbox `UnknownEffect` exist. One teardown claim in the evidence remains inaccurate below. |
| F3 capability/driver provenance | Partly resolved: capability evidence/digest and driver observation bindings are checked. Producer identity and trusted-host harness scope remain open. |
| F4 only two lifecycle transitions | Mostly resolved: 21 cells/routes now cover the named lifecycle. Artifact-bearing cells are not connected to the artifact witness gate. Semantic lifecycle remains correctly absent for R08. |
| F5 open message/semantic payload kinds | Resolved: agent messages use a closed payload enum; generic semantic serde ports were removed. |
| F6 unreleasable reconciliation retry | Changed, but unsafe: terminal process reconciliation now releases retry while effect/safe state deliberately remain unresolved. |
| F7 goal/cache truth | Goal scope, per-operation planning, content, and store transitions are resolved. Durable equivalent-evidence refresh remains over-conservative. |
| F8 blocking host calls | Resolved for this runtime: `Coordinator` accepts sealed `LocalAgentMailbox`; actual native invocation occurs outside it through a launch ticket. Live harness behavior remains R16 evidence. |

## Open findings

### P0 — terminal reconciliation releases a still-unknown external effect

`ReconciliationState::Terminal` deliberately projects to
`ExecutionState::UnknownEffect` and `SafeState::NeedsReconcile`
(`crates/zap-runtime/src/reconciliation.rs:135-164`). Nevertheless
`RetryHistory::retry_due_with_reconciliation` treats `Terminal` like
`NotStarted` (`retry.rs:135-142`), and `RetryReleasedCell` checks only that
reconciliation row before removing waits (`runtime_updates.rs:297-340`). The
store test confirms this path using a terminal observation with no receipt and
then accepting release (`tests/runtime_persistence.rs:937-964`). A later retry
could repeat an effect whose outcome remains unknown.

Smallest fix: release immediately for `NotStarted`; for terminal work require a
persisted collected/postcondition or positive safe-state record bound to this
job/effect. `UnknownEffect`/`NeedsReconcile` must remain blocking.

### P1 — positive safe state accepts a parsed evidence ID with no evidence

`SafeStateRecordedCell` requires only a nonempty deduplicated `Vec<EvidenceId>`;
it never loads or validates those records, their job/effect/safe-boundary scope,
or applicability (`stop_ingress.rs:217-265`). The assembled store test passes
the nonexistent `evidence-safe-runtime` and persists `Safe`
(`runtime_persistence.rs:1132-1139`). Revalidation later trusts this state.

Smallest fix: add a transaction-bound safe-state evidence provider/scope. A
positive state must resolve current accepted evidence for the exact job,
attempt, effect and declared safe-stop verifier; retain those evidence IDs in
the durable record/view.

### P1 — artifact-bearing runtime events bypass artifact existence enforcement

Core verifies only digests returned by each cell's `PayloadArtifacts` scope
before opening the writer (`zap-core/src/commit.rs:712-727`). Runtime registers
candidate, verification result, liveness checkpoint and malformed-candidate
cells with ordinary `CellSet::single` (`zap-runtime/src/registration.rs:117,
127,130-131`), so their artifact lists are invisible to that gate. The payloads
are at `job_ingress.rs:117-121` and `runtime_updates.rs:35-45,76-93`; the real
store test commits `ArtifactDigest::hash(b"candidate-runtime")` without
publishing bytes (`runtime_persistence.rs:1161-1165`). This proves record
persistence only, not artifact durability.

Smallest fix: implement `PayloadArtifacts` for those four payloads, register
them with artifact-scoped cell constructors, wire the fixed
`ArtifactWitnessProvider`, and make assembled fixtures publish actual bytes
before committing the reference.

### P1 — candidate producer identity is still self-asserted

Candidate ingress checks job/attempt/packet/contract/basis but never checks
`candidate.producer.actor` (`job_ingress.rs:166-205`); that caller-provided actor
is copied into `CandidateProvenanceRecord`. The assembled test invents
`PrincipalId("worker-runtime")` directly (`runtime_persistence.rs:1144-1155`).
The stored producer/acceptor comparison can therefore be evaded by naming a
different producer.

Smallest fix: persist the producer actor in the trusted assignment/dispatch
record and derive `ProducerRef` during trusted collection, or require exact
equality with that stored actor. Worker payload text must not establish it.

### P1 — trusted-host harness binding is not enforced by its grant

`TrustedHostBinding` carries `harness_id` (`zap-core/src/trust.rs:123-134`), but
`TrustedHostHandle::authorize` neither compares nor copies it into the grant
(`trust.rs:175-201`). Runtime cells validate provenance against a stored
capability, but cannot verify that the admitted trusted handle was scoped to
that harness. The report's claim that the grant binds harness is therefore too
strong.

Smallest fix: include the bound harness in `TrustedObservationGrant`/
`AdmittedAuthority` and require driver provenance, capability and external
handle to match it transactionally.

### P2 — equivalent capability refresh becomes pending contradiction

`CapabilityObservedCell` compares full `AgentCapabilities::digest` when the
effective harness/adapter/environment is unchanged
(`capability_goal.rs:166-181`). That digest includes observation ID and evidence
(`zap-core/src/agent/capabilities.rs:146-168`), so semantically identical values
with refreshed provenance become `PendingAdjudication`, contrary to
REPORT-R12's refreshed-equivalent claim.

Smallest fix: compare a canonical capability-value digest that excludes
observation identity/evidence, while retaining those fields in provenance.

## Evidence limits

The first integration scenario does close the original database/service block
before `RedbStore::open` (`runtime_persistence.rs:618-752,757`), so it is a real
cold database reopen. It does not drop *all original handles*: the first
`trusted_slot` and `internal_slot` are declared outside that block and hold the
opaque handles (`615-616`, populated at 62-82); lines 754-755 only shadow the
bindings. Rust drops those earlier shadowed values at the enclosing scope end.
Move the first slots inside the teardown block or explicitly drop them before
open to prove the claimed service-handle lifecycle fence.

The injected launch ticket, observations and `AtomicBool` policy correctly test
mechanics without claiming a real native launch. Acceptance still requires the
R07 persisted eligibility composition, R08 semantic contracts, R16 live native
receipt/goal evidence, and the artifact/evidence repairs above.
