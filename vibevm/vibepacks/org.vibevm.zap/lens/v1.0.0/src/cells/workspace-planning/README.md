# Shared Wayfinder planning

Zap Wayfinder opens one `QuicklensSourceRuntime` for each registered
project/context. That process owns the ZAP reader, Data, Coordinator and Owner
channels, source watcher, workflow store, ordinary authoring journal and
composite authoring journal. Browser clients use the five typed
`plan.*.v1` workspace commands. Agent MCP processes call the bounded
`/v1/agent-plan/*` proxy and never receive a ZAP credential.

Dynamic milestone authoring is deliberately staged. First call
`codlens_plan_prepare_composite` with the exact discovered intent basis and
typed create/revise drafts. It durably binds the returned milestone and
revision IDs. Build successor content from those IDs, then call
`codlens_plan_author_composite`. The shared journal records the composite
candidate before its Data mutation, reconciles that identity, records the
assessment command before submission, and hands the immutable successor to the
existing ordered-effect workflow. `codlens_plan_proposal` correlates only the
saved operation ID; model-supplied prepared bytes are never trusted.

Source notifications trigger a fresh authoritative read. Wayfinder stores the
previous and next source/plan bases and any source-observed adopted plan refs in
project history. It does not label a snapshot revision as an application
receipt. A changed source basis makes subsequent preview or mutation stale;
paused projects retain the history signal without automatically continuing.

Each MCP call is authenticated by the retained adapter session. Wayfinder maps
the broker workspace/conversation to exactly one configured project/context.
Cross-project IDs in request bodies carry no authority. Human Owner decisions
remain available only through the separately authenticated UI command.
