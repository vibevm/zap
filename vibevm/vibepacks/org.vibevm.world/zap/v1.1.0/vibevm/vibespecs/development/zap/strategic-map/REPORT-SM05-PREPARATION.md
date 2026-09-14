# SM05 package-source helper preparation

Status: **prepared, not assembled**. The helper targets only
`flow:org.vibevm.world/zap@=1.1.0` and writes a payload under
`org.vibevm.world/zap/v1.1.0` plus `ZAP-PAYLOAD-MANIFEST.json` at the external
registry root. It validates both `vibe.toml` and the Cargo workspace package version
before staging.

The accepted 1.0 assembler protections remain:

- exact parity with all 27 `.vibeignore` entries;
- `**/target/**` plus an explicit any-depth target-segment selection predicate;
- the same target predicate in the final forbidden staged-row guard;
- explicit root files/directories and required-file checks;
- two complete source-row captures with bytes and SHA-256, refusing source drift;
- byte/hash comparison of every staged file against the captured source;
- no replacement of an existing payload or external manifest;
- forbidden development, dependency, tool-config, token, trust, Python, and cache
  material checks;
- deterministic path-sorted payload rows and a SHA-256 manifest digest;
- provenance manifest outside the package payload.

Focused read-only verification parsed the PowerShell AST with zero errors, matched the
27 supported exclusions to the 27 current `.vibeignore` entries, verified root and
nested `target` paths are rejected while `targeted` is not, found both 1.1.0 version
declarations, and confirmed the source-drift, staged-hash, no-clobber, and final target
guards remain present. The helper is LF-only and has SHA-256
`4cf0ed895294d99dde56922a0d2586faa3657fda77b7d2f4276ab1dc104c1ef7`.

No staging directory, manifest, payload, installed slot, registry artifact, test file,
or publication was created by this preparation. Root authorization is still required
before invoking the helper for the final unique artifact.
