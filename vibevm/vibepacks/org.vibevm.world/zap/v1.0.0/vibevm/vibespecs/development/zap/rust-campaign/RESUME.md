# Resume ZAP Rust MVP implementation

Owner accepted the complete vision and authorized planning plus execution on
2026-09-13. ZAP implementation, scoped checks, and the earlier publication
commission are active. Do not invoke local Qwen or execute NEXT product work.

Workspace: C:/Users/olegc/git/v/vibevm-next, branch next.
Package: vibevm/vibepacks/org.vibevm.world/zap/v1.0.0.
Read PLAN.md and campaign.json in this directory, then current task packets and
checkpoints. Full project boot was already read during this continuous session;
workers use their exact quiet packet instead of the full lane.

Accepted code baseline before Rust: 2d48bd51 (Python prototype and economics
design). Uncommitted Python economics code is unaccepted reference work. Its
completion-ready/direct-close omission is recorded in ../REVIEW-CE-INTEGRATION.md.
Never silently accept, discard, or mix it with Rust commits.

Current frontier: R04 durable store/service and R06 knowledge/adaptation coding,
plus independent REVIEW-R11 of the runtime/continuation candidate. R03 generic foundation
is accepted; its unsupported durable/service/completion behavior remains R04.
R01 architecture is accepted, with coordinated APIrev8/9 amendments.
R00 plan/checkpoints are accepted in 047e8834;
R02 normative requirements and the complete 279-fact map are accepted in 04b6b615.
R14-PREP legacy fixture data and replay receipts are accepted as bounded evidence,
with deliberate exclusions in REPORT-R14-PREP.md. A private copy of all 134
prototype Python sources exists at
C:/Users/olegc/.vibe/zap/development/vibevm-next/checkpoints/python-candidate-before-rust-20260913.

R01-FOUNDATION is now accepted after correcting cross-crate record registration,
object-safe read interfaces, wire error ownership, and empty bootstrap registry
semantics. zap_rust_legacy_prep implements R04; zap_rust_domain implements R06;
zap_vision_rust independently reviews the R11/R12 candidate. zap_rust_runtime is
available for review repairs. R03 wire8 tests, typed core proof and clean
core/app compilation are accepted. The contracts distinguish transaction-entry
permission from post-gate commit intent, per-operation goal capabilities,
trusted grant bootstrap and bounded runtime work views. The shared completion
contract requires all subsystem providers; its real service implementation is
R04 work. OwnerControl and admitted producer/acceptor identities remain distinct.
R03 source capture before storage edits:
C:/Users/olegc/.vibe/zap/development/vibevm-next/checkpoints/r03-before-store-20260913T093018530Z.
It contains106 source/config files with hashes; domain/runtime behavior in that
capture is still candidate. R05's four REVIEW-R05 findings are repaired and the
registered domain foundation is accepted (eight tests, scoped clippy/format).
Missing control/revalidation/promotion/query integrations remain mapped work.
R04's first real redb atom passed atomicity/retry/conflict/reopen evidence;
CommitService integration is still in progress. Next: review store/service,
knowledge and independent runtime findings. Read per-task checkpoints
and reconcile native worker status before replacing a job.

Roles: Senior gpt-5.6-sol/ultra for architecture/review, no production coding;
Middle gpt-5.6-sol/high for implementation; Junior gpt-5.6-luna/high when useful.
Use native collaboration. No external launcher or local inference fallback.

Checkpoint every coherent edit unit and before/after long commands, aiming at
least every five minutes during active work. Save results even when unfinished.
Do not rely on chat history, hidden reasoning, or a final answer for continuity.
Do not redeem a usage reset; Owner described using the UI themselves.

Current API amendments are in API-AMENDMENTS.md (rev8/9). Runtime owns the two
shared core DTO directories named there. Checkpoints use structured serialization,
parse-before-replace, actual process-clock timestamps and previous-valid backups.
R03's malformed intermediate JSON was repaired; ROOT-0010 verified all three
active worker checkpoints parse. If a new partial file appears, recover the
previous valid snapshot and reconcile actual source/job state before retrying.
