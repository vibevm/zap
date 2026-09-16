# Codlens host adapters

These templates connect supported host lifecycle surfaces to the Codlens broker.
They contain placeholders only and are never written to a user's live host
configuration automatically.

Build the shared package first, then run the headless setup with explicit
user-local paths:

```text
CODLENS_DATABASE_PATH=<user-local>/lens.sqlite
CODLENS_CREDENTIAL_FILE=<user-local>/credentials.json
codlens setup '{"workspaceId":"workspace.example","conversationId":"conversation.example"}'
codlens start
```

Setup creates the credential file without printing its tokens and restricts the
database and credential file to the current user. A copied Codex plugin cannot
infer the package installation path: set `CODLENS_CLI_PATH` to the installed
package's real `dist/cli.js`. The wrapper refuses with an actionable diagnostic
when that path is absent.

In the shared Wayfinder profile, agent plan tools proxy through the same
authenticated loopback gateway as inbox tools. Wayfinder alone owns configured
ZAP channels, source watchers and workflow journals; MCP processes do not open
`CODLENS_PLAN_CONFIG_FILE` or a second workflow writer. The legacy standalone
agent-plan schema/example remains only for direct library compatibility; the
production MCP entry ignores that environment variable. `QUICKLENS_CONFIG_FILE`, based on
`quicklens-runtime.example.json`, is used by the trusted local UI service and
holds its separate human responder and Owner channels. Relative database and
specification paths resolve from the directory containing each config file, so
MCP processes launched from different project directories share durable state.

For multiple projects, configure one Wayfinder `agentGateway.scopes[]` entry per
exact workspace/conversation. Each entry names separate generated agent and
human-responder credentials. A shared Codex launch profile may use
`lensMcp.scopeCredentials[]` to select a distinct protected credential file for
each exact scope. Owned prebound coordinators disable `codlens_connect`; the
broker still revalidates the current binding on every plan/question operation.

The Codex plugin uses `${CODEX_PLUGIN_ROOT}` to locate its small wrapper. The
wrapper invokes the installed `codlens` executable, or the executable named by
`CODLENS_CLI_PATH`, with shell expansion disabled. Hook calls perform a bounded,
non-waiting inbox check; they do not wait for a human answer.

Claude Code can use the hook template as the baseline. Its channel template is
separate because native channels are a research-preview, per-session opt-in. A
standard MCP server alone cannot inject an unsolicited model message.

Qwen Code's loopback HTTP hook template offers context at lifecycle safe points.
For a TUI launched with `--input-file` and `--json-file`, the separate dual-output
opt-in template enables a structured regular-file submit queue without writing to
the terminal or prompt widget. Busy submissions retry when the TUI becomes idle;
the watcher polls every 500ms. The equivalent `dualOutput` setting requires a
restart. Codlens never emits `confirmation_response`, because that command answers
a human tool-approval request. The native peer inbox remains unavailable on native
Windows, and ACP-driven sessions refuse peer input. A separately managed
`qwen serve` adapter is the other supported idle-wake path.

OpenCode uses an exact known server and session binding. The adapter checks idle
state before dispatch, but treats an idle-to-busy race and asynchronous HTTP
acceptance as uncertain until message-history reconciliation. It never controls
the visible TUI prompt.

## Native identity evidence

| Host             | Documented native shape used by Codlens                                                                                                                                                                                             | Routing boundary                                                                                                                                                                                         |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Codex            | Common child hook `session_id` is the parent session; `SubagentStart` and `SubagentStop` provide `agent_id`. [Codex hooks](https://learn.chatgpt.com/docs/hooks)                                                                    | A parent-only tool hook is ambiguous when several child bindings exist. An inherited parent handle does not select a child. Use the event-native `agent_id` or a separately attested exact actor handle. |
| Claude Code      | Subagent hook input includes `agent_id`, including tool hooks executed inside a subagent. [Claude hooks](https://code.claude.com/docs/en/hooks)                                                                                     | Route by the exact session plus `agent_id`; never infer from `agent_type`, description, or display name.                                                                                                 |
| Qwen Code 0.23.4 | The installed bundled hook guide documents common `session_id` and `agent_id` inside subagents; `SubagentStart`/`SubagentStop` also name `agent_id`. [Qwen hooks](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/) | Route by both values. If an event/version omits `agent_id`, use a separately attested broker actor handle or refuse ambiguity.                                                                           |
| OpenCode         | The server exposes session IDs, `parentID`, and the children endpoint rather than the shared hook schema. [OpenCode server](https://opencode.ai/docs/server/)                                                                       | Preserve the exact child session binding. Parent/child labels and titles are display data.                                                                                                               |

These are documented and fixture-tested shapes, not live-host inference results.
Every host falls back to the explicit broker actor handle when its event or
version lacks a native child identifier. MCP session metadata and display names
never supply that identity.

For every host, `persisted`, `offered`, `host_accepted`, and
`actor_acknowledged` are separate observations. A hook output is an offer. Only
an authenticated `codlens_ack` broker operation establishes actor acknowledgement.

Requirement: `spec://org.vibevm.zap/lens/PROP-001#adapters`.

## Shared Wayfinder and Quick Lens

The shared local service is Zap Wayfinder. Zap Quick Lens connects through the
named WorkspaceClientPort operations exposed by its trusted workspace gateway;
it does not connect to an agent/MCP listener directly. Use the one-use pairing
ticket printed by `zap-wayfinder <config.json>` for the local workspace profile.

Host adapters retain their native identity and lifecycle boundaries. A parent
session label, display name, terminal text or model output is not enough to
route a child operation. Managed terminal input requires an explicitly owned
terminal lease and control epoch; native children expose only the capabilities
their host reports.

Model policy is configured by task purpose/class, tier and effort. The selected
policy revision, concrete profile/model and later host observation stay
separate. Test actors use the configured small/Luna/low fixture; coding-worker
selection remains complexity- and budget-aware. No provider credential belongs
in a plugin, hook, MCP message or browser payload.

See the [Wayfinder operator guide](../vibevm/vibespecs/WAYFINDER-GUIDE.md) for
local startup, scopes, lifecycle, history, questions and protected-web limits.
