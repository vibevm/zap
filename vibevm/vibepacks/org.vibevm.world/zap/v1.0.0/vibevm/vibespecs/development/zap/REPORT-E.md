# ZAP-E automatic coordinator report

## Result

The ZAP package now contains a persistent automatic coordinator rather than a
frontier printer. It performs real durable worker and verification subprocess
execution, source refresh, action admission, conflict-aware dispatch,
asynchronous semantic selection/review/reassessment/acceptance, candidate
collection, central acceptance and restart reconciliation.

`runtime.py` is the public facade. Pure projection handlers live in
`runtime_model.py`; packet/config/action construction in `runtime_packets.py`;
the continuing coordinator in `runtime_loop.py`; scheduling, liveness, source
refresh, generated-artifact capture and review reconciliation in the focused
`runtime_*.py` companions; relevant semantic revalidation and closure views in
`runtime_semantic.py`; the provider-independent JSON
semantic adapter in `coordinator_adapter.py`; and the ready Codex Sol/xhigh CLI
bridges in `provider_bridge.py` and `worker_provider_bridge.py`. Every
implementation module remains below 500 lines.

Runtime review/semantic requests are trusted-host observations rather than
public agent data, preventing an untrusted caller from enqueueing model spend.

## Durable behavior

`runtime.job-claimed` atomically invokes B's domain dispatch reducer and stores
the exact job, attempt, packet and reservation before external spawn. The exact
packet survives restart, so an ambiguous submit is reconciled by
ProcessTransport job/descriptor idempotency rather than relaunched blindly.
Running, stopped, interrupted, candidate, accepted, waiting and unknown-effect
states remain distinct.

Read/write subject conflicts, named resource capacity, integration ownership
and review capacity are checked at claim. Parallel independent tasks overlap;
conflicting tasks receive a durable rejected semantic result and are selected
again only after the reservation clears. There is no arbitrary plan/token cap.

Worker output remains candidate. Blocked, stopped, invalid and failed worker
reports cannot become candidates. Verification runs only explicitly configured
argv bindings; a failed verifier cannot enter acceptance. Successful verifier
stdout binds the exact generated path/hash/bytes, which E captures in the
private artifact store, explicitly assesses for applicability/closure, rechecks
for drift and supplies as immutable content to semantic review together with the
parsed worker report. The semantic adapter can propose
evidence/stage/integration/work acceptance, but every transition passes the
trusted service and B's current outcome/obligation/stage/source checks. A green
worker with no configured proof remains unaccepted.

Relevant source bytes are reobserved before work advances. Known source
invalidation and task-contract/goal changes create scoped, coalesced adaptive
review requests. Changed native VibeVM XML facts receive a bound unassessed
capture plus reassessment trigger. Unrelated work is not named by the known
affected closure. Model calls are asynchronous; transport reconciliation and
stop delivery occur first on every tick. Relevant changes stale a response;
unrelated appends require an explicit stored relevant-scope rebind and never
rewrite model semantics.

Transient quota/rate-limit/provider failures become persisted waits with retry
times and unchanged attempt history. They do not create architectural approach
failures. Semantic-provider failures likewise wait durably before a fresh
request. Unknown external effect never retries without reconciliation.
Deterministic configuration failures have no timed retry and release on a
relevant input or hashed executable/profile change. Invalid model output is preserved
privately with validator feedback and receives one controlled repair attempt on
the same logical basis. Identical no-action selection is suppressed. Relevant
new evidence reopens awaiting review; unrelated fog does not.

Applied review job reconciliation is durable and blocks submit/advance until
continue/finish/drain/preserve/revalidate effects complete. Revalidation increments
B's proof generation, so old evidence cannot satisfy the new attempt. Trusted
nonblocking action assessments preserve one stable prepared claim while their
process result is pending. Runtime loads share the service projection cache.

## Stop truthfulness

Campaign and scoped pauses stop new admission immediately. The runtime records
stop request, actual delivery, termination/process state and task safe-boundary
proof separately. Transport exit alone does not acknowledge the task boundary.
Without a configured postcondition verifier it records
`interrupted_needs_reconciliation` and keeps safe state unknown.

A captured job that naturally succeeds/fails before delivery uses C's bound
`already_terminal` resolution with its attempt/descriptor/result hash and
`signal_delivered=false`. Safe state still requires independent verified output
or postcondition evidence before owner resume. Cooperative `stopped` and forced
`interrupted` cannot take that route. Forced termination is opt-in at transport
construction and requires an explicit per-profile delay.

## Real semantic adapter

The JSON coordinator adapter uses ProcessTransport and an exact request/response
schema. Provider output uses a fully closed schema with payload JSON strings
and bridge-injected immutable bindings. The ready bridge passes a bounded prompt by stdin to the verified local
`codexrunner.ps1` Sol/xhigh lane with strict config, repository boot disabled,
output schema, and output-last-message. Provider auth homes are preserved by
name in the private process environment; ZAP control credentials are not.
No real model call was made in E tests; root owns the separate Sol proof.
The matching ready coding-worker bridge consumes `zap-worker-packet/1`, passes
bounded task/rule/stop instructions to Sol/xhigh and emits an explicitly
unaccepted candidate report.

Automatic success closure is requested only after current active obligations
are covered by current acceptance and runtime work is resolved. The automatic
adapter permits only original/revised closure. Finished state, classification
and successful completion are reported separately.

## Live Sol/xhigh proof

Root ran `zaplib.runtime_live_probe` in an isolated non-Git campaign through the
packaged worker and semantic bridges. The actual Sol/xhigh worker produced the
exact 15-byte artifact with SHA-256
`19b32baf08503ceab0fc41f2e4880162cc8528ec040be59e71eed35787de65af`.
The configured verifier, private output capture, applicability/closure
adjudication, central evidence/stage/work acceptance and automatic success
closure completed. Earlier preserved probe stores retain the launcher,
sandbox-policy and invalid-response diagnostics; the successful worker/check
was reused while semantic response schema was repaired.

## Verification

Focused E tests cover:

- registry/schema/route partition and forbidden control output;
- real JSON subprocess semantic adapter plus Codex bridge argv/stdin/schema;
- inactive and unknown-assessment zero-spawn behavior;
- no acceptance from worker success without configured proof;
- activated real worker → candidate → real configured check → evidence/stage/
  work acceptance;
- overlap for independent work and serialization for conflicting writes;
- restart while/after external completion without duplicate attempts;
- persisted quota backoff and retry without approach pollution;
- source-closure and task-goal selective invalidation;
- native-facts refresh, relevant-scope semantic rebind and evidence-gated
  success closure;
- exact fog/dependency/applicability/native-fact context, sparse pivot expansion
  and current B2 proof carryover;
- generated output capture/content review and post-verification/closure drift;
- controlled semantic repair, no-action suppression and evidence reopening;
- async trusted assessment with stable prepared command identity and shared
  projection cache;
- full adaptive job reconciliation and validation-generation proof refresh;
- campaign stop, scoped stop, cooperative delivery, verified safe state, resume;
- natural terminal-before-delivery without a false delivered claim.
- outside-workspace verification refusal and a ready bounded coding-worker
  Sol/xhigh bridge.

Command:

```text
python -B -m unittest test_runtime_model.py test_coordinator_adapter.py test_worker_provider_bridge.py test_runtime_admission.py test_runtime_invalidation.py test_runtime_loop.py test_runtime_recovery.py test_runtime_stop.py test_runtime_closure.py test_runtime_semantic.py test_runtime_artifacts.py test_runtime_live_probe.py test_runtime_liveness.py test_runtime_reconciliation.py test_runtime_assessment.py
```

The focused suite contains 56 tests and passed in 531.103 seconds. Its unit and
process fixtures made no network/model calls. The separately authorized root
live probe supplied the real model proof above. No NEXT execution, publication,
Git mutation, live user-local product mutation or full product panel occurred.
