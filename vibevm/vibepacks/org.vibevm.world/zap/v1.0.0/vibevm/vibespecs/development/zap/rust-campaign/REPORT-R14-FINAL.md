# R14 full legacy migration candidate report

Status: reviewable candidate, not full R14 acceptance.

## Verified implementation boundary

The Rust legacy reader preserves zap/1 base, journal, line terminators, pending
tail, IDs, path spelling and domain-separated hashes. Arbitrary-size integer
tokens now retain exact integer spelling instead of crossing through `f64`.
The supported legacy event subset replays deterministically, and snapshot
validation compares offered state with cold replay and the fixed reducer
identity. Unsupported event semantics refuse explicitly.

The official archive import validates base, map, body, line, prefix, pending
tail, inventory and event lineage from the retained bytes. The normalized app
import re-derives its submitted projection from those validated bytes. It
materializes legacy nodes as `Planned` `WorkRecord` rows, task contracts as
inactive records that require later admitted adaptation, mandate and obligation
rows, plus exact node/task metadata and captured raw sources. It invents no
lowering origin and cannot make imported work executable. Nonzero legacy
histories refuse normalized import in both preparation and the official cell
guard until all replayed semantic families have a current draft projection.

The public `zap.domain.legacy-projection` query reads counts and exact work,
contract, obligation, mandate, node-metadata and task-constraint records.
Count verification paginates and imposes no 4096-record campaign ceiling.

## Actual inactive import

The named read-only input was:

- `C:/Users/olegc/.vibe/zap/migrations/next-runtime-4a051fa7b3b0/base.json`
- `C:/Users/olegc/.vibe/zap/migrations/next-runtime-4a051fa7b3b0/events.jsonl`

The preflight and post-import snapshots agreed on these identities:

- base SHA-256: `c081856d386c6f229927584ae6c77c056c32f58049d7e777c823894f56604e5e`
- journal SHA-256: `b8aa78d4988f2d4e200a62a19c2bce92f53d4fcc4d2613a89bd18d9b4aa8511f`
- decoded plan-source SHA-256: `d7e8ce7f43e69f69c286fdf8d7e0c0be9ecf98f93bf606dbcfbeb8d0c8c96f45`
- source revision: 0, one committed genesis event, no pending tail

The fresh destination and its staging name were both verified absent before
the successful run. The compiled Rust CLI then published:

`C:/Users/olegc/.vibe/zap/migrations/next-rust-20260913T1959462179878Z`

Its machine-local configuration is
`C:/Users/olegc/.vibe/zap/migrations/r14-import-config-20260913T1959462179878Z.json`.
The immutable receipt is the destination's `import-receipt.json`.

The terminal receipt records store
`next-rust-store-20260913t1959462179878z`, revision 1 and projection digest
`a5ac0159a58d1c6ac71578077fca55dddb177e059855c50e8bb3c6e3298127f8`.
It reports exactly 425 nodes, 212 contracts, 64 mandates and 1,292 obligations.
`authority_activated`, `commands_executed` and `pointer_switched` are all false.

Bounded compiled-binary reads reopened the published store and confirmed:

- snapshot revision 1, two events, 2,632 records and zero optional index rows;
- the only non-genesis event is `legacy.projected-import-recorded` under
  `ServiceInternal` operation `legacy-projection-import`;
- `NEXT-EXECUTION-AUTHORITY` is `Planned`, retains parent
  `NEXT-RESOURCES`, and maps signed legacy order -1 to current order 0 while
  retaining the original signed value in metadata;
- `M-01-A.1` is an inactive version-1 contract with nine obligation links;
- `NEXT-COMMISSION` retains disposition `owned` and 71 work links;
- its task constraint retains source digest
  `813e062db64b010df48abc9b7aa14a3995f3906578509f2ada5fd9db86f968af`
  and `adaptation_required=true`.

No NEXT task, transport, model, local inference, charter activation or store
pointer switch occurred.

## Verification evidence

- The legacy corpus passed 7/7, including exact codec bytes, boundary-crossing
  integers, journal/tail behavior, mapping, cold replay and snapshot matching.
- The explicit actual-source test passed 1/1 against the named source and exact
  425/212/64/1292 denominator.
- Exact legacy source files passed rustfmt; `zap-legacy` clippy passed with
  warnings denied. One known shared-core schema-1 compatibility field warning
  remained outside the R14 crate.
- The earlier complete normalized-import fixture passed before the final
  recovery review. It demonstrated inactive projection, exact retry, reopen,
  audit, retained unknown fields, completed-staging recovery and source-drift
  refusal. A later deliberately filtered command selected zero tests and is not
  counted as evidence.
- The latest `zap-legacy` aggregate attempt reached its import-service audit but
  failed while schema-2 replay integration was changing concurrently. The R07
  owner subsequently reported that the audit replay and app event-summary call
  sites were repaired. This report does not claim a rerun after that repair.

## Measured performance boundary

The source base is 3,277,342 bytes. The published `zap.redb` has a logical size
of 671,092,736 bytes and occupies 671,092,736 bytes on NTFS: 163,841 allocated
clusters at 4,096 bytes. The receipt is 1,087 bytes. The observed import ran
from 23:08:15 +03 through the post-23:25:11 file boundary, approximately 17
minutes. Root observation recorded about 749 CPU seconds and a peak working set
of 13,915,303,936 bytes.

The executed binary performed two full replay audits because it predated the
source change that removes the duplicate post-publication audit. No third audit
was run. Repeated task-group `source_raw`, duplicate history-index bodies and
repeated canonical value materialization are concrete amplification candidates,
not yet a proven causal diagnosis. R17 must repair and measure this before full
product acceptance.

## Remaining work before full R14 acceptance

The current source still writes `import-receipt.json` directly with
`create_new`. A crash during that write can leave a partial receipt which staged
recovery mistakes for a completed receipt. The next bounded repair must prepare
and validate a sibling receipt, then publish it atomically without overwriting an
existing receipt, or preserve and explicitly recover the partial bytes.

Staged recovery must also verify that the staging path is a plain directory,
reject symlink/reparse points, and refuse unrelated contents before committing
or renaming it. It may resume only the exact expected empty/precommit store,
committed no-receipt store, or completed receipt for the requested source and
command.

After those source fixes land against a coherent R07/R08 tree, meaningful tests
must rerun the official nonzero-history cell refusal, precommit staging,
postcommit/pre-receipt recovery, completed-receipt recovery, unrelated-content
and symlink refusal, exact retry, reopen and audit. The legacy import-service
audit and app composition tests must also rerun after the schema-2 integration
repair. These pending tests and the performance repair prevent a full R14 claim.

The accidental disposable build directory `C:/Users/olegc/.vibe Subs zap`
remains untouched. Its constrained cleanup was rejected by automatic approval
review with reason `blocked by policy`; no other tool, language or agent may
retry that deletion.
