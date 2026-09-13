# Performance observations requiring R17 evaluation

These are observations from in-flight candidate work, not completed benchmarks
or claims about logical payload size.

R14 actual NEXT import, observed 2026-09-13 around 20:14 UTC:

- Input base.json: 3,277,342 bytes; events.jsonl: 203 bytes.
- Denominator: 425 nodes, 212 contracts, 64 mandates, 1292 obligations.
- Worker checkpoint reported PID 8696, started 20:08:15 UTC, CPU time 317.2s,
  staging redb file length 1,077,956,608 bytes, operation still committing or
  replay-auditing. File length is not measured allocated storage or logical
  record payload. No complete timing/receipt existed at this observation.
- Configuration:
  C:/Users/olegc/.vibe/zap/migrations/r14-import-config-20260913T1959462179878Z.json.
- Preserve the in-flight operation and reconcile its outcome before retry.
  Measure materialization, commit, audit and publication separately. Inspect
  raw-byte representation, repeated source copies, derived history and redb
  allocation when diagnosing amplification. Do not guess which is responsible.
- The current candidate service audits before writing its receipt and calls
  published verification with another audit. Avoid repeated full work on the
  same already-verified immutable boundary where a sound receipt can be reused;
  retain meaningful cold-open verification.

R17 should compare final actual measurements with this early observation and
repair demonstrated bottlenecks. A successful migration alone is not evidence
that the large-graph performance requirement has been met.

Root source observations, not a measured attribution of the elapsed time:

- `zap-app/src/legacy_import/translate.rs` retains a complete task-group
  `source_raw` in each task constraint; multiple tasks can therefore retain
  identical source bytes. The legacy manifest also retains the complete base.
- `zap-store/src/engine/record_history.rs` encodes the complete history entry
  into both the by-record and by-revision indexes, including before/after
  values. Current records have a separately encoded envelope with byte vectors.
- `zap-wire/src/canonical.rs::encode_json` materializes serde_value, emits JSON,
  and parses/emits canonical JSON; the opaque wrapper subsequently calls
  `from_canonical_json`, parsing/emitting again. Byte vectors inside JSON
  envelopes are represented as integer arrays. These are concrete candidates
  for allocation/representation cost and require measurements before choosing
  a repair. Preserve exact canonical event bytes and explicit codec semantics.

Later root observation around 20:22 UTC: final destination and receipt existed
at `next-rust-20260913T1959462179878Z`, reporting revision 1 and the exact
425/212/64/1292 inactive denominator. The owning process was still running;
CPU time was 749.05s, working set 1,692,499,968 bytes, and peak working set
13,915,303,936 bytes. Published database file length was 671,092,736 bytes.
These are actual OS process/file observations, not estimates. The terminal
operation duration and physical allocation still require the worker receipt.
Peak memory and amplification on this small input require repair before the
full performance/product gate. Do not rerun another identical whole-store
audit merely to repeat evidence already obtained by the operation's two audits.
