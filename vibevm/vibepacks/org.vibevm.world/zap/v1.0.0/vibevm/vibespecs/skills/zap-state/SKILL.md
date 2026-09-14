---
name: zap-state
description: Inspect, query, prepare and reconcile an isolated ZAP campaign through its typed Rust command surface.
---

# ZAP state

Use the package-declared Rust binary through the current project's Vibe lock:

```text
vibe bin build zap --assume-yes --offline
vibe bin exec zap -- capabilities
vibe bin exec zap -- STORE.redb REQUEST.json
vibe bin exec zap -- serve READ-CONFIG.json
```

The build flag is explicit consent to compile installed package code. First run
`vibe bin list --offline` when binary availability is uncertain.
`capabilities` is a safe process probe and may expose only the default read
surface. Query a configured application service's `/v1/capabilities` endpoint
for the exact live registry. Do not infer support from a desired profile or a
Vibe version string.

The one-shot form accepts a strict `MachineRequest` JSON object. Useful read
kinds include `capabilities`, `snapshot`, `events`, and `query`. Responses bind
the store identity, revision, cursor, completeness, and continuation where
applicable. A stale page or event cursor is an explicit conflict or gap.
Opening a view performs no semantic judgment or model call.

Use the protected application service for `prepare_effect_bundle`,
`prepare_effect_comparison`, `prepare_projected_record`, and `reconcile`.
Preparation is read-only: it derives registered basis, affected scope,
sequential projected state, item digests, and preflight evidence from one
captured store boundary. Preserve the returned immutable payload and item
digests through assessment and approval. Never guess a before or after basis,
recanonicalize an approved payload, or treat preparation as admission.

Dreamer exploration uses the same read-only preparation boundary. Keep a
hypothetical branch, grill questions and answers, declared uncertainty,
structural burden, and projected consequences detached. Promote or remove it
only through the returned comparison and ordinary economics, Owner, pause,
affected-job, and effect gates. A detached branch does not become work or hold
scope merely because it exists.

Read current state before proposing a mutation. Exact retry uses the identical
command identity and bytes. If the revision or relevant basis changed, reread
and reconsider instead of overwriting. Unknown effects use the reconciliation
request with the original command ID and digest.

Keep stores, reader credentials, service files, packets, archives, source
captures, and workspace roots outside the installed package slot. Credentials
belong in protected files and never in request JSON, argv, packets, events, or
logs. Installation and import do not activate a campaign or start work.

For semantics, read the package's `ZAP-RUNTIME.xml`, `ZAP-RUST-STORAGE.xml`,
`ZAP-DATA-AND-VIEWER.xml`, and `ZAP-CHANGE-ECONOMICS.xml` permanent contracts.
