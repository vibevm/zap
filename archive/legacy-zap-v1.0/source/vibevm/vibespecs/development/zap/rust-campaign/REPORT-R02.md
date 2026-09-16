# R02: normative Rust MVP requirements

Status: candidate for coordinator acceptance.

## Result

The accepted V01-V24 vision is now represented by permanent English normative
facts without claiming that the Rust product already implements them. All
pre-existing fact IDs in the five original ZAP XML specifications remain
present. Their status text and production wording now distinguish historical
Python prototype evidence from required Rust behavior.

Three focused permanent contracts were added:

- `ZAP-AGENT-PROTOCOL.xml` defines role versus authority, desired and resolved
  profiles, producer and acceptor separation, token-first packet construction,
  source closure and omissions, typed abstraction, exact packet and attempt
  lineage, compact messages, native `AgentHost`, liveness, recovery, goals,
  focused verification, and the current local-inference prohibition.
- `ZAP-LOWERING-AND-DREAMER.xml` defines the four linked plan representations,
  checked semantic lowering versus pure rendering, stage-debt conservation,
  strategic forks, portable weak-execution round trips, local weak coordination,
  detached Dreamer branches, persisted grill state, exact promotion and removal,
  economics scope, and Owner-stop precedence.
- `ZAP-RUST-STORAGE.xml` defines the Rust-only production surface, typed reducers,
  one-transaction `LogicalEvent` and index mutation, controller fencing, bounded
  graph queries, incremental invalidation, checkpoints and unknown-effect
  recovery, trusted-local versus distrustful cold start, explicit legacy and new
  epochs, generic-project portability, and truthful observability.

The stable vocabulary is aligned with the R01 foundation: `StoreId`, `BaseId`,
`Revision`, `Sequence`, `EventId`, `CommandId`, `RelevantBasis`, `AgentHost`,
`SemanticProvider`, `AgentCapabilities`, `DispatchIntent`, `ExternalJobHandle`,
`StrategicPlanRevision`, `LoweringRevision`, `Packet`, `WeakBundle`, and
`DreamBranch`. Shared boundary anchors include
`RUST-STORAGE-ONE-TRANSACTION`, `RUST-STORAGE-TRUSTED-LOCAL-BOUNDARY`,
`RUST-STORAGE-TYPED-REDUCERS`, `StoreEpoch`, `LegacyEpoch::Zap1`, `AGENT-HOST-BRIDGE`,
`AGENT-INTENT-IS-NOT-LAUNCH`, `LOWERING-STRATEGY-SURVIVES`, and
`DREAMER-NONEXECUTABLE`.

## Requirement denominator

`REQUIREMENTS.json` contains 24 vision records and grouped mappings for every
normative fact. Each group records responsible R00-R19 tasks, current product
state, concrete evidence needs, and final disposition. It also retains four
easy-to-lose legacy groups: codec/hash/CAS behavior, authority and NEXT
inertness, the economics closure P1, and truthful prototype capability mapping.

The root-accepted R14-PREP corpus at commit `5745c866` is recorded as accepted
bounded legacy evidence. It covers canonical legacy encoding, reserved tags,
non-finite and duplicate-member handling, exact newline-sensitive hash domains,
exact-integer `base_revision`, journal tail behavior, replay, and cold snapshot
equality. Rust import and integration evidence remain pending under R14; the
corpus does not bless the economics candidate or assign zap/2 identities.

## Document checks

- All eight normative XML documents parse successfully.
- They contain 279 unique fact IDs; the five retained documents preserve every
  fact ID present at `HEAD`.
- `REQUIREMENTS.json` parses and maps 279 of 279 facts exactly once in its fact
  groups. All primary V01-V24 fact references resolve, and every task R00-R19 is
  reachable.
- All eleven relative document links in the requirements baseline resolve from
  the map's directory.
- New authored specifications and the requirement map contain no unintended
  non-English text. Every normative fact remains `spec/plan`.

These are document checks only. R02 changed no production source and ran no
product tests, external launcher, native worker, or local model inference.

## Integration risks and follow-up ownership

1. `vibevm/vibespecs/boot/13-flow-zap.xml` still describes a bounded Python
   reference implementation and points to the legacy Python API. It was outside
   R02 ownership. R15 must replace that installed projection only after actual
   Rust capabilities and public contracts exist.
2. Existing Python economics work remains candidate evidence. R07 and R16 must
   prove the shared completion-blocker predicate through scheduler and direct
   close; neither runtime may overlook a pending selected decision, hold,
   unconsumed effect, forecast, or unknown external outcome.
3. The package manifest and generated boot/index files do not yet publish the
   three new permanent specifications. R15 owns registration after the Rust
   surface exists; early registration must not advertise unimplemented commands.
4. R03 proceeds against the accepted R01-FOUNDATION child boundary while the
   broader R01 architecture task continues. Later API refinement must preserve
   the stable semantic anchors above or issue an explicit coordinated revision.
5. R01 fixes `StoreEpoch` zap/2 for new stores and `LegacyEpoch::Zap1` for
   read-only import, with domain-separated legacy digests and an explicit ID
   map. R03 and R14 must preserve that seam; package version remains 1.0.0.

R02 is ready for coordinator review. Product implementation state remains
unverified until the mapped downstream tasks supply applicable evidence and the
coordinator records acceptance.
