# ZAP automatic coordinator and runner API

The runner is a persistent Python 3.11 standard-library coordinator. It uses
the composed application service for every event, a `ProcessTransport` for
actual argv execution, and an explicit asynchronous semantic coordinator
adapter. It never executes task/check prose as a shell command and never treats
a worker result as acceptance.

## Public construction

`zaplib.runtime` exports the maintained surface:

```python
AutomaticCoordinator(
    store,
    handlers,
    service,
    transport,
    semantic,
    config,
    *,
    clock_ns=time.time_ns,
    sleeper=time.sleep,
)
```

`handlers` is the complete explicit registry. `service` is the matching
`ApplicationService`; runtime action kinds must be in its `action_kinds`, data
kinds in `data_kinds`, and receipt kinds in `observation_kinds`. `transport`
implements the public `ProcessTransport` methods. `semantic` implements:

```python
submit(request_id, request) -> receipt
poll(request_id, request) -> {ready, ...}
request_stop(request_id, stop_id) -> receipt
```

`tick() -> dict` performs one nonblocking coordination pass. `run(*,
stop_event=None, max_ticks=None) -> dict` continues with the configured idle
poll interval. `max_ticks=None` has no arbitrary campaign limit. An empty
frontier by itself never schedules closure; `campaign_complete` is
true only for B closure classifications `original` or `revised`.
`campaign_finished` reports any operator/domain closure, and
`closure_classification` preserves partial/unreachable without advertising
success. The continuing loop stops on `campaign_finished`.

## Configuration

```python
WorkerProfile(
    argv,
    cwd,
    standing_rule_paths=(),
    environment={},
    resource_capacities={},
    review_capacity=1,
    integration_capacity=1,
    branch_for_work={},
    stop_mode="cooperative",
    terminate_after_seconds=None,
    required_inherited_environment=(),
)
```

`argv` has exactly one whole `{packet_file}` element. `environment` is passed
only to the private transport descriptor; it is not journaled or included in a
worker packet. Cooperative stop is the default. `stop_mode="terminate"`
requires both an explicitly termination-capable transport and a caller-supplied
positive delay; the runtime has no hardcoded forced-stop timeout.

```python
VerificationSpec(
    check_id, argv, cwd, target, toolchain, environment_label,
    subjects, cases, source_refs, environment={}
)
```

Each configured check binds actual argv/cwd, target, subjects, cases, toolchain,
environment label, and sources. Contract `checks` remain identifiers/bindings;
the runtime never shell-evaluates them. `RuntimeConfig(worker,
verifications={}, assessment_provider=None, safe_state_verifier=None,
artifact_capture=None, artifact_reader=None, artifact_allowed_root=None,
transient_backoff_ns=30000000000, idle_poll_seconds=.1)` supplies those profiles.

Legacy `assessment_provider(state, action)` callbacks return exactly
`{values,drain_targets}`. A nonblocking trusted adapter may instead expose
`assess(state, action, command_context)` and returns the same result or `None`
while its exact process observation is pending. Pending, stale, or malformed
assessment results defer only that action while receipt and stop reconciliation
continue. Missing rule inputs remain unknown and prevent spawn.
`safe_state_verifier(state,job,pause,transport_receipt)` is an effect
adapter that may return `{state,receipt}` only after checking the task's declared
safe boundary or isolated postconditions. PID exit, transport `stopped`, and
forced `interrupted` do not establish this proof.

`artifact_capture`, `artifact_reader` and `artifact_allowed_root` are configured together by the
trusted host. The callback signature is
`callback(resolved_path, allowed_root, *, source_id, source_kind,
applicability_scope) -> {source, blob}` and matches an
`artifacts.capture_source_blob` partial with its private store pre-bound. The
callback only captures bytes. E journals source recording, applicability,
dependency and closure through their separate trusted routes before binding the
artifact to a verification.
`artifact_reader(handle, expected_sha256, *, offset=...)` is a read-only
`PrivateArtifactStore.read` callback. E verifies every page binding and the
assembled bytes/hash, then includes the relevant immutable content in semantic
acceptance/reassessment together with the parsed, hash-verified worker candidate
report. Provider transcripts remain separate private transport artifacts.

When artifact capture is configured, each successful verifier writes one JSON
object to stdout: `{schema:"zap-verification-output/1", result:"pass",
artifacts:[{path,sha256,bytes}], summary}`. The configured target must appear
exactly once. Relative target and artifact paths resolve against the verification
cwd. E verifies the transport-recorded stdout bytes and hash, then requires the
new private capture to match the verifier's target hash and byte count. A change
before capture refuses the binding; a change after capture becomes source drift
and stales acceptance.

## Runtime projection and events

`runtime_state(state)` returns the detached `zap-runtime/1` projection.
`runtime_frontier(state)` returns domain-ready work with no unresolved runtime
attempt; it is not action authority. State under `extensions.runtime` contains
jobs, attempts, read/write/resource/integration reservations, verification
jobs, review requests, semantic requests, resource waits, observations and a
non-authoritative closure-status view.

`RUNTIME_HANDLERS`, `RUNTIME_EVENT_SCHEMAS`, and `RUNTIME_EVENT_ROUTES` have
the same keys. `RUNTIME_DATA_KINDS`, `RUNTIME_ACTION_KINDS`, and
`RUNTIME_OBSERVATION_KINDS` are disjoint and cover every handler.

Actions:

- `runtime.job-claimed` → `work.dispatch`
- `runtime.job-candidate-recorded` → `plan.lower`
- `runtime.verification-claimed` → `verification.run`
- `runtime.retry-released` and `runtime.job-reset-ready` → `plan.lower`

Trusted observations record submitted/status/result/stop receipts,
verification submission/results, semantic results, resource waits and central
acceptance already performed by domain. Review and semantic requests also use
the trusted host route because allowing a public data caller to enqueue them
could spend coordinator resources. `RUNTIME_DATA_KINDS` is therefore explicitly
empty; worker facts and domain proposals retain their owning data routes.
`RUNTIME_CAPABILITIES` exposes the complete structured registry to F.

The claim reducer validates current task-contract version/hash, actual source
captures, conflicts and capacities, then invokes B's public
`domain.work-dispatched` handler on the same copied projection before it stores
the runtime job/reservation. One journal event therefore cannot leave an active
domain node without a persistent job identity. External submission occurs only
after that claim commits.

## Packets and concurrency

The canonical `zap-worker-packet/1` includes campaign/base/state/domain/outcome
bindings, full current task contract, contract version/hash, exact read/write
subjects, shared resources, integration owner, standing-rule paths, configured
check bindings, safe boundary, source captures, job/attempt identities and
`producer_result="candidate_only"`. The packet text itself is stored in the
claim so restart can retry the exact transport descriptor without recompiling
against changed state.

Read claims serialize with any active writer on the same read/write subject,
two writers on the same subject, exhausted named resources, or exhausted
integration-owner capacity. Read/read overlap is allowed. Resource capacities,
review capacity and integration capacity are explicit configuration snapshots,
not universal token/plan limits. Active/candidate jobs are not redispatched.

## Tick order and recovery

Every tick loads and replays first, then:

1. reconciles durable worker and verification transport receipts;
2. refreshes relevant actual source bytes through the privileged
   `evidence.adjudicate` route;
3. delivers applicable stops and records transport delivery separately;
4. classifies terminal results as candidate, interruption or resource wait;
5. releases only due persisted retries;
6. runs configured narrow verification jobs;
7. discovers/coalesces evidence, source, contract/goal and fog review triggers;
8. polls existing semantic requests;
9. schedules the next review/reassessment/acceptance/selection request.

Applied adaptive reviews run through the composed reconciliation registry before
work advances. `reconciliation_blocks_progress(state, job_id)` prevents a
captured job from submitting, becoming candidate, or starting verification
until its continue/finish/drain/preserve/revalidate effect is durably resolved.
Drain delivery, process exit and independent safe-state proof remain separate.
Selective revalidation advances B's validation generation; old proof cannot
close or satisfy the fresh attempt.

The semantic process is asynchronous, so stop delivery and job reconciliation
continue while it runs. A response binds exact request hash and state revision.
Relevant policy/outcome, selected contract/source/frontier, accepted job/proof,
review-trigger or closure-denominator change makes it stale. An unrelated
append may proceed only after exact relevant-scope revalidation and a durable
`runtime.semantic-rebound` event binding old/new revisions, response hash and
basis hash. For a review command, E verifies the original review capture against
the stored request and then advances only its ZAP/domain CAS fields to the
post-rebind/current command boundary. The semantic decision and transition are
unchanged.
Transport `unknown_effect` stays live/unknown and is reconciled by receipt or
postcondition before another launch. A claim without a recorded submit receipt
reuses the stored packet/job ID; `ProcessTransport` idempotency discovers an
already submitted job rather than duplicating it.

Rate limit, quota, authentication and provider-unavailable classifications
create durable resource waits with observed and next-retry times. Server hints
are honored when present; otherwise the configured backoff is used. Retry
releases the same logical work without creating an architectural approach or
forgetting attempts. No credential switching/copying occurs.
Semantic coordinator transport failures use the same persisted wait discipline;
the old request is released at its recorded boundary before a new request is
created, avoiding a hidden retry loop around provider/CLI policy.
Deterministic configuration failures persist without a timer and release only
when relevant input or the hashed executable/profile changes. Invalid model
output preserves the raw response privately, returns structured validator
feedback, and permits one controlled repair request for the same logical basis;
a second invalid response waits for changed input/profile. Identical selection
`no_change` is not polled repeatedly. A review awaiting evidence reopens only
when its scoped sources, jobs, proof, fog or native facts change. Actionable
source/contract/native-fact invalidation requires an applied keep-route or pivot
review; plain `no_change` cannot clear it.

## Stops

A campaign pause blocks new actions through C immediately. Run/branch pauses
affect only matching reservations; campaign scope dominates. The runtime asks
the transport to stop each captured active run and records:

- the request and actual delivery;
- cooperative/explicit termination state;
- terminal process/result observation;
- separately verified task safe-boundary evidence.

If a captured job naturally finishes before signal delivery, E stores the
trusted terminal result hash and uses C's `already_terminal` delivery
resolution. It never claims the signal was delivered. This satisfies only the
delivery obligation; resume still requires the independent safe-state receipt.
`stopped` and `interrupted` cannot use already-terminal. Without a configured
safe verifier the runtime creates `interrupted_needs_reconciliation` review and
leaves the pause safe state unknown. Resume preserves attempt/failure history.

## Semantic coordinator and Codex bridge

`JsonProcessCoordinatorAdapter` runs every `zap-coordinator-request/1` as a
durable `ProcessTransport` job and validates exact `zap-coordinator-response/1`.
Responses contain request identity/kind/revision/hash, disposition, rationale,
nullable selection, and typed command drafts. Selection may name only captured
frontier work. Review/reassessment commands are restricted to B/D proposal
families. Acceptance commands are restricted to evidence, stage, integration
and work acceptance. Control activation/amendment/stop/resume cannot be emitted
by this adapter. Automatic closure is a separate semantic request emitted only
when every current active obligation is covered by current acceptance, no
deferral is open, and runtime jobs are resolved. Its only allowed command is
`domain.campaign-closed` with `original` or `revised`; partial/unreachable
closure remains an explicit operator/owner workflow.

Selection rows contain compact title/goal/conflict/stage/obligation/source
bindings plus exact source captures; full task steps and checks travel only in
the selected worker packet. Review, reassessment and acceptance requests carry
only the named review rows, related jobs/check receipts, active intent/outcome,
relevant obligations/ownership/contracts/proof, and source IDs/captures. They do
not serialize full domain, knowledge or history maps. Compact preservation-ID
inventories let a pivot name proof for B2 validation without copying proof rows.

For a pivot, the semantic command remains `domain.review-proposed`. Its normal
payload may use `transition.schema="zap-domain/sparse-review-transition/1"`
with `intent_id`, `outcome_id`, `changed_dispositions`, `ownership_changes`,
`work_changes`, the four `preserved_*_ids` lists, `job_reconciliation`,
`tradeoffs`, and `preserved_benefits`. Before service submission, E invokes
`domain.build_sparse_review_transition(state, transition)`, substitutes the
returned complete transition, and journals only ordinary
`domain.review-proposed`; the sparse wrapper never reaches replay.

The provider-facing output schema is fully closed. Immutable request bindings
are injected by the bridge, and command payloads arrive as `payload_json`
strings that the bridge decodes to objects and passes through the existing
exact handler validators. Every request also carries `command_contracts`,
generated only for its allowed kinds from `DOMAIN_EVENT_SCHEMAS` and
`KNOWLEDGE_EVENT_SCHEMAS`; review contracts include B's sparse-transition input
schema. Boot-disabled providers therefore receive the exact required fields,
enums, identifier bindings and no control-command contract.

`codex_sol_xhigh_profile(cwd, launcher=None)` supplies the verified
`codexrunner.ps1` bridge argv and required provider-owned auth-home environment
names. `codex_sol_xhigh_adapter(transport_root, allowed_workspace_roots, *,
cwd, launcher=None)` constructs the ready durable adapter. `None` discovers
`codexrunner`/`codexrunner.ps1` on PATH; an explicit portable path may be passed.
The bridge invokes:

```text
powershell.exe -NoProfile -NonInteractive -File <codexrunner.ps1>
  exec --strict-config --ignore-user-config
  -c project_doc_max_bytes=0
  -m gpt-5.6-sol
  -c model_reasoning_effort="xhigh"
  --output-schema <schema> --output-last-message <result> -
```

The prompt is passed on stdin and argv uses no shell interpolation. The default
profile preserves `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` and required platform
runtime variables for provider-owned authentication. ZAP control credentials
are never copied to worker/coordinator environment, packets or events.

`codex_sol_xhigh_worker_profile(cwd=..., launcher=None,
standing_rule_paths=..., sandbox="workspace-write",
approval_policy="auto-review")` is the matching ready coding-worker profile. It uses
`worker_provider_bridge.py`, strict config and Sol/xhigh, reads the complete
`zap-worker-packet/1`, passes only bounded contract/subject/rule/stop-file
instructions by stdin, and returns `zap-worker-candidate/1` with
`accepted=false`. Its `required_inherited_environment` lists provider auth-home
names for ProcessTransport construction. Pass that tuple as the worker
`ProcessTransport(inherited_environment=...)`; the profile never copies values
into the journal. The default auto-review policy passes `--approve-for-me`, whose
CLI contract selects the workspace-write sandbox; it does not also pass the
incompatible `-s` flag. With `approval_policy="never"`, the bridge passes the
explicit `-s read-only|workspace-write|danger-full-access` choice. The last
choice is only for an explicitly isolated/authorized host profile; the bridge
exposes no approval/sandbox bypass flag. The semantic
bridge always uses `-s read-only`. Generic worker argv profiles remain supported.

When a bound source is a native VibeVM XML spec and its bytes change, the runner
captures current fact markers through D's native adapter and records
`runtime.native-facts-observed` as unassessed data, then creates a scoped
reassessment request for known consumers. It does not silently rewrite accepted
facts.

## Reproducible live probe

`zaplib.runtime_live_probe.prepare_probe(root)` prepares and activates an
isolated one-task campaign without creating transport jobs or calling a model.
`execute_probe(root, *, launcher=None, timeout_seconds=900,
worker_sandbox="workspace-write")` uses the public worker/coordinator adapters,
exact verifier protocol, private output capture, domain acceptance and
success-only automatic closure. CLI preparation is the default:

```text
python -B -m zaplib.runtime_live_probe --root <isolated-root>
```

Execution is explicit. On a Windows host whose nested workspace sandbox cannot create
worker shell processes may select the owner-authorized isolated policy visibly:

```text
python -B -m zaplib.runtime_live_probe --root <isolated-root> --execute \
  --launcher <codexrunner.ps1> --worker-sandbox danger-full-access
```

The probe never enables Codex's approval/sandbox bypass flag. Its worker still
receives one exact write subject and candidate-only contract, and the runtime
reuses a completed worker/check when only semantic acceptance needs repair.

## Acceptance boundary

Worker completion becomes `candidate`. Configured verification produces a core
observation with bounded transport artifact hashes. The semantic acceptance
response proposes B evidence adjudication, stage, integration and work
acceptance commands; each goes through `apply_host_action` and the exact domain
checks. A green process without configured/accepted evidence remains
unaccepted. Adaptive review proposals go through B and are applied only through
`adaptive.apply`. Sparse review changes remain B's responsibility; the runner
does not generate or rerun the full 1,292-obligation denominator.
