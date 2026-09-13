# ADR R01-S1: transactional ZAP store

Status: accepted architecture for implementation. This ADR selects the zap/2
storage model; it does not claim the dependency or performance has been
implemented or measured.

Normative bases include `ZAP-DATA-AND-VIEWER` append/replay/snapshot clauses,
`ZAP-RUNTIME` recovery and trust clauses, the accepted V18/V22 vision, and the
new R02 anchors `RUST-STORAGE-ONE-TRANSACTION`,
`RUST-STORAGE-TRUSTED-LOCAL-BOUNDARY`, and
`RUST-STORAGE-TYPED-REDUCERS`, plus
`RUST-STORAGE-EXPLICIT-EPOCH` for zap/1 versus zap/2.

## Decision

zap/2 uses one embedded database as the canonical event store and current
indexed projection. One database write transaction commits all of these or
none of them:

1. one immutable logical event;
2. its exact idempotency result;
3. all changed current-state typed records;
4. every mandatory index row affected by those records;
5. the new revision/head/checkpoint metadata.

The ordered logical-event table is semantic authority. Current records and
indexes are rebuildable projections of the base plus those events. Their being
in the same transaction eliminates the journal-ahead/index-ahead recovery
boundary; it does not make an index authoritative or make an external effect
atomic with the database.

Portable JSONL/XML snapshots and tails are exports from a committed read
transaction. They are not a second live journal. The separately durable
segmented-file journal plus derived database remains a rejected-for-baseline
alternative below and must not be mixed into this model.

## Selected engine and toolchain compatibility

Pin `redb = "=4.2.0"` with its default `std` feature and no experimental API.
The package toolchain is Rust 1.93+/edition 2024. The local metadata probe on
2026-09-13 reported redb 4.2.0 `rust-version: 1.90`, so its declared MSRV is
compatible. This establishes version compatibility only.

The selection uses documented properties needed by the contract: one writer,
MVCC read transactions concurrent with the writer, typed B-tree tables, atomic
write transactions, checksummed crash recovery, and immediate durability.
References:

- <https://docs.rs/redb/4.2.0/redb/struct.Database.html>
- <https://docs.rs/redb/4.2.0/redb/struct.WriteTransaction.html>
- <https://docs.rs/redb/4.2.0/redb/enum.Durability.html>

Every production commit explicitly selects `Durability::Immediate`. It also
enables redb quick repair for the transaction. The documented quick-repair mode
stores allocator state and enables two-phase commit, trading slower commits for
bounded crash reopen. This is chosen because crash/compaction continuity is a
first-order requirement. R17 must measure commit cost and recovery cost; a
change to this durability policy requires a new ADR rather than an unnoticed
optimization.

redb is an implementation dependency, not a public wire format. ZAP owns the
table schema, record codecs, event codec and migrations. A future redb upgrade
must open a copied fixture, verify application schema/behavior, and use an
explicit ZAP storage migration if redb key/value encoding or behavior changes.
ZAP composite keys use its own length-prefixed byte encoding rather than redb's
tuple-value encoding.

## Store topology and trust boundary

One campaign store has this physical shape:

```text
<store>/
  zap.redb                 canonical events and indexed projection
  blobs/sha256/aa/<hex>    immutable content-addressed artifacts
  staging/                 incomplete unpublished artifact writes
  recovery/                immutable repair/import evidence
  exports/                 optional derived snapshots and event tails
```

Control credentials and provider credentials live outside this directory. The
store and blobs live outside every worker write subject. Supported mutations
go only through the trusted Rust service; workers receive proposal channels and
their assigned workspace, never raw database or blob-root write authority.

The supported integrity claim covers normal process crash, torn storage writes
handled by the selected engine, accidental corruption detected by checks/codecs,
untrusted command payloads, and workers without store write access. It does not
claim prevention or detection of an operating-system administrator or hostile
same-user process able to replace the database, binary and trusted configuration.
The event digest chain is diagnostic integrity, not an independent trust anchor.

## Store identity, genesis and logical events

`StoreIdentity` is exactly `{store_id, campaign_id, base_id, store_epoch,
codec_epoch, reducer_epoch}`. A trusted effect boundary supplies a new random
`StoreId`; the pure kernel never calls randomness. `BaseId` is the typed digest
of the canonical zap/2 base manifest. Copying a store preserves `StoreId` only
when it is an exact store copy; importing history creates a new store ID and an
explicit provenance link.

Genesis is the single exception to ordinary increment: it is event sequence
zero and projection revision zero with no prior event digest. Every later event
has `sequence == revision == previous_revision + 1`.

`LogicalEvent` has the closed fields:

```text
store_epoch, codec_epoch, reducer_epoch,
store_id, campaign_id, base_id,
sequence, revision, previous_revision,
event_id, command_id, transaction_id,
kind, causes, relevant_basis, authority_basis,
reason, payload, payload_digest, command_digest,
previous_event_digest, event_digest
```

`payload` is the strictly decoded cell payload re-encoded by the zap/2
canonical codec. `command_digest` binds the entire canonical command frame.
`event_digest` hashes a domain-separated canonical event body excluding only
itself. `previous_event_digest` binds the prior committed event, except at
genesis. `authority_basis` is a typed reference to no authority (data), a
trusted observation source, a service admission, or an Owner/coordinator
decision. It never embeds a credential.

The events table is append-only by module privacy and checked invariants:

- only `zap-store::commit` can open it for write;
- the next sequence key must be absent and exactly head + 1;
- an existing `EventId`, `CommandId` or sequence can only return an exact
  idempotent result;
- no production API updates or deletes event/idempotency rows;
- event-chain, header and revision invariants are checked before commit and on
  audit.

This is logical append-only behavior inside a mutable database file. It is not
a claim that an attacker with arbitrary file-writing tools cannot rewrite it.

## Transaction algorithm

`CommitService::execute` follows this exact order:

1. Strictly decode the command through the registered typed cell.
2. Use the service-owned `TransactionPermit` to begin one redb write
   transaction, obtain its opaque `TransactionBinding`, and read store
   identity/head. The permit grants entry only; it cannot write a row.
3. If `CommandId` or `EventId` exists, compare the stored command digest. Return
   its receipt on an exact match; otherwise refuse `IdempotencyConflict`.
4. Check store/base/campaign, current revision, relevant basis, controller
   fencing, route authority, sticky pauses, economics holds and the cell's
   typed preconditions against this transaction's pre-state.
5. Invoke the pure transition cell with `StateReader` and an empty `ChangeSet`.
6. Validate record versions, references, graph invariants, route/index coverage
   and every mutation in the completed set.
7. Derive a `ValidatedCommitIntent` bound to this transaction, containing the
   canonical `LogicalEvent`, `CommitReceipt`, record mutations and mandatory
   index mutations. `AtomicWrite::apply_commit` verifies the binding, inserts
   all of them and updates the head metadata. No other write method is exposed.
8. Commit with immediate durability. Return success only after commit returns
   success or later reconciliation proves this exact command committed.

A semantic command may change many projection rows while remaining one logical
event. An external effect is never invoked in step 5 or inside the write
transaction. Its lifecycle is a committed intent/claim, the out-of-transaction
effect, then a separate trusted observation event.

redb documents that a non-poison commit error may leave the transaction either
durably committed or not committed while preserving atomicity. On such an
error ZAP marks the local outcome unknown, stops further writes through that
database handle, closes/reopens it, and looks up the exact `CommandId` and
digest. It returns committed when the receipt exists, not-committed only when a
successful integrity/open check proves the ID absent, and otherwise
`UnknownEffect`. It never retries the logical command before reconciliation.
A poisoned transaction is treated as aborted only according to the engine's
documented error contract and still produces a structured diagnostic.

## Private table catalog

`zap-store` owns fixed redb table definitions. Feature crates see registered
record families, not table names.

| Table | Key | Value | Law |
| --- | --- | --- | --- |
| `meta` | closed metadata key | canonical typed metadata | exact store/head/epoch catalog |
| `events` | big-endian `Sequence` | canonical `LogicalEvent` | immutable, contiguous |
| `event_ids` | encoded `EventId` | `Sequence` | unique reverse lookup |
| `commands` | encoded `CommandId` | `IdempotencyRecord` | exact bytes/result or conflict |
| `records` | family length + family + encoded typed key | `RecordEnvelope` | current registered projection |
| `index_rows` | index family + encoded index key + subject/key suffix | `IndexEnvelope` | current mandatory indexes |
| `subject_history` | typed subject + sequence | event ID/digest | bounded history lookup |
| `artifact_records` | artifact digest | size/media/state/provenance | only durable published blobs |
| `legacy_objects` | import ID + legacy kind + ordinal | raw-byte identity/mapping | migration evidence, never active authority |

`RecordEnvelope` binds record family/codec epoch, key digest, record version,
value bytes and value digest. `IndexEnvelope` binds index family/query epoch,
source record family/key/version and a typed index value. The store validates
that every index mutation is declared by the registered record descriptor and
that no required old index row survives a replace/remove.

Mandatory index families at the complete MVP are:

- typed entity lookup and stable display identity;
- parent-to-child and child-to-parent;
- typed edge outgoing and incoming;
- direct prerequisite/dependent and local unresolved-blocker count;
- ordered current frontier/readiness blockers;
- obligation-to-owner and work-to-obligation;
- source-to-known-dependent and evidence applicability;
- work/subject reservation and resource occupancy;
- job/attempt by state and external handle;
- active pause, hold, wait and unresolved effect by affected subject;
- lowering/packet/dream lineage;
- entity-to-event history;
- normalized search term to typed entity.

The store does not eagerly persist every transitive closure. Impact and
invalidation traverse adjacency/reverse adjacency with explicit limits and may
persist a revision-bound scoped result. A future reachability structure needs
R17 evidence and a query-epoch change.

## Artifact publication

Large source, packet, stdout/stderr, verification and bundle bytes live in the
content-addressed blob area. A logical event may reference a blob only after
these steps:

1. create a new random staging file below `staging/` with exclusive creation;
2. outside any database transaction, stream bytes while computing SHA-256 and
   length, flush and fsync;
3. acquire the artifact publication lock, verify the expected digest/length,
   and publish to the exact digest path with no-overwrite semantics;
4. while retaining that lock and an open no-follow handle to the exact
   published file, create an immutable `PreparedArtifactWitness` binding store,
   path containment, file identity, digest and length;
5. open the short database transaction and validate only the witness binding
   and unchanged file identity/metadata; do not reread or rehash blob content;
6. commit its `ArtifactRecord` and referencing logical event together, then
   release the handle and artifact lock.

A failure before step 3 leaves removable staging residue. A failure after
publication but before database commit leaves an unreferenced immutable blob,
never a committed dangling reference. Recovery may adopt it only when an exact
pending intent expects that digest; otherwise garbage collection may remove it
after a recorded grace boundary. A committed artifact is never overwritten.
Deletion requires zero committed references, a recorded collection candidate,
and a second current transaction; foreign or mismatched bytes are quarantined,
not deleted.

The full content hash is therefore paid before the writer is opened. The
prepared witness is valid only while its ownership guard remains live and only
for the exact store/digest/length. It is neither serializable nor reusable by a
later command. This keeps large artifact I/O outside the single-writer critical
section without allowing a referencing event to race an ordinary supported
artifact replacement.

## Reads, cursors, indexes and caches

Each service query owns one redb read transaction and therefore one MVCC
snapshot. The returned page binds `StoreIdentity`, revision, query epoch,
normalized query digest and completeness. Continuation cursors also bind the
last ordered key. Since the service does not retain a database transaction
across requests, a continuation at a different current revision returns
`StaleCursor`; callers request a fresh snapshot or event tail.

Required indexes are part of the committed projection and are never called a
cache. Optional computation caches are separate namespaced record families
whose descriptor declares input digests, reducer/query version, committed
revision and replacement policy. A cache miss or stale cache changes latency,
not semantics. It may be dropped transactionally or rebuilt from canonical
events/current typed records.

Search normalization has its own epoch. Case folding, token boundaries and
ranking are deterministic algorithms selected by that epoch. A change rebuilds
the search index and changes query capabilities; it never changes entity text
or IDs.

Portable snapshot export contains store/base/revision, epochs, head event and
digest, projection digest, record/index family catalog, and an event-tail
cursor. Snapshot plus tail is complete only for the exact same store/base and a
contiguous event chain. An export file cannot be edited and reimported as an
authorized command.

## Controller fencing and concurrent work

redb serializes database writers, but that is not controller authority. The
trusted service stores a monotonically increasing `ControllerEpoch` and exact
controller lease/heartbeat record. Privileged admission binds the current
epoch through `PrincipalContext`; a recovered older controller cannot commit
after a newer epoch is installed. Expiration alone does not prove external
effects stopped. Controller takeover reconciles active/unknown effects first.

Database transactions remain short. They do not wait for a model, worker,
filesystem scan, build, test, network call, Owner response or backoff. The
runtime can execute many independent jobs while their claims/observations pass
through the single sequencer. A blocked database writer is dispatched to a
dedicated storage task so it does not hold the scheduler's collection loop.

## Normal open, distrustful audit and recovery

Normal open assumes the trusted-local-store boundary above. It asks redb to
open/recover, validates application/store/table/codec/reducer/query epochs,
reads the head and checkpoint in one snapshot, checks the head event and
idempotency/index catalogs, and resumes from that committed state. It does not
rehash or replay all historical events on every launch. The checkpoint is a
transactionally consistent acceleration boundary, not cryptographic proof
against file replacement.

Distrustful audit is explicit and O(total canonical history plus projection).
It reads genesis and every event, verifies the digest/revision chain, replays
typed reducers into a fresh sibling scratch database, and compares projection
and mandatory index digests at the same head. It never repairs the live file in
place. A successful audit can publish a new verified store copy through an
explicit switch receipt. An incomplete or corrupt audit preserves both source
bytes and diagnostics.

If redb reports corruption or cannot establish a commit outcome, the service
enters read/refusal recovery mode and performs no new mutation. It captures the
original database and relevant blob identities under `recovery/`, then attempts
read-only event extraction or restores from an exact revision-bound export.
Recovery always writes a fresh sibling store and switches only after full
replay/identity checks. It never silently drops a middle event or rewrites a
committed history row to obtain a green open.

This design protects recovery semantics, not storage hardware from losing the
only copy. Operators who require disaster recovery configure revision-bound
backups/exports and verify them; ZAP capabilities report whether that facility
is actually configured.

## zap/1 legacy epoch and zap/2 import

`zap-legacy` exposes a read-only `LegacyEpoch::Zap1` reader. It preserves exact
base bytes, journal bytes and line terminators, pending-tail bytes, snapshot
bytes, source-path spelling, IDs, unknown fields, refusal diagnostics and these
separate digest domains:

```text
BaseFileIncludingLf, PackedCommandWithoutLf,
CommittedJournalPrefixIncludingTerminators, PendingTailRaw,
PackedProjectionState, ReducerIdentity, SnapshotFileIncludingLf
```

Legacy digests never become zap/2 digests merely because both use SHA-256.
`PendingTail { byte_len, sha256 }` is explicitly uncommitted. A malformed
newline-terminated middle record refuses migration and remains preserved.

Migration creates a fresh sibling zap/2 store. It records an `ImportIdMap`
whose input is a tagged `LegacyId` and output is the corresponding typed zap/2
ID. A legacy ID satisfying the target type's grammar is preserved exactly.
Otherwise the new ID is deterministically `legacy:<kind>:<sha256>`, with the
full original identity retained in the map. Collisions refuse rather than gain
a suffix dependent on iteration order.

Legacy history lives in `legacy_objects` with per-record offset, length,
body/full-line digest, event ID and legacy revision. The zap/2 genesis event
binds the exact legacy base digest, committed-prefix digest, last committed
legacy event/revision, final legacy projection digest, ID-map digest, migration
report and any pending-tail evidence. The current zap/2 projection begins from
that fully validated imported state at revision zero. Historical views can
traverse the mapped legacy records; zap/2 reducers never silently reinterpret
them.

The Rust legacy reader must match the frozen compatibility corpus at every
legacy revision. A corrected zap/2 rule, including the shared completion/hold
predicate, is recorded as new-epoch behavior. It must not change a historical
legacy projection or be described as byte-compatible legacy replay.

Import never activates a charter, starts NEXT, submits a worker, resolves the
unaccepted economics candidate, or changes source MUP files. The configured
store pointer changes only after validation and an explicit import/switch
operation. The old store and recovery evidence remain available.

## Alternatives

**Segmented canonical files plus a derived database.** This preserves a
human-readable journal and isolates event recovery from database corruption,
but event fsync and projection commit form two durability boundaries. Recovery
and client visibility need a replay cursor. It remains a valid future ADR
alternative and is rejected for the baseline because the accepted preference
is one transaction for event plus mandatory indexes.

**Canonical database plus separately authoritative JSONL.** Rejected. No order
of two independent commits makes them one atomic authority.

**SQLite or an external database service.** Not selected. Both could satisfy
transactions, but the current requirement favors a package-owned pure-Rust,
offline embedded component. Reconsider only with measured redb failure or an
accepted deployment requirement.

**Trust every self-hashed snapshot.** Rejected. A same-store self-hash cannot
authenticate arbitrary replacement. Fast normal open rests on the declared
trusted local boundary and redb crash integrity; other deployments audit or
add an independent trust anchor.

## Required implementation evidence

R04 must demonstrate atomic event/record/index/idempotency visibility, exact
retry/conflict, stale revision/basis, commit-error reconciliation, reopen after
interrupted writes, quick-repair policy, typed record registration, index
replacement, cursor gaps, blob orphan/dangling-reference prevention, and full
rebuild equivalence. R14 adds the frozen zap/1 byte/digest/replay corpus and
nonactivating import. R17 measures warm/cold open, commit latency, recovery,
bounded lookup/search/subgraph/history and large invalidation separately. None
of these performance properties is accepted by this ADR alone.
