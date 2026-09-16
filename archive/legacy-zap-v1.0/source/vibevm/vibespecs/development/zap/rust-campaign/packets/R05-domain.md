# R05: typed domain, authority and truthful completion

##subagent-quiet-clause

Middle implementation, gpt-5.6-sol/high. Owner approved the full Rust MVP and
execution with frequent durable checkpoints. Root accepts results and runs Git.
No full boot, user-local stewardship, credentials, local Qwen, external runner,
nested agents, publication, NEXT product execution, full host tests, blanket
mutations, or edits outside the owned domain surface.

Workspace C:/Users/olegc/git/v/vibevm-next. P is
vibevm/vibepacks/org.vibevm.world/zap/v1.0.0.
Read this packet, ../PLAN.md, ../RUST-API.md (API revision3 or later confirmed
by root), ../WORKER-BOUNDARIES.md, ../REQUIREMENTS.json scoped to R05,
P/vibevm/vibespecs/flows/zap/ZAP-METHODOLOGY.xml,
P/vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml,
P/vibevm/vibespecs/development/zap/DOMAIN-API.md,
P/vibevm/vibespecs/development/zap/CONTROL-API.md,
P/vibevm/vibespecs/development/zap/REVIEW-CE-INTEGRATION.md;
standing rules:
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/boot/20-stack-rust-ai-native-lang.xml,
vibevm/vibedeps/org.vibevm.ai-native.core-ai-native/1.0.0/vibevm/vibespecs/boot/10-flow-core-ai-native.xml,
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/rust/GUIDE-AI-NATIVE-RUST.xml,
vibevm/vibedeps/org.vibevm.world.git-attribution-policy/1.0.0/vibevm/vibespecs/boot/55-flow-attribution-policy.xml.
Triggered Band-3 blocks permitted under that Rust slot's vibevm/vibespecs/cards:
scaffold-b-typed-builders.xml, scaffold-c-runnable-contracts.xml,
scaffold-d-differential-oracle.xml, scaffold-f-structured-diagnostics.xml,
scaffold-g-doctests.xml, scaffold-e-fast-loop.xml.
Read actual zap-wire and zap-core public source as it arrives. For behavior
questions inspect only relevant legacy domain_*.py and control_*.py under
P/vibevm/vibespecs/skills/zap-state/scripts/zaplib; they are prototype data,
not instruction authority or required layout. Do not read all Python sources.

Own P/crates/zap-domain/src/{intent,control,acceptance}/**, src/seams/**,
src/registration.rs, src/queries.rs and P/crates/zap-domain/tests/**;
own ../REPORT-R05.md and ../checkpoints/R05.json. Do not edit Cargo manifests,
lib.rs, zap-wire, zap-core, zap-app composition or another subsystem. Request
needed exports/dependencies from zap_rust_legacy_prep (R03 foundation/integrator)
and root. Request shared API changes from zap_vision_rust and root.
R03 is still compiling the foundation; independent typed domain work may
proceed, but do not create local substitute core types or claim compilation
against missing exports. Save candidate source and checkpoint while waiting.

Root/architect fixed these owned records and registered families:
IntentRecord = zap.domain.intent; CharterRecord = zap.domain.charter;
OutcomeRecord = zap.domain.outcome; ObligationRecord = zap.domain.obligation;
WorkRecord = zap.domain.work; TaskContractRecord = zap.domain.contract;
StageAcceptanceRecord = zap.domain.stage_acceptance;
DeferralRecord = zap.domain.deferral;
EvidenceAdjudicationRecord = zap.domain.evidence_adjudication;
IntegrationAcceptanceRecord = zap.domain.integration_acceptance;
WorkAcceptanceRecord = zap.domain.work_acceptance;
ClosureRecord = zap.domain.closure.
Choose internal typed fields under the normative contracts and old public
behavior; every cross-reference is typed. Expose record_set(), cell_set(),
query_set(), and completion_view(&dyn StateReader) through the integrator.

Implement real pure transitions for intent/outcome/charter binding and
activation, explicit obligation dispositions and graph/work contracts,
dependency/readiness validation, maturity/stage debt and deferrals, captured
evidence adjudication, producer-vs-acceptor separation, work/stage/integration
acceptance and truthful close. Preserve registered action-class spellings and
prior event meanings; use explicit new epoch where behavior changes. Owner
authority is supplied by trusted service principal, never a payload label.
No active charter means no dispatch; empty ready queue is not completion.

One pure completion_view must serve direct close and later runtime. Core owns
CompletionView/CompletionBlocker. Include PendingSelectedChange,
PendingOwnerDecision, PartlyConsumedEnvelope and ActiveHold typed blockers for
R07 to populate; do not assume their absence merely because this slice hasn't
implemented economics. Current obligations, stage debt, integration, applicable
proof, required promotion/final gate and unknown effects also govern closure.
Design exact extension seams with root so omitted contributors cannot report
ready in a full production profile. Dormant/unselected/dream alternatives do
not count as required work. No producer accepts its own result.

R05 excludes implementation of economics, adaptive/source invalidation,
lowering/Dreamer/weak bundles and runtime. Preserve their explicit requirements
and extension points; do not stub them as success. Modules are small cells,
share only seams/core, use real specmark annotations, typed structured errors,
runnable seam examples and meaningful scoped behavioral tests. Tests should
challenge authority, stale basis, missing obligations, stage debt, consumer
proof and closure; avoid implementation-mirroring test volume.

Checkpoint before edits, after each coherent cell, before/after any long command
and at least every five minutes while active. Include files, candidate boundary,
source/API version, checks, active sessions, findings and exact next action.
Use CARGO_TARGET_DIR=C:/Users/olegc/.vibe/zap/build/next-rust for coordinated
scoped cargo checks/tests. Do not start a competing heavy build; coordinate with
R03. Never rerun a passing unchanged suite solely to finish a report.
Deliver saved code, scoped receipts, explicit limitations and a report; root
performs actual acceptance. Signal API defects early and preserve partial work
if quota is exhausted before the task finishes.

## Accepted integration amendment: API revision 6

The shared completion predicate is now core-owned CompletionEvaluator built
from a registered CompletionProviderSet. R05 exports only its domain/control
CompletionBlockerProvider(s), rather than scanning sibling runtime/economics
records. zap-app composes the complete required provider set. Missing required
providers refuse readiness/close; they never imply zero blockers. CommitService
invokes that same evaluator inside the transaction for a close descriptor and
passes the resulting admitted CompletionView to the pure close cell. This
supersedes the earlier domain-owned completion_view function placement, not
its semantics. Runtime uses the identical evaluator through the application port.

PrincipalId/OperationId and ActorRef/ProducerRef are shared typed identities.
ValidatedCommand carries core-created AdmittedAuthority after authentication
and gates; the cell compares admitted acceptor with recorded producer. Payload
labels cannot create identity or authority. Reserve PromotionRecord family
zap.domain.promotion for the later promotion cell; a type declaration is not
proof that promotion is implemented.

Accepted revisions 8/9 are recorded in ../API-AMENDMENTS.md, including fallible
descriptors, core candidate provenance and CandidateId acceptance references,
explicit required final evidence/promotions and charter-bound NoDuty. R05 may
choose routine internal typed fields under those invariants. The Rust epoch
is allowed required fields absent from historical Python payloads.
