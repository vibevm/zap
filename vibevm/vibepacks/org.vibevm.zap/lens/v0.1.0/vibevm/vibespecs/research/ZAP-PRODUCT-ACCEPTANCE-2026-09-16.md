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
- Multiple top-level development plans in one registered Git project, with the
  original checkout adopted in place, owned root and isolated managed-child
  worktrees, fresh HEAD/CAS checks, separate integration checkout, bounded diff,
  registered test evidence, human review and writer-fenced promotion. Exact
  plan/worktree/integration nodes support shared notes and Trash semantics.

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

Later bounded coordinator probes exercised the shared provider lifecycle.
Codex/Luna and Claude Code/Haiku completed question publication, idle Pause,
answer retention, Continue and automatic wake/acknowledgement, active-turn
interrupt, same-conversation context retention, one bootstrap and observed
Stop. Qwen Code with a free model completed the main question/pause/wake/context
flow. OpenCode completed the same main flow after its request shape was
corrected. The Qwen and OpenCode live receipts retain cleanup failures from the
old exit-unsubscription ordering; they are not retroactively relabelled as
passes. The exact shared raw-exit-to-settled Stop repair subsequently passed
four focused public no-model lifecycle tests.

The real shared-canvas run rendered three contexts and 14 selectable nodes in
both light and dark product themes. The deterministic layout retains named
contexts, worktree lanes, source-owned integration artifacts and the one
recorded cross-context merge target. Readability was reviewed on the actual
1600×1100 workspace rather than only a graph fixture.

## Verification boundary

The joined final source gate passed all five TypeScript configurations, Node
274 passed/zero failed/one explicit external Rust-fixture skip, tooling 2/2,
Vitest 24/24 across nine files, and clean lint, formatting, browser boundary and
build. The final zero-inference corpus passed 14/14 and reported 50 explicit
coverage gaps (`coverageComplete = false`) rather than disguising them as test
failures or completed scope.

Conform reports zero findings across 46 gated cells and zero exemptions.
Specmap reports 153 units, 489 tags, 562 edges, zero suspect links, zero orphan
roots and 16 visible warnings. The portable verification commands are:

```text
typescript-ai-native conform check --path .
typescript-ai-native specmap --check --path .
typescript-ai-native specmap --gate --path .
```

The reviewed generated metadata is refreshed separately before these checks;
the commands above verify it without hiding warnings or creating exemptions.

Real PTY checks verified process exit and release of owned Windows terminal
resources. Upstream node-pty can print `AttachConsole failed` during cleanup
after confirmed exit; the test file completes naturally and leaves no owned
descendants. No dependency patch or forced test-process exit is used for the
accepted result.

Installed-package verification is recorded with the final artifact receipt.
Receipts and screenshots contain synthetic task data; credentials and local
pairing material are excluded from this repository.

## Limits retained

The Qwen and OpenCode common Stop repair has deterministic exact-mechanism proof
after their live main-flow receipts; those two full inference flows were not
repeated solely to replace cleanup status. Bounded inbox waiting does not
promise universal host wake or implicit approval.

Model policy does not invent compatible profiles or silently substitute a
provider. Native children remain host-owned, while managed work uses owned
processes and exact attempts. Dynamically supplied trusted planning-source
configuration must be reconnected after restart; public state becomes
unavailable/pending rather than pretending the source was restored. Full RLM
orchestration, remote execution hosts, distributed locks, multi-user/crowd
execution and admission, public tunnel deployment, Gamelens and IDE shells are
outside this slice.
