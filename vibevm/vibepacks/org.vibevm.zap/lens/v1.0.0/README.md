# Zap Quick Lens

Start with the [first-run guide](vibevm/vibespecs/QUICK-START.md) to install a
release, connect an existing agent account and add your first project. The
[operator guide](vibevm/vibespecs/WAYFINDER-GUIDE.md) covers protected host
settings and advanced integrations. To build a user-local application directly
from a local Vibe registry, use the
[source-install guide](vibevm/vibespecs/SOURCE-INSTALL-GUIDE.md).

`org.vibevm.zap/lens` is the shared client and agent-integration package for ZAP,
presented to users as Zap Quick Lens.
Its npm identity is `@org.vibevm.zap/lens`. Product entry points share one
implementation:

- **Zap Quick Lens** — the practical browser/Electron client, using Qwik 2,
  Sigma.js and Graphology;
- **gamelens** — the future game presentation;
- **codlens** — integration with coding agents.

The 1.0 release provides the durable `lens/1` communication
foundation, normal empty-workspace startup, project registration, explicit
coordinator launch, a unified multi-project workspace, managed work and the
browser-safe Zap Quick Lens renderer for browser and Electron. The renderer
exposes plan intent, preview, apply, reconcile and exact Owner decision ports.
Zap Wayfinder owns the configured ZAP workflow and journals; agent MCP processes
are authenticated proxies into that shared runtime. Dynamic milestone
create/revise preparation and successor adoption use one durable ordered
assessment. A broker receipt does not mean a plan was executed.

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

MCP itself provides tool calls and durable inbox reads. `codlens_inbox_wait`
offers an authenticated bounded wait for an idle worker; it returns without
acknowledging a delivery or granting approval. Host hooks or supported native
channels can deliver at safe points. There is no universal promise that every
provider can wake a stopped or busy model process; wake behavior still depends
on the host, version and session configuration. See the
[host integration instructions](integrations/README.md).

## Build and run

Node.js 24 or later is required. From this package directory:

```text
npm ci
npm run verify
```

`verify` checks formatting, strict types, tests, lint and the production build.
The Node suite caps concurrency at four so Windows PTY and disposable-process
fixtures do not saturate ConPTY resources; every assertion and timeout remains active.
The compiled command entry is `dist/cli.js`; `dist/mcp.js` is the MCP stdio
connector. An installed npm package also provides `codlens` and `codlens-mcp`.

The normal native installation command is:

```text
vibe install -g org.vibevm.zap/zap
```

Use `vibe update -g org.vibevm.zap/zap` and
`vibe uninstall -g org.vibevm.zap/zap` for the same user application. This
requires a Vibe version that implements the global application dispatcher; an
older installed Vibe CLI cannot interpret this new declaration retroactively.

The advanced local-registry bootstrap remains available as
`node tooling/install-source.mjs`. It is intended for source development,
isolated acceptance and an explicit registry override. It materializes pinned
packages into a dedicated marked host, builds an immutable runtime generation
and deploys receipt-owned launchers. It does not modify `PATH` or provider
credentials. See the
[source-install guide](vibevm/vibespecs/SOURCE-INSTALL-GUIDE.md).
Use `--npm-registry <credential-free-http(s)-url>` only when the build must
override the registry already selected by npm configuration.
On Windows, the default engine build requires Visual Studio Build Tools with
Desktop development with C++ and a Windows SDK; `--lens-only` omits that engine
build. After moving the original checkout, status and uninstall remain
available from the retained Lens slot under
`<settings>/opt/apps/zap/vibevm/vibedeps/org.vibevm.zap.lens/1.0.0/tooling/install-source.mjs`.
Advanced-bootstrap updates still require `--registry` pointing at an existing
local registry.

### Deterministic ZapMock gate

`zap-mock-agent` is a deliberate synthetic test agent with identity
`zap-mock/deterministic-v1`. It uses no provider credentials, remote inference,
or provider fallback. Run the bounded no-model gate with:

```text
npm run test:mock
npm run simulate:mock -- --list
npm run simulate:mock -- --id model.full-lifecycle --repeat 3 --seed review-seed
npm run simulate:mock -- --coverage
zap-mock-agent --help
```

The simulation runner is a developer-source command. `--id` and `--tag` select
known colocated scenarios, `--repeat` derives replayable iteration seeds, and
`--all` executes every registered runner. Unknown selections fail. Passing
registered scenarios and complete feature coverage are separate results:
coverage gaps stay visible, and only `--require-complete-coverage` turns them
into a failing gate. Scenario data selects a fixed registered test entry; it is
never an arbitrary command string.

The managed process form accepts generated local files and an inline
`--assignment` JSON object. Its `--tick` and `--open-gate` controls advance
logical model state; they do not sleep or poll a model. Mock evidence proves
Zap API, queue, lifecycle, question, inbox and acknowledgement behavior. It
does not prove compatibility with Codex, Claude Code, OpenCode or Qwen Code;
those require the separate explicitly selected real-provider checks.

### Zap Quick Lens renderer

After an installed build, ordinary local startup needs no multi-file JSON:

```text
zap-quicklens
```

The launcher reuses the existing Wayfinder owner for `~/.vibe/zap`, or starts
one owner, serves the built renderer on loopback, and opens a one-time paired
browser session. The legacy `zap-quick-lens` spelling remains supported. Use
`zap-quicklens --electron` for the same owner in the
Electron shell. Opening or closing either viewer does not start or stop agents.
An empty workspace is valid; **Add project** validates an existing directory,
shows the protected detected profile/model/effort, and leaves inference stopped
until **Start development** is pressed.

Protected local settings are read from `~/.vibe/zap/settings.json`:

```json
{
  "version": 1,
  "uiPort": 4174,
  "proxy": { "mode": "inherit" },
  "coordinatorDefaults": { "modelId": "your-installed-model-id", "effort": "low" },
  "repositoryMergeIdentity": null
}
```

Ordinary startup enables the stable local repository execution host and the
bounded `repository.consistency` test profile. A null merge identity still
allows repository discovery, plan worktree preparation and reads; an operation
that must create a Git commit refuses until a protected name and email are
configured. Zap does not invent a bot author.

`proxy.mode` may be `inherit`, `direct`, or `explicit`; explicit proxy URLs may
not contain credentials. One shared policy applies to owned Codex, Claude Code,
OpenCode and Qwen Code launches and to managed workers unless a protected
provider profile supplies a narrower override. A generic explicit example is:

Omit `coordinatorDefaults.proxy` to keep Codex on that shared policy, or set it
to a protected `inherit`, `direct`, or `explicit` proxy policy to override only
the local Codex coordinator. Other provider profiles retain their own existing
proxy override field.

```json
{
  "version": 1,
  "proxy": {
    "mode": "explicit",
    "httpsProxy": "http://proxy.example:8080",
    "noProxy": "localhost,127.0.0.1,::1"
  }
}
```

Credentials belong in the provider's protected environment file, never in the
proxy URL. Provider profiles distinguish executable discovery, configuration,
authentication observation and launchability. Merely finding `claude`,
`opencode`, or `qwen` on `PATH` does not make it launchable: the operator must
select a protected model and environment first. These settings never enter
browser project-registration payloads. Existing advanced Wayfinder JSON remains
available through `zap-quicklens --config`.

Run `zap-server` for the same normal product stack without opening a browser or
Electron viewer. It still serves the Quick Lens HTTP client and uses the same
`~/.vibe/zap` settings/state by default. Starting it does not start an agent or
model turn. `zap-wayfinder` remains the advanced command that requires an
explicit Wayfinder configuration.

Managed workers and native provider children are different execution paths.
Managed work has a durable task/run, Lens-owned terminal, explicit model-policy
selection, bounded packet, typed report and human review. Native children remain
owned by their provider and expose only the controls and identity evidence that
provider supplies. Legacy managed requests use protected policy tier bindings;
new specialized work uses the shared named execution catalog. A caller may use
an explicit registered-profile override only with an
explicit reason. Neither path treats terminal output or process exit as an
accepted result.

The authenticated workspace now renders authorized project regions in one
pan/zoom map while retaining each project/context boundary. Selecting a project,
agent, task or run opens its scoped card or terminal; the map does not invent
cross-project dependencies or authority. Exact objects can carry passive notes
or deferred instructions. Archived notes and removed objects remain in
recoverable Trash; restoring a removed object creates an ordinary planning
intent rather than reviving an old plan.

### Parallel plans and repository workspaces

One registered Git project can hold several top-level development-plan
contexts. **New plan** observes the selected checkout HEAD, prepares an owned
root worktree, and registers a new context with its own conversation,
coordinator launch choices, protected cwd and pending algorithm-source binding.
The original registered checkout remains the default context. Reading it for
repository work metadata-adopts a plan envelope in place; it does not move the
checkout or restart its coordinator.

Managed work can request an isolated child worktree. The server records the
plan, parent, basis commit, execution host, actor/task/run assignment and exact
cwd before launch. Resume returns to that same worktree and preserves dirty
edits. Provider-native children remain provider-owned: Lens does not claim that
every native child can be assigned a separate cwd. A native coordinator started
for a prepared top-level plan uses that context's protected root worktree when
its provider supports the configured cwd.

Integration uses a separate owned checkout. The source plan owns the
integration artifact and conflict-resolution task even when the recorded target
is the original checkout in another context. The target context independently
controls promotion through the owned-writer gate. Candidate preparation,
bounded diff, registered test evidence, human review and promotion are distinct
records. Conflicts create a managed resolution task; no automatic stash, reset,
force-push or silent overwrite is used. Notes can bind directly to exact plan,
worktree and integration objects and retain their source snapshot.

A newly prepared plan may coordinate and run work while its planning source is
pending. Trusted source attachment verifies that context's broker scope, active
ZAP store/campaign/base/revision and worktree-local workflow/specification
paths before binding. Dynamically supplied protected source configuration is
not persisted by this slice. After restart, an unrestored source is shown as
unavailable and its algorithm binding returns to pending until trusted
reconnection; parent planning authority is never copied into the new context.

Current limitations remain material. Codex with Luna and Claude Code with Haiku
completed the bounded question, idle/active Pause, late-answer wake,
same-conversation Continue and Stop flow. Qwen Code with a free model and
OpenCode completed the same main question/pause/wake/context/bootstrap flow.
Their live receipts retained cleanup failures because the exit subscription was
removed before the owned exit reached common lifecycle projection. The shared
fix is covered by four focused public no-model lifecycle tests; those historical
receipts are not relabelled as cleanup passes. See the
[acceptance record](vibevm/vibespecs/research/ZAP-PRODUCT-ACCEPTANCE-2026-09-16.md)
for the exact evidence boundary.

The zero-inference corpus currently contains 17 registered scenarios and keeps
coverage gaps explicit. Real temporary-Git/loopback-HTTP tests and the corpus
are component and product-runtime evidence. Multi-user identities,
remote execution hosts, distributed writer leases, contributor admission and
crowdsourced machines/accounts are future architecture, not implemented
features of this local release.

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

The [execution catalog guide](vibevm/vibespecs/EXECUTION-CATALOG-GUIDE.md) covers
named account/model configurations, task specialization, the economy/quality
slider, selected subscription meters and additional protected account homes.
The [catalog acceptance record](vibevm/vibespecs/research/ZAP-EXECUTION-CATALOG-ACCEPTANCE-2026-09-16.md)
records actual browser, mock-runtime and bounded Luna/Haiku evidence.

The current local Zap Wayfinder, Zap Quick Lens, Codlens, project/context,
channel-separation, lifecycle, model-policy and protected-web guidance lives
in [WAYFINDER-GUIDE.md](vibevm/vibespecs/WAYFINDER-GUIDE.md). It records the
current setup fields and capability limits, including which paths remain
configuration or integration work.
