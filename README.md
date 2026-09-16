# Zap

Zap is a local adaptive planning and agent-workspace product. This repository
ships two mutable `1.0.0` packages from branch `1.0`:

- `flow:org.vibevm.zap/zap@=1.0.0` — the Rust planning, storage, command, and
  query engine.
- `tool:org.vibevm.zap/lens@=1.0.0` — Quick Lens, Wayfinder, managed agents,
  local HTTP/MCP surfaces, and the source/binary application tooling.

The source registry is rooted at [`vibevm/vibepacks`](vibevm/vibepacks).
Historical predecessor source is retained under [`archive`](archive) and is
not part of the published registry.

## Install and run

The normal user route is Vibe's global application command:

```text
vibe install -g org.vibevm.zap/zap
zap-quicklens
```

`zap-server` starts the same product and HTTP UI without opening a viewer.
Verified Windows x64 releases contain Node.js 24, production npm dependencies
(including Electron and node-pty), and the Rust `zap.exe`; they do not require
a host Node.js or Rust installation. Vibe can instead build from this source
when the user explicitly chooses the source route.

## Develop

Requirements are Rust 1.93, Node.js 24, npm, and the platform prerequisites
documented by Lens. Work in the package that owns the change:

```text
cd vibevm/vibepacks/org.vibevm.zap/zap/v1.0.0
cargo build --locked
cargo test --locked

cd vibevm/vibepacks/org.vibevm.zap/lens/v1.0.0
npm ci
npm run verify
```

Prefer affected checks while iterating. The source installer has focused tests
under `tooling/source-install`; binary distribution tooling lives beside it.
Protocol identifiers such as `.../1` have their own compatibility lifecycle
and do not change merely because the package release is 1.0.0.

## Release layout

The source manifest carries one stable distribution-index locator. Release
automation publishes `DISTRIBUTIONS.json` and platform ZIPs outside the source
tree. Every ZIP has an exhaustive `vibe-application-distribution.json` binding
its source commit/tree, platform, launchers, and file hashes. Source and binary
selection remain independently auditable.

Zap is licensed under UPL-1.0. See [LICENSE.md](LICENSE.md).
