# Zap Wayfinder operator guide

Zap is the product. Zap Wayfinder is the local shared coordination service.
Zap Quick Lens is its browser and Electron client. Codlens is the current
agent-facing integration name; Gamelens and future Codlens IDE integrations
remain planned presentations over the shared contracts.

This guide describes the current local interfaces and their limits. It does not
claim that a public HTTPS tunnel, a real password, a remote host, or a complete
plan admission workflow has been configured.

## Build the package

Use Node.js 24 or later from the Lens package directory:

```text
npm ci
npm run verify
```

The focused build commands are:

```text
npm run build:node
npm run build:quicklens
```

To install a durable user-local runtime from an explicit local Vibe registry,
follow the [source-install guide](SOURCE-INSTALL-GUIDE.md). That workflow owns a
separate marked installer host and immutable generated payload. It is distinct
from building this checkout in place.

`dist/wayfinder.js` is the Zap Wayfinder entry point. `dist/cli.js` and
`dist/mcp.js` are the Codlens CLI and MCP connector. The package scripts are
the source of truth for the current command names.

For deterministic developer replay without provider inference:

```text
npm run simulate:mock -- --list
npm run simulate:mock -- --id <scenario-id> --repeat 3 --seed <seed>
npm run simulate:mock -- --coverage
npm run simulate:mock -- --all
```

The runner discovers indexed colocated scenario documents and dispatches only
known test entries. Selection by unknown ID/tag fails. Effective seeds and input
positions are written to receipts. Passed scenarios and complete coverage are
reported separately; use `--require-complete-coverage` only when an incomplete
inventory should fail the run. These are deterministic component/runtime
checks, not evidence that every real provider supports current Pause or wake.

## Normal local start

The normal native user installation is:

```text
vibe install -g org.vibevm.zap/zap
```

This command requires a Vibe version with global application support. The
advanced Node source bootstrap remains a development/isolated-registry path.

After an installed build, start the product with:

```text
zap-quicklens
```

The launcher reuses the owner already serving the local Zap data directory or
starts one owner, serves the built renderer on loopback and opens a one-use
paired browser session. The legacy `zap-quick-lens` alias is retained.
`zap-quicklens --electron` opens the same owner through
the Electron shell. An empty workspace is a valid first screen.

`zap-server` starts the same normal owner and Quick Lens HTTP service but never
opens a viewer. It uses the same default settings and state, and starts no
coordinator, worker, child agent or model turn. `zap-wayfinder` remains the
advanced explicit-config entrypoint.

The ordinary setup flow is deliberately two-step:

1. **Add project** selects an existing directory and one protected configured
   provider profile. Registration validates the directory, persists stable
   project/context identities and creates an isolated broker scope. It does not
   start a model.
2. **Start development** explicitly launches the selected coordinator. Installed,
   configured, authentication-observed and launchable are separate provider
   states; a detected executable that still needs a model or protected
   environment stays unavailable.

Opening or closing a viewer does not start, stop or duplicate a coordinator.
Projects restored from the product registry are prepared against the live
runtime before they become launchable.

Protected defaults live in the user-local Zap settings file. A generic shared
proxy example is:

```json
{
  "version": 1,
  "uiPort": 4174,
  "proxy": {
    "mode": "explicit",
    "httpsProxy": "http://proxy.example:8080",
    "noProxy": "localhost,127.0.0.1,::1"
  },
  "coordinatorDefaults": {
    "modelId": "your-protected-model-id",
    "effort": "low"
  },
  "repositoryMergeIdentity": null
}
```

Ordinary startup enables the local repository execution host and the bounded
`repository.consistency` profile. Leave `repositoryMergeIdentity` null when you
only need discovery, worktree preparation and reads. Configure a protected
`{"name":"…","email":"…"}` value before an integration operation that
must create a Git commit. Wayfinder never substitutes an invented author.

The shared proxy policy applies to owned Codex, Claude Code, OpenCode and Qwen
Code processes and to managed workers. A protected provider profile may select
`inherit`, `direct` or its own explicit route. Proxy URLs cannot contain
credentials; provider credentials belong in protected environment files. Local
Wayfinder, MCP and provider-control loopback addresses remain in the no-proxy
set, and TLS verification remains enabled.

`coordinatorDefaults.proxy` is the protected Codex-only override. Omit it to
inherit the shared `proxy`, or set `{ "mode": "direct" }` (or another complete
proxy policy) without changing Claude Code, OpenCode or Qwen Code profiles.

For Claude Code, OpenCode or Qwen Code, add a protected
`providerCoordinators` entry with its stable profile ID, provider, absolute
executable and working directory, selected model/effort, optional argument
prefix, and optional environment reference. `environmentFiles` maps that
reference to an absolute protected JSON file; its values are merged only into
the launched process. `managedWorkers` maps a worker profile and optional policy
tier to one of those protected source profiles plus its selected model/effort.
Tier names must be unique. Leaving a detected provider unconfigured keeps it
visible but unlaunchable rather than guessing a model or credential source.

## Advanced Zap Wayfinder configuration

Run Wayfinder with a strict JSON configuration path:

```text
node dist/wayfinder.js <wayfinder-config.json>
```

The advanced configuration fields include `version: 1`, `state.databasePath`,
`state.modelPolicyDatabasePath`, `gateway.host`, `gateway.port`,
`gateway.namespace`, `gateway.pairingToken`, `gateway.allowedHosts`,
`gateway.allowedOrigins`, `profiles`, `projects`, optional `modelPolicies`,
optional `routing`, `agentGateway`, `planning`, `managedTerminals`, and the
separate protected web profile. `state.databasePath` and
model-policy paths must be absolute. Use placeholders in operator copies and
keep credentials out of the file wherever a local secret mechanism is
available.

`projects` is the trusted registration list. Each entry carries a project,
context, `repositoryRootRefs`, `coordinatorLaunchOptions`, and protected
launch data. The protected working directory and profile reference are server
configuration; browser and agent messages never supply them.

Wayfinder prints a JSON receipt with its loopback gateway address and an
attach URL containing a fresh one-use pairing ticket. The ticket is temporary
connection material: pass it to the trusted browser/Electron launcher through
the documented pairing channel and do not put it in a saved URL, shell history,
agent message, or screenshot. A running owner can issue another ticket with:

```text
node dist/wayfinder.js <wayfinder-config.json> --issue-pairing-ticket
```

The runtime keeps the durable workspace and model-policy stores separate from
the browser's view state. Reconnecting a client uses the same owner/runtime;
it does not start a second coordinator.

## Start Zap Quick Lens

Build and run the browser shell locally:

```text
npm run build:quicklens
npm run dev:quicklens
```

The shared project/agent workspace demo is explicitly synthetic:

```text
http://localhost:4173/?workspace-demo=1
```

`?demo=1` remains the separately labelled legacy task-map demo. For a
production composition, omit both demo flags. The browser should receive a
trusted workspace gateway URL and a one-use pairing ticket from Wayfinder.
Electron receives equivalent values through its trusted main-process
environment. The renderer does not receive broker credentials, password
verifiers, or raw agent endpoints.

The shared workspace view reads authorized project lists, project/context
details, agent networks, managed tasks/runs, output pages, questions, chat,
execution state, notes, Trash and history through `WorkspaceClientPort`. Its
single pan/zoom canvas lays out all authorized project regions. Selecting a
project, actor, task or run opens the exact scoped card or terminal. Collapse
and camera state do not merge project contexts, and the graph does not invent
cross-project edges or authority.

## Agent and UI channels

There are two deliberately separate channels:

- The localhost agent channel serves broker/MCP and host adapter traffic. It
  uses one configured credential per exact workspace/conversation scope.
- The local shared-workspace UI channel uses a one-use Wayfinder pairing ticket
  and then a scoped loopback session. The separate protected web profile uses
  its password, server-side sessions, CSRF, exact origin checks and configured
  proxy proof. Neither channel forwards arbitrary MCP, broker, process, shell,
  or agent routes.

Do not expose a localhost agent port through a general proxy. A UI login does
not create an agent identity, Coordinator authority, or Owner approval. The
shared server binds the authenticated UI session to its trusted role and
registered project subset.

## Codlens setup and host integrations

Codlens setup takes a JSON scope and writes protected local credentials:

```text
codlens setup '{"workspaceId":"workspace.example","conversationId":"conversation.example"}'
codlens start
```

These are the actual command names exposed by the package; use the installed
`codlens` executable or `node dist/cli.js`. `CODLENS_DATABASE_PATH`,
`CODLENS_CREDENTIAL_FILE`, `CODLENS_URL`, and the optional bind/allowlist
variables are trusted local environment configuration. Keep their values out
of agent prompts and public documentation.

An `agentGateway` may start with no configured scopes. Project registration
creates each exact workspace/conversation scope lazily, persists its catalog and
writes separate protected agent and human-responder credential files. A shared
Codex profile receives the matching scope credential; non-Codex launch
preparation receives a protected provider-specific MCP config. The gateway
refuses a mismatched project/context/scope and reuses the same exact scope after
restart.

Every owned coordinator is prebound to one actor and adapter session. Its MCP
process can call `codlens_assigned_context` without arguments to discover that
safe handle and exact scope; no credential is returned. It should not create a
second actor with `codlens_connect`. Claude Code and Qwen Code receive an MCP
config file, OpenCode receives its local `mcp` configuration through
`OPENCODE_CONFIG`, and Codex receives its scoped MCP configuration. Credentials
remain referenced through protected files rather than model prompts.

Codex, Claude Code, Qwen Code and OpenCode adapters use exact native identity
where their host exposes it. A display name, prompt text, process label, or
parent label is never a routing identity. Native child controls remain host
controlled. A host may report a native child without providing direct input.

Coding workers are selected through the project's model policy. Protected
managed-worker templates bind policy tiers to registered provider profiles,
models and effort. Ordinary worker creation resolves that policy; an explicit
profile override names a registered profile and carries a reason. Request text
cannot invent an executable, working directory, model or provider. Model policy
records requested tier and effort, resolved profile/model, capability evidence,
and later host observation separately.

Managed work is the Lens-owned path: it has durable task/run/attempt identities,
a bounded packet, explicit target references, a real terminal, typed report and
separate human review. Native provider children remain provider-owned and expose
only observed native identity and supported controls. A managed worker may
create a bounded child through authenticated MCP; the server derives project,
context and parent identity. Process exit, a report and human acceptance remain
separate records.

## Coordinator lifecycle

A configured coordinator launch records a durable claim before any host start.
The initial coordinator follows the complete applicable project instructions
and boot material. A delegated worker receives a bounded packet and its quiet
clause when the host contract requires one; it is not the project coordinator.

The project lifecycle is per project/context:

- Pause prevents new dispatch and requests supported interruption or a safe
  stopping point. It does not freeze model computation at a token boundary.
  Claude Code, OpenCode and Qwen Code report Pause as unsupported when the
  configured adapter has no verified pause primitive; the UI must not present
  that as a successful suspension.
- Stop prevents new execution, retains chat/questions/history/results, and
  requests termination of owned work. It does not delete the repository or
  roll back a plan.
- Continue reconciles the saved context and resumes the registered conversation
  when the host supports it. A changed policy does not replace a running or
  saved immutable selection. Missing native context is an explicit recovery
  state.

Saved user input, host acceptance, running state, observed exit, uncertain
state, and judged completion are separate observations. Opening another client
does not duplicate a coordinator.

The current managed PTY port supports input, resize, interrupt and stop, but it
does not claim process suspension. If a project has a running managed terminal,
Pause closes the dispatch gate and reports unsupported or uncertain rather than
claiming the whole project is paused. Stop remains the explicit owned-process
termination path and waits for observed exit.

## Chat, questions, history and terminals

Chat is durable project/context conversation state. New input is queued or
steered only through a supported host operation; ordinary send does not
silently interrupt a turn. Rich questions are durable groups with explicit
answers, revision checks, deadlines, amendments and cancellation. A suggested
answer is not a submitted answer.

Question publication returns immediately. An idle authenticated worker can use
`codlens_inbox_wait` with a bounded timeout, then explicitly read and acknowledge
the addressed delivery. Waiting does not acknowledge, approve or wake a busy
model. Host hooks may offer safe-point delivery, but there is no universal wake
guarantee across all provider versions and session modes.

History retains project/context/actor scope, source event identity and source
ordering. Global display order does not claim a causal clock across projects.
Reconnections use cursors; missing coverage is reported rather than invented.

Native sessions expose observed public output and only supported native
controls. Managed agents are a separate configured mode with Lens-owned
processes and real virtual terminals. Terminal observation requires authorized
project/session/host scope; multiple observers can inspect the same stream
without taking control. Terminal input, resize and terminal interrupt/stop
require the named terminal lease and control epoch. Project Stop uses its
separately authorized project lifecycle action to stop owned resources; it is
not granted by an observer lease. Logging into Quick Lens or opening chat does
not grant terminal input. A terminal string that looks like an approval is not
an authoritative approval.

## Anchored notes and recoverable Trash

**Open notes** on a canvas card works against an exact
`ProjectObjectReference`: project, semantic object or relationship, agent, work
task or work run. A passive note records context without dispatching work. A
deferred instruction is offered only when a declared matching boundary starts.
Managed work always includes its exact task and run references in that check;
native work must declare target references explicitly. Empty or unresolved
native targets stay `waiting_for_target`; prompt text and display labels are
never guessed as identities.

Offer, read and acknowledgement are separate. The before-work receipt contains
the exact attachment/version for the addressed actor and attempt. A worker must
acknowledge that pair explicitly. The same authenticated path is available to
managed and declared native work through MCP and loopback HTTP.

**Move to Trash** archives a note and suppresses deferred delivery while
retaining its versions and former anchor. Source observation may also archive a
removed object with its captured context. Restoring a note restores the note;
relinking chooses another exact anchor. **Propose restore** for a removed object
creates an ordinary planning intent. It does not resurrect an obsolete plan or
infer that temporarily missing or filtered data was deleted.

## Multiple plans, worktrees and integration

**New plan** creates another development-plan context inside the selected Git
project. Wayfinder observes the selected committed HEAD, prepares an owned root
worktree, and registers an independent context, conversation, broker scope,
launch options and protected cwd. The project's original registered checkout
stays the default context. The first repository read metadata-adopts that
existing context in place so isolated managed work does not require relocating
or restarting its coordinator.

Use the plan selector to switch contexts. Each plan, worktree, integration,
actor and run keeps its project and context identity in the map and detail
cards. A worktree read observes current Git HEAD without mutating the durable
record; the next preparation command CAS-records that exact committed head.
Dirty edits are disclosed and are not copied into a child fork.

Managed workers may request an isolated child worktree. Preparation records the
parent, basis commit, execution host and actor/task/run assignment before the
terminal starts. Continue uses the same assigned worktree and does not demand
that it still match its initial clean HEAD. Provider-native children remain
provider-owned and are not universally assignable to isolated cwd. A native
coordinator launched for a prepared top-level context receives that context's
protected root cwd only through the provider's supported launch mechanism.

Integration is a separate checkout owned by the requesting/source plan. Its
recorded target may be the original checkout in another context; that target's
actual context supplies writer activity and promotion authority. The sequence
is prepare candidate, inspect bounded diff, run a registered test profile,
record human review, then promote against the exact target HEAD. A conflict
creates an explicitly authorized managed resolution task in the integration
checkout. There is no automatic stash, reset, force-push or conflict choice.

Notes can target `plan_workspace`, `worktree` and `integration` references.
These objects have their own authoritative annotation catalog, separate from a
semantic planning snapshot, so a semantic refresh cannot falsely trash a
worktree note.

The repository writer gate is host-local. It resolves the recorded target
worktree's real context, observes settled project lifecycle plus exact managed
work and terminal activity, and holds one local target lease through promotion.
The same lease prevents session/chat/Continue and managed prelaunch from
reopening that target. Distributed leases, remote execution hosts and
multi-user merge authority are not implemented.

## Plan and source boundaries

ZAP remains authoritative for planning rules, economics, admission, holds,
proposals, decisions and applied effects. Wayfinder coordinates projects,
agents, dispatch, questions, chat and observed history; it does not become a
second planning engine. A preview is not an applied plan. Inspecting history
does not activate an old plan. Source-change signals and plan rebuilds retain
their exact source basis and revision.

The current renderer and local Wayfinder composition preserve unavailable
states when a ZAP source or policy provider is not configured. That is an
honest capability boundary, not a successful plan operation.

A prepared development plan may remain source-pending while its coordinator,
chat and managed work operate. Trusted dynamic attachment verifies the exact
broker context, live ZAP store/campaign/base/revision/adopted-plan identity and
worktree-local workflow/specification paths before binding. Two independent
plans cannot silently share the same active planning identity. Dynamically
supplied protected source configuration is not persisted by this slice: after
restart, an unrestored source is explicitly unavailable and the algorithm
binding is pending until a trusted caller reconnects it. No parent source is
copied into a child plan.

When `planning.contexts` is configured, Wayfinder owns one protected ZAP/source
runtime and journal per registered project/context. Browser controls use
`project.snapshot.v1` and the five `plan.intent|preview|apply|reconcile|decide.v1`
commands. Production MCP uses the same central runtime through authenticated
`/v1/agent-plan/*` routes; it does not open a workflow database or ZAP
credentials in every stdio process. Dynamic milestone create/revise flows first
bind precursor identities, then record the composite successor and assessment,
then execute the prepared effects in order. Source changes are freshly hashed,
recorded with before/after bases, and make stale mutations refuse.

The planning portion of the trusted Wayfinder config has this shape (shown as a
fragment; every credential value is a protected placeholder):

```json
{
  "planning": {
    "configDirectory": "ABSOLUTE_PATH_TO_ZAP_CONFIG",
    "contexts": [
      {
        "projectId": "project.example",
        "contextId": "context.example",
        "source": {
          "protocol": "quicklens/1",
          "workspaceId": "workspace.example",
          "conversationId": "conversation.example",
          "sourceLabel": "Example ZAP plan",
          "broker": {
            "endpoint": "http://127.0.0.1:43111",
            "humanPrincipalToken": "REPLACE_WITH_EXACT_SCOPE_HUMAN_TOKEN"
          },
          "zap": {
            "reader": {
              "endpoint": "http://127.0.0.1:44000",
              "credentialId": "reader.example",
              "bearer": "REPLACE"
            },
            "data": {
              "endpoint": "http://127.0.0.1:44000",
              "credentialId": "data.example",
              "bearer": "REPLACE"
            },
            "coordinator": {
              "endpoint": "http://127.0.0.1:44000",
              "credentialId": "coordinator.example",
              "bearer": "REPLACE"
            },
            "owner": {
              "endpoint": "http://127.0.0.1:44000",
              "credentialId": "owner.example",
              "bearer": "REPLACE"
            }
          },
          "workflowDatabasePath": "ABSOLUTE_PATH_TO_ZAP_STATE/example-plan.sqlite",
          "specifications": [
            {
              "root": "ABSOLUTE_PATH_TO_PROJECT_SPECIFICATIONS",
              "include": ["**/*.md", "**/*.xml"]
            }
          ]
        }
      }
    ]
  }
}
```

The Owner credential is consumed only by the authenticated human decision
path. Agent MCP routes cannot select it by label or request body.

## Protected web profile

The password profile is separate from the localhost agent channel. The existing
scrypt verifier is initialized or rotated through the dedicated web-auth
command; plaintext passwords are not command-line arguments, URLs, local
storage, agent messages, or ordinary history. Sessions use Secure, HttpOnly,
SameSite cookies and CSRF/origin checks. Viewer, operator and Owner roles have
different named operation grants; project scope comes from trusted runtime
configuration.

The listener remains loopback. A separately managed HTTPS tunnel may forward
only the protected web listener and must inject the configured proxy proof and
exact forwarded HTTPS origin. This package does not install a tunnel, publish
a hostname, create a real password, modify firewall rules, or claim public
deployment. Setup gaps must be resolved by the operator's local configuration.

## Recovery and future clients

Developer recovery state is separate from product state. Project facts belong
in this repository; per-user stewardship, credentials, pairing tickets and
session recovery records remain local and are never copied into this guide.

Gamelens and Codlens VS Code/IDEA integrations can reuse the same
WorkspaceClientPort and actor/run graph. The current Quick Lens workspace
already provides the unified multi-project canvas; future clients must retain
the same project/context identities and cannot merge plans, camera state or
authority.

The four provider families have registered drivers, shared proxy and MCP
preparation. Codex/Luna and Claude Code/Haiku completed the bounded live
question, idle and active Pause, late-answer delivery, same-conversation
Continue and Stop flow. Qwen Code with a free model completed the main live
question/pause/wake/context flow, and OpenCode completed the same main live flow
after its request shape was corrected. The Qwen and OpenCode receipts still
record cleanup failures from the old exit-unsubscription ordering. The exact
shared Stop repair is covered by four focused public no-model lifecycle tests;
the historical receipts are not rewritten. The
[acceptance record](research/ZAP-PRODUCT-ACCEPTANCE-2026-09-16.md) distinguishes
live evidence from deterministic adapter tests and records the remaining limits.
