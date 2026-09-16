# Vibe source installation acceptance, 2026-09-16 {#root}

Accepted on Windows x64 with Vibe 1.0.0, Node 24.18.0, npm and the installed
Rust/MSVC toolchain. The test used an isolated Vibe settings directory; the
normal user installation, provider accounts and PATH were unchanged. No model
was called and no Zap binary release was published or downloaded.

The actual source installer materialized Lens 0.1.0, Zap 1.1.0 and their two
Vibe dependencies. It built Node/browser/Electron and compiled the Rust engine
from a cold target directory. Vibe then packaged and deployed all nine commands
as 18 Windows launcher files. Its subsequent deployment plan matched all 18
active ownership receipts.

An update switched to a private copy of the source registry and reused the same
validated immutable generation:
`4225cfc22dce0559ccdca747acc392b4f74f925fbce0d9a09cf52d2886ad5183`.
With that source registry temporarily unavailable, deployed Quick Lens and mock
agent help worked, `zap capabilities` returned JSON, and Quick Lens opened the
real onboarding and execution catalog in Chrome without page errors or agents.
The owned runtime processes were closed after inspection.

Uninstall, invoked from the retained installed source slot while the original
registry was unavailable, reversed all 18 receipt-owned targets. It preserved
the source slots, runtime cache, application state sentinel and unrelated file
in `opt/bin`. Read-only status then reported `undeployed`.

Verification also passed 14 tooling tests, the registered deterministic
`source-install.bootstrap-build-lifecycle` scenario, four corpus discovery
tests, production/test TypeScript, ESLint and formatting. Conform reported no
findings; specmap reported no suspects or orphans. The 303 npm lock URL changes
were independently compared with the prior lock: only the known mirror prefix
changed, with every version and integrity value preserved.

POSIX launcher generation has deterministic coverage; a real Linux/macOS
installation was not part of this acceptance. Node/npm and, for the default
engine installation, Rust plus the platform C++ toolchain remain prerequisites.
Use `--lens-only` to omit the engine. Operational commands and retained-source
management paths are in [the source installation guide](../SOURCE-INSTALL-GUIDE.md).
