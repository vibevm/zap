# Build the Windows x64 Zap distribution {#root}

`guide r1`

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

Use an unpacked official Node.js 24 Windows x64 distribution that includes npm
and its license. Put output outside the source checkout. Substitute the two
values printed by the witness command:

```text
node <lens>/tooling/distribution/build-windows.mjs `
  --source-root <clean-zap-checkout> `
  --lens-root <clean-zap-checkout>/vibevm/vibepacks/org.vibevm.zap/lens/v1.0.0 `
  --engine-root <clean-zap-checkout>/vibevm/vibepacks/org.vibevm.zap/zap/v1.0.0 `
  --node-root <unpacked-node-v24-win-x64> `
  --output <absent-or-new-output-directory> `
  --source-commit <full-head-oid> `
  --source-tree <sha256-tree/1:digest> `
  --cargo cargo
```

`--offline` additionally requires npm and Cargo inputs to be cached. The
builder rechecks the source witness before doing any build work, prepends the
bundled Node directory to every npm lifecycle `PATH`, requires x64 Node,
builds Rust for `x86_64-pc-windows-msvc`, stages actual Rust/Node/npm license
notices, rejects an incomplete Electron or node-pty runtime, writes the strict
descriptor, and creates a ZIP with regular file entries only.

## Release boundary {#release}

The output directory and ZIP are candidate artifacts. They are not published
or accepted merely because the builder completed. Vibe core must verify the
descriptor and ZIP, compare application and source identity with the strict
release index, install into an isolated settings root, exercise both public
commands, and prove source-independent uninstall. Release automation then
publishes the ZIP and strict `vibe-application-distribution-index/1` asset; the
source tree stays unchanged.
