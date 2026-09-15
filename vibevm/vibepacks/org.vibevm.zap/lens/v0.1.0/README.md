# Lens

`org.vibevm.zap/lens` is the shared client and agent-integration package for ZAP.
Its npm identity is `@org.vibevm.zap/lens`. Product entry points share one
implementation:

- **quicklens** — the practical browser/Electron client, using Qwik 2, Sigma.js
  and Graphology;
- **gamelens** — the future game presentation;
- **codlens** — integration with coding agents.

The current 0.1 development slice provides the headless `lens/1` communication
foundation. Quicklens graphics and admitted ZAP plan-control workflows are the
next layer; a broker receipt does not mean a plan was executed.

## Background communication

One local broker owns a durable SQLite journal. Agent MCP processes connect to
that service through authenticated loopback HTTP. A parent and its nested
subagents have separate addressed inboxes even when they use separate MCP
processes. Questions return immediately and remain pending until answered,
cancelled or expired. Ordinary agent conversation can continue in parallel.

Stable request IDs deduplicate retries. Reconnecting fences stale bindings;
observation cursors and actor acknowledgements are separate. Stage 0.1 retains
the full journal and performs no automatic pruning. Invalid continuation
boundaries require resynchronization rather than silently skipping history.

MCP itself provides tool calls and durable inbox reads. Host hooks or supported
native channels can deliver at safe points; idle wake depends on the host,
version and session configuration. See the [host integration instructions](integrations/README.md).

## Build and run

Node.js 24 or later is required. From this package directory:

```text
npm ci
npm run verify
```

`verify` checks formatting, strict types, tests, lint and the production build.
The compiled command entry is `dist/cli.js`; `dist/mcp.js` is the MCP stdio
connector. An installed npm package also provides `codlens` and `codlens-mcp`.

For a first local setup in PowerShell:

```powershell
$lensDataDir = Join-Path $env:LOCALAPPDATA 'ZAP/lens'
$env:CODLENS_DATABASE_PATH = Join-Path $lensDataDir 'lens.sqlite'
$env:CODLENS_CREDENTIAL_FILE = Join-Path $lensDataDir 'credentials.json'
$env:CODLENS_URL = 'http://127.0.0.1:32191'

node ./dist/cli.js setup '{"workspaceId":"workspace.example","conversationId":"conversation.example"}'
node ./dist/cli.js start
```

Choose the workspace/conversation scope at setup. Setup creates protected local
credentials without printing their values and refuses to overwrite an existing
credential file. Keep the broker in its own process; agent sessions connect
independently. The example scopes are test names, not discovered project data.

Configure the agent's MCP server to run `node <installed-package>/dist/mcp.js`,
with `CODLENS_URL` and `CODLENS_CREDENTIAL_FILE` inherited from its trusted
environment. The model receives addressed handles, not the broker's credential
material. A handle alone cannot authenticate an HTTP operation.

The [Codex plugin source](integrations/plugins/codlens/.codex-plugin/plugin.json)
contains MCP, hook and skill integration. When copying it to a plugin cache,
set `CODLENS_CLI_PATH` to the installed package's real `dist/cli.js`.
Host configuration templates use placeholders and require the matching local
service/session configuration.

## Shared implementation boundary

Explicit package exports include `./protocol`, `./broker`, `./transport`,
`./http`, `./mcp`, and `./host-adapters`. There is no universal root export
that pulls Node services into a browser bundle. The future renderer and
Electron shell use separate platform boundaries and build outputs.

The planning engine is the separate `org.vibevm.zap/zap` package. Lens uses its
public interfaces rather than importing Vibevm's implementation. Both packages
currently remain in the Vibevm repository; extraction into a separate product
repository is deferred.

Ordinary chat and graphical controls can both initiate planning intent. The
domain layer must preserve existing scoped authority, reject stale plan/source
previews, and reconcile affected in-flight results. These requirements are
specified in [the client contract](vibevm/vibespecs/PROP-002.xml); they are not
implemented by the message broker alone.

Contracts: [communication](vibevm/vibespecs/PROP-001.xml),
[design and verified host constraints](vibevm/vibespecs/research/AGENT-LENS-PROTOCOL-2026-09-15.md).

License: [UPL-1.0](LICENSE.md). Author: Oleg Chirukhin.
