# R17 bounded storage representation repair

Status: architecture contract for coordinator review. This repair is limited to
the demonstrated representation bottleneck. It does not change ZAP authority,
durability, artifact publication guarantees, existing logical event identity,
or domain semantics.

## Evidence and non-negotiable boundary

The actual inactive R14 import converted a 3,277,342-byte base into 2,632
current records plus two events. The resulting redb file was 671,092,736
logical and allocated bytes; the approximately 17-minute operation consumed
about 749 CPU seconds and reached a 13,915,303,936-byte peak working set. The
old executable performed two full audits. These observations demonstrate an
amplification defect, but do not apportion it among encoding, history indexes,
redb allocation, audit, or import materialization.

One contributor has been measured independently: 90 task-group captures total
1,057,941 raw bytes once per group, while copying each group's complete raw
source into all 212 task constraints retains 2,671,173 bytes, a 2.5249x raw
duplication ratio. The complete history entry is also currently a value in two
indexes, and JSON represents nested byte vectors as integer arrays. Those two
items remain source observations until the small fixture measurements below.

The following identities are frozen:

- every original zap/1 byte string, line terminator, offset, length and
  domain-separated digest;
- every committed zap/2 canonical command, payload, output and logical-event
  byte string and digest, including schema-1 and schema-2 events;
- strict duplicate-member, unknown-field, non-finite-number and canonical JSON
  validation;
- event order, replay result, v1 typed record value, record version, current
  index meaning, idempotency receipt, and the existing v1 projection-digest
  algorithm and result;
- immutable artifact no-clobber publication, witness lifetime and recovery.

No repair may reinterpret bytes already hashed or signed. Logical codec epochs
remain about canonical wire meaning. Internal physical representations receive
their own explicit versions and migration path.

## Bounded repair boundary

Only P0 and byte-identical A1 are mandatory. Physical A2 and logical import B
are separate conditional authorizations after the coordinator accepts the
preceding measurements. P0 identifies whether canonical materialization or
table representation is the remaining dominant defect. B, if selected,
removes imported source duplication through the existing immutable artifact
protocol and explicitly versions the changed payload and record shapes. The
sequence stops when the measured gain no longer justifies the next atom.

## Probe P0 — required before selecting Atom A order

Add one deterministic Rust-only fixture with four canonical task-group sources,
eight tasks per group, a 32 KiB padding field per group, 32 matching nodes and
contracts, two mandates, one acceptance-derived obligation per task, final LF
bytes and a genesis-only journal. Its checked summary golden fixes source,
payload, event and projection hashes plus counts.

Run legacy translation, v1 payload canonical encoding, immediate commit, hash
audit and replay audit as separate fresh processes. For each, record wall and
user+kernel CPU, peak working set, exact input/unique-source/payload/event bytes,
per-table key/value bytes, database length and allocated filesystem bytes. Run
the small probe in debug and release; logical bytes/digests must match. P0 does
not authorize either expensive follow-on atom. After A1, select A2 only when
current-record plus history values still account for at least one third of
stored table bytes, or their isolated commit/audit peak is at least one third
of the total. After the selected internal repair, select B only when cloned
task-group bytes are at least 20% of canonical event plus record/history value
bytes, or the encode probe attributes at least 20% of peak to those clones.
Otherwise stop and report the measured dominant remainder. No table repair is
credited against the 14 GB peak until this split exists.

## Atom A1 — byte-identical allocation repair

In `zap-wire/src/canonical.rs`, retain the current typed encoder as a test-only
reference. Convert its already finite-checked `serde_value::Value` directly to
sorted `StrictValue`, serialize once, and use a crate-private constructor valid
only for those produced bytes. Public `from_canonical_json` remains unchanged.
Reference and new bytes, digests and errors must match for the canonical corpus,
nested byte vectors, all integer widths, finite float boundaries, unordered
maps, nested non-finite values and invalid map keys. Keep this optimization only
if the encode probe peak improves by at least 25%.

Independently, `zap-store/src/history.rs::projection_digest` streams SHA-256 in
the existing table order and exact big-endian length framing instead of building
one projection-sized `Vec`. The v1 digest golden must remain exact.

Rerun P0 after A1. If end-to-end peak and CPU are each at most half of their
same-profile baselines and physical record/history rows are below the A2
trigger, stop this repair after A1. That stop does not claim storage or read
scalability; the later R17 packet still measures those properties.

## Atom A2 — compact physical schema 2

Persist private `physical_schema=2` for new stores. Absence maps to frozen v1
only when the current `record_history_schema=1` marker and complete v1 catalog
exist; mixed/unknown catalogs refuse. Store identity and all logical epochs,
events, commands, typed record values and ordinary index meanings stay exact.
`RedbStore::physical_schema()` exposes the checked value. New snapshot and
rebuild receipts bind both physical schema and projection-digest algorithm;
their serialized shape is explicitly snapshot/receipt schema 2, while the
current shape retains a frozen schema-1 decoder.

V2 adds only `records_v2`, `history_by_revision_v2`,
`history_by_record_v2` and `history_event_meta_v2`; the existing `index_rows`
table is reused unchanged. A record value is binary:

```text
"ZRV2" | value_codec:u32be | version_codec:u32be |
version_len:u32be | value_len:u64be |
sha256(storage_key):32 | sha256(value):32 | version | canonical value
```

Lengths are checked before allocation, trailing bytes refuse, and descriptor
epochs, hashes, decoded key and version are rechecked. `HistoryBodyV2` is
exactly `"ZHV2"`, mutation tag `0/1/2`, a four-bit presence mask, two zero
reserved bytes, `before_version:u32be`, `before_value:u64be`,
`after_version:u32be`, `after_value:u64be`, then the present byte strings in
that order; the mask distinguishes absent from an empty value. History stores
that body once under revision+record key and its 32-byte SHA-256 under the
record-order secondary key. `HistoryEventMetaV2 {event_id, command_id, reason,
event_digest}` is exact canonical JSON stored once per revision. Both query
orders reconstruct the unchanged public history entry;
missing primary/meta, orphan secondary, bad length/hash and duplicates are
corruption. All rows still commit with event/receipt/head under the existing
immediate-durability transaction. `index_rows` continues to store the exact
canonical index value; it gains no envelope.

V1 stays readable, writable through its frozen codec, auditable and exportable.
New creation defaults to v2. Upgrade is only
`rebuild_physical_v2(source, absent_destination)`: replay exact events into a
fresh sibling, fsync a source/head/schema-bound receipt and publish the sibling
directory by no-clobber rename. Source and configured pointer remain untouched.
Event/command bytes, typed record bytes/versions, index rows and reconstructed
history entries must agree. V1/v2 physical projection digests deliberately
differ and are both recorded; a versioned streaming logical-row digest must
agree as rebuild evidence. That digest starts with
`zap/logical-projection/1\0`, then streams class-tagged, u64-length-prefixed
rows in canonical key order: current family/key/version/value, ordinary index
key/value, and each reconstructed public history entry. It excludes the
duplicate secondary key and physical event-meta row. This digest is audit
evidence, never an admission or authority input.

Accept A2 only if the small fixture's record-plus-history value bytes fall to
at most 35% of v1 and total allocated database bytes fall to at most 50%, with
exact logical equality and no more than 25% regression in bounded current
record/history reads. Failure rejects A2 and returns the measurements; it does
not authorize compression or another storage design.

## Atom B — explicitly new artifact-backed import format

This is a separate logical change. Freeze the present serde payload/cell/record
types for event `legacy.projected-import-recorded`. Add event
`legacy.projected-import-v2-recorded` with `payload_schema=2` and new families
`zap.legacy.import_manifest.v2`, `zap.legacy.object.v2` and
`zap.domain.legacy-task-constraint.v2`. It does not claim old typed record bytes
or projection digests remain equal. The existing global logical-event schema 2
continues to carry both event kinds; no global event-envelope revision is
needed. The import receipt gains an explicit schema-2 shape and retains an
exact schema-1 reader for already published receipts.

V2 stores base and journal as whole immutable artifacts and each distinct task
group once. Manifest/object records carry artifact digest, byte length, legacy
digest domain/value and exact journal ranges. A task constraint carries source
path, task-group artifact/digest/length and its own small `contract_raw`; it has
no complete group `source_raw`. Translation holds each group once as
`Arc<[u8]>` plus task-to-group references. Legacy and artifact digest wrappers
remain distinct even when their SHA-256 bytes agree.

Extend `ArtifactStore` only with byte/reader preparation and
`open_verified(digest,length)`, retaining staging, hash, fsync, hard-link
no-clobber and live witness behavior. `PayloadArtifacts<V2>` supplies the exact
sorted digest set before the DB writer opens. The pure cell validates reference
closure, range bounds, digest domains/maps/counts and never reads a file. The
trusted preparation validates actual range bytes and digests before authorizing
the frame; replay consumes the stored v2 payload. Keep query
`zap.domain.legacy-projection` frozen for v1 and
add `zap.domain.legacy-projection-v2`, returning locators rather than hydrating
unbounded raw bytes. Resolved base, journal, tail and task-source bytes must
match the originals exactly.

Accept B only if it removes every per-task complete-group copy and reduces the
fixture's combined canonical event plus record/history value bytes by at least
20% relative to the accepted internal format. If the pre-B trigger or this gain
fails, retain v1 import and record the artifact-backed proposal as unselected.

## Ownership, proof and cost

Order is P0, A1, coordinator gain review, conditional A2, another gain review,
then conditional B. No conditional atom starts from this document alone. P0
owns a new `zap-app/tests/representation_fixture.rs` and cfg-test counters. A1 is
foundation/store. A2 owns only `zap-store/src/{schema.rs,engine/**,history.rs,
physical/{v1,v2}.rs}`. B splits migration-owned `zap-legacy`/`zap-app` modules
from new semantic-owned `zap-domain/src/legacy_projection/*_v2.rs`; the
integrator alone changes lib.rs, registries and composition. B waits for the
already-active import receipt/recovery edits and does not touch R07/R08 files,
authority, completion or global semantic epochs.

Focused proof covers old/new canonical equality; v1 exact reopen/digest/audit;
v2 insert/replace/remove, both history orders/cursors/replay/reopen; cross-schema
event/receipt/record/index/history equality; unknown/mixed/truncated/corrupt
schema and rebuild crash boundaries; frozen v1 event replay; v2 artifact
no-clobber, missing/mismatch/symlink refusal; one blob per unique task group;
and v1/v2 equality for counts plus unchanged Work/Contract/Obligation/Mandate/
Node values. Manifest/task-source values are intentionally versioned and differ.

After focused tests, perform exactly one release actual-sized import into a new
sibling and one cold replay audit; never touch the existing R14 output. Measure
phase wall/CPU/peak, cold and warm open, logical and allocated DB/artifact bytes,
and per-table bytes. State OS-cache method; a new process alone is not a cold
disk claim. Compare to 671,092,736 allocated bytes, 13,915,303,936 peak bytes,
~749 CPU seconds and ~17 minutes/two audits, while labeling the old build
profile unknown. Less than 2x improvement in total allocated bytes or peak means
the repair is insufficient and the measured remainder goes to later R17 rather
than widening this task.

The engineering estimates are human implementation/review effort, not an
automatic commitment or worker-clock promise: P0/A1 is 0.5 day (medium
uncertainty); A2 is 1.5–2 days (high uncertainty around dual-schema recovery
and redb allocation); B is 1–1.5 days (high uncertainty around cross-crate
versioned queries); integration plus one actual comparison is 0.5 day (medium
uncertainty). The 3.5–4.5 developer-day total applies only if both conditional
atoms are selected. Expected xhigh coding-worker wall time before review is
roughly 1–3 hours for P0/A1, 4–8 hours for A2, and 3–6 hours for B; build queue,
review and evidence time are additional and can invalidate those estimates.

Small debug/release probes should stay under five minutes total. Reserve at
most 30 minutes and one fresh destination for the one final actual import plus
audit, performed only at the coordinator-selected stopping boundary; failure
returns evidence and does not automatically authorize another atom or another
actual run. Write a durable phase receipt if the bound is exceeded. P0 must
resolve canonical peak share, v1 table-byte split and redb page overhead; this
contract assigns none of them in advance.
