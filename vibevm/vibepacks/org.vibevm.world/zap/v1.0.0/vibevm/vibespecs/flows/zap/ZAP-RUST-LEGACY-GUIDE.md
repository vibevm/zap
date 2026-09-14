# Rust legacy import usage guide {#root}

`guide r1`

This non-normative guide describes the public `zap_legacy` API and its existing usage scenarios. The [crate exports](../../../../crates/zap-legacy/src/lib.rs) expose the 28 locally declared types cataloged below. The reexported `LegacyEpoch` remains a `zap_wire` type.

The [storage contract](ZAP-RUST-STORAGE.xml) governs preservation, epochs and import authority. Reading source bytes, constructing a manifest and committing an import are separate operations. Import records retain historical authority classifications; they do not activate a charter or dispatch work.

Links to test sources identify executable examples, not claims that those tests were run for this guide or that a whole campaign is accepted.

## Decode preserved values and handle refusals {#legacy-codec}

`guide r1`

Use [unpack and packed](../../../../crates/zap-legacy/src/codec.rs) for the frozen legacy representation. Inspect the returned value before choosing a transformation, and retain original file bytes separately when their identity matters. A general JSON serialization round-trip is not a substitute for the legacy codec.

| Type | Role in this operation |
| --- | --- |
| `LegacyValue` | Decoded legacy value, including preserved large integer spellings and tagged values. |
| `LegacyTag` | Meaning of a retained date, time or special numeric tag. |
| `LegacyCodecError` | Refusal returned by codec operations, inspected through its code and message. |
| `LegacyDiagnostic` | Bounded diagnostic produced for downstream reporting. |

`LegacyCodecError::diagnostic` produces the bounded form defined in [types.rs](../../../../crates/zap-legacy/src/types.rs). Error fields are private; callers consume operation results rather than inventing error constructors.

The [legacy corpus](../../../../crates/zap-legacy/tests/legacy_corpus.rs) demonstrates exact packed-byte round-trips, integers that exceed fixed-width ranges, duplicate-member refusal and invalid tagged-value handling. Those examples distinguish tagged nonfinite values from invalid bare non-JSON numbers.

## Map historical identities without losing their spelling {#identity-mapping}

`guide r1`

Construct a [LegacyId](../../../../crates/zap-legacy/src/types.rs) with its historical kind and original spelling. Then use [ImportMapBuilder](../../../../crates/zap-legacy/src/mapping.rs) to select the destination identity category, map it and finish an ordered mapping collection.

| Type | Role in this operation |
| --- | --- |
| `LegacyKind` | Historical category that participates in the retained identity. |
| `LegacyId` | Kind and bounded original spelling of one historical identity. |
| `CurrentImportId` | Typed destination identity selected by the mapping. |
| `ImportIdMap` | Preserved association between historical and current identities. |
| `SubjectTarget` | Destination subject category for a subject mapping. |
| `ImportMapBuilder` | Accumulates mappings and checks destination collisions. |

`deterministic_spelling` preserves an already valid current spelling; otherwise it derives a stable spelling while `LegacyId` retains the original. This crate's `LegacyId` is distinct from the string wrapper exported as `zap_wire::LegacyId`.

The `deterministic_mapping_and_lineage_are_lossless` example in [legacy_corpus.rs](../../../../crates/zap-legacy/tests/legacy_corpus.rs) maps both an existing event name and a path containing Unicode. A mapping changes representation, not the authority of its source.

## Read committed history and retain incomplete bytes {#history-read}

`guide r1`

[Zap1Reader::read](../../../../crates/zap-legacy/src/journal.rs) accepts the original base and journal bytes. Consume the returned committed history and inspect any pending tail separately. A fragment without its final line terminator does not become a committed event.

| Type | Role in this operation |
| --- | --- |
| `Zap1Reader` | Entry point for validating the legacy base and journal. |
| `LegacyStoreRead` | Read result containing committed history and retained source material. |
| `LegacyEventRecord` | One observed event line with its historical identity and duplicate classification. |
| `LegacyReadError` | Read or lineage refusal with a diagnostic byte offset. |
| `LegacyDigestDomain` | Identifies which exact byte representation a digest describes. |
| `LegacyDigest` | Digest paired with that legacy byte-domain label. |
| `PendingTail` | Uncommitted trailing bytes retained for subsequent recovery decisions. |

The digest and tail types are defined in [types.rs](../../../../crates/zap-legacy/src/types.rs). `LegacyDigest::hash` hashes the supplied bytes with SHA-256 and retains the domain label; it does not add a cryptographic salt. File bytes including LF, packed command bytes without LF and journal prefixes with their terminators remain distinct inputs.

The reexported `LegacyEpoch` is documented by its owning [wire epoch source](../../../../crates/zap-wire/src/epochs.rs). The reader returns the legacy epoch; this is separate from the current Rust store epoch and package release version.

The journal and CRLF cases in [legacy_corpus.rs](../../../../crates/zap-legacy/tests/legacy_corpus.rs) show exact offsets, duplicate events, pending fragments and corrupt-middle refusal. Inspect `LegacyReadError::code`, `message` and `offset`; its construction helper is crate-private.

## Validate a projection and snapshot against history {#projection-snapshot}

`guide r1`

Use [LegacyProjection::replay](../../../../crates/zap-legacy/src/replay.rs) to derive state from a validated history. [LegacySnapshot::parse](../../../../crates/zap-legacy/src/snapshot.rs) takes both snapshot bytes and that history, so snapshot data is checked against the historical boundary rather than accepted from its timestamp or presence.

| Type | Role in this operation |
| --- | --- |
| `LegacyProjection` | Replayed legacy state and its packed-byte identity. |
| `LegacySnapshot` | Snapshot whose history, state and reducer bindings were checked during parsing. |

The `snapshot_binds_exact_history_state_and_reducer` example in [legacy_corpus.rs](../../../../crates/zap-legacy/tests/legacy_corpus.rs) checks preserved digest domains and rejects a modified revision. Successful legacy replay describes the frozen legacy behavior; it does not assert that later Rust corrections are byte-identical behavior.

## Inspect a source directory without activating it {#source-inventory}

`guide r1`

[LegacySource::open](../../../../crates/zap-legacy/src/source.rs) reads a legacy source directory and returns its history plus inventory. `LegacyInventory::from_base` derives the inventory from an already decoded base. These operations do not dispatch source tasks.

| Type | Role in this operation |
| --- | --- |
| `LegacySource` | Read source, retained path spelling and its derived inventory. |
| `LegacyInventory` | Counts, source identity and authority classification derived from the base. |

The [actual-source example](../../../../crates/zap-legacy/tests/actual_source.rs) runs its source inspection only when `ZAP_LEGACY_ACTUAL_SOURCE` is configured. Without that variable it returns without exercising a source. Its fixed pilot counts are fixture expectations, not a universal shape for imported projects or guaranteed evidence from every test run.

## Construct preserved import lineage {#import-lineage}

`guide r1`

Use [LegacyObjectRecord::from_event and LegacyImportManifestRecord::new](../../../../crates/zap-legacy/src/records.rs) with the validated reader result and identity map. Preserve source spelling, bytes and historical classification through these constructors. Manifest construction is preparation; it does not write a destination store.

| Type | Role in this operation |
| --- | --- |
| `LegacyObjectKey` | Stable destination key for one imported source object. |
| `LegacyObjectRecord` | Preserved event object and its mapping to current identity. |
| `LegacyAuthorityClass` | Historical authority classification retained as data. |
| `LegacyImportManifestRecord` | Import boundary tying source history, mapping and inventory together. |

The records implement canonical encoding and `StoredRecord`; `LegacyObjectKey` supplies the record-key encoding. Those traits make records usable by registered storage machinery. They do not make caller-constructed records accepted state.

The lineage example in [legacy_corpus.rs](../../../../crates/zap-legacy/tests/legacy_corpus.rs) builds records from real fixture events, constructs the manifest, and checks original bytes and inactive execution flags. Later payload validation rechecks the assembled lineage before import admission.

## Commit an inactive import through the registered service {#import-service}

`guide r1`

[LegacyImportPayload::validate](../../../../crates/zap-legacy/src/import.rs) rederives the supplied history and checks its manifest, objects and preserved byte identities. The same source defines `LegacyImportCell`, whose registered `legacy.import-recorded` operation uses the service-internal route.

| Type | Role in this operation |
| --- | --- |
| `LegacyImportPayload` | Prepared manifest and objects submitted for import validation. |
| `LegacyImportOutput` | Result returned after the registered import transition. |
| `LegacyImportCell` | Transition that records validated inactive lineage through the service. |

[Registration functions](../../../../crates/zap-legacy/src/registration.rs) supply the import cell, record families and route. The crate's query and capability sets are empty; registering an import route does not advertise unrelated operations.

The [official-service import example](../../../../crates/zap-legacy/tests/import_service.rs) composes a trusted bootstrap and registered service, rejects malformed input without advancing the store, reopens it, commits the valid import and checks exact retry and audit. Its bootstrap is explicit fixture infrastructure, not a grant obtained by constructing payload data.

The [CLI import operation](ZAP-CLI.md) is the configured application entry point. Keep reading, payload validation, service authorization, committed receipts and source activation distinct. An imported authority classification never implicitly starts the imported campaign.
