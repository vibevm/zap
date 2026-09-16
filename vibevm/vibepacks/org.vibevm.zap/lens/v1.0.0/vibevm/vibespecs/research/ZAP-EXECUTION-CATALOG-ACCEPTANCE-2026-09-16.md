# Execution catalog and local preview acceptance — 2026-09-16

This extends the [local product baseline](ZAP-PRODUCT-ACCEPTANCE-2026-09-16.md)
with PROP-015. The shared catalog is implemented in Wayfinder; Quick Lens uses
the same public operations and resolver as agent-initiated managed work.

## Accepted behavior

- A fresh workspace can add a protected account connection and a server-named
  model configuration before any project exists. Exact configurations can be
  selected for coordinator registration and specialized managed tasks.
- Several accounts and models can serve one project. Distinct Codex and Claude
  homes isolate logins. Existing configured provider/model identifiers survive
  catalog import, including qualified OpenCode and OpenRouter-backed Qwen IDs.
- The owner can edit names, allowed effort/context choices, specialization
  priorities, economy/quality preference and subscription threshold. Readonly
  clients cannot mutate these settings or initiate account observation.
- Codex model choices and effort values use the selected binding's observed
  app-server model list. They are not copied from another account or inferred
  solely from the API reference. Windows Claude discovery resolves its npm
  command shim to the installed native executable without a shell fallback.
- Actual selection, configuration name, account binding, model, applied
  effort/context and decision basis are pinned. New process starts and resumes
  recheck revoked permissions without silently switching assignments.
- Named coordinator profiles survive a cold restart. A disabled configuration
  leaves the project and history readable while refusing a new launch.
- Quota refresh reads the selected Codex binding without inference. It retains
  actual bucket/window/reset information and leaves unknown values unknown.
  Editable UI drafts survive refresh; semantic retries preserve their original
  result rather than recomputing a new selection.

## Independent product evidence

The actual browser exercised an empty workspace, account/configuration creation,
renaming, slider and quota editing, effort restriction, specialization-score
editing, real quota refresh with unsaved changes, save/reload and first-project
registration by configuration identity. The light and dark views were inspected.
The test started no agent. Slider verification uses actual keyboard interaction;
Playwright's range `fill` did not reproduce the browser event path reliably.

A separately paired HTTP client with `catalogAdministrator=false` could read
the catalog but received `forbidden` for preferences, account creation and quota
refresh, even with those operations in its generic action allowlist. The
catalog snapshot remained unchanged. A principal name beginning with
`principal.web` supplied no administration authority.

The synthetic service scenario covers ten model families, two accounts of one
provider, specialization, slider endpoints, quota freshness, authorization,
renaming, replay/reopen and pinned selections. The actual runtime scenario goes
through authenticated HTTP, catalog selection, a real managed terminal,
ZapMock MCP question/answer/report and natural process exit. Synthetic identities
remain `zap_mock` / `zap-mock/deterministic-v1`; no real credentials or inference
are used by those scenarios.

The complete registered corpus passed **16/16** with zero LLM inference. Its
inventory still exposes **60 coverage gaps**; this is not a claim that every
public operation has a standalone simulation.

Two tiny live coordinator checks then passed through newly created named
configurations: **Codex with gpt-5.6-luna/low**, and **Claude Code with
claude-haiku-4-5-20251001/provider default**. Each read a tiny project boot,
returned the requested short reply and completed Stop with no remaining owned
process. Model selection was observed at the provider boundary. No comparative
model-quality pilot or expensive-model inference was performed. An earlier
Claude attempt failed at Windows process creation before any provider/model
process; its shim fix was verified without inference before the successful run.

## Delivery boundary

The final source gate passed all five TypeScript configurations, formatting,
lint, browser import boundaries, the Node/browser/Electron production builds,
**296 Node tests with zero failures and one explicit external Rust-fixture
skip**, two tooling tests and 24 renderer tests. The registered mock corpus is
reported separately above; passing it does not erase its coverage inventory.

The shareable local preview includes the browser and Electron clients. The
Windows portable distribution includes Node and production dependencies, with
their licenses; agent CLIs and Git are external prerequisites. Account homes,
settings, databases, pairing material and developer receipts are excluded.

Codex and Claude default logins are discovered locally. OpenCode and Qwen use
protected configured profiles; their exact model identifiers are preserved.
The reference catalog is an editable heuristic, not ten newly implemented
provider drivers or a performance ranking. Subscription observations for other
agents remain unavailable unless a verified adapter supplies them.

The Sol image-task preference is implemented and simulated. Real image dispatch
still requires a trusted execution path with an actual image-generation tool;
ordinary CLI discovery does not attest that capability. Image-only API model
names are not accepted as coding conversation models.

Authoritative planning-engine actions require a configured planning source.
Pending sources remain explicit. Remote hosts, multiple human users,
crowdsourced capacity, Gamelens, IDE shells and public tunnel deployment remain
outside this local preview. These boundaries are also stated in the
[first-run guide](../QUICK-START.md).
