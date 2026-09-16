# R17 A2 compact physical schema 2 candidate

Status: selected A2 implementation and bounded evidence are complete for root
review. B, an actual-sized 425-node run, pointer switching, publication and the
later broad R17 graph/query panel remain unselected.

## Implemented physical boundary

New stores default to private physical schema 2. `RedbStore::physical_schema()`
reports the checked schema, and tests may explicitly create physical v1 stores
to retain the frozen writer/replay proof. An old store with no
`physical_schema` marker opens as v1 only when its schema-1 marker and complete
v1 table catalog exist. Unknown, missing, partial and mixed v1/v2 catalogs
refuse; no old bytes are reinterpreted.

Physical v2 adds only `records_v2`, `history_by_revision_v2`,
`history_by_record_v2` and `history_event_meta_v2`. Ordinary `index_rows`, event
bytes, command receipts and logical epochs are unchanged.

Current records use the checked binary `ZRV2` envelope from the selected
contract: value/version codec IDs, bounded version/value lengths, storage-key
and value SHA-256 values, then exact version and canonical value bytes. Decode
rejects truncated, trailing, wrong-magic, wrong-codec, wrong-key, wrong-value,
noncanonical and typed key/version-mismatch rows.

History stores one `ZHV2` body in revision order, a 32-byte body digest in
record order, and canonical event ID/command ID/reason/event digest metadata
once per revision. The presence mask distinguishes absent and empty values.
Both query orders reconstruct and validate the unchanged public
`RecordHistoryEntry`; missing primary/meta, orphan or bad secondary rows,
reserved bits, invalid lengths, trailing bytes and typed value mismatches are
corruption.

The existing v1 `SnapshotManifest` remains its frozen shape. The new
schema-2 `PhysicalSnapshotManifest` names physical schema, physical projection
algorithm/digest and the versioned logical-row digest. Both physical and logical
digests come from one redb read transaction. The logical digest streams exact
current family/key/version/value rows, unchanged ordinary index rows and
reconstructed public history rows. It excludes the v2 secondary digest and
physical metadata rows.

## Safe v1-to-v2 rebuild

`rebuild_physical_v2` accepts only a physical-v1 source and a fresh sibling
destination. It replays the source's exact event bytes through the supplied
registered replay context into a v2 staging store. The schema-2 receipt binds
source path, destination path, source identity/head/schema/physical digest,
destination schema/physical digest, equal logical-row digest and an explicit
`pointer_switched=false`.

Final publication follows the root-approved reservation amendment now recorded
in `REPAIR-R17-REPRESENTATION.md`. A prepared claim is parsed, fsynced and
hard-linked without clobber. A fresh claim must successfully `create_dir` the
final destination; a recovered claim must find the exact hard-linked reservation
marker. An empty destination without that marker is ambiguous and refuses.
The verified staging database is hard-linked into the reservation, opened and
checked against the exact source/head/schema/logical receipt before the ready
receipt is hard-linked last as the publication point. The reservation marker
and claim are removed only after verification and directory sync.

Exact retry, completed publication, postcommit/no-receipt staging and a partial
prepared receipt recover. Partial bytes are retained under their content digest.
If a crash occurs after the ready receipt is linked but before reservation or
claim retirement, recovery first re-verifies the completed database and receipt,
then finishes cleanup; absence of the now-redundant reservation marker does not
strand a valid publication. Cleanup treats an already removed owned staging
receipt or database as a completed step. Any remaining database must match the
verified final database byte-for-byte, any remaining receipt must equal the
ready receipt, and partial evidence must retain its content-addressed name;
foreign or unverified leftovers refuse and are preserved.
Foreign claims, files, empty directories and unrelated destinations remain
untouched. A crash after destination creation but before the reservation marker
is deliberately ambiguous and refuses rather than adopting the directory.

## Accepted P0 fixture measurement

The accepted P0 v1 import store was opened read-only at
`C:/Users/olegc/.vibe/zap/r17-p0-a1-debug-20260913T2230Z/published-import-debug/zap.redb`.
It is the exact 32-node, 32-contract, 2-mandate, 34-obligation fixture from
`REPORT-R17-P0-A1.md`, with four task groups and 32 KiB padding per group. No
import was repeated. Debug and release replayed that same source into separate
fresh v2 siblings. Both produced the same logical and physical bytes:

| Measure | v1 | v2 | v2 / v1 |
| --- | ---: | ---: | ---: |
| current + history value bytes | 39,993,249 | 8,847,677 | 22.12% |
| current record values | 13,283,683 | 4,425,703 | 33.32% |
| primary history values | 13,354,783 | 4,416,407 | 33.07% |
| secondary history values | 13,354,783 | 5,312 | 0.04% |
| history event metadata | 0 | 255 | — |
| database length | 136,318,976 | 50,339,840 | 36.93% |
| NTFS allocated bytes | 136,318,976 | 50,339,840 | 36.93% |

`fsutil sparse queryflag` reported all three files non-sparse. Query extents
reported 33,281 source clusters and 12,290 clusters for each destination at
4,096 bytes, so allocated bytes equal file length. The A2 limits are at most
35% for current+history values and at most 50% for allocated database bytes;
both profiles clear them.

The combined bounded probe reuses one read snapshot and reads all 32 Work and
TaskContract records plus Work history twice. Debug measured 86,512,900 ns for
v1 and 26,016,600 ns for v2 (30.07%). Release measured 11,051,200 ns and
2,142,400 ns (19.39%). This is a warm OS-cache comparison in one process, not a
cold-disk claim. It demonstrates no 25% read regression on the selected fixture.

Debug rebuild took 63.3255857 seconds; release took 9.8352797 seconds. Profile
differences are reported directly and are not attributed to storage alone.
Both runs produced logical-row digest
`e32e14416c4dfb157be379567b11badc0d8891d17d57852f79d408a076a21c47`,
v1 physical digest
`5a04e0ade49bf7236563231055892d10ae07488da4e3f33b87ea81cf67a6e1fb`,
and v2 physical digest
`465fa491bef84d01c12e07b2ba9166f2178149f9509d7bb213a57b6611adb67b`.

The debug destination is
`C:/Users/olegc/.vibe/zap/r17-p0-a1-debug-20260913T2230Z/physical-v2-a2-debug-20260914T0051Z`;
the release destination is its sibling
`physical-v2-a2-release-20260914T0054Z`.

## Controlled storage microbenchmark

A separate synthetic store microbenchmark uses 32 record rows in four groups
with 32 KiB padding per row. It is not the accepted P0 import fixture. It
measured 2,107,007 / 12,636,736 record+history value bytes (16.67%), 16,781,312 /
35,663,872 database and allocated bytes (47.05%), and v2/v1 read ratios of
13.47% debug and 2.33% release. Its matching logical and physical digests are
recorded in `checkpoints/R17-A2.json`; it is supporting codec evidence only.

## Focused verification

- `cargo test -p zap-store --lib`: 10 passed, 3 ignored measurement phases.
- Focused A2 rebuild/catalog/corruption suite: 2 passed, 1 ignored measurement.
- Accepted P0 fixture A2 phase: debug 1/1 and release 1/1 passed.
- Controlled storage measurement: debug 1/1 and release 1/1 passed.
- `cargo test -p zap-app --test legacy_import`: 9/9 passed on default physical
  v2, covering receipt/staging recovery and import audit.
- `cargo test -p zap-legacy --test import_service`: 1/1 passed.
- `cargo clippy -p zap-store --all-targets --no-deps -- -D warnings` passed.
- `cargo clippy -p zap-app --lib --tests --no-deps -- -D warnings` passed.
- Exact-file Rust 2024 rustfmt and contamination scans passed.

The actual 425-node source/store/pointer was not read, replayed, audited or
modified. No model, local inference, transport, Git or publication operation
ran. The two previously denied build-directory cleanup debts remain under the
owner's no-retry instruction.
