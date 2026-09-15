# Zap Quick Lens

`org.vibevm.zap/lens` is the shared client and agent-integration package for ZAP,
presented to users as Zap Quick Lens.
Its npm identity is `@org.vibevm.zap/lens`. Product entry points share one
implementation:

- **Zap Quick Lens** — the practical browser/Electron client, using Qwik 2,
  Sigma.js and Graphology;
- **gamelens** — the future game presentation;
- **codlens** — integration with coding agents.

The current 0.1 development slice provides the headless `lens/1` communication
foundation and one browser-safe Zap Quick Lens renderer for browser and Electron.
The renderer exposes plan intent, preview, apply, reconcile and exact Owner
decision ports. Zap Wayfinder owns the configured ZAP workflow and journals;
agent MCP processes are authenticated proxies into that shared runtime. Dynamic
milestone create/revise preparation and successor adoption use one durable
ordered assessment. A broker receipt does not mean a plan was executed.

The authenticated workspace renderer accepts a separately paired loopback
Wayfinder gateway through `?workspace-gateway=<scoped-loopback-url>#workspace-pair=<one-time-token>`.
Electron uses `QUICKLENS_WORKSPACE_GATEWAY_URL` and
`QUICKLENS_WORKSPACE_PAIRING_TOKEN`; these workspace credentials stay in the
trusted main process.

## Background communication

The local central service is presented as Zap Wayfinder. It owns a durable
SQLite journal, and agent MCP processes connect through authenticated loopback
HTTP. A parent and its nested
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

### Zap Quick Lens renderer

Build the one Qwik 2 renderer and both platform shells:

```text
npm run build:quicklens
```

Run the browser development shell at `http://localhost:4173/?demo=1`:

```text
npm run dev:quicklens
```

Serve an existing production browser build at
`http://localhost:4174/?demo=1` without loading Qwik's SSR preview hooks:

```text
npm run preview:quicklens
```

Run the built Electron shell with the explicitly labelled synthetic fixture:

```text
npm run quicklens:demo
```

Omit `?demo=1` or `--demo` for production composition. Without an injected
`QuicklensDataSource`, the renderer shows an unavailable state rather than
displaying synthetic data as live planning state. The data-source seam is the
browser-safe `@org.vibevm.zap/lens/quicklens-model` export; Node-only ZAP and
credential adapters stay behind the browser/Electron composition boundary.

The default **Goal structure** view centers only a source-identified active
outcome and places resolved adopted milestones and known decomposition outward.
The **Work order** view uses only verified prerequisite-to-dependent relations
for its left-to-right steps. Known prerequisites establish no order within one
stage; resources and holds remain independent, and supporting objects remain an
unordered context band. Missing members, partial relations
and prerequisite cycles are called out in the graph rather than assigned a
plausible-looking order. Search and filters reuse positions computed from the
full snapshot, so hiding an earlier step cannot manufacture a new start.

A trusted Node composition can expose that source with
`createQuicklensGateway` from `@org.vibevm.zap/lens/quicklens-service`. Give
each gateway a distinct namespace; its `start({host, port})` result supplies the
actual loopback port and namespaced base path. Serve the browser
renderer on the same loopback hostname, then open its production URL with the
gateway URL in `?gateway=` and the generated one-time pairing token in
`#pair=`. For example:

```powershell
$env:QUICKLENS_CONFIG_FILE = 'C:\protected\quicklens.json'
npm run start:quicklens-service
```

The service prints its public loopback address record without printing the
pairing token or underlying broker/ZAP credentials. Use that returned address
in the renderer launch URL. For example:

```text
http://localhost:4174/?gateway=http%3A%2F%2Flocalhost%3A43100%2Fquicklens%2Fbrowser#pair=<one-time-token>
```

The browser exchanges the token before its first read, removes the fragment,
and subsequently uses the gateway's HttpOnly, SameSite=Strict cookie. It never
reads the cookie value. The trusted Electron main process uses the same gateway
without passing pairing material through the preload bridge:

```powershell
$env:QUICKLENS_GATEWAY_URL = 'http://127.0.0.1:43100/quicklens/electron'
$env:QUICKLENS_PAIRING_TOKEN = '<one-time-token>'
electron dist/quicklens/electron/main.js
```

### Authenticated web profile

The remote web profile is a separate entry and configuration. It never exposes
the broker, MCP, raw ZAP routes, shell, or a general proxy. Initialize or rotate
its password verifier from a trusted terminal; the command reads the password
without accepting it as a command-line argument and defaults to
`~/.vibe/zap/quicklens-web-auth.json`:

```text
quicklens-web-auth init
quicklens-web-auth rotate
```

Set `QUICKLENS_WEB_CONFIG_FILE` to the strict `quicklens-web/1` configuration,
then start `quicklens-web` or run:

```text
npm run start:quicklens-web
```

This listener still binds loopback. Its configured `publicOrigin` must be an
HTTPS origin. A separately managed tunnel may forward only this listener and
must inject the configured `X-Quicklens-Proxy-Proof` while setting exact
`X-Forwarded-Proto: https` and `X-Forwarded-Host` values. Requests without that
proof are refused, including direct loopback requests. The start record prints
the public `/?web=1` URL without printing the proxy proof, password, session,
CSRF value, or backend credentials.

The web gateway applies its configured `viewer`, `operator`, or `owner` UI role
to an explicit allowlist of named operations. Password authentication does not
create agent, Coordinator, or Owner authority. Sessions are server-side,
expiring and revocable; their cookie is Secure, HttpOnly and SameSite=Strict.
Password rotation is reloaded by the running gateway and invalidates sessions
bound to the previous verifier. This package does not install or launch a
tunnel, publish a hostname, or modify firewall settings. The implementation
follows the [OWASP session](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html),
[authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html),
[password storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html),
and [CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
guidance.

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
`./http`, `./mcp`, `./host-adapters`, `./quicklens-model`,
`./quicklens-service`, `./specification-watch`, and `./zap-client`. There is no
universal root export that pulls Node services into a browser bundle. The
renderer and Electron shell use separate platform boundaries and build
outputs.

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

## Operator guide

The current local Zap Wayfinder, Zap Quick Lens, Codlens, project/context,
channel-separation, lifecycle, model-policy and protected-web guidance lives
in [WAYFINDER-GUIDE.md](vibevm/vibespecs/WAYFINDER-GUIDE.md). It records the
current setup fields and capability limits, including which paths remain
configuration or integration work.
