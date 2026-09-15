# Lens workspace implementation plan

Status: implementation authorized, 2026-09-15. Product contract:
[PROP-005](../PROP-005.xml), [PROP-006](../PROP-006.xml) and the first-experience
priority in [PROP-007](../PROP-007.xml).
This plan replaces the earlier single-project integration order. Existing
headless communication and ZAP contracts are retained; unfinished prototype
changes are candidates until their relevant integration path is accepted.

## Deliverable and boundaries

Deliver a shared local Lens application server and one Quicklens browser/Electron
client supporting registered projects, persistent Codex coordinator sessions,
native child agents, chat, rich user questions, history with plan-change
provenance, project focus and a multi-project board. Add an optional local
managed-agent terminal with real interactive output and controlled human input.

Keep all application behavior reusable by later Gamelens and Codlens VS Code/
IDEA clients. Those presentations, additional agent-product implementations,
distributed execution and a full RLM strategy remain later work. Their adapter,
host, task/result and strategy interfaces are part of this delivery. The initial
implementation/test product is Codex; implementation workers are native
gpt-5.6-sol with high reasoning effort. No comparative model pilot is included.

Explicit coordinator start is the initial default. Opening a window, switching
projects and subscribing from a second client must not start a model or repeat
the coordinator boot. A new coordinator follows the actual project's full
applicable boot contract and explicitly knows its role. Native workers receive
bounded task packets. Small isolated test projects keep protocol probes cheap.

The first useful screen is the project/agent operations workspace: switch
projects, inspect global or project activity, navigate the subagent network and
select an actor to see its real output. Chat and richer control follow within
that same screen. The native agent network is distinct from the task/goal map.

## Delivery route

| Stage | Concrete result | Dependencies | Minimum acceptance |
| --- | --- | --- | --- |
| LW-01 Shared contracts and persistence | Product-neutral project/context/actor/session/attempt models; typed chat, grouped questions, answers, immutable event history and terminal capabilities; SQLite store. | Existing broker identities and model conventions. | Two projects stay isolated; identical request retry is stable; conflicting content refuses; answer revision and amendment preserve history. |
| LW-02 Coordinator adapters | Common adapter/AgentHost/strategy interfaces and actual Codex JSONL app-server integration; coordinator bootstrap, resume, turns, interruption and observable native children. | LW-01 public identity contract. | Protocol fixture covers start/resume/events/request epochs; one short real Codex conversation proves the supported path. No arbitrary existing-session attachment claim. |
| LW-03 Shared application server | One owner of registered project runtimes, coordinator launch claims, source watchers, ZAP workflows and input dispatch; thin authenticated MCP and UI clients. | LW-01, LW-02. | Two clients see one coordinator; repeated start does not duplicate it; source/actor authority comes from trusted context; reopening UI leaves running work intact. |
| LW-04 Project/agent operations UI, then interaction | First project switching, global/project/actor activity, agent network and selected output; then Quicklens chat and rich actionable inbox, ZapAskUserQuestion and answer history in the same workspace. | LW-01, LW-03. | First demonstrate two project scopes and a sourced native child/output selection; then a UI message and cross-client question/answer resume the same addressed actor. |
| LW-05 Projects and planning | Focused project view and simultaneous project board; existing semantic graph/cards; central ZAP plan workflow and history of proposed/held/applied changes. | LW-03, LW-04 and existing ZAP public interfaces. | Two projects show distinct state and remain running across focus changes; one real plan-change path has exact before/after provenance; stale plan/source application refuses. |
| LW-06 Local managed terminals | Optional real PTY/ConPTY AgentHost, fixed registered agent profiles, terminal observation, input control transfer, input/resize/interrupt and run/result evidence. | LW-01, LW-03; native route remains independent. | Real interactive process in terminal; two observers; one human controller fences automation input; return control; closing a viewer does not close the agent; native-only startup works without managed module. |
| LW-07 Package and runnable delivery | Documented configuration/start/resume, common server/client exports, browser and Electron build, portable package and account-independent recovery notes. | Accepted preceding stages. | Required package type/lint/build/floor checks once on frozen source; one installed-consumer smoke and one concise browser/Electron visual acceptance. |

## Parallel work and integration order

First wave uses disjoint ownership. The model/store worker freezes the public
WorkspaceClientPort and runtime schemas. The adapter worker builds the Codex
process adapter independently, prioritizing sourced native-child and output
events. The client worker builds the project/activity/agent workspace and
transport against the frozen port, while preserving the accepted task graph and
palette. Only one worker edits package manifests, lockfiles and build config.

Second wave joins these into the shared application service and routes MCP
through it. Reuse the existing question/broker identity and plan preparation
logic where it fits; do not keep a competing per-MCP workflow writer. Complete
the previous dynamic milestone candidate/admission integration through this
shared owner, with actual committed results distinguished from stored proposals.

Third wave supplies the optional managed-terminal backend and UI, then the
runnable delivery. Host-specific capabilities and missing credentials are
explicit. Remote/public deployment remains separate from local implementation;
the password-protected gateway keeps raw broker, agent and ZAP listeners local.

## Acceptance discipline

Run focused, meaningful checks for the stage being changed. Preserve required
strict TypeScript/cell/build checks, but do not repeatedly run the whole package
while sibling schemas are moving. Re-run a check when code or a concrete concern
changes, not to accumulate duplicate evidence. Documented adapter support,
scripted protocol fixtures, actual backend requests and live model runs remain
separate evidence categories.

No test runs against the real NEXT campaign or modifies the read-only design
reference. Temporary ZAP servers use uniquely copied binaries so Windows file
locks cannot block another build. Every owned test process has an identified
lifetime and cleanup path. Live model probes use short tasks and an explicitly
selected weak Codex model; no silent paid-provider substitution or reset-credit
consumption is part of testing.

## Recovery and checkpoints

Project requirements and design decisions live in the package specs and this
plan. Per-user execution state lives outside the repository: complete stage
status, current frontier, worker packets/checkpoints, decision summaries, changed
files, evidence references, active process ownership and exact next actions.

Refresh the recovery record after a meaningful accepted step, before a long
operation, when ownership changes, and periodically during sustained work.
Reading those files must be sufficient to resume after account/model/session
loss. Worker reports are evidence; root acceptance is recorded separately.
Checkpoints contain conclusions and rationale summaries, not secrets or private
model reasoning. Ordinary account changes require no custody/handoff ceremony.
