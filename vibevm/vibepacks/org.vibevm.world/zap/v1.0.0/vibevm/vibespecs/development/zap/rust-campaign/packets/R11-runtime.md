# R11/R12: native execution, concurrency and durable continuation

##subagent-quiet-clause

Middle implementation, gpt-5.6-sol/high. Owner authorized the complete Rust MVP
and frequent filesystem checkpoints. Root accepts/commits/publishes. No full
boot, central stewardship, credentials, local Qwen inference (even discovery),
external model runner, nested agents, NEXT execution or full host test panel.
Use fakes/recorded host metadata for protocol verification now; a real native
integration probe is coordinated by root after the assembled Rust service works.

Workspace C:/Users/olegc/git/v/vibevm-next. P is
vibevm/vibepacks/org.vibevm.world/zap/v1.0.0.
Read this packet, ../PLAN.md, ../RUST-API.md (accepted revision7),
../WORKER-BOUNDARIES.md, ../STORAGE-ADR.md; ../REQUIREMENTS.json scoped to
R11/R12; P/vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml and
P/vibevm/vibespecs/flows/zap/ZAP-AGENT-PROTOCOL.xml;
P/vibevm/vibespecs/development/zap/RUNNER-API.md, TRANSPORT-API.md and
REVIEW-CE-INTEGRATION.md for preserved behavior boundaries;
standing files:
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/boot/20-stack-rust-ai-native-lang.xml,
vibevm/vibedeps/org.vibevm.ai-native.core-ai-native/1.0.0/vibevm/vibespecs/boot/10-flow-core-ai-native.xml,
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/rust/GUIDE-AI-NATIVE-RUST.xml,
vibevm/vibedeps/org.vibevm.world.git-attribution-policy/1.0.0/vibevm/vibespecs/boot/55-flow-attribution-policy.xml.
Triggered Band-3 blocks permitted under the Rust slot's vibevm/vibespecs/cards:
scaffold-b-typed-builders.xml, scaffold-c-runnable-contracts.xml,
scaffold-f-structured-diagnostics.xml, scaffold-g-doctests.xml,
scaffold-e-fast-loop.xml, scaffold-h-simulators.xml.
Read zap-wire/zap-core public source as it becomes available. Only for a narrow
behavior ambiguity inspect relevant runtime_*.py or transport*.py under
P/vibevm/vibespecs/skills/zap-state/scripts/zaplib. No full Python scan or boot.

Own P/crates/zap-runtime/src/** except lib.rs, and its tests/**;
own ../REPORT-R11.md, ../REPORT-R12.md and checkpoints/R11.json.
Do not edit shared Cargo/lib.rs/wire/core/domain/store/app/API code. R03
zap_rust_legacy_prep owns initial shared composition; R05 zap_rust_domain owns
semantic records. Ask for required core DTO/export/port changes; never import
zap-domain or create substitute types to evade the sibling boundary. Start
owned protocol/scheduler code against frozen contracts while foundations
compile; acceptance requires actual R03/R04/R05 integration.

Implement typed runtime record families, pure registered state transitions,
deterministic conflict/resource-aware ready selection, separate execution/
collection/safe/acceptance state, and a nonblocking coordinator step loop over
CampaignReadPort/CommandPort and separate AgentHost/SemanticProvider ports.
Commit claims/intents before effects. No model/host/filesystem waits occur
inside a DB write transaction; one slow request cannot block unrelated jobs.
No active charter/held subject/Owner pause may dispatch through another route.
The runtime must consume the shared completion_view, never recreate closure.

Native bridge is the default selected mechanism: produce a durable exact
DispatchIntent for a cooperating harness driver; accept only a bound receipt
from that driver. AwaitingHarness is not launch. Preserve external handles,
packet/contract/basis identities and message IDs for idempotent reconnect,
collection, cancellation, safe drain and recovery. A subprocess adapter, if
implemented, is explicitly selected and never an automatic fallback.
Do not invoke codexrunner or any local model to claim native support.

Implement compact machine messages/results, candidate artifact and check
references, heartbeat coalescing vs useful checkpoints, malformed-response
repair that preserves successful work, retry classification/backoff and
unknown-effect reconciliation. Provider quota/overload is not an architectural
failure. Credentials/account changes do not reset logical attempt/history.
Actual effects and durable acceptance remain distinct.

R12 continuation: capabilities cached by effective harness/toolset/version/
configuration identity, desired vs resolved model/effort, explicit missing or
unsupported values, Owner-selected transport. Goal capabilities have separate
read/create/update/clear support; no inference of update from create. Generate
bounded resume and campaign/per-assignment GOAL projections from stored state.
Record exact actual acknowledgment, manual-required instruction, unsupported,
unknown or stale application state. Never mark unfinished goal complete just
to replace text. No inference call for discovery. One-goal mode uses umbrella
plus references, never the entire concatenated work graph.

Every primary state has a machine view and recorded source/decision reason.
Small typed owned records are fine; untyped JSON domain payloads and success
stubs are not. Sparse verification should prove two independent jobs progress,
conflicts refuse, paused/held work stays stopped, unknown launch is reconciled,
retry counters survive restart and goal capability fallbacks remain honest.
Use injectable fake host/clock/ports and scoped tests; do not rebuild the host.

Save start and incremental checkpoints at coherent file boundaries, before/
after long operations and at least every five minutes while active. Preserve
candidate source and exact next step if quota ends. Use shared
CARGO_TARGET_DIR=C:/Users/olegc/.vibe/zap/build/next-rust and coordinate heavy
cargo operations with R03. Signal a useful partial runtime protocol early.
Final reports distinguish implementation from fixture-only evidence and pending
service integration; root alone accepts the result.

APIrev6 clarification: the shared completion_view is produced by core's
CompletionEvaluator from startup-registered CompletionBlockerProvider sets.
R11 exports its runtime provider; it does not scan domain/economics records or
duplicate the predicate. Missing required providers refuse readiness/close.
CommitService uses the same evaluator transactionally for direct close.
OwnerControl(ControlClass) is distinct from ordinary privileged product actions;
no coordinator action or runtime receipt may mint Owner control authority.

Accepted revisions 8/9 are recorded in ../API-AMENDMENTS.md. R11 additionally
owns zap-core/src/agent/** and zap-core/src/execution_views/** for shared
runtime DTOs/ports. R03 owns core lib.rs integration and generic core/authority/
candidate provenance. This temporary shared-surface assignment is part of R11's
packet; coordinate directly and do not invent sibling-dependent substitutes.
