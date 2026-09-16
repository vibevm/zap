# Quicklens trusted service

This Node-only composition implements the browser-safe `QuicklensDataSource`.
It combines one configured ZAP client, one authenticated lens principal port,
one bounded specification watch, and an injected `PlanWorkflowPort`.

`read` obtains capabilities and active context, follows bounded strategic-map
pages, enriches incomplete cards with `zap.map.object.v1`, reads the adopted
milestone view, and combines scoped broker actors/questions. Every ZAP page must
match the active-context store, base, and revision. Extra pages, missing current
strategy, stale adopted plans, and unavailable registered queries remain
explicit partial, stale, or unavailable states.

Question answers use the human-responder principal and exact question revision.
Plan intent is a typed broker notice to the explicitly selected active root actor
holding `plan:propose`; labels never grant authority. Specification changes send
reassessment notices to eligible roots, but file text is never interpreted as
authority or applied automatically.

Preview and apply perform a fresh filesystem digest and active-context check.
The SQLite workflow immutably binds the broker-delivered intent, selected actor,
proposal, preview, operation, and exact basis. It persists an uncertain marker
before either admission or product mutation. Exact reconciliation is required
after transport loss. A typed ZAP refusal is terminal. Human Owner decisions
use a separately configured Owner client and the backend-returned hold bindings;
agent MCP tools cannot select or invoke that client.

`quicklens-service` reads `QUICKLENS_CONFIG_FILE` and starts the live source plus
the namespaced cookie gateway. Its address output contains only host, port and
base path. The browser pairs once at `<basePath>/v1/pair`; all named routes and
the bounded invalidation cursor live below that base path. A cursor ahead of a
restarted gateway yields a `reconnect` invalidation and resets to the new head.

Separate MCP processes use `CODLENS_PLAN_CONFIG_FILE`. That strict config accepts
only Coordinator ZAP authority, workflow storage, source roots and scope; human
responder, Owner and UI pairing credentials are rejected as extra fields. The
agent may register an explicit chat intent with `codlens_plan_intent`, then use
the returned intent/message identities in the staged flow:
`codlens_plan_discover` reads the exact current bases, `codlens_plan_author`
durably prepares/reconciles the plan and assessment metadata, and
`codlens_plan_proposal` binds its returned immutable successor to the intent.
GUI-originated intents instead arrive as exact addressed broker messages. Both
paths prove the authenticated adapter session before preparing a proposal.
Ordered effects persist their current admission step and completed prefix; a
restart reconciles the same command and cannot replay an already committed
prefix. Completion is reported only after the last product is committed.
The current authoring seam adopts a successor assembled from existing milestone
revisions. It does not yet author `milestone.created` or `milestone.revised`
effects, so clients must not present dynamic milestone creation as available.

Placeholder-only examples are `integrations/quicklens-runtime.example.json` and
`integrations/codlens-plan-runtime.example.json`. Copy them to protected
user-local files and replace every placeholder. Never store live credentials in
the package or repository.

This cell is never imported by browser code. A trusted Electron/main or local
service composition exposes only its `QuicklensDataSource` result DTOs.
