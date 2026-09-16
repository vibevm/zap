# Zap contributor instructions

This is the standalone Zap product repository. Do not load the VibeVM host
repository boot lane or treat its local stewardship state as Zap project state.

Before changing a package, read its `vibe.toml`, README, and the exact specs or
tests governing the requested surface. Current publishable source lives only
under:

- `vibevm/vibepacks/org.vibevm.zap/zap/v1.0.0`
- `vibevm/vibepacks/org.vibevm.zap/lens/v1.0.0`

`archive/legacy-zap-v1.0` is historical evidence. Do not import it, publish it,
or update its proof claims as though they described the current product.

Use human-authored attribution, Conventional Commits, atomic changes, and
affected tests. Never add AI/tool co-author trailers. Do not commit generated
release indices, binaries, credentials, user state, caches, `node_modules`, or
`target` output. Release artifacts are derived from an exact source commit and
verified outside the source tree.

Keep package release versions separate from wire/schema/protocol versions.
Source installation must snapshot both Zap-owned packages and may resolve
external dependencies only through ordinary configured Vibe registries.
Binary installation must run from its bundled relative payload without host
Node.js/Rust or a source rebuild.
