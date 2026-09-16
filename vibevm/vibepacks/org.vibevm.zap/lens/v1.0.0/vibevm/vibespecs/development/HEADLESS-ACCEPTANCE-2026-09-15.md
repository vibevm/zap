# Headless lens acceptance

Accepted boundary: the shared `lens/1` communication foundation. Quicklens UI,
ZAP plan-control orchestration and source-change reassessment remain subsequent
work; this receipt does not mark the whole client complete.

## Package and structural checks

The corrected `@org.vibevm.zap/lens@0.1.0` npm artifact has SHA-256
`72be4256a92f9b9029634e371db1fae29d9dbc5d7f698d454551933c2b1b5b0e`.
It includes the compiled CLI/MCP entries, shared modules, integration templates,
plugin, README, license, Vibe manifest and product specifications. A fresh
offline consumer installed it; all six packaged README links resolve.

- Seven-step TypeScript floor: all steps passed, none disabled.
- Node tests: 35/35, including setup-issued finish/forward capabilities.
- Per-cell fast loop: 7/7.
- Conform: zero findings, seven gated cells, zero exemptions.
- Specmap: 22 units, 46 tagged items, 66 edges; zero suspects or warnings.
- Test-gate: 35 parsed passing results.
- Diagnostic-rule parity tests: 2/2; Vitest type tests: 2/2.
- Guarded clean production build, plugin validation and skill validation passed.

The earlier artifact ending in SHA-256 `...6eff54531` is retained as historical
evidence. Its setup lacked lifecycle capabilities; it is not the accepted input.

## Real Codex CLI round trip

The coordinator tested the corrected package from a separate offline-installed
consumer on Windows, with Node 24.18, Codex CLI 0.152.1 and `gpt-5.6-luna` at
low reasoning effort. The broker ran as a separate hidden background process.
Generated test credentials and data stayed in an isolated local scope.

1. A real Codex run called `codlens_connect` and `codlens_ask`. Both succeeded.
   The tool returned an open question with no answer, and Codex finished its
   turn without waiting for a person.
2. A separate CLI call supplied a test answer using the human-responder role.
3. A second real Codex run, with a new MCP connector process, used the same
   addressed handle to call `codlens_inbox` and `codlens_ack`. Both succeeded,
   and the model returned the received `LENS_E2E_OK` marker.
4. An independent CLI inbox read returned no pending deliveries, verifying the
   acknowledgement against broker state. Credential-value checks found no
   fixture secrets in the model logs. The owned test broker was stopped.

The initial Codex invocation required MCP tool approval and refused the call
under its noninteractive policy. The successful runs used invocation-local
documented approval settings for only the explicitly allowlisted test tools.
No global Codex settings or permission policy were changed. Tool permission
does not grant human-answer or ZAP plan authority.

## Host and lifecycle evidence limits

Tests cover separate parent, child and nested-child MCP processes, restart,
canonical retries, fenced bindings, scope isolation, out-of-order answers,
deadlines, explicit amendment, sparse acknowledgement and bounded forwarding.
An end-to-end setup-produced credential test also covers a child finishing
before a delayed human answer and the parent's policy-checked forwarding.

Codex, Claude Code, Qwen Code and OpenCode have explicit routing fixtures based
on their documented native identities. Ambiguous parent-only events refuse
automatic routing; actor display names do not select recipients. These fixture
checks are distinct from live host inference. The live model round trip above
proves Codex MCP calls and durable delivery, not unsolicited idle wake.

Qwen Code 0.23.4 was installed and a short free OpenRouter model trial was made.
It returned HTTP 403 before a model response, followed by a Windows shutdown
assertion. No live Qwen inference, native channel wake, or live Claude/OpenCode
inference is claimed by this receipt.
