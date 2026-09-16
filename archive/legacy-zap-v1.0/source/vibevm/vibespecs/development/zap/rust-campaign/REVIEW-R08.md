# Independent Senior review: R08/R09 lowering and weak round trip

Status: repair required before bounded R08/R09 acceptance. Root retains acceptance.

## Findings

### P1 — the new lowering record is disconnected from the executable work graph

`planning.lowering-applied` validates and inserts only `LoweringRecord`
(`crates/zap-domain/src/lowering/cells.rs:118-190`). The already registered
`domain.plan-lowered` path separately creates `WorkRecord` and
`TaskContractRecord` rows (`crates/zap-domain/src/control/cells.rs:243-281`).
Neither path binds or atomically derives the other. `LoweredWork` carries only
an embedded contract, route and checks (`lowering/model.rs:74`), with no lowered
dependency/parent/origin graph, and `WorkerPacketRecord` remains a planning
record (`lowering/records.rs:68`). The runtime claim accepts a complete
caller-supplied `RuntimeJobRecord` (`crates/zap-runtime/src/transitions.rs:23,
162`) and never resolves a `WorkerPacketRecord` or this lowering.

Consequently an economics-admitted lowering can be durably marked applied while
creating no work or active contract the scheduler can use; the inverse existing
`domain.plan-lowered` mutation can create executable rows with no strategy,
lowering or packet lineage. The service scenario hides this split by seeding an
existing `work.one` and the strategy (`tests/lowering_service.rs:42`) and never
dispatches a lowered result. This is missing R08 product behavior, not R12/R13
transport work. Repair should make one admitted semantic operation atomically
publish or bind the checked lowering and exact executable work/contract graph,
then prove the resulting packet can be resolved into the runtime claim without
re-entering those identities as caller labels.

### P1 — conservation accepts nonexistent or unauthorized dispositions

The shape checker has the correct complete active-obligation denominator, but
its escape routes are not resolved against authoritative state. A `Successor`
needs only a sorted nonempty ID vector, and `Inapplicable` needs only empty work
assignments (`lowering/transitions.rs:182-200`); successor existence, outcome
lineage and the supplied `AuthorizationRef` are never checked. A stage can be
`Discharged` by any `EvidenceId` or `Deferred` by any `DeferralId`
(`lowering/model.rs:58`), while `validate_stage_debt` checks only that one row
matches each declared route stage (`lowering/transitions.rs:336-370`). Embedded
contracts are hashed and internally compared, but are not checked against or
published as current `TaskContractRecord`s. Open deferrals are also collected
campaign-wide rather than for the strategy outcome (`lowering/cells.rs:178-184`).

This lets a lowering apparently conserve an obligation or stage by naming a
fictitious successor, evidence row, deferral or authority reference. Repair
must resolve every successor/disposition/evidence/deferral against the current
outcome, charter and applicable proof, preserve parent/child and dependency
origins, and add negative service cases for each false typed label.

### P1 — packet routing and source closure are still caller assertions

The transaction-basis gate is a sound partial boundary: packet render requires
the lowering semantic digest, recomputed lowering basis roots, work read/write
subjects, selected checks and safe-stop text to match the lowering
(`lowering/cells.rs:239-276`). A related source recapture therefore makes the
old render stale without requiring a new economics admission for a pure
rerender.

The rest of the promised packet is not derived or checked. `PacketAssembly`
and `AssembledPacket` omit campaign/store/base/strategy, the contract and its
digest, resources, integration ownership, obligation coverage, maturity and
stage debt (`lowering/model.rs:291-343`). Desired role/profile, fragments,
instruction authority, abstraction, fork projection and free-text result
contract arrive from the internal caller. `route_role` is used only by the pure
helper tests, never by a registered service path. Packet assembly merely
requires at least one included fragment (`lowering/packets.rs:144`); the real
service fixture succeeds with one `Protocol` fragment and no assignment or
source bytes (`tests/lowering_service/fixtures.rs:271-289`). Its instruction
authorization is also just a parsed reference.

Thus a caller can label high-consequence work Junior, omit the assignment and
required source, invent instruction authority, or change the result contract
while passing the current render gate. Repair must derive immutable contract,
role and debt fields from current records; validate capability-backed routing;
bind each required source capture to matching bytes/artifact or a typed
retrieval handle; project the actual prepared fork; and use a typed
candidate-only result contract.

### P1 — the bundle manifest does not establish an offline execution envelope

Bundle export checks that the named strategy exists, but never requires
`manifest.strategy_id == lowering.strategic_revision_id`
(`lowering/cells.rs:324-352`). Permission references, capability observations
and stop conditions are only nonempty sorted vectors
(`lowering/packets.rs:182-188`); no current record or packet field is consulted.
The stop field is `ConditionId`, not the required `StopRuleId`
(`lowering/model.rs:348-363`). Archive entries are path/length checked but are
not required to represent the packet digest, its included fragments, lowering
source digests, rules or prepared forks. The service fixture demonstrates the
gap with fabricated permission and capability IDs, a fork condition used as a
stop condition, and an unrelated `packet-entry` digest
(`tests/lowering_service/fixtures.rs:310-323`).

R15 may legitimately own physical archive creation, byte rehashing, symlink
defence and no-clobber publication. It cannot recover semantic entries omitted
from a valid domain manifest. R12 may supply actual observations, but R08/R09
must consume them through a validated witness/provider or record an honest
capability wait; a parseable observation ID is not capability evidence. R13
transport likewise cannot repair the manifest. Require exact strategy/lowering
lineage and derive permission, capability, stop, packet, rule, source and fork
closure before handing the manifest to the later archive adapter.

### P1 — weak-return provenance and reassessment are not the reported cycle

Standalone `planning.encounter-recorded` bypasses `classify_return` and its
failure/approach and set checks (`lowering/cells.rs:369-427`). Return import
checks only packet membership plus sorted artifact/evidence IDs, so attempt,
problem, fork and candidate IDs are otherwise producer labels; any current
Attempt/Choice/Observation is classified `ApplicableCandidate` even when
`candidate_id` is absent (`lowering/packets.rs:224-270`). There is no binding to
a durable runtime attempt, producer, contract, packet basis, selected
alternative or candidate provenance. This does avoid accidental acceptance or
authority activation, but it does not establish trustworthy encounter lineage.

The reported revised-lowering cycle is also only temporal adjacency. The test
imports an encounter, independently recaptures a source, then constructs a
second complete lowering (`tests/lowering_service.rs:155-230`); the second
lowering consumes no bounded return delta or `ReturnImportRecord`, and no causal
reference identifies which returned assumption or evidence required change.
Repair should share one encounter validator, require exact packet/attempt/fork/
candidate and old/new-basis provenance, preserve problem/approach history, and
make strong reassessment produce a bounded affected delta that causally yields
only the needed new lowering or an explicit no-change result.

### P2 — the advertised bundle query is not bounded by the requested bundle

`zap.planning.bundle` calls `scan_all::<EncounterRecord>` and filters only after
materializing the complete encounter family (`lowering/queries.rs:50-53`);
`scan_all` pages until the family is exhausted (`seams/storage.rs:36-63`). The
returned manifest vectors also have no count or encoded-size bound, and
zero-length entries do not consume `maximum_bytes`. R17 can replace scans with
indexes, but the present operation cannot be described as a bounded query.
Either bound this query now or narrow the R09 claim to a correctness-first
unbounded scan with explicit R17 closure.

### P2 — the service evidence does not cover all six registered routes

The record/cell/query registration itself is real. The service test exercises
semantic lowering, packet rendering, bundle export and return import, including
duplicate import and related-source invalidation. It seeds the strategy through
a test-only owner cell instead of `planning.strategy-proposed` and never calls
`planning.encounter-recorded`. Core exposes `AgentDataGrant` but no issuer, so
those two `DataProposal` routes cannot currently be exercised by an external
caller. R13 may expose commands, but transport alone cannot manufacture the
missing non-authorizing grant. Add the official bounded issuer/integration seam
and exercise both routes, or explicitly stop claiming a complete registered
service round trip.

## Evidence and recommendation

The six planning families, six cells and one query are registered. The
`planning.lowering-applied` route is genuinely `Privileged(plan.lower)` with a
payload-derived `BasisPurpose::Lowering`, and the service fixture uses the R07
change-admission provider rather than a local bypass. Lexical archive paths,
foreign return identity, conflicting bundle reuse and duplicate encounter
insertion receive useful checks. Weak execution remains explicitly simulated,
and this review found no model call, upload, credential inclusion or authority
activation path.

This was a source review under the packet prohibition on builds and tests. The
reported 33 selected regressions, 7 final lowering tests and scoped clippy are
worker receipts, not rerun evidence. The checkpoint's line scan names
`cells.rs` as 541 lines while the reviewed file is currently 588 lines; it is
still under the 600-line limit, but the receipt does not carry source digests,
so applicability to the reviewed bytes is not mechanically established.

Recommendation: repair and re-review. The current candidate is a useful typed
scaffold with a real economics/basis seam, but its central R08/R09 promises can
still be satisfied by internally consistent labels without executable work,
source/capability closure or return provenance. R12, R13, R15 and R17 remain
valid downstream owners only for actual capability discovery, transport,
physical archive effects and indexed scale after these domain invariants are
closed.
