# ZAP product group

Active ZAP development lives in this group so the product can later move into
its own repository. That repository move has not happened.

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
Published `org.vibevm.world/zap@1.0.0` is historical compatibility; its release
identity and prior verification receipts are preserved.
