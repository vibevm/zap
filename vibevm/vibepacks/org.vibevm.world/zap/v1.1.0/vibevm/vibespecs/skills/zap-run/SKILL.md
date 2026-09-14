---
name: zap-run
description: Operate ZAP's protected Rust service with cooperating native drivers, durable receipts and explicit authority.
---

# ZAP runtime

Run the installed Rust binary through Vibe:

```text
vibe bin build zap --assume-yes --offline
vibe bin exec zap -- serve-runtime APPLICATION-CONFIG.json
vibe bin exec zap -- recover-service-lease APPLICATION-CONFIG.json
vibe bin exec zap -- archive-publish APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- archive-verify APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- archive-entry APPLICATION-CONFIG.json REQUEST.json
vibe bin exec zap -- import-legacy LEGACY-IMPORT-CONFIG.json
```

`serve-runtime` is the explicit start boundary. Its strict configuration binds
one store, store create/open mode, endpoint and lease files, packet capture
directory, material and workspace adapters, native host capabilities, worker
profiles, trust channels, capacity, limits, and an authenticated loopback
server. Keep the configuration and every credential outside worker-writable
roots. Installation alone starts nothing.

The application service exposes protected routes for data commands, Owner and
coordinator control, trusted observations, effect preparation, runtime
step/run/inspect, native driver work, projected-record reads, and unknown-effect
reconciliation. Use the endpoint file written by the service instead of
guessing its selected port. Every request carries the credential ID in the
authorization scheme expected by the server and the opaque credential in the
protected authorization header. Do not copy credential bytes into request
bodies, argv, packets, events, receipts, or logs.

Run one runtime step when you need a bounded coordinator action. A runtime run
has an explicit maximum step count. Recheck campaign and scoped pauses, pending
economics, affected holds, capacity, safe-job state, packet lineage, and exact
authority before each live action. A successful transport is not accepted
work; verification, producer/acceptor separation, applicable evidence, and the
registered product effect still decide admission.

## Cooperating native driver

The `zap` binary does not invoke Codex or another harness itself. A cooperating
host uses the native route to:

1. list bounded pending dispatch intents;
2. request the exact prepared launch for one dispatch identity;
3. launch only when its measured capabilities match the packet;
4. persist the external handle and typed receipt through the protected service;
5. reconcile an uncertain launch by that same dispatch identity.

Do not silently fall back to a subprocess, launcher, another model, or local
inference when native support is unavailable. Goal application is similarly
capability driven: record applied, manual, or unavailable truthfully. A manual
goal is an instruction for the Owner or host, not a claim that the runtime set
client state.

Packet source material and workspace effects pass through configured filesystem
adapters with exact roots, relative paths, digests, operations, size bounds, and
reparse-point checks. Returned offline bundles use the bounded portable archive
adapter. They carry lineage and results but no authority credential. Verify an
archive before importing it and let strong-side reassessment decide whether its
observations and candidates remain applicable.

Use `archive-publish` with a strict bundle request and the configured trusted
channel. Use `archive-verify` before transfer or import, and `archive-entry` for
bounded archive-only inspection. These commands use the production `ZAPBNDL2`
and typed `ZAPENTRY2` codecs; they never re-create missing live authority.

`recover-service-lease` removes only a provably stale lease for the exact
configuration. It does not repair store history or infer that an external
effect failed. After interruption, inspect durable runtime state first, then
reconcile unknown dispatch or commit effects. Reuse exact receipts on retry.

`import-legacy` is a separate, inert migration operation. It preserves legacy
bytes and identities under the legacy codec while writing the current Rust
store epoch. It neither activates a charter nor launches work.

For semantics, read `ZAP-RUNTIME.xml`, `ZAP-AGENT-PROTOCOL.xml`,
`ZAP-RUST-STORAGE.xml`, and `ZAP-LOWERING-AND-DREAMER.xml` in the installed
package.
