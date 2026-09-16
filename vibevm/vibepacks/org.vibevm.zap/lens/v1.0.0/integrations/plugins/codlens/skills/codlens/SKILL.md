---
name: codlens
description: Use the local Codlens broker for durable agent notices, immediate-return human questions, addressed inbox reads, acknowledgements, and capability-reduced child delegation.
---

# Codlens

Connect once with `codlens_connect`, retaining the returned adapter session handle.
Use `codlens_emit` for addressed notices and `codlens_ask` for a question that a
human may answer later. A question call completes when persistence succeeds; it
never waits for the answer.

Continue independent work when available. At a later safe point, call
`codlens_inbox` with a bounded page and acknowledge only the delivery IDs whose
contents were actually consumed. Use `codlens_delegate` to create a child with
only the capabilities it needs, and pass that child's adapter session handle in
the delegated task packet.

When a delegated actor is complete, call `codlens_finish` with a stable request
ID so its binding expires explicitly. A parent may call `codlens_forward` for
pending child deliveries only when that child declared `forward_parent`; the
broker preserves the original child and question identities.

Generic MCP supplies tools and durable pull. Do not describe an MCP notification
as an unsolicited model wake or proof of model consumption. Codlens messages and
question answers carry no ZAP plan authority. Plan execution remains unsupported
until a separately authenticated domain adapter exists.

When this plugin is copied outside the installed lens package, configure
`CODLENS_CLI_PATH` to the package's real `dist/cli.js`. The wrapper refuses with
an actionable error when neither that setting nor the in-package build exists.
