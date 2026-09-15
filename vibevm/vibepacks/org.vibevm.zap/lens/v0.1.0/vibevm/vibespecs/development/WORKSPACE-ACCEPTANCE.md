# Zap workspace acceptance

This record distinguishes implemented contracts, deterministic integration
checks, and observations with actual host processes. It accompanies the
[workspace delivery plan](../plans/LENS-WORKSPACE-PLAN-v0.1.md).
The entries describe the accepted MVP behavior and its verification limits.

## Accepted observations on 2026-09-15

| Path | Evidence and boundary |
| --- | --- |
| Codex coordinator lifecycle | Two isolated projects used actual Codex 0.152.1 with the small Luna profile. Pause retained queued chat, the other project stayed independent, Continue delivered the input, and Stop followed by Wayfinder recreation resumed the same native conversation without another bootstrap. The live pause was an idle/queued-input case; active and late-child control have deterministic protocol tests. |
| Native subagent inspection | A real native child and its parent relationship were observed, with the child's public reply. Browser inspection subsequently reused this saved data without another model call. Native child creation requested the small model; that request is distinct from an independently observed child model setting. |
| Rich questions | A real coordinator published a grouped question through MCP. A browser submitted an answer while the project was paused. No new model turn was dispatched during the pause. Continue triggered the owned notification, and the same coordinator read and acknowledged the answer. This bounded probe used three Luna turns with low effort. |
| Browser and Electron | Both built shells read the same saved native output, selected the child, switched themes, and observed a newly recorded event without manual refresh while retaining the selected actor. The corrected Electron bridge passes plain IPC data; subscription iteration and cancellation live in the renderer. No model was started by this check. |
| Dynamic milestone and next plan | Two separate production MCP processes used one Wayfinder planning runtime and the accepted Rust ZAP backend. The durable workflow completed milestone creation followed by plan adoption, retaining both ordered effect identities. An independent read of the workflow database confirmed the completed phase and two-effect prefix. This observation alone does not establish response-loss or stale-source handling. |
| Planning recovery and source drift | Focused workflow tests establish persistence before sending and reconciliation; independent ZAP-client tests cover an unknown mutation outcome. Source-watch tests establish stale-basis refusal and source-change handling against a simulated Rust endpoint. The MVP accepts these checks alongside the real dynamic path; transport-loss injection through the entire central process chain was not run. |
| Managed terminal transport | Real local PowerShell PTYs exercised two independently paired HTTP clients, server-side terminal discovery, agent-to-terminal identity, observation, exclusive input control, takeover fencing and actual exit. Managed status distinguishes observed exit from unknown state after restart. These checks do not establish a provider-specific managed orchestration strategy. |
| Managed terminal UI | Actual Chromium windows exercised the configured terminal controls and xterm input. Closing the first viewer left its PowerShell process running; a second viewer used a fresh ticket, discovered the terminal from the server, and acquired control. A computed marker appeared in xterm's rendered content and durable output. The exact child process was independently confirmed absent after exit. |
| Protected web transport | Headless Chrome used a synthetic HTTPS proxy and password verifier with a real Wayfinder workspace store. Login, two-project rendering, project switching, the selected-project header, the actual Sign out button, return to login and subsequent unauthorized access were observed. The live workspace has no demo marker. No public tunnel or real deployment credential was configured. |
| Model selection | The real Codex adapter under a deterministic JSONL host preserved the selected profile, model and effort through bootstrap, chat, stop, continuation and full service recreation despite policy/default changes. A no-inference native handshake independently confirmed the low thread setting. Configured Wayfinder policy preview resolved Luna/low and refused another project's access without launching a host. Requested, resolved and observed settings remain separate. |

## Validation boundary

Aggregate verification passed 165 Node tests, two tooling tests and 24 Vitest
checks, with one explicit external-backend fixture skipped in the default suite.
Type, formatting, lint, browser-boundary and Node/browser/Electron build checks
passed. Conform reports zero findings across all 36 cells without exemptions;
the specification map has zero orphaned code items, suspects or warnings.

Portable packaging is checked separately from behavioral integration: a clean
consumer installs the archive, resolves its public exports and starts the native
workspace service without launching a model. The artifact's accompanying receipt
records that result. Repeating a model comparison or a full cross-provider matrix
is outside this MVP acceptance.

The first implementation targets Codex. The shared contracts retain separate
native and managed execution modes for later adapters. Remote execution hosts,
full RLM strategies, Gamelens, IDE shells and the unified multi-project canvas
remain the explicit later work described in the delivery plan.
