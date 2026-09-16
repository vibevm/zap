# R14 legacy compatibility preparation

## Result

`legacy-corpus/` is a small deterministic evidence set rendered by the existing
Python reference validators, codec, reducer, replay helper, and snapshot reducer
identity function. It contains exact legacy `zap/1` bytes and SHA-256 values; it
does not define the corrected Rust epoch or grant execution authority.

The corpus has three layers:

- `codec-cases.json` records packed UTF-8, Base64, SHA-256, tagged wire values,
  round-trip values, and stable refusal codes/messages for the distinct encoding
  cases.
- `hash-domains.json` names the byte boundary for base, command, event line,
  journal prefix, pending tail, projection state, reducer identity, and snapshot
  hashes.
- `tiny-campaign/` contains the raw MUP sources, canonical `base.json`, a fixed
  five-line `events.jsonl`, a revision-4 snapshot, and machine-readable expected
  projection/refusal summaries.

`metadata.json` binds the corpus to the reference-source hashes and interpreter
version. `generate_legacy_corpus.py` reproduces all nine data files in memory and
compares their exact bytes with `--check`.

## Known legacy behavior

The canonical encoder applies `wire` before compact JSON encoding with ASCII
escaping, recursively sorted JSON object keys, forbidden bare non-finite numbers,
and UTF-8 output. Ordinary mapping insertion order therefore does not affect the
bytes. A mapping containing a literal `$zap_type` key is exceptional: it is
escaped as a tagged mapping whose value is an ordered pair list. The outer tag
object is sorted, but the pair-list order follows the input mapping. Legacy byte
identity for such collision mappings is consequently insertion-order-sensitive.

Dates, times, and datetimes use the type name and `isoformat()` value. Non-finite
floats use tagged `nan`, `inf`, and `-inf` values. Negative zero survives the
JSON round trip with its sign. Exponent spelling is the interpreter's emitted
JSON spelling, captured as bytes rather than restated as a language-independent
numeric normalization rule.

Parsing refuses duplicate JSON members before tagged decoding with `DUPLICATE`.
Bare JSON `NaN`/`Infinity` and unknown tagged values refuse with `ENCODING`.
Malformed tagged envelopes refuse with `FIELDS`.

The legacy base digest covers `packed(base) + LF`; the command digest covers
`packed(command)` without a terminator. A replayed event reconstructs the command
with `base_revision = previous_revision` before checking that digest. Journal
prefix hashes include every committed record's exact line terminator. An
unterminated final fragment is not committed and is identified by its raw byte
length and SHA-256. LF and CRLF lines have distinct identities even though both
count as terminated records in the legacy reader.

`base_revision` must have exact Python `int` type and equal the current projection
revision; a Boolean is refused even though Python otherwise treats it as an
integer subclass. Unknown event kinds refuse. Unknown plan, node, and task fields
survive in the parsed projection; an unknown task-group field survives only in
the exact raw source capture.

## Tiny campaign evidence

The fixture begins at revision 0 in draft mode with no owner contract. Four fixed
commands classify node `T`, record observed evidence, record an observed fact
bound to that evidence, and record an unknown region. The expected file includes
the full packed projection SHA-256 and a focused summary at every revision from
0 through 4. Node states remain planned and the owner contract remains null.

Pure incremental application and `replay_committed` produced identical final
projection bytes. A read-only `load_store` probe loaded five committed journal
records, no pending tail, and revision 4. A cold `load_snapshot_tail` probe
derived the same revision-4 state and found zero later events. The refusal set
also fixes stale and Boolean revisions, an unknown kind, extra payload fields,
an observed fact without evidence, a conflicting duplicate event, a sequence
gap, and a command-hash mismatch. An identical duplicate event is accepted
idempotently and applied once.

## Reproduction evidence

From `development/zap/rust-campaign/`:

```text
python -B legacy-corpus/generate_legacy_corpus.py --check
```

The command returned exit 0 and reported all nine generated files equal. The
checkpoint records every corpus file hash, the generator hash, exact reference
source hashes, and the successful read-only store/snapshot probe. No full test
panel, compilation, transport, model, command execution, or source activation
was invoked.

## Deliberate limits

These fixtures are useful legacy import evidence, not a complete Python port.
They do not cover every graph invariant, every core or extension event, snapshot
tamper permutation, crash recovery, writer locking, migration publication, or
large-graph performance. They do not assert a `zap/2` identity mapping before
the Rust architecture fixes that rule.

No change-economics event or closure-P1 behavior appears in the corpus. Existing
economics candidates remain unaccepted evidence and are not blessed as desired
Rust behavior. The fixture contains no credentials, user-local state, real NEXT
plan, or NEXT execution authority.
