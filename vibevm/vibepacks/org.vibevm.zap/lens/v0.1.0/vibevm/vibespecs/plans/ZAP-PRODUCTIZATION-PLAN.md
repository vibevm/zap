# Zap productization route

This owner-authorized work follows the transport/workspace MVP at `a994165b`.
The current requirements are [PROP-010](../PROP-010.xml) and
[PROP-011](../PROP-011.xml). The earlier MVP acceptance remains evidence for its
documented paths; it is not acceptance of the larger product workflows below.

## Current implementation checkpoint

The productized local path now starts from an empty workspace, registers an
existing project without launching inference, and starts the selected model only
after **Start development**. One owner serves browser/Electron clients, the
single multi-project map, scoped cards/terminals, rich questions, managed work,
anchored notes and recoverable Trash. Dynamic broker scopes and provider MCP
files are prepared per project rather than predeclared in one static list.

Codex, Claude Code, OpenCode and Qwen Code share the protected proxy policy and
provider-specific coordinator adapters. Managed worker templates feed the
project model policy; ordinary work resolves policy, while an explicit
registered-profile override carries a reason. Managed work and native provider
children remain separate paths. An idle worker has a bounded authenticated
inbox wait, but the product does not claim universal wake behavior.

Qwen Code completed the real managed question, browser answer, delivery
acknowledgement and typed-report flow through an HTTP proxy; human review
succeeded after Wayfinder restart. Not all four providers have completed a
current real-model end-to-end run. Provider Pause is unsupported where no
verified primitive exists. The final delivery boundary and evidence are in the
[acceptance record](../research/ZAP-PRODUCT-ACCEPTANCE-2026-09-16.md).

## Delivery and acceptance

| Slice | User-visible outcome | Acceptance boundary |
| --- | --- | --- |
| Local start and project setup | Open Zap, add an existing project, choose an available agent and explicitly start development. | A clean local workspace opens without invented sample projects or manually authored protocol IDs. Adding a project does not launch a model. Another client sees the registration and can reconnect to the same service. |
| Agent products | Codex, Claude Code, OpenCode and Qwen Code have real registered provider drivers and truthful capabilities. | Installed/configured/authenticated/exercised states are distinct. Supported conversation and managed paths are exercised with available inexpensive profiles; unavailable account setup is actionable rather than hidden behind a nominal provider label. |
| Agent networking | Choose a shared proxy or a profile-specific inherited, direct or explicit route. | Every owned agent/provider process receives the effective policy, native children inherit it where supported, local control traffic bypasses it and TLS stays verified. No global environment mutation, credential disclosure or silent direct/provider fallback. |
| Managed work | Delegate a bounded task, inspect its real agent terminal, answer questions, receive a report and review it. | Durable launch claim precedes spawn; server identities and model selection are preserved. Human control fences automation. Closing a viewer does not kill work. Process exit is separate from report and acceptance. |
| Questions for people | Understand who is asking, why and in which project; answer ordinary rich question groups comfortably. | Choices, descriptions, custom/text/multiple answers, limits, drafts, cancel/amendment, deadlines and accessible feedback work. Drafts do not cross question revisions or silently overwrite another client. Cancellation and delivery states reflect the actual recipient path. |
| One project map | Several project regions and their work appear in one pan/zoom canvas. | Exact scoped selection opens the right card or terminal; layout indicates decomposition/dependencies, and unavailable or partial data remains visible. Camera/collapse state survives refresh. No cross-project authority or edges are invented. |
| Anchored notes | Capture a note or future instruction against an exact graph element without disturbing current work. | Passive notes do not dispatch. The next declared matching work boundary receives the eligible instruction version; offer, read and acknowledgement are distinct. Unknown native task binding remains waiting rather than guessed. |
| Recoverable Trash | Find removed elements and their notes, inspect the former context and restore/relink deliberately. | Soft archive suppresses deferred delivery and preserves history. Partial data, filters or temporary disconnect never mean deletion. Restoring annotations does not revive an old plan; object restoration uses the ordinary planning proposal path. |
| Runnable delivery | The same shared server serves browser and Electron, with clear setup/recovery states. | Focused real user-flow checks, required repository gates, an installed-package check and an account-independent checkpoint. No redundant broad/model comparison campaign. |

## Shared contracts and ownership

`ProjectObjectReference` is the common identity for a project, semantic object,
semantic relationship, agent, work task or run. It contains project ID, context
ID, reference domain and exact source reference. Display labels, camera positions,
captured source basis and snapshots are separate data. This avoids note routing
or graph selection by visual similarity.

The application server owns project registration, provider configuration, managed
task/run state, questions, annotations, delivery eligibility and Trash. Provider
drivers own product-specific session/process operations. PTY infrastructure owns
the actual terminal. ZAP retains plan/economics/admission authority. A result
report, a terminal exit, an answer and a plan approval are different records.

Provider-native session/message/turn IDs and Zap submission correlations are
different records too. An accepted submission can precede a native turn ID;
adapters retain explicit correlation provenance and reconcile later events
without fabricating a native identifier or sending the same request again.

Wire/model schemas remain below runtime implementations in the import graph.
The common workspace protocol must not import a managed backend which imports
the workspace protocol in turn. New operations use explicit public DTO
projections; passing an outer transport envelope into a strict inner schema is
not an adapter.

## Local start flow

The ordinary launcher discovers an existing owner or starts one local
Wayfinder, serves the built renderer, and opens the selected browser/Electron
presentation. Advanced explicit JSON configuration remains supported, but is not
the only way to reach a usable screen.

An empty workspace and zero available providers are valid setup states. An
authorized human may add an existing directory through a folder chooser or a
server-local path entry. The server validates the registration, generates stable
identities and protected broker bindings, and offers registered provider/model
profiles. This permission does not let an ordinary run override its protected
working directory or submit arbitrary executable arguments.

Projects without an initialized ZAP campaign still expose real coordination and
managed work; the semantic planning source is explicitly unavailable until
configured. Setup must not fabricate a campaign, adopt a discovered legacy plan,
or treat a terminal process as an agent task. Existing planning sources attach
through the accepted protected ZAP interface.

The current workspace canvas lays out authorized projects together while every
node and edge retains project/context identity. Notes and deferred instructions
anchor through `ProjectObjectReference`; managed starts add their exact task/run
references, and native callers must declare targets. Trash preserves archived
notes and removed-object context. Restoring an object remains a planning
proposal, never a direct resurrection.

## Deferred scope

Full RLM strategy execution, authenticated remote execution hosts, Gamelens,
native VS Code/IDEA shells and real public-tunnel deployment remain separate
future implementations. The unified canvas and the four priority agent products
are no longer deferred by this productization request.

Execution assignments, in-progress evidence, process ownership and the next
unfinished step are maintained in the user-local productization recovery plan.
Accepted project behavior and its limits are recorded in repository documents.
