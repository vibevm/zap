# R01: Rust integration contract

##subagent-quiet-clause

Senior architecture task. Owner accepted the entire Rust vision and authorized
implementation. You architect and document; do not write production code or
tests. Root handles acceptance/Git. No nested agents, external launchers, local
Qwen inference, publication, credentials, or user-local stewardship reads.

Workspace C:/Users/olegc/git/v/vibevm-next. P is
vibevm/vibepacks/org.vibevm.world/zap/v1.0.0. You own only
P/vibevm/vibespecs/development/zap/rust-campaign/RUST-API.md,
STORAGE-ADR.md, WORKER-BOUNDARIES.md and checkpoints/R01.json.
Save a checkpoint at start, each design boundary, and at least every five
minutes while active. Include remaining questions and next exact action.

Read this packet and ../PLAN.md, P/vibevm/vibespecs/research/zap/ZAP-RUST-MVP-VISION-2026-09-13.md;
P/vibevm/vibespecs/flows/zap/ZAP-METHODOLOGY.xml, ZAP-ADAPTIVE-CYCLE.xml,
ZAP-CHANGE-ECONOMICS.xml, ZAP-RUNTIME.xml, ZAP-DATA-AND-VIEWER.xml;
P/vibevm/vibespecs/development/zap/REVIEW-CE-INTEGRATION.md;
vibevm/vibespecs/common/PROP-024-code-bearing-packages.xml;
vibevm/vibedeps/org.vibevm.ai-native.rust-ai-native-lang/1.0.0/vibevm/vibespecs/rust/GUIDE-AI-NATIVE-RUST.xml;
vibevm/vibedeps/org.vibevm.ai-native.core-ai-native/1.0.0/vibevm/vibespecs/boot/10-flow-core-ai-native.xml;
vibevm/vibedeps/org.vibevm.world.git-attribution-policy/1.0.0/vibevm/vibespecs/boot/55-flow-attribution-policy.xml.
Do not read full AGENTS/boot. These named rules bind; report a missing packet
rule instead of loading unrelated instructions.

You may inspect targeted Python files under P/vibevm/vibespecs/skills/zap-state/scripts/zaplib,
existing API markdown under P/vibevm/vibespecs/development/zap, and installed
rust-ai-native-lang manifests/specmark source solely to determine public build
and annotation seams. Read-only cargo/rustc version and crate metadata probes
are allowed, but no model call or tests. Consult official dependency docs if
uncertain. Do not inspect secrets or generate bytecode.

Deliver a concrete integration contract that three Middle implementers can
follow independently: package Cargo workspace/crate DAG, small module/cell
boundaries, exact public types/enums/traits and constructor invariants, storage
commit/index/query APIs, pure command transitions and trusted service gates,
semantic and native AgentHost interfaces, stable IDs/digests/relevant basis,
economics/shared closure semantics, packets/roles/lowers/weak bundles/dreams,
recovery/resume and goal capability contracts. Avoid an untyped JSON loophole
for core semantics. Explain how extensibility avoids editing every crate.

Choose the storage model (prefer one DB transaction for immutable logical
events and required indexes), and settle checkpoint trust/legacy epochs without
claiming arbitrary same-user tamper prevention. Validate redb MSRV compatibility
or choose a justified supported version. Align with Rust 1.93+/edition 2024.
Specmark must be the real distributable VibeVM discipline dependency, not a
home-grown no-op replacement or a path into the host source checkout.

Specify foundation -> independent semantic/runtime implementation seams with
one owner of each composition file. State which legacy behavior is preserved,
which defects are corrected with explicit epoch, and how complete V01-V24
coverage will be accepted. No implementation stubs count as features. Report
when the foundation contracts are ready so root can launch coding early.
