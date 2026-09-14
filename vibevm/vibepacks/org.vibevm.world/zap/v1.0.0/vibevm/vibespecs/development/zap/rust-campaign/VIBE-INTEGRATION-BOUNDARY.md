# Verified host integration boundary

Root checked the Git working tree and committed paths from Rust baseline
2d48bd51 through457f3c02 on2026-09-14. All changes belong to the ZAP package;
no host VibeVM source change was required by this Rust implementation so far.

ZAP owns its Cargo workspace, CLI/service, persistence and protocol. Its
manifest declares rust-ai-native-lang1.0.0 and its Cargo workspace declares
the Rust libraries it uses. Rust1.93 is the declared minimum compiler. VibeVM
integration uses existing package install/build and specification query
capabilities. A cooperating native harness driver supplies harness-only calls.
No source-checkout path to host VibeVM may be a shipped dependency.

An older installed Vibe CLI lacks the current build capability despite sharing
version1.0.0 with the inspected current host binary. Capability probing and the
ordinary neutral-consumer proof remain necessary; this record does not claim
that final installed proof has already passed.

Temporary Owner instruction: use offline installation while a separate Claude
worker fixes the online-install hang. That bug is outside this ZAP task; no
local repair or online-installer investigation is authorized by this record.
