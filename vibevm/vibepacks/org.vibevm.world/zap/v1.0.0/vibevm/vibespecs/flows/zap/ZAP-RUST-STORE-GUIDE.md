# Rust store usage guide {#root}

`guide r1`

This non-normative guide describes the [zap_store public exports](../../../../crates/zap-store/src/lib.rs): 20 named local types, plus two concrete handles exposed through public transaction associated types. The catalogs keep those associated handles separate from crate-root imports.

The [storage contract](ZAP-RUST-STORAGE.xml) governs canonical history, trusted admission, artifacts and recovery. Creating storage, preparing an artifact, committing a mutation, rebuilding a projection and accepting product work are separate operations.

## Create or reopen the configured store {#store-lifecycle}

`guide r1`

Use [RedbStore::create or open](../../../../crates/zap-store/src/engine.rs) with the configured destination and store identity. Creation refuses an existing path. Opening reads the recorded identity and physical format; it does not select an Owner campaign or establish application credentials.

| Type | Role in this operation |
| --- | --- |
| `RedbStore` | Configured transactional store handle and entry point for storage operations. |

Attach the required record registrations and query epoch with [with_records](../../../../crates/zap-store/src/engine/store.rs). The store crate's own [registration functions](../../../../crates/zap-store/src/registration.rs) return empty record and capability sets, so application composition supplies the product's actual record contracts.

Read `identity`, `physical_schema` and `head` from the configured store when binding subsequent operations. Normal reopen and a replay-based integrity audit are different operations under the [storage trust contract](ZAP-RUST-STORAGE.xml).

## Use transaction handles through the service contracts {#transaction-handles}

`guide r1`

[RedbStore's TransactionStore implementation](../../../../crates/zap-store/src/engine/transaction.rs) exposes read and write handles through associated types. They are usable through the trait and inferred values, but their concrete names are not reexported as `zap_store` root imports.

| Type | Role in this operation |
| --- | --- |
| `RedbRead` | Concrete associated read handle supplied by TransactionStore. |
| `RedbWrite` | Concrete associated write handle supplied to a permitted transaction callback. |

The read handle represents its captured transaction boundary. The write handle is supplied to a transaction callback bound by a `TransactionPermit`; constructing data or holding the store handle does not manufacture that permit. Ordinary product mutations enter through the configured commit service, retaining admission, atomic commit and exact-retry checks.

## Read committed events and retain their cursor {#event-history}

`guide r1`

[event_at_revision and event_tail](../../../../crates/zap-store/src/history.rs) return stored event bytes and identity at an observed boundary. Consume the returned cursor when the tail reports more data; do not derive a cursor from how many events a presentation displayed.

| Type | Role in this operation |
| --- | --- |
| `StoredEvent` | One stored event's sequence, byte identity and original bytes. |
| `EventTailCursor` | Store/base-bound position for continued event reading. |
| `TailCompleteness` | Complete tail result or continuation cursor. |
| `EventTail` | Returned committed event slice and its observed boundary. |

Decode event bytes using the registered logical-event contracts for their schema. A stored event, an exact retry and a new application acceptance remain distinct facts. Store/base/revision binding belongs to the cursor and is checked by the tail operation.

## Inspect snapshots and choose the intended integrity check {#snapshot-audit}

`guide r1`

[snapshot_manifest](../../../../crates/zap-store/src/history.rs) describes the observed logical boundary. `audit_hashes` checks persisted history and receipt identities; replay-based `audit`, `audit_with_admission` and `audit_with_replay_context` add their respective registered transition/admission context.

| Type | Role in this operation |
| --- | --- |
| `SnapshotManifest` | Observed logical store boundary and projection identity. |
| `AuditReport` | Result tying checked history/commands to a snapshot. |

Choose the audit variant that matches the stored event schemas and required providers. A hash check is not a substitute for rederiving projections with the applicable semantics. Consume the resulting report with its snapshot identity and checked denominator rather than treating an arbitrary serialized report as proof.

## Inspect physical storage independently of logical identity {#physical-representation}

`guide r1`

The [physical schema](../../../../crates/zap-store/src/physical.rs) and [physical snapshot operations](../../../../crates/zap-store/src/history/physical.rs) describe the storage representation separately from logical history. Use `physical_snapshot_manifest`, `logical_row_digest` and `physical_table_metrics` for their stated purposes.

| Type | Role in this operation |
| --- | --- |
| `PhysicalSchema` | Selected physical table/encoding representation. |
| `PhysicalProjectionAlgorithm` | Algorithm used for physical projection identity. |
| `PhysicalSnapshotManifest` | Physical representation identity alongside its logical snapshot. |
| `PhysicalTableMetrics` | Observed row and byte counts for the physical tables/database. |

Different physical representations can preserve the same logical rows. Retain the reported algorithm and schema when comparing digests or measurements. Byte and row metrics are observations of the inspected database, not a promised latency or a package release version.

## Rebuild selected derived indexes at an exact boundary {#derived-indexes}

`guide r1`

[rebuild_indexes_v2](../../../../crates/zap-store/src/engine/indexes/rebuild.rs) takes selected index families, algorithm identities and an expected revision. It obtains contributions from registered records and returns the resulting catalog and row count.

| Type | Role in this operation |
| --- | --- |
| `IndexRebuildReceipt` | Catalog and row count returned by the selected index rebuild. |

Use [index_catalog](../../../../crates/zap-store/src/engine/indexes.rs) to inspect the current catalog. A rebuild result describes derived index state; it does not change the meaning of canonical history or grant application maintenance authority.

## Prepare and verify a separate physical rebuild {#physical-rebuild}

`guide r1`

[rebuild_physical_v2](../../../../crates/zap-store/src/engine/rebuild.rs) rebuilds a supported V1 source into a separate destination using the supplied replay context. Its recovery paths validate their recorded source/destination identity and reserved staging state.

| Type | Role in this operation |
| --- | --- |
| `PhysicalRebuildReceipt` | Source/destination identity and pointer-switch state for a physical rebuild. |

Consume the returned source and destination manifests and `pointer_switched` state explicitly. A rebuilt destination does not implicitly switch the application's configured store or authorize unrelated repairs. Keep this operation distinct from the selected derived-index rebuild.

## Advance and cancel a durable traversal session {#traversal-sessions}

`guide r1`

The [traversal session operations](../../../../crates/zap-store/src/engine/traversal.rs) bind a session to principal, store revision, query and catalog identities. Begin from a `TraversalSessionSpec`, then advance with the expected generation and exact request identity.

| Type | Role in this operation |
| --- | --- |
| `TraversalSessionSpec` | Identity and bounds used to create a derived traversal session. |
| `TraversalAdvanceRequest` | Generation-bound request to advance that session. |
| `TraversalAdvanceResult` | New progress or an exact cached retry result. |
| `TraversalCancelReceipt` | Cancellation and bounded cleanup outcome. |

The advance callback receives the core `DerivedTraversalState` interface and returns canonical response bytes. Distinguish a newly advanced result from an exact retry. Cancellation is separate from completed cleanup; consume `TraversalCancelReceipt` instead of assuming all derived rows disappear immediately.

## Prepare, publish and reopen content-addressed artifacts {#artifact-publication}

`guide r1`

[ArtifactStore::prepare_file](../../../../crates/zap-store/src/artifact.rs) copies source bytes into staging and obtains their content identity. Consume `PreparedArtifact::publish` to publish the verified content; `ArtifactStore::open_verified` reopens by expected digest and length.

| Type | Role in this operation |
| --- | --- |
| `ArtifactStore` | Content-addressed artifact preparation and verified-reopen entry point. |
| `PreparedArtifact` | Verified staged content awaiting publication. |
| `PublishedArtifact` | Published content with a retained file witness. |

`PublishedArtifact::read_verified` reads through its retained witness, and `verify` rechecks that witness. Published bytes are separate from a committed reference or accepted evidence. The configured commit path obtains its artifact witnesses through the core witness-provider interface; callers do not substitute an unverified path for a publication.

## Existing integration examples {#integration-examples}

`guide r1`

- [Commit-service example](../../../../crates/zap-store/src/engine/tests/service_commit.rs): a registered typed Owner operation, persisted result and exact retry through the service.
- [Store engine examples](../../../../crates/zap-store/src/engine/tests.rs): atomicity, conflict and reopen cases within the store's test harness.
- [Physical rebuild examples](../../../../crates/zap-store/src/engine/tests/a2.rs): preserved logical rows, recovery of post-commit staging and refusal of unrelated or invalid destinations.
- [Artifact examples](../../../../crates/zap-store/src/artifact.rs): publication without overwrite, verified reopen after restart and refusal of a wrong length/hash or non-file destination.

These existing sources illustrate integration with explicit fixtures. Their existence is not a fresh execution receipt or a blanket integrity claim for a different store.

## Private implementation boundaries {#internal-boundaries}

`guide r1`

`TraversalWrite` is a source-level public declaration inside the private [traversal module](../../../../crates/zap-store/src/engine/traversal.rs), without a public reexport or concrete callback signature. Consumers use `DerivedTraversalState`; they do not construct the hidden traversal writer. `TraversalHeader` and `CachedPage` are private storage details.

The private `StoreArtifactGuard` in [artifact.rs](../../../../crates/zap-store/src/artifact.rs) retains witnesses behind the core artifact-guard trait. Test-only commit fixtures are likewise not public mutation APIs. By contrast, `RedbRead` and `RedbWrite` are exposed through public associated types and are documented in the transaction-handles section.

