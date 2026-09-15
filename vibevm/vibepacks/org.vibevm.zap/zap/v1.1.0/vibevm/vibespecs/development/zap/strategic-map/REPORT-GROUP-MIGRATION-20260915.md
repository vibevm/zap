# ZAP 1.1 package-group migration

Status: **source, traceability, conformance, tests, and clippy are green** for
the active ZAP 1.1 package at
`vibevm/vibepacks/org.vibevm.zap/zap/v1.1.0`.

## Identity boundary

- Active package identity: `flow:org.vibevm.zap/zap@=1.1.0`.
- Live specification namespace: `spec://org.vibevm.zap/zap/...`.
- Crate names, wire behavior, command names, and version remain unchanged.
- The published historical package at `org.vibevm.world/zap/v1.0.0` was not
  read, rewritten, regenerated, or republished.

The migration updated 359 enumerated tracked files before formatting:

- 343 Rust source and test files under the nine product crates;
- package identity and installation instructions in `vibe.toml` and
  `README.md`;
- `specmap.toml`, the current discipline debt registry, and the regenerated
  `specmap.json`;
- current boot/flow/research specification references;
- the executable package-source assembler at
  `vibevm/vibespecs/development/zap/strategic-map/tools/Build-PackageSource.ps1`.

The assembler now stages `org.vibevm.zap/zap/v1.1.0` and records
`flow:org.vibevm.zap/zap@=1.1.0`. Its no-clobber, explicit selection, ignore,
byte-parity, and forbidden-material checks are unchanged.

## Preserved historical evidence

Thirty-four old-group references remain intentionally in these ten immutable
pre-migration evidence files:

- `MS06-ARTIFACT-AUDIT.json`
- `MS06-INSTALLED-RECEIPT.json`
- `MS06-SOURCE-MANIFEST.json`
- `checkpoints/SM05-PREPARATION.json`
- `receipts/SM05-INSTALLED.json`
- `REPORT-MS06-INSTALLED.md`
- `REPORT-SM05-INSTALLED.md`
- `REPORT-SM05-PREPARATION.md`
- `SM05-ARTIFACT-AUDIT.json`
- `SM05-CARGO-receipt.json`

Those references describe the package identity and installed paths that the
prior runs actually observed. Rewriting them would falsify their evidence.
Package-local vendor source bytes were also left untouched.

## Verification

- `cargo fmt --all -- --check`: passed.
- `rust-ai-native specmap --check --path .`: 687 specification units, 1,442
  tagged code items, 1,749 edges, zero suspects, zero warnings, zero gated
  orphans, and zero exemptions.
- `rust-ai-native conform --path . check`: 343 files, nine gated crates, zero
  findings, zero frozen baseline entries, and zero exemptions.
- `cargo test --workspace --locked --offline --no-fail-fast`: passed across
  every workspace unit, integration, compile-fail, and doctest target.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`:
  passed with `CARGO_BUILD_JOBS=1`.
- `cargo metadata --no-deps --format-version 1 --locked --offline`: all 11
  workspace package manifests resolve inside the new package root.
- Production Rust source contains no absolute developer path, `vibevm-next`
  reference, or host `vibevm/vibepacks` dependency.

The coordinator independently assembled a new-group source payload containing
589 files with manifest digest
`00c2c4bd1a1a9b6a78be638fde79802539cd64b9b13288607600844e923983b9` and
installed three packages into an isolated consumer. `vibe bin list` resolved
`zap` to `org.vibevm.zap/zap`. The coordinator then completed the isolated
installed-package release build offline in 2m 33s and successfully ran
`vibe bin exec zap -- capabilities`. The probe returned the expected process
read surface; its empty command/query registry does not represent a configured
application service. The release build retained one pre-existing dead-code
warning for `preexecution_milestone_request`; the migration did not change
that function.

Coordinator review compared all 517 tracked Rust files with their original
revision: after the intended namespace substitution, whitespace-normalized
contents matched. All seven vendored payload files retained their prior
SHA-256 values. The staged patch also passed `git diff --cached --check`.

## Packaging caveats

- The namespace change is a new package identity, not an update in place of the
  historical published group. Consumers must install the new full identity.
- New receipts must record the new identity; prior receipts remain immutable.
- Package assembly remains no-clobber. A repeated assembly needs a fresh empty
  registry destination rather than replacing an existing payload.
- No package publication, extraction activation, `NEXT` activation, credential
  operation, Git operation, or model inference occurred in this migration.
