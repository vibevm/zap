# R17 P0 and A1 measured representation repair

Status: bounded P0/A1 candidate for coordinator judgment. A1 clears its
isolated canonical-encoder peak threshold and preserves exact wire/projection
bytes. The end-to-end fixture still has high redb, history and replay
amplification. No A2, B, actual-sized import, published R14 store mutation,
NEXT execution or full host panel was performed.

## Source and build identity

The accepted repair contract was
`REPAIR-R17-REPRESENTATION.md` SHA-256
`9c4d623d476c7d8fbb140039209869e573ce01f101152eecd27ae56b9b58c91d`.
The pre-A1 source identities were:

- `crates/zap-wire/src/canonical.rs`
  `aa14a828fa10731535a6bad5de7750ad70850e3ead4afb7e7d245ecaa202ffdb`;
- `crates/zap-store/src/history.rs`
  `7991b0744165fc2dafd8d7162edf3124e8588ce05d9c4379a83509590ea8272d`.

The measured candidate identities are:

- `crates/zap-wire/src/canonical.rs`
  `453300b7433f0da44c5469cfa7918fa61881de828290941733bfb86628ee2915`;
- `crates/zap-store/src/history.rs`
  `29b7930f3a04e28c943b0c0f415e2a3d99d88c996ced61e73ab286bb9a2a4f0b`;
- `crates/zap-app/src/legacy_import/service.rs`
  `d98c649c739a82af4a78dc6a990b7a83dc19081bdc9188140896cac7deb05002`;
- wire oracle/probe
  `37422bfc6f37f6dd809f74d7b25ce47328f3dfe96a37b00fc8c50c6424be9f04`;
- projection oracle/probe
  `0393ee6d92ecc5ac2e6b51c8efdd4647ac469c1e6b5d22fbd09ef2246519add0`;
- deterministic app fixture
  `baa5e238943926833c78b86e35d5e65cfa90056f7f0e59e6f02637b85e5044d8`;
- private app phase probe
  `c616298b634d30ee6a7eff46ffd31f2b4cbcec5fa7fb16f0f90343c85e0e7b8c`.

All Cargo work used the required shared target
`C:/Users/olegc/.vibe/zap/build/next-rust`. Debug means Cargo's unoptimized
test profile with debuginfo; release means Cargo's optimized release test
profile. Every timing below is a fresh test process. CPU is Windows combined
user+kernel `TotalProcessorTime`; peak is OS `PeakWorkingSet64`. No allocator
counter was available, so process peaks are not reported as allocator-only
attribution. Fresh process does not imply cold disk cache; no cold-disk claim
is made.

## Implemented A1 boundary

`zap-wire` now converts the already inspected `serde_value::Value` tree
directly into sorted `StrictValue`, serializes it once, and uses a private
constructor for those produced bytes. Public `from_canonical_json` retains its
strict parse, duplicate-member and exact-canonical checks. The old
serialize/parse/canonicalize implementation remains compiled only in tests as
the differential and performance reference.

The direct conversion preserves every `serde_value` variant used by the old
path: all integer widths supported by that serializer, byte vectors as integer
arrays, option/newtype/unit behavior, numeric/string/character map keys,
post-stringification collision refusal and sorted string-key order. Finite
f32/f64 values take a bounded per-number compatibility conversion because the
enabled serde_json arbitrary-precision feature gives the existing codec a
non-obvious float representation. An 8,192-step deterministic bit corpus for
each float width plus explicit finite boundaries agrees with the old encoder.
Nested non-finite values and keys, invalid sequence keys, duplicate canonical
keys and serialization errors return the same typed error.

`zap-store::history::projection_digest` now updates SHA-256 directly in the
unchanged table order with the unchanged u64 big-endian key length, key, u64
value length, value framing. The accumulator implementation remains test-only
as the byte-equivalence and performance reference.

One prerequisite composition defect was corrected with root authorization:
the inactive legacy importer now builds the real projected-import cell and
route subset while retaining the full registered records, queries and
providers. Before correction, both the P0 fixture and the existing tiny import
failed immediately with the same store `Conflict` because the importer passed
the full campaign cell/route set through newly mandatory provider checks. The
first failed P0 processes were PID 142912 and PID 94432. After the narrow
initializer correction, the existing tiny import test passes. This is a
correctness/composition repair and is not counted as an A1 performance gain.
Other R14 receipt/staging/recovery work remains outside this report.

## Deterministic fixture golden

The Rust fixture has four task groups, eight tasks per group, a 32 KiB padding
field in each group, 32 matching nodes and contracts, two mandates, one
acceptance-derived obligation per task, final LF bytes and a genesis-only
journal. Its checked summary is:

| Field | Exact value |
| --- | ---: |
| base bytes | 216,906 |
| base SHA-256 | `008ce4a4fb36ae796f8f331c4da3e2722cafb3e0fc1e4fb362af240680807c90` |
| journal bytes | 184 |
| journal SHA-256 | `210045afadc0a7d979c3fce1fbce7f41f788873a04ba8e1b540f8034d1ceffc8` |
| plan-source bytes | 33 |
| plan-source SHA-256 | `72adb1cd7903368d7588b0c7bfd73c1ac1ba28a999177e4597fc6a0a48d5478f` |
| nodes / contracts / mandates / obligations | 32 / 32 / 2 / 34 |
| unique task-group capture bytes | 145,664 |
| current per-task cloned source bytes | 1,165,312 |
| canonical import payload bytes | 4,410,069 |
| canonical import payload digest | `26a974ebbf4f9ded47ae7c13388ebb2027c5d23a5be4e624cb3629be3e37064b` |

The payload digest above is for the exact common source-path spelling used by
the paired old/direct runs. A first release trial used another absolute root;
it correctly produced a different digest because preserved source spelling is
logical data. That trial was excluded. For the accepted debug/release store
comparison, the debug result was moved intact to the recoverable sibling
`published-import-debug`, and release was run against the same source and
destination spelling. Both profiles then produced projection digest
`5a04e0ade49bf7236563231055892d10ae07488da4e3f33b87ea81cf67a6e1fb`
and byte-identical table summaries.

## Canonical encoder old versus A1

Each cell is the median of three fresh processes. Old/reference performs the
prior typed serialization, whole-value JSON canonicalization and opaque-wrapper
validation. Direct uses A1. Payload construction occurs before the internally
timed encode region; process peak remains inclusive of fixture preparation.

| Fixture/profile | Old peak | A1 peak | Peak reduction | Old encode | A1 encode | Encode speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| structural 3,546,275-byte fixture, debug | 109,899,776 | 67,485,696 | 38.59% | 1.684551s | 0.362126s | 4.65x |
| structural fixture, release | 109,740,032 | 61,669,376 | 43.80% | 0.371034s | 0.081354s | 4.56x |
| actual 4,410,069-byte import payload, debug | 142,786,560 | 85,970,944 | 39.79% | 1.952053s | 0.451153s | 4.33x |
| actual import payload, release | 131,133,440 | 75,767,808 | 42.22% | 0.355433s | 0.103142s | 3.45x |

All six structural outputs in each profile were exactly 3,546,275 bytes with
SHA-256
`c5532e0272294efd8e3f288e8aa6f0f313c374ca357aa13d715ba21b8edf26e0`.
All actual-payload old/direct outputs had the exact payload bytes and digest in
the fixture golden. The old/direct process PID sets were:

- structural debug reference `24868,149480,113136`, direct
  `129148,47868,129240`; release reference `121132,140572,124148`, direct
  `120532,73176,112072`;
- actual payload debug reference `133328,138856,139584`, direct
  `92108,132288,125400`; release reference `83652,140008,128524`, direct
  `28216,114128,78112`.

The actual-payload receipts are under
`C:/Users/olegc/.vibe/zap/r17-p0-a1-payload-measurements-20260913T2236Z`;
the structural receipts are under
`C:/Users/olegc/.vibe/zap/r17-p0-a1-encoder-measurements-20260913T2222Z`.
A1 therefore exceeds the contract's required 25% encode-probe peak reduction
in both profiles and should be retained.

## Projection digest old versus A1

The same committed fixture has 40,024,419 bytes after applying the frozen
length framing across records, indexes and the two history orders. Every old
and streaming run produced the exact projection digest above.

| Profile | Old peak | Stream peak | Peak reduction | Old digest time | Stream digest time |
| --- | ---: | ---: | ---: | ---: | ---: |
| debug | 122,843,136 | 82,771,968 | 32.62% | 0.660829s | 0.602082s |
| release | 103,952,384 | 62,808,064 | 39.58% | 0.076986s | 0.039889s |

Debug reference PIDs were `80988,121504,11632` and stream PIDs were
`123872,83188,118968`. Release reference PIDs were
`131212,55308,121632` and stream PIDs were `128068,53120,66056`.
Receipts are under
`C:/Users/olegc/.vibe/zap/r17-p0-a1-projection-measurements-debug-20260913T2237Z`
and
`C:/Users/olegc/.vibe/zap/r17-p0-a1-projection-measurements-release-20260913T2240Z`.

## Fresh-process phase measurements

`commit_and_verify` is the current importer operation, not an isolated commit
measurement. It includes translation, payload/event encoding, immediate commit,
exact retry, snapshot construction and the importer's verification/replay
audit. The code has no narrower public timing seam, and this report does not
subtract phases or fabricate an isolated commit time.

| Phase/profile | PID | Timed operation | Process wall | CPU | Peak working set |
| --- | ---: | ---: | ---: | ---: | ---: |
| legacy translation, debug | 54260 | 0.026990s | 0.862143s | 0.031250s | 10,194,944 |
| legacy translation, release | 145096 | 0.004814s | 0.407390s | unavailable at Windows timer resolution | 4,661,248 |
| commit_and_verify, debug | 112080 | 55.947449s | 57.116825s | 52.281250s | 1,499,197,440 |
| commit_and_verify, release | 36596 | 11.339079s | 11.571846s | 10.609375s | 1,490,251,776 |
| hash audit, debug | 96604 | 4.170653s | 5.929448s | 4.515625s | 314,130,432 |
| hash audit, release | 51632 | 0.725653s | 1.007678s | 0.687500s | 348,524,544 |
| replay audit, debug | 104020 | 27.543644s | 28.236218s | 26.312500s | 1,376,317,440 |
| replay audit, release | 137252 | 4.599729s | 5.350633s | 4.328125s | 1,370,984,448 |

The fixture/process stdout and stderr are under
`C:/Users/olegc/.vibe/zap/r17-p0-a1-debug-20260913T2230Z`. All four successful
store/audit phases report revision 1, two checked events, one checked command
where applicable and the same projection digest. The large gap between debug
and release CPU is profile evidence, not an A1 before/after claim. The
approximately 1.49 GB commit-and-verify and 1.37 GB replay peaks remain high in
both profiles. No full-operation old/reference baseline was measured, so these
numbers do not establish A1's end-to-end percentage improvement.

## Table and filesystem attribution

The debug/release stores have the same exact table accounting:

| Table | Rows | Key bytes | Value bytes |
| --- | ---: | ---: | ---: |
| `meta` | 4 | 50 | 245 |
| `events` | 2 | 16 | 13,280,214 |
| `commands` | 1 | 17 | 931 |
| `records` | 166 | 6,406 | 13,283,683 |
| `index_rows` | 0 | 0 | 0 |
| `record_history_v1` | 166 | 8,398 | 13,354,783 |
| `revision_history_v1` | 166 | 8,398 | 13,354,783 |
| total | 505 | 23,285 | 53,274,639 |

The redb file length is 136,318,976 bytes. `fsutil file queryextents` reported
33,281 allocated 4,096-byte clusters and `fsutil sparse queryflag` reported
the file is not sparse, so allocated filesystem bytes equal file length:
136,318,976. Raw table keys plus values are 53,297,924 bytes; redb file/page
overhead is therefore 83,021,052 bytes and the file is 2.558x the measured raw
table content. This separates filesystem allocation from logical table bytes.

Current record plus both complete history value copies are 39,993,249 bytes,
75.07% of all table value bytes. This exceeds A2's one-third table-value
trigger. The two history orders alone duplicate 13,354,783 value bytes. A2 is
not authorized by this packet; this is evidence for the coordinator's later
selection, not an implementation start.

The four raw task-group captures occupy 145,664 unique bytes and 1,165,312
bytes under the current per-task clones. That raw cloned total is only 2.187%
of the measured canonical event plus record/history value-byte denominator
(53,273,463 bytes), below B's raw 20% trigger. This comparison does not claim
that raw clone bytes equal their encoded contribution: the current nested byte
arrays expand inside payload, event, record and history envelopes, and this P0
probe does not isolate that encoded attributable share. B remains unselected
and still lacks the contract-required exact artifact-to-normalized-projection
validation/replay proof; preparer labels would not replace independent source
rederivation.

## Focused verification

- `cargo test -p zap-wire`: 7 unit and 8 integration tests passed; 2 ignored
  measurement phases. The differential suite includes the 8,192-step finite
  bit corpus per float width, explicit boundaries, nested bytes, unordered and
  mixed-key maps, canonical-key collision, invalid keys, Unicode, ingress
  duplicate/canonical refusal and nested non-finite refusal.
- `cargo test -p zap-store --lib`: 6 tests passed; 2 ignored measurement
  phases. Existing atomic commit/retry/reopen, owner cell, admission, schema
  refusal, history seek and artifact tests remain green.
- `cargo test -p zap-app --test legacy_import
  inactive_projection_import_publishes_recovers_and_reopens -- --exact`: 1
  test passed after the narrow initializer repair.
- Twelve structural encoder, twelve actual-payload encoder, six projection
  digest and the named translation/commit/hash/replay measurement processes all
  exited zero except the two preserved pre-fix composition diagnostics and one
  deliberately excluded full-cell replay diagnostic.

The exact durable checkpoint is `checkpoints/R17-P0-A1.json`; its valid backup
is `checkpoints/R17-P0-A1.prev.json`. No process remains live and no external
effect is unresolved. The coordinator can retain A1, then decide whether the
75.07% physical row trigger justifies separately authorizing A2. The available
P0 evidence does not authorize A2 or B and does not replace the later R17
query/graph performance work.
