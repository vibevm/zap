# ZAP Python API

The supported embedding surface lives beside `zap.py` in `zaplib`. Python 3.11+
and the standard library are sufficient. Import public facades (`engine`,
`control`, `domain`, `knowledge`, `runtime`, `backend`) rather than private
model/reducer modules.

## Composition and storage

```python
from zaplib.engine import (
    Engine, build_engine, ENGINE_HANDLERS, ENGINE_DATA_KINDS,
    ENGINE_ACTION_KINDS, ENGINE_OBSERVATION_KINDS,
    ENGINE_EVENT_DESCRIPTORS,
)

engine = build_engine(store, trust=None, host_principal=None)
state, events, pending_tail = engine.load()
capabilities = engine.capabilities()
```

The engine composes the core/control/domain/knowledge/runtime registries and
refuses route omissions or collisions. `capabilities["events"]` is the current
machine-readable event contract; `capabilities["operations"]` includes the
runtime profile JSON Schema and the sparse review builder. Do not maintain a
second hand-written copy of nested payload schemas.

Low-level storage functions are:

```python
import_mup(plan_path, tasks_dir, out, *, fault=None) -> dict
load_store(store, handlers=ENGINE_HANDLERS) -> (state, events, pending_tail)
record(store, command, handlers=ENGINE_HANDLERS) -> dict
create_snapshot(store, snapshot_path, handlers=ENGINE_HANDLERS,
                *, reducer_version="zap-reducer/1", capture_hook=None) -> dict
load_snapshot_tail(store, snapshot_path, handlers=ENGINE_HANDLERS,
                   *, reducer_version="zap-reducer/1",
                   verification_cache=None) -> dict
repair_pending_tail(store, *, repair_id, expected_tail_sha256,
                    handlers=ENGINE_HANDLERS, fault=None) -> dict
```

`record` is a storage primitive. An application must expose untrusted commands
through `ApplicationService`; handing raw `record` to a caller bypasses the
trusted routing boundary.

`Engine` creates one process-local `ProjectionCache` and injects its loader and
recorder into `ApplicationService`; backend and runtime therefore share it.
`ProjectionCache.load(store, handlers)` rereads exact base/journal bytes on
every call, returns detached state/events, replays only complete appended
suffix lines, and cold-replays when base or reducer objects change. A changed
already-verified journal prefix refuses `CACHE_PREFIX_DRIFT`; after reviewing a
deliberate store replacement, call `cache.invalidate(store)` explicitly.
`cache.stats()` reports cold, warm, suffix, drift and invalidation counts. The
cache is guarded by an in-process reentrant lock, changes no disk format and
sets no plan-size or event-count limit.

## Trust and application service

```python
from zaplib.control_trust import CredentialAuthority, Principal
from zaplib.service import ApplicationService

service = ApplicationService(
    store, ENGINE_HANDLERS, trust, host_principal=None,
    action_kinds=ENGINE_ACTION_KINDS,
    data_kinds=ENGINE_DATA_KINDS,
    observation_kinds=ENGINE_OBSERVATION_KINDS,
)
```

Public methods are:

```text
submit_agent(command)
submit_control(command, *, credential_id, credential)
submit_observation(command, *, credential_id, credential)
submit_host_observation(command)
submit_host(command)
apply_control_action(command, action, assessment,
                     *, credential_id, credential, exception_id=None)
apply_host_action(command, action, assessment, *, exception_id=None)
authorize_read(*, credential_id, credential)
route_descriptors()
```

`submit_host*` and `apply_host_action` are only for a trusted process holding a
configured `Principal`. Never expose them as generic worker routes. Credential
bindings are opaque and campaign scoped. `bootstrap_trust(store, state,
trust_dir)` creates owner/coordinator/reader material outside the campaign;
`load_trust(config_path, state, *, store=None)` loads it at trusted startup.

Control helpers are exported from `zaplib.control`:

```text
active_charter(state) -> dict | None
active_policy(state) -> dict | None
control_state(state) -> dict
pause_applies(state, *, branch_id=None, run_id=None) -> list[dict]
require_action(state, action_class, *, branch_id=None, run_id=None) -> dict
assess_action(state, action, assessment) -> dict
validate_action(value) -> dict
validate_assessment(value) -> dict
```

`CONTROL_EVENT_SCHEMAS`, `CONTROL_VALUE_SCHEMAS`, owner/coordinator/internal
kind sets and `ACTION_CLASSES` are public registry data. A pure assessment does
not itself prevent an effect or authorize an already executed action.

## Domain and knowledge

`zaplib.domain` exports `DOMAIN_HANDLERS`, `DOMAIN_EVENT_SCHEMAS`, explicit
data/action routes, `domain_state`, `domain_frontier`,
`current_acceptance_coverage` and `intent_fingerprint`.

For a large adaptive review, call:

```python
build_sparse_review_transition(state, request) -> dict
```

The input must match `SPARSE_REVIEW_TRANSITION_SCHEMA`. The function expands
omitted active obligations to complete retained rows and mutates nothing. Only
the returned full object is valid as `domain.review-proposed.transition`.

`zaplib.knowledge` exports `KNOWLEDGE_HANDLERS`, `KNOWLEDGE_EVENT_SCHEMAS`,
`KNOWLEDGE_EVENT_ROUTES`, `knowledge_state`, `knowledge_snapshot` and
`invalidation_closure`. Event descriptors are exact nested contracts; source
capture is an effect-adapter observation, while applicability/closure/proof
adjudication remains an action. JSON Schema validates the complete payload
shape; reducers additionally enforce persisted references, revisions, cycles
and identity/hash relationships named by `x-zap-state-validation`.

## Captured content

```python
from functools import partial
from zaplib.artifacts import PrivateArtifactStore, capture_source_blob

artifacts = PrivateArtifactStore(private_root, create=True)
capture = capture_source_blob(
    artifacts, path, allowed_root,
    source_id=None, source_kind="file", applicability_scope=None,
)
```

The result is exactly `{source, blob}`. The descriptor binds canonical root,
relative path, hash and byte count; the blob descriptor binds the private
content-addressed version. `read_registered_source` and
`PrivateArtifactStore.read` accept only a registered handle and expected hash.
Portable promoted proofs preserve both the exact
`source_captures_at_adjudication` witness and B's `validation_generations`
mapping. A legacy evidence row without that mapping materializes generation 0;
no later proof generation is inferred.

Permanent promotion from `zaplib.knowledge_promotion` is deliberately
effect-injected:

```text
build_promotion_proposal(state, *, promotion_id, fact_id, source_refs, target)
promote_fact(state, proposal, project_root,
             *, authorization, event_writer, fault=None)
```

The authorization and event writer are trusted application callbacks. A failed
event receipt reports or rolls back the owned filesystem effect truthfully; it
does not invent a committed promotion.

## Runtime and provider bridges

`zaplib.runtime` exports the composed runtime registry and:

```text
WorkerProfile(...)
VerificationSpec(...)
RuntimeConfig(worker, verifications={}, assessment_provider=None,
              safe_state_verifier=None, artifact_capture=None,
              artifact_reader=None,
              artifact_allowed_root=None,
              transient_backoff_ns=30000000000,
              idle_poll_seconds=0.1)
AutomaticCoordinator(store, handlers, service, transport, semantic, config,
                     *, clock_ns=time.time_ns, sleeper=time.sleep)
runtime_state(state) -> dict
runtime_frontier(state) -> list[str]
```

`RuntimeConfig.artifact_capture`, `artifact_reader` and
`artifact_allowed_root` are configured together. The callback signatures are:

```text
callback(resolved_target, artifact_allowed_root,
         source_id="artifact:<attempt_id>:<check_id>",
         source_kind="file", applicability_scope=None) -> {source, blob}
artifact_reader(handle, expected_sha256, *, offset=0, limit=65536)
                -> zap-content/1
```

The coordinator records capture, applicability, dependency and closure before
accepted verification evidence. A convenient binding is
`partial(capture_source_blob, artifacts)` together with `artifacts.read`.
Acceptance/reassessment requests may inline only the verified registered output
and parsed candidate report relevant to that review; credentials, private
prompts and provider transcripts remain outside the request.

For semantic owner stop predicates, `zaplib.runtime_assessment` exports:

```text
JsonProcessAssessmentAdapter(transport, *, argv, cwd)
build_assessment_request(state, action) -> dict
validate_assessment_response(request, value) -> dict
```

The adapter callable returns `None` while its ProcessTransport job is pending,
then `{values, drain_targets}`. `assessment_for` turns pending into
`ASSESSMENT_PENDING`, which the coordinator defers while continuing receipt and
stop reconciliation. The request binds exact action, policy, base, source set
and ZAP revision. A changed revision gets a different durable request ID; a
stale response refuses. Provider failure/null values remain unknown. The
adapter never accepts drain targets, credentials or an actor role from its
response.
`JsonProcessAssessmentAdapter.status(request_id=None)` returns the non-secret
request/action/policy/source binding, transport state/receipt hash and completed
values. `BackendApplication.assessment_status` exposes the same projection to
an authenticated reader at `GET /v1/assessments`.

The durable `ProcessTransport(root, allowed_workspace_roots, *,
inherited_environment=None, environment_allowlist=(),
allow_process_termination=False)` exposes `submit(job_id, *, argv, cwd, packet,
environment=None)`, `status(job_id)`, `reconcile(job_id)`, `collect(job_id)` and
`request_stop(job_id, request_id, *, mode="cooperative",
terminate_after_seconds=None)`. Forced termination requires both construction
capability and an explicit positive delay; exit and task safe-state proof remain
separate.

Ready adapters are:

```text
codex_sol_xhigh_adapter(transport_root, allowed_workspace_roots,
                        *, cwd, launcher=None)
codex_sol_xhigh_worker_profile(*, cwd, standing_rule_paths=(), launcher=None,
                               resource_capacities=None, review_capacity=1,
                               integration_capacity=1, branch_for_work=None,
                               sandbox="workspace-write",
                               approval_policy="auto-review")
```

The worker profile accepts only `read-only`/`workspace-write` and
`auto-review`/`never`; no bypass mode exists. PowerShell launchers require
`pwsh`; a native `codexrunner` is invoked directly. Construct worker
`ProcessTransport` with both `profile.required_inherited_environment` and the
keys of `profile.environment` in `environment_allowlist`.

The isolated release probe is:

```text
prepare_probe(root) -> dict
execute_probe(root, *, launcher=None, timeout_seconds=900.0) -> dict
python -B -m zaplib.runtime_live_probe --root ROOT [--execute]
```

Prepare mode activates only its isolated fixture and creates no provider/
transport job or result artifact. Execute mode performs real external effects;
it requires the caller's authorization and is never run by installation.

## Backend

```text
BackendApplication(engine, artifacts=None, coordinator=None)
create_server(application, *, host="127.0.0.1", port=0,
              allow_nonlocal=False, allowed_origins=(),
              max_body_bytes=2097152, max_follow_seconds=30.0)
serve(application, *, host="127.0.0.1", port=8765, ...)
```

The framework-independent application exposes snapshot/tail/graph/search/
detail/content reads plus the same service-backed command routes. `/v1/stream`
is a finite SSE batch and `/v1/follow` is a bounded live subscription. A tick
credential must exactly equal the configured host principal. See
[the HTTP contract](ZAP-BACKEND-API.md) for paths and recovery behavior.

External byte/process effects occur after durable local claims and are
reconciled by receipts/postconditions. The package does not claim a distributed
transaction across the journal, filesystem, subprocess or provider.
