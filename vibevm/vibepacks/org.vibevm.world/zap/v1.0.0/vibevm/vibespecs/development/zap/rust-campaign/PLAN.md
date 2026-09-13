# ZAP Rust MVP implementation campaign

Owner accepted the complete Rust MVP vision on 2026-09-13 and instructed:
write the plan and execute it, with frequent filesystem checkpoints so a
ChatGPT account replacement or Codex reset can resume the work. This lifts the
implementation/test/publication pause for ZAP. Earlier publication authorization
continues. NEXT product execution and local Qwen inference remain prohibited.

## Authority and result

Deliver the complete accepted vision in
`../../../research/zap/ZAP-RUST-MVP-VISION-2026-09-13.md` (V01-V24), integrated
with existing methodology, adaptive cycle, economics, provenance, and control.
All production executable source and adapter helpers are Rust. Package version
remains 1.0.0; new store/wire epochs are explicit. The Python implementation and
unaccepted economics candidate are migration/reference evidence, not completion.
Do not begin the unrelated 425-node NEXT product campaign or build the future UI.

## Durable execution

`campaign.json` is this development campaign's machine work graph. `RESUME.md`
is its short derived navigation entry. Per-task packets define exact reading,
write ownership, checks, and expected results. Per-task checkpoints distinguish
prepared, working, candidate, verified, and accepted state. Only the coordinator
records acceptance and commits product work. Reports are evidence, not authority.

Before dispatch, save the packet and assignment. Save progress after each
coherent edit unit, before any long build/test/external operation, immediately
after its result, and at every integration/acceptance boundary. While active,
target a progress checkpoint at least every five minutes; this is a work
discipline, not a claim that a timer survives a dead process. Workers update
their own checkpoint only. Root updates the campaign and RESUME after material
changes. Never leave the only useful result in chat or in a final response.

A checkpoint records task/attempt, exact files, accepted and candidate boundary,
checks and exit status, live process/job handles, unresolved effects, findings,
next concrete action, and packet/API revision. No hidden reasoning or credentials
are saved. Unknown operation outcomes are reconciled before retry. Reset/account
replacement does not start a new logical campaign or erase failure history.

Keep code edits small and saved on disk. A compilable candidate may be committed
only after review and adequate scoped checks. Earlier unaccepted Python changes
must not be bundled into accepted Rust commits. Frequent checkpoint files do
not mean repeatedly rerunning tests or making semantically mixed commits.

## Work graph and acceptance

| ID | Result and exit evidence | Dependencies | Ownership / vision |
| --- | --- | --- | --- |
| R00 | Approved vision, durable plan/packets/resume; prototype candidate preserved separately | none | coordinator / all |
| R01 | Explicit Rust types/seams, storage and trust ADR, portable build/toolchain plan, module ownership; no ambiguous shared APIs | R00 | Senior architecture / V01,V02,V10-V13,V17-V19,V22 |
| R02 | Normative English requirements reconcile V01-V24 and existing behavior; complete requirement-to-task denominator | R00 | Senior specification / all |
| R03 | Standalone package Cargo workspace, typed wire/core contracts and real specmark/toolchain binding; scoped build | R01 | Middle foundation / V04,V12,V18,V19 |
| R04 | Transactional event store, CAS/idempotency, bounded indexed queries, snapshots/recovery and exact history | R03 | Middle storage / V09,V13,V18,V20 |
| R05 | Intent/charter authority, graph obligations, stages/debt, applicability and shared completion predicate | R03 | Middle domain / V01,V02,V03,V17 |
| R06 | Fact/source capture, uncertainty, scoped adaptive change and preserved evidence | R04,R05 | Middle domain / V01,V07,V16,V20 |
| R07 | Economics assessments/forecasts, exact four-hour decisions, affected holds, general stop precedence and closure P1 repair | R04,R05 | Middle control / V16,V17 |
| R08 | Checked strategic lowering, typed packet lineage, role routing, progressive disclosure/abstraction and selected checks | R05,R06 | Middle planning / V03-V07,V21 |
| R09 | Portable weak-execution bundles and encounter journal with idempotent stale-aware return import | R04,R08 | Middle planning / V08 |
| R10 | Isolated Dreamer overlays, persisted grill questions/answers, exact promotion/removal with cost/authority/reconciliation | R06,R07,R08 | Middle planning / V15 |
| R11 | Concurrent durable scheduler, resource/subject claims, native host bridge, typed results, liveness, retry and effect reconciliation | R04,R05 | Middle runtime / V10-V13 |
| R12 | Capability cache, desired/resolved role profiles, generated resume/GOAL, actual/manual/unavailable application states | R08,R11 | Middle runtime / V04,V05,V14 |
| R13 | Rust CLI and machine backend, consistent snapshots/tails, bounded graph/detail/search/why queries, trusted route separation | R04,R05,R11 | Middle surface / V09,V20 |
| R14 | Read-only legacy codec/history importer and isolated actual NEXT migration with exact mappings; no source changes/activation | R04,R05,R06 | Middle migration / V22 |
| R15 | Portable package binary declaration/install flow, thin native-agent skills, no shipped production Python, permanent APIs/specmaps | R02,R03,R08,R11,R12,R13 | Middle packaging / V18,V19,V22 |
| R16 | Integrated native parallel/recovery, adaptation, economics, Dreamer, weak-bundle and isolated-consumer acceptance evidence | R06-R15 | coordinator + bounded independent review / V21 |
| R17 | Declared node/edge/event scale evidence; incremental query/invalidation and cold/warm integrity costs | R04,R06,R13 | Middle performance / V18,V21 |
| R18 | Production discipline debts discharged, final scoped package gate, installation of publishable source-only payload | R16,R17 | coordinator / V21,V22 |
| R19 | Publish 1.0.0, verify installed registry payload and capability evidence; preserve migration/doc change record; stop | R18 | coordinator / V22,V24 |

R01 freezes the integration seams before concurrent Rust authors touch shared
interfaces. Initial implementation uses three disjoint ownership tracks:
foundation/store/migration; semantic planning/control; runtime/surfaces. Crate
and module composition belongs to the named integrator. Workers must request
an API revision rather than editing another track's exports. Shared compilation
is coordinated; a heavy gate never holds the sole scheduling thread.

The table names result boundaries, not permission prompts. Routine implementation
continues under the accepted vision. A new material scope change outside this
baseline follows the agreed economics and Owner rules. Do not re-gate every
baseline task merely because the entire accepted campaign is large.

## Execution detail and staged obligations

R01 fixes identifiers, role versus authority, command/observation/control routes,
pure transition interfaces, atomic store commit, indexes and query cursors,
external-effect intent/receipt contract, semantic-provider versus AgentHost,
capture fingerprints, and code-bearing distribution. Prefer one transactional
database for journal and mandatory indexes. redb is a candidate; validate its
MSRV and useful APIs before fixing the dependency. No VibeVM source-checkout
path dependency is a valid distribution seam.

R03-R05 form the first vertical slice: create an isolated draft campaign, inspect
it, admit a typed change under trusted control, persist it, reopen, and query
the same result. Use real Rust implementation, not a Python subprocess facade.
Typed IDs, structured REQ errors, pure domain functions and machine contracts
start here; formal tooling wiring is explicit and cannot silently disappear.

R06-R10 retain the original intent/outcome distinction and scoped proof reuse.
Semantic lowering and pure render remain different commands. Every debt and
obligation has a disposition. Dormant alternatives and dreams are non-executable.
Wire data is data, not instruction authority. No effect occurs during simulation.

R11-R13 implement the full intent -> native host invocation -> bound handle ->
candidate -> selected verification -> central acceptance path. Native capabilities
are actual harness metadata/receipts; a subprocess is never silently called
native. Resume reuses saved handles and reconciles unknown external effects.
Full arbitrary-agent OS isolation is not claimed. Unknown data is not false.

R14 freezes legacy bytes and digest domains, maps imported identities and every
obligation/contract, and explicitly treats corrected Rust behavior as a new
epoch. Keep the source MUP plan and existing ZAP migration untouched. The real
425-node graph is import/query evidence; it grants no NEXT execution authority.

R15 installs in a separate ordinary VibeVM project using only shipped sources
and declared dependencies. Prototype Python is retained as nonproduction
reference or excluded; normal package commands and helpers must not invoke it.
The future Qwik canvas remains a separate client. No binary blob is committed.

R16-R18 select meaningful integration scenarios once at stable boundaries.
Reuse prior applicable receipts. No per-worker full panel, blanket mutations,
whole VibeVM workspace tests, or local Qwen call. A real native bridge exercise
uses permitted cloud/native workers; weak-mode fixture evidence is labeled as
simulated and does not claim Qwen quality. Public capabilities describe actual
available operations and unsupported adapters honestly.

## Production and release closure

Temporary staged implementation may use explicit deferrals with exact closure
tasks, but production acceptance requires runnable contracts, spec traceability,
scoped discipline gates, recovery evidence, all V01-V24 dispositions, no Python
runtime dependency, and no pending selected changes or unknown effects. A green
frontier is insufficient. Preserve the open economics lifecycle P1 until its
shared runtime/direct-close fix has independent evidence.

Before removing temporary campaign specifications, promote their unique facts,
decisions, scope changes and evidence to permanent specs and project records.
Retain a compact archived execution receipt. Publication follows the existing
authorized registry route, version 1.0.0, and a clean installation proof. Stop
after completion and await further Owner improvements.
