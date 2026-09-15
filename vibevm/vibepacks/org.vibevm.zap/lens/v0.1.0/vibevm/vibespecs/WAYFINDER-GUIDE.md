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

`dist/wayfinder.js` is the Zap Wayfinder entry point. `dist/cli.js` and
`dist/mcp.js` are the Codlens CLI and MCP connector. The package scripts are
the source of truth for the current command names.

## Start Zap Wayfinder

Run Wayfinder with a strict JSON configuration path:

```text
node dist/wayfinder.js <wayfinder-config.json>
```

The current configuration fields include `version: 1`, `state.databasePath`,
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
details, agent networks, output pages, questions, chat, execution state and
history through `WorkspaceClientPort`. Switching projects changes view scope;
it does not change another project's context or launch authority.

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

A shared `agentGateway` contains a unique `scopes` entry for every registered
broker workspace/conversation. Each entry uses its own generated agent and
human-responder credentials. A Codex profile shared by several projects uses
`lensMcp.scopeCredentials` to map each exact scope to its protected credential
file. The adapter refuses an unmapped scope before `thread/start`. An owned,
prebound coordinator receives its adapter session ID and has `codlens_connect`
disabled, so its MCP process cannot select a different project scope.

Codex, Claude Code, Qwen Code and OpenCode adapters use exact native identity
where their host exposes it. A display name, prompt text, process label, or
parent label is never a routing identity. Native child controls remain host
controlled. A host may report a native child without providing direct input.

Coding workers are selected for task complexity, verification cost and budget.
Agents used to test protocols use the configured inexpensive test profile; the
current policy fixtures use the small Luna tier with low effort. No model name
implies a price, capability, or fallback. Model policy records requested tier
and effort, resolved profile/model, capability evidence, and later host
observation separately.

## Coordinator lifecycle

A configured coordinator launch records a durable claim before any host start.
The initial coordinator follows the complete applicable project instructions
and boot material. A delegated worker receives a bounded packet and its quiet
clause when the host contract requires one; it is not the project coordinator.

The project lifecycle is per project/context:

- Pause prevents new dispatch and requests supported interruption or a safe
  stopping point. It does not freeze model computation at a token boundary.
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
    "configDirectory": "C:/zap-wayfinder/config",
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
          "workflowDatabasePath": "C:/zap-wayfinder/state/example-plan.sqlite",
          "specifications": [
            { "root": "C:/projects/example/specifications", "include": ["**/*.md", "**/*.xml"] }
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

Gamelens, Codlens VS Code/IDEA integrations and a future unified canvas can
reuse the same WorkspaceClientPort and actor/run graph. A unified canvas may
place project regions in one scene, but it must retain project/context identity
and cannot merge plans, contexts, camera state, or authority. The current card
and workspace views do not claim to implement that future canvas.
