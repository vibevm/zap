# R04: real transactional storage and application service

##subagent-quiet-clause

Middle implementation, gpt-5.6-sol/high. Start when root accepts the generic
R03 handoff. Owner authorized full Rust MVP and frequent durable checkpoints.
Root accepts/commits/publishes. No full boot, user-local stewardship, credentials,
local Qwen, external model runner, nested agents, NEXT execution, full host test
panel or blanket mutations. No production Python.

Workspace C:/Users/olegc/git/v/vibevm-next. P is
vibevm/vibepacks/org.vibevm.world/zap/v1.0.0.
Read this packet, ../PLAN.md, ../RUST-API.md, ../API-AMENDMENTS.md,
../STORAGE-ADR.md, ../WORKER-BOUNDARIES.md, ../REQUIREMENTS.json scoped R04,
P/vibevm/vibespecs/flows/zap/ZAP-RUST-STORAGE.xml,
P/vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml,
P/vibevm/vibespecs/development/zap/STORAGE-API.md,
P/vibevm/vibespecs/development/zap/REVIEW-CE-INTEGRATION.md;
standing files:
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/boot/20-stack-rust-ai-native-lang.xml,
vibevm/vibedeps/org.vibevm.ai-native.core-ai-native/1.0.0/vibevm/vibespecs/boot/10-flow-core-ai-native.xml,
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/rust/GUIDE-AI-NATIVE-RUST.xml,
vibevm/vibedeps/org.vibevm.world.git-attribution-policy/1.0.0/vibevm/vibespecs/boot/55-flow-attribution-policy.xml.
Triggered Band-3 blocks: scaffold-b-typed-builders.xml,
scaffold-c-runnable-contracts.xml, scaffold-d-differential-oracle.xml,
scaffold-e-fast-loop.xml, scaffold-f-structured-diagnostics.xml,
scaffold-g-doctests.xml and scaffold-h-simulators.xml under the Rust slot's
vibevm/vibespecs/cards. Read existing Rust core/wire/record/service source;
domain/runtime public exports only as integration consumers, not whole source.

Own P/crates/zap-store/**; generic zap-core commit/service/authority/completion
implementation that you owned under R03; package Cargo dependency table and
lib.rs/app composition integration. R11 keeps core/agent/** and execution_views/**;
R05 keeps domain cells. No sibling imports in implementation crates: ports in
core, composition in app. Own ../REPORT-R04.md and checkpoints/R04.json.
Coordinate any shared signature change and update RUST-API plus consumer notice.

Implement the real redb4.2.0 backend and official CommitService, not a facade
over an in-memory snapshot or Python. The single transaction commits exact
logical event, idempotency receipt, validated registered records and required
indexes, and head metadata. Use Immediate durability and the accepted quick
repair policy. A typed TransactionPermit authorizes entering the store; a
ValidatedCommitIntent is created only after transaction-bound gates. No public
raw record/journal mutation backdoor. Match store/base/campaign, exact command
identity, current revision and relevant basis, principal/route, charter/control,
pause/hold, required providers and cell preconditions before committing.

Finish core CompletionEvaluator, with exact required CompletionProviderSet,
real provider calls against the same pre-state, typed blocked reasons, and the
identical transactional predicate for direct close and runtime. No missing
provider may look like zero blockers. Finish core authority/registrar handling
needed to apply real cells. OwnerControl activation must authenticate without
requiring a charter already active; coordinator grants cannot use Owner control.
Trusted observations may drain existing jobs during pause without launching work.

Implement current indexed reads and bounded pagination/cursors, registered
record/index codecs, duplicate/ref/key/version checks, read snapshots and
consistent snapshot+tail export. Do not rebuild the entire plan/history for
ordinary command/lookup. Prioritize ID/family reads, event/idempotency/history,
typed outgoing/incoming and ready/blocker index infrastructure; index definitions
come from registered typed record contributions. Required indexes are committed
with records and old rows are removed/replaced exactly.

Normal reopen validates the declared trusted-local boundary and application
epochs/head/catalog under engine crash recovery. Explicit audit fully verifies
event/hash/reducer history into a fresh sibling store and compares projections.
Commit errors with unknown outcome close/reopen/reconcile exact command ID
before retry. Corrupt history is retained and refused, never silently skipped.
No test or recovery operation deletes actual user data or changes the old MUP
or ZAP stores. Tests use fresh isolated directories.

Implement immutable content-addressed artifact preparation/publication: hash and
fsync before DB writer, preserve a verified handle/lock witness through short
commit, never overwrite an existing referenced blob, reject mismatched bytes/
symlinks/path escape, and keep post-publication/pre-commit orphans recoverable.
No giant file hash or external call while holding the database transaction.
Use safe Rust/platform wrappers; do not add arbitrary unsafe code.

Prove meaningful failure boundaries with scoped tests: atomic event+record+
index visibility, exact retry vs conflict, stale basis/revision, reopen after
interruption, index replacement, bounded/foreign/stale cursors, artifact orphan
and dangling-reference prevention, required-provider closure, direct-route
authority and no effects in replay. Use a small real typed cell and, when ready,
R05's actual domain cells. A fake can inject commit ambiguity, but report the
difference between injected and actual engine-crash evidence. No full host panel.

After first durable vertical slice works, checkpoint and notify root before
expanding tests. End with real registered API, scoped receipts, known limits,
remaining stage debts and next action. Save every coherent edit unit; serialize,
parse and atomically replace checkpoints with previous-valid backup using the
actual process clock. Use CARGO_TARGET_DIR=C:/Users/olegc/.vibe/zap/build/next-rust
and coordinate heavy cargo work. Do not rerun unchanged passing suites for prose.
