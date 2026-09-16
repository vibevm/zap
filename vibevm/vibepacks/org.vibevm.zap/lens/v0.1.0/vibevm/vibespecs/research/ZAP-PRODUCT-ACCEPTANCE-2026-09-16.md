# Zap local product acceptance — 2026-09-16

This slice implements the local product workflows in PROP-010 and PROP-011.
The shared Wayfinder service owns state and agent channels; Quick Lens is the
browser/Electron presentation. Gamelens and native IDE shells remain future
clients of the shared interfaces.

## Accepted user flows

- Normal empty startup, protected provider discovery, project registration
  without inference, and explicit coordinator launch with visible model choice.
- Separate project/context state, global and scoped event history, a single
  multi-project map, semantic object cards, and an agent network with output
  and managed terminals.
- Managed task preparation, policy-selected worker model and effort, explicit
  reasoned profile overrides, start/interrupt/stop, typed report and separate
  human review. Later policy changes do not alter an already prepared run.
- Human question groups with choices, multiple answers, custom/text input,
  draft retention, cancellation, amendment and readable answer history.
  New questions and reports update the open view without erasing drafts or
  reclaiming another project/context's focus.
- Exact object notes and deferred instructions; versioned delivery at a
  declared work boundary; recoverable Trash and explicit restore/relink.
  Partial snapshots and filters do not imply deletion. Restoring a removed
  object proposes normal planning work rather than reviving an old plan.
- Shared process-local proxy configuration for owned Codex, Claude Code,
  OpenCode and Qwen Code launches, including managed workers. Profile overrides,
  direct mode and loopback bypass preserve local control-channel access.

## Real agent and browser evidence

An actual installed Qwen Code 0.23.4 process used OpenRouter's
`poolside/laguna-s-2.1:free` through an HTTP proxy. The managed run obtained its
assigned authenticated context and published a structured question. Quick Lens
was already open on Questions before publication; the new question appeared
without navigation or reload.

The browser submitted a random answer generated only after question arrival.
The same actor consumed the delivery through the bounded inbox channel,
explicitly acknowledged its delivery ID, and submitted a typed report containing
that answer. Persisted broker records verified acknowledgement independently of
the report. After restarting Wayfinder, the same answer, history and report were
read back and the report was accepted using the human UI. The restart/review
step started no model or coordinator.

The original harness compared report text byte-for-byte and rejected one extra
space after a fixed prefix. The persisted report contained the exact random
answer. A separate no-model continuation checked the answer and delivery
identity, then completed browser review; no extra inference was spent on text
whitespace. A separate replay mismatch used different path separators with the
same request ID; preserving the original request bytes made replay succeed.

This live proof also found and corrected scoped MCP permission prompts, missing
event-driven question refresh, and hidden answer values in the answered view.
Generated Zap MCP communication/work tools receive exact per-run permissions;
shell/files, arbitrary MCP servers and plan mutation tools keep normal approval.

## Verification boundary

The full Node suite completed naturally: 208 passed, zero failed, one explicit
external Rust-fixture skip. Focused checks cover subsequent UI refresh, context
fencing, draft preservation and answer presentation. Tooling tests passed 2/2
and Vitest passed 24/24. Strict types, formatting, lint, browser boundary and
Node/browser/Electron builds passed. Conform has no findings or exemptions;
specmap has no orphan roots or suspect links. Existing specmap warnings remain
visible rather than being exempted.

Real PTY checks verified process exit and release of owned Windows terminal
resources. Upstream node-pty can print `AttachConsole failed` during cleanup
after confirmed exit; the test file completes naturally and leaves no owned
descendants. No dependency patch or forced test-process exit is used for the
accepted result.

Installed-package verification is recorded with the final artifact receipt.
Receipts and screenshots contain synthetic task data; credentials and local
pairing material are excluded from this repository.

## Limits retained

Codex's earlier native live evidence remains separate from this managed Qwen
proof. Claude Code and OpenCode adapters have deterministic transport and launch
tests; the current cycle does not claim four-provider live-model coverage.
Non-Codex Pause is unsupported where no verified suspension primitive exists.
Bounded inbox waiting does not promise universal host wake or implicit approval.

Model policy does not invent compatible profiles or silently substitute a
provider. Native children remain host-owned, while managed work uses owned
processes and exact attempts. Full RLM orchestration, remote execution hosts,
public tunnel deployment, Gamelens and IDE shells are outside this slice.
