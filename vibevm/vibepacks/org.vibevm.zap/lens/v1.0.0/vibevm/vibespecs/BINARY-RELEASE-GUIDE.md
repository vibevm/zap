# Build the native Zap distributions {#root}

`guide r2`

The binary release is derived from a clean committed checkout. It does not
write artifact URLs or hashes into `vibe.toml`; release automation publishes
those facts in the external `DISTRIBUTIONS.json` asset.

## Prepare the exact source witness {#source-witness}

Commit the intended source and ensure `git status --porcelain=v1
--untracked-files=all` is empty. The root `.gitattributes` pins LF source bytes.
Then run the read-only witness command:

```text
node <lens>/tooling/distribution/build-windows.mjs `
  --source-root <clean-zap-checkout> `
  --print-source-witness
```

It observes the exact HEAD, creates and extracts a temporary `git archive`,
computes Vibe recipe `sha256-tree/1` over both the clean checkout and archive,
and refuses any mismatch. The recipe excludes only `.git`, `.vibe`, `target`,
`node_modules`, and `.vibeignore`; it does not follow `.gitignore` or silently
exclude `dist`.

## Build {#build}

The release workflow builds five native targets from the same witness: Windows
x64, Linux x64-musl, Linux x64-GNU, macOS Intel and macOS ARM64. Each runner uses Node.js 24
for its own platform and puts output outside the source checkout. Windows uses
`build-windows.mjs`; POSIX runners use `build-posix.mjs --target linux-x64-musl`,
`linux-x64-gnu`,
`macos-x64` or `macos-arm64` with the same source/node/output arguments.

The Windows spelling remains:

```text
node <lens>/tooling/distribution/build-windows.mjs `
  --source-root <clean-zap-checkout> `
  --lens-root <clean-zap-checkout>/vibevm/vibepacks/org.vibevm.zap/lens/v1.0.0 `
  --engine-root <clean-zap-checkout>/vibevm/vibepacks/org.vibevm.zap/zap/v1.0.0 `
  --node-root <unpacked-node-v24-win-x64> `
  --output <absent-or-new-output-directory> `
  --source-commit <full-head-oid> `
  --source-tree <sha256-tree/1:digest> `
  --cargo-target-dir <private-cache-outside-source> `
  --cargo cargo
```

`--offline` additionally requires npm and Cargo inputs to be cached. The
builder rechecks the source witness before doing any build work, prepends the
bundled Node directory to every npm lifecycle `PATH`, requires x64 Node,
builds Rust for `x86_64-pc-windows-msvc`, stages actual Rust/Node/npm license
notices, rejects an incomplete Electron or node-pty runtime, writes the strict
descriptor, and creates a ZIP with regular file entries only.
`--cargo-target-dir` may reuse a private target cache across identical
toolchain/target retries; it never changes the committed source witness.

## Release boundary {#release}

Each output directory and ZIP is a candidate artifact. They are not published
or accepted merely because the builder completed. Vibe core must verify the
descriptor and ZIP, compare application and source identity with the strict
release index, install into an isolated settings root, exercise both public
commands, and prove source-independent uninstall. Release automation then
publishes only after all five receipts name the same source commit/tree. The
composed strict `vibe-application-distribution-index/1` contains exactly five
platform rows; the source tree stays unchanged.
