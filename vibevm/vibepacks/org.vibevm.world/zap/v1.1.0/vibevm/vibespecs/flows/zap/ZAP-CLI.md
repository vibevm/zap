# ZAP CLI

ZAP ships one Rust binary through the package's `[[binary]]` declaration. In an
installed VibeVM project, resolve and run it through the project lockfile:

```text
vibe bin build zap --assume-yes --offline
vibe bin exec zap -- capabilities
```

`--assume-yes` is explicit consent to compile package code. Offline use requires
ZAP and its declared dependencies to be present in the selected registry or
materialized cache. The binary itself has no Python, GitHub, model-provider, or
source-checkout dependency.

With no arguments, `zap` prints the exact command synopsis. A successful command
writes one JSON response when it has a response body. A failure writes a typed
`ZapError` JSON object to stderr and exits 2. JSON inputs reject unknown fields.

## Commands

```text
zap capabilities
zap STORE.redb REQUEST.json
zap serve READ-CONFIG.json
zap serve-runtime APPLICATION-CONFIG.json
zap recover-service-lease APPLICATION-CONFIG.json
zap archive-publish APPLICATION-CONFIG.json REQUEST.json
zap archive-verify APPLICATION-CONFIG.json REQUEST.json
zap archive-entry APPLICATION-CONFIG.json REQUEST.json
zap import-legacy LEGACY-IMPORT-CONFIG.json
```

`capabilities` is a process-level safe probe. It reports the default read-only
surface. It does not open a campaign and must not be used to infer the richer
registries of a configured application service.

`STORE.redb REQUEST.json` opens an existing store through `ReadApplication` and
executes one strict `MachineRequest`. Supported read kinds are `capabilities`,
`snapshot`, `events`, and `query`. Service, runtime, archive and mutation
requests require `serve-runtime` or their dedicated command.

`serve READ-CONFIG.json` starts the authenticated read server. The strict
configuration supplies an existing store, loopback address, reader credential
ID/file, request/response bounds, connection limit, event page size, and I/O
timeout.

`serve-runtime APPLICATION-CONFIG.json` starts the protected application
service, automatic coordinator, native-driver rendezvous, and read server. The
configuration binds create/open store mode, one immutable service instance,
endpoint and lease records, packet-capture directory, filesystem material and
workspace adapters, portable archive limits, native capabilities, worker
profiles, Owner/coordinator/data/trusted channels, capacity, and bounded request
lifetime. Installation never starts this service.

`recover-service-lease` checks the same application configuration. It removes
only an exact stale instance-bound endpoint/lease pair after proving that its
process is unavailable and the observed bytes did not change. It does not
repair store history or decide an unknown external effect.

`archive-publish` accepts a strict `BundleArchiveRequest`, opens the configured
service, authenticates through its protected trusted channel, and publishes a
no-clobber `ZAPBNDL2` archive. `archive-verify` verifies the named archive and
manifest without recapturing live campaign material. `archive-entry` accepts a
`BundleEntryReadRequest` and returns one bounded typed `ZAPENTRY2` entry from
archive bytes alone. Archive verification and entry reads grant no authority.

`import-legacy` accepts a `LegacyImportConfig`, preserves supported zap/1 source
bytes and mappings under the frozen legacy codec, and writes the current Rust
store epoch. The destination must be new or exactly resumable. Import does not
activate a charter or dispatch work.

## Machine requests and exact retry

`MachineRequest` is tagged by `kind`. The application surface includes:

- `capabilities`, `snapshot`, `events`, and bounded `query`;
- `command`, `control`, `agent`, and `observation` with canonical protected
  commands;
- `runtime_step`, bounded `runtime_run`, and `runtime_inspect`;
- `native_driver` pending-intent, exact prepare-launch, restore, atomic spawn
  outcome, bounded retry release, job-observation, and candidate operations;
- `prepare_effect_bundle`, `prepare_effect_comparison`, and
  `prepare_projected_record` read-only preparation;
- `publish_bundle_archive`, `verify_bundle_archive`, and `read_bundle_entry`;
- command `reconcile` by exact command ID and digest.

A new mutation binds the store identity, base revision, command identity,
canonical payload, relevant basis, affected scope, policy, authority, and the
registered effect contract. Retrying the identical command is idempotent. A
changed digest under the same identity or stale revision refuses. When a
protected HTTP submission exceeds its response timeout, the service returns an
`Unknown` status with the exact command ID/digest while processing may continue;
call `reconcile` before considering another submission.

Effect preparation performs no write, approval consumption, model call or
external effect. It derives each registered effect's local before/after basis,
affected scope/job evidence, sequential projected state and item digest on one
captured snapshot. Preserve the returned request and payload bytes through
assessment and approval. NoOp has its own exact comparison request.

## Runtime and native host

Runtime step/run always rechecks current charter, policy, economics, pauses,
holds, affected safe jobs, resources, packet lineage, evidence and completion.
The binary does not launch a native harness. A cooperating host reads durable
pending intents, requests an exact prepared launch, invokes only a measured
compatible capability, and returns the handle and typed receipts through the
protected service. Unsupported native or goal support remains unavailable or
manual; it never silently becomes a subprocess or model fallback.

Native recovery requests bind exact JobId/DispatchId. A spawn outcome also
binds a stable observation CommandId, consumed authorization revision and
observation time. Known refusal and response loss are distinct. The former can
create a bounded retry; the latter remains Unknown and cannot relaunch. Slot
availability is a separate trusted observation and is never inferred from a
terminal task status. Restore and result collection use the persisted receipt
and handle without authorizing or consuming another launch.

Credentials are opaque bytes in protected files. Never place them in request
JSON, command payloads, packets, events, argv, worker environment or logs. Store,
endpoint, lease, packet, material, workspace, archive and artifact locations are
runtime configuration outside the installed source slot.

Material-adapter paths are resolved relative to the supplied configuration
directory. Store, endpoint, lease, credential and packet-capture paths are used
as configured; supply absolute paths when the launch directory can change.
The optional Vibe query adapter runs its explicitly configured executable with
an exact project path, offline/agent flags, an output bound and a timeout. That
child process inherits the host environment and working directory. Its capture
is bounded evidence, not a hermetic execution environment or a worker launcher.

See [the backend contract](ZAP-BACKEND-API.md),
[the runtime contract](ZAP-RUNTIME.xml), and
[the agent protocol](ZAP-AGENT-PROTOCOL.xml).
