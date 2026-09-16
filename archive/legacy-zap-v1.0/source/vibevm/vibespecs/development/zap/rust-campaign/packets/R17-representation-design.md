# R17 bounded storage representation repair design

##subagent-quiet-clause

Senior architecture only; no production edits, builds, tests, Git, model calls,
local inference, NEXT execution, import retry or mutation of the pilot store.
Workspace/package and exact standing instructions are those named by
R04-store.md; reuse already-read files. Read that packet's named rules,
../REPORT-R14-FINAL.md, ../PERFORMANCE-OBSERVATIONS.md,
../STORAGE-ADR.md, accepted core/storage APIs and permanent
ZAP-RUST-STORAGE.xml. Inspect only canonical wire serialization, record/history
envelopes and indexes, import translation/materialization, artifact ports and
their relevant tests. Do not read the full boot or private plans/credentials.

Actual evidence:3,277,342-byte base input,425/212/64/1292 records, approximately
17-minute two-audit import,13,915,303,936-byte peak working set and671,092,736-byte
logical/allocated database. Candidate mechanisms include repeated source_raw
copies, integer-array encoding of byte vectors inside envelopes, full history
entry copied into both indexes and repeated canonical materialization. These
are source observations, not measured attribution. Do not run another huge
import to rediscover them.

Produce REPAIR-R17-REPRESENTATION.md as a bounded implementable contract, not a
new storage system. Separate exact logical event/record identities from derived
physical encoding. Preserve original zap/1 and committed zap/2 bytes/digests,
finite-number/duplicate-key/canonical validation, event replay and no-clobber
artifact guarantees. Explicitly version any physical representation or new
payload shape; never reinterpret existing signed/hashed bytes as a new format.

Choose the smallest sequence of high-value repairs after inspecting actual
data flow. Specify exact file/type/encoding/index ownership, old/new reads,
atomic rebuild/recovery and how original preserved bytes remain accessible.
Explain whether large raw material belongs behind the existing immutable
artifact protocol, and avoid carrying duplicate complete task-group sources in
every task when exact references suffice. If old event payloads require a
frozen reader, name it rather than assuming the live serde shape remains equal.

Keep R07/R08 current core feature work unblocked: no new generic authority
framework or global semantic epoch merely to optimize an internal index.
Distinguish quick independent internal repair from a logical format change
requiring its own proof. Give a deterministic small amplification fixture,
exact golden/replay checks, relevant failure/recovery cases and one final
actual-sized comparison with cold/warm/CPU/memory/allocated bytes measured.
Debug versus release CPU cost must be explicit; release build alone cannot
explain away storage and memory amplification.

Save useful sections/checkpoint at most five minutes while active. Return a
short concrete ordered contract, assumptions needing measurements and an
estimated implementation cost, then stop. Root accepts and assigns Sol/xhigh
coding separately. Full R17 graph latency/index benchmarking remains its later
packet; this task targets the demonstrated representation bottleneck only.
