# ZAP

ZAP is a Rust runtime and methodology for adaptive campaigns. It keeps intent,
obligations, uncertainty, decisions, lowering, agent packets, execution,
evidence, holds, and recovery in one typed durable history. The coordinator can
revise a route inside an Owner-defined envelope while exact authority and
economics gates control every live semantic effect.

Installing ZAP adds its specifications, three agent skills, and the `zap`
binary declaration to a VibeVM project. Installation does not create or select
a campaign, activate a charter, start a service, invoke a model, or migrate an
existing plan.

## Install and build

The package is `flow:org.vibevm.world/zap` version `1.0.0` under UPL-1.0. A
consumer needs a Vibe executable that actually supports `build` and `bin`, a
Rust toolchain, and the declared Rust AI Native dependency available through
its registry or materialized dependency cache. Check command support directly;
the Vibe display version alone is insufficient.

From an ordinary VibeVM project:

```text
vibe build --help
vibe bin --help
vibe install flow:org.vibevm.world/zap@=1.0.0 --offline
vibe bin build zap --assume-yes --offline
vibe bin list --offline
vibe bin exec zap -- capabilities
```

Supply the registry required by the project when the package is not already in
its configured registry. An offline build succeeds only when ZAP and every
declared dependency are available locally. `--assume-yes` is the explicit
consumer consent to compile package code and proc macros. Vibe compiles the
binary from the installed package slot; build output is outside the package source identity.
No production command needs Python, GitHub, an external database service, or a
VibeVM source checkout.

`zap capabilities` is a safe process-level probe. It reports the default
read-only machine surface. The capabilities returned by a configured running
application service are richer and derive from that service's actual command,
query, host, and adapter registrations.

## Native command surface

Run installed commands through Vibe so binary resolution stays bound to the
consumer lockfile:

```text
vibe bin exec zap -- capabilities
vibe bin exec zap -- STORE.redb REQUEST.json
vibe bin exec zap -- serve READ-CONFIG.json
vibe bin exec zap -- serve-runtime APPLICATION-CONFIG.json
vibe bin exec zap -- recover-service-lease APPLICATION-CONFIG.json
vibe bin exec zap -- archive-publish APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- archive-verify APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- archive-entry APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- import-legacy LEGACY-IMPORT-CONFIG.json
```

`zap STORE.redb REQUEST.json` opens an existing store and executes one typed
read request. `serve` exposes the authenticated loopback read service.
`serve-runtime` opens the protected application service and automatic
coordinator from an explicit configuration. `recover-service-lease` performs
the bounded stale-lease recovery check for that same configuration.
`import-legacy` imports an explicitly configured legacy archive into a new Rust
store and does not activate it.

The three archive commands publish a configured no-clobber portable bundle,
verify its exact manifest, or read one bounded typed entry from archive bytes
without a live campaign store. Publication uses the configured trusted channel;
verification and entry reads grant no authority.

The application service provides authenticated JSON endpoints for
capabilities, snapshots, event tails and streams, bounded queries, protected
commands, control, trusted observations, runtime steps and inspection, native
driver coordination, effect preparation, projected-record reads, and unknown
effect reconciliation. The server accepts loopback addresses only. Credentials,
material roots, workspace grants, capacity, native capabilities, worker
profiles, and archive limits come from protected configuration. Unknown fields
and unregistered operations are refused.

The binary does not call a native harness by itself. A cooperating host reads a
durable dispatch intent through the native-driver route, launches the exact
packet with its own capability, and returns the typed receipt. Dispatch intent,
external handle, transport completion, candidate verification, admission, and
acceptance remain separate durable states. Unsupported host capabilities do not
fall back to a subprocess or model provider.

## Campaign behavior

The [methodology](vibevm/vibespecs/flows/zap/ZAP-METHODOLOGY.xml) defines intent,
the Owner charter, work types, stages, facts, verification, parallelism, stops,
and completion. The
[adaptive cycle](vibevm/vibespecs/flows/zap/ZAP-ADAPTIVE-CYCLE.xml) recomputes
fog, value, remaining cost, and feasibility after material observations.

Lowering preserves every obligation and verification disposition while mapping
an outcome revision to executable work. Packet rendering is a pure capability
projection over that checked lowering. A packet records exact source material,
omissions, workspace grants, checks, lineage, and role; a generated goal is
reinforcement and never authority.

Dreamer branches are detached projections. Questions, answers, bounded
uncertainty, structural burden, and projected consequences remain saved without
changing live work, holds, or approvals. Promotion and removal use the normal
prepared-effect comparison, economics, Owner, pause, affected-job, and retry
gates. A charter-expanding dream requires the exact combined Owner decision.

Semantic alternatives are prepared read-only against one captured state.
Preparation derives basis, affected scope, sequential projected state, and
preflight evidence without consuming approval or writing. A selected change is
applied only through its immutable prepared envelope. At four attributable
hours or less it may be automatic when every other condition permits; above the
configured threshold, unknown material impact, charter expansion, pauses, and
stops retain their explicit decision paths.

## Operation and recovery

Use [zap-draft](vibevm/vibespecs/skills/zap-draft/SKILL.md) to prepare or revise
a campaign without activation, [zap-state](vibevm/vibespecs/skills/zap-state/SKILL.md)
for bounded inspection and preparation, and
[zap-run](vibevm/vibespecs/skills/zap-run/SKILL.md) for an explicitly configured
service and cooperating native driver.

Every mutation binds the store identity, revision, payload, relevant basis,
affected scope, policy, authority, and registered effect contract. Retrying the
same exact command is idempotent. A stale command requires rereading and a new
decision. Unknown external effects reconcile by exact command identity and
receipt; they are never silently relaunched. Snapshots and event cursors bind a
committed boundary. Cold reopen restores persisted state; a replay-based audit
separately checks projections against the registered semantics.

Source and workspace adapters accept only explicitly configured roots. Packet
material is captured and verified by digest. Portable returned bundles use a
bounded archive format and contain no authority credential. Credentials,
private stores, service leases, endpoint files, logs, packets, and captured
artifacts stay outside the package payload.

## Rust integration guides

These guides describe actual constructors, acquisition paths and returned
states. Data construction remains separate from trusted admission.

| Integration surface | Guide |
| --- | --- |
| Typed storage, authority and execution contracts | [Core](vibevm/vibespecs/flows/zap/ZAP-RUST-CORE-GUIDE.md) |
| Campaign, proof, economics and planning operations | [Domain](vibevm/vibespecs/flows/zap/ZAP-RUST-DOMAIN-GUIDE.md) |
| Machine requests and responses | [Machine API](vibevm/vibespecs/flows/zap/ZAP-RUST-MACHINE-GUIDE.md) |
| Service, transport and material adapters | [Application](vibevm/vibespecs/flows/zap/ZAP-RUST-APP-GUIDE.md) |
| Scheduling, dispatch and recovery | [Runtime](vibevm/vibespecs/flows/zap/ZAP-RUST-RUNTIME-GUIDE.md) |
| Transactions, history and artifacts | [Store](vibevm/vibespecs/flows/zap/ZAP-RUST-STORE-GUIDE.md) |
| Frozen codecs and migration input | [Legacy](vibevm/vibespecs/flows/zap/ZAP-RUST-LEGACY-GUIDE.md) |

## Scope

The [implementation and evidence notes](vibevm/vibespecs/flows/zap/ZAP-RUST-IMPLEMENTATION-NOTES.md)
describe verified mechanisms, migration limits, external-host evidence and
process boundaries.

ZAP ships the Rust service and machine interface. A future strategy-map canvas
and IDE client are separate consumers. The historical Python prototype remains
research evidence in the source repository and is excluded from the normal
package payload and execution path. Local Qwen inference is not invoked or
probed by installation, build, or these commands.

Permanent contracts:

- [Runtime](vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml)
- [Rust storage](vibevm/vibespecs/flows/zap/ZAP-RUST-STORAGE.xml)
- [Agent protocol](vibevm/vibespecs/flows/zap/ZAP-AGENT-PROTOCOL.xml)
- [Change economics](vibevm/vibespecs/flows/zap/ZAP-CHANGE-ECONOMICS.xml)
- [Lowering and Dreamer](vibevm/vibespecs/flows/zap/ZAP-LOWERING-AND-DREAMER.xml)
- [Data and future viewer](vibevm/vibespecs/flows/zap/ZAP-DATA-AND-VIEWER.xml)

License: [UPL-1.0](LICENSE.md).
