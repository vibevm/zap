# ZAP product group

Zap is maintained in its standalone product repository. Both shipped packages
use the mutable 1.0.0 release line on branch `1.0`.

| Package | Responsibility |
| --- | --- |
| `org.vibevm.zap/zap` | Planning engine, durable state, admission, queries and execution |
| `org.vibevm.zap/lens` | Shared agent communication, host integrations and client family |

Lens is one TypeScript package, with `quicklens` (browser/Electron), `gamelens`
(future game presentation), and `codlens` (agent plugin) entry points. Shared
code lives in explicit internal cells and exported subpaths. Privileged Node
and Electron code must not enter the browser dependency graph. Product names
are not separate versions of the protocol or copies of the planning model.

The engine and lens communicate through public APIs. They do not import
Vibevm's implementation. Tooling dependencies remain explicit and replaceable.
The predecessor `org.vibevm.world/zap@1.0.0` source is historical compatibility
material under `archive/legacy-zap-v1.0`, outside this registry. Its release
identity and prior verification receipts remain historical evidence.
