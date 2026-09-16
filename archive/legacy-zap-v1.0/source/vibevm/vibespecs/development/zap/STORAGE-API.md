# ZAP durable storage companion API

The committed `base.json` plus `events.jsonl` sequence remains canonical.
Snapshots, migration reports, quarantine copies, and completion receipts are
derived or auxiliary evidence. None activates a charter or executes commands.

## Atomic import and replay

`zaplib.storage.import_mup(plan_path, tasks_dir, out, *, fault=None)` preserves
the legacy base bytes/hash and import receipt. It writes both canonical files in
a fresh sibling staging directory, then publishes the complete directory with
one rename. Failures before publication leave `out` absent; failure after the
rename leaves a complete readable store. The `fault` callback is only an
injected test/effect boundary (`after_base`, `after_events`, `after_publish`).

`read_committed_journal(store) -> (newline_terminated_lines, pending_tail)`
returns only committed records and reports a final fragment by byte count/hash.
`replay_committed(state, records, handlers=..., *, seen=None) ->
(state, applied, seen)` is the reusable CAS/hash/sequence/idempotency tail
reducer. `load_store` uses the same invariants. `STORAGE_OPERATIONS` exposes
machine-readable descriptors for `import-mup` and `record`.

## Snapshots

`create_snapshot(store, snapshot_path, handlers=..., *, reducer_version=
"zap-reducer/1")` writes a new `zap-snapshot/1` derived file. It binds:

- immutable base SHA-256 and projection schema;
- explicit reducer version, sorted handler kinds, and reducer identity hash;
- projection revision and canonical state hash;
- committed journal prefix SHA-256, byte boundary, line boundary, and last
  event identity.

`load_snapshot_tail(...)` refuses foreign base, reducer/version/handler set,
projection hash, shortened or changed prefix, and invalid cursor identity. It
validates the stored global plan. On a cold read it rebuilds the snapshot state
from the validated base and exact prefix before using it, so a forged state plus
a recomputed self-hash confers no authority. An optional trusted in-process
verification cache is populated only after that derivation succeeds; its key
binds base, prefix, snapshot-state, and reducer identities. A warm read with the
same key can skip prefix reduction while still checking those identities. A
restart verifies cold again, and no serialized `verified` flag is read.

Every later committed event is replayed through `replay_committed`. A final
incomplete tail remains visible and unapplied. Full replay and snapshot-plus-tail
therefore yield identical canonical projections. Snapshot creation holds the
normal non-stealing writer lock across projection and prefix capture, preventing
a normal append from entering between them. `SNAPSHOT_OPERATIONS` describes the
two supported operations.

## Migration

`migrate_mup(plan_path, tasks_dir, out, *, report_path=None, fault=None)` reads
and hashes all sources, performs the atomic new-store import, verifies the
sources did not change, and writes a new `migration-report.json` inside the
store. It never edits the MUP plan or task files.

Verification compares the complete imported `*.json` source path-set, byte
sizes, hashes, and the captures embedded in `base.json`, catching additions,
removals, ordinary drift, and change-then-restore ABA during capture. If a crash
leaves an exact revision-zero store or an exact report, the same request resumes
without overwriting it. Any differing store/report refuses.

`build_migration_report(store)` requires a fresh revision-zero import. Its
`zap-migration-report/1` output maps every node, mandate, and task identity;
copies all task contracts and node constraints; includes exact raw captures;
and reports unknown top-level, node, task, and task-group fields with their
values. Imported owner/authority-like labels remain `unverified_data`.
`MIGRATION_OPERATIONS` explicitly reports no activation or execution.

## Pending-tail repair

`repair_pending_tail(store, *, repair_id, expected_tail_sha256, handlers=...,
fault=None)` is the only repair operation. It acquires the existing exclusive
writer lock and never steals one. Before switching the journal it validates the
entire committed prefix with the supplied handler registry, verifies the exact
pending-tail hash, and atomically publishes an immutable recovery directory:

- `original-events.jsonl` — every original byte;
- `pending-tail.bin` — the removed fragment;
- `committed-events.jsonl` — the intended active prefix;
- `repair-receipt.json` — old/new/tail hashes, sizes, base, revision, and the
  explicit non-authorizing repair mode.

Only after those files exist does repair atomically replace `events.jsonl` with
the captured committed prefix. It then writes `switch-completed.json` and
replays the result. A crash after quarantine leaves the original journal active;
a crash after switch leaves a complete journal plus the pre-switch receipt.
Repeating the same repair ID/hash resumes either boundary and writes the missing
completion receipt. Any other journal/quarantine content refuses.

A corrupt newline-terminated middle record fails normal replay before a repair
directory is prepared. It is never reclassified as a final fragment. The
`after_quarantine` and `after_switch` fault hooks exercise these crash states.
`RECOVERY_OPERATIONS` is the machine-readable operation descriptor.
