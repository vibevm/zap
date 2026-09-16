# ZAP control and authorization API

This file freezes the integration surface implemented by `zaplib.control` and
`zaplib.service`. Trust configuration is supplied to the application service by
its host and is never campaign state. Replaying accepted events does not consult
credentials; admitting a new privileged event always does.

## Delegated action classes

The wire strings are immutable for `zap-control/1`:

```text
outcome.adopt
adaptive.apply
task.update
evidence.adjudicate
work.accept
stage.accept
fact.promote
campaign.close
work.dispatch
verification.run
plan.lower
```

Draft proposals, unverified observations and classifications are data-only.
The domain mapping is:

- expected outcome adoption: `outcome.adopt`
- adaptive review application: `adaptive.apply`
- task-contract replacement and formal-deferral state changes: `task.update`
- evidence applicability or acceptance: `evidence.adjudicate`
- producer/work acceptance: `work.accept`
- achieved-stage acceptance: `stage.accept`
- accepted fact or integration promotion: `fact.promote`
- campaign closure: `campaign.close`
- dispatch into execution: `work.dispatch`
- execution of a verification: `verification.run`
- structural lowering or work-graph transition: `plan.lower`

All privileged product transitions honor a sticky pause. Collecting already
available results and preparing a proposal for the owner remain data-only.

## Domain-facing policy view

```python
active_policy(state) -> None | dict
active_charter(state) -> None | dict
pause_applies(state, *, branch_id=None, run_id=None) -> list[dict]
require_action(state, action_class, *, branch_id=None, run_id=None) -> dict
assess_action(state, action, assessment) -> dict
```

`active_policy` returns `None` until a charter is activated. Otherwise it
returns a fresh dictionary with at least this exact stable core:

```json
{
  "campaign_id": "campaign-id",
  "base_sha256": "64 lowercase hex characters",
  "revision": 1,
  "intent_binding": {"intent_id": "intent-id", "sha256": "64 lowercase hex characters"},
  "allowed_actions": ["outcome.adopt"],
  "adaptation": {
    "allow_target_revision": true,
    "mutable_obligations": ["obligation-id"],
    "essential_obligations": ["obligation-id"],
    "allowed_dispositions": ["retained", "replaced", "excluded", "unattainable"]
  }
}
```

The implementation also reports `charter_id`, `charter_sha256`, the active
versioned `stop_policy`, every active scoped pause in `pauses`, and `pause` as
a compatibility selection (campaign first, otherwise the first ID). Callers must ignore
unknown additive fields. `require_action` validates the immutable action class,
active charter, delegation and sticky pause, returning the same policy view or
raising `Refusal`. A one-revision internal grant created by the application
service is the only exception to pause refusal.

`assess_action` is pure. Its exact `action` input is:

```json
{
  "schema": "zap-action/1",
  "action_id": "stable-action-id",
  "action_class": "work.dispatch",
  "campaign_id": "campaign-id",
  "base_sha256": "64 lowercase hex characters",
  "charter_revision": 1,
  "payload_sha256": "64 lowercase hex characters",
  "source_captures": [
    {"source_id": "source-id", "sha256": "64 lowercase hex characters"}
  ],
  "branch_id": null,
  "run_id": null,
  "problem_id": null
}
```

`branch_id`, `run_id` and `problem_id` are nullable stable identities. Source
captures are sorted and unique by `source_id`; changing payload bytes or any
capture requires a new assessment. A nonempty capture list is checked against
the persisted `extensions.knowledge.sources`/`source_captures` projection.
Missing, invalidated, unavailable, hash-mismatched or unknown sources produce
`needs_evidence`.

The exact `assessment` input is:

```json
{
  "schema": "zap-assessment/1",
  "assessment_id": "stable-assessment-id",
  "policy_id": "stop-policy-id",
  "policy_revision": 1,
  "phase": "before_action",
  "values": {"changes_public_format": true},
  "drain_targets": ["run-id"]
}
```

`phase` is `before_action` or `after_action`. Assessment values are concrete
JSON scalars or null; a missing or null value needed by an applicable rule is
unknown. `drain_targets` is a sorted unique list of current runs captured by
the caller, then checked for exact equality with affected active jobs in the
persisted runtime projection. Omission of a running or unknown-effect job is a
refusal. Results distinguish `clear`, `needs_evidence`, `pause` and
`too_late`, and separately expose evaluation, stop-delivery and actual safe
state. Pure assessment reports `eligible_for_admission` and `would_prevent`.
It always returns `action_admitted=false`, `prevention_performed=false` and
`prevented_action=false`; only the application service and a runtime receipt
can establish those later facts. An `after_action` clear result is not eligible
to replay the old effect.

## Charter and policy

A full charter uses schema `zap-charter/1` and contains exactly:

```json
{
  "schema": "zap-charter/1",
  "charter_id": "charter-id",
  "campaign_id": "campaign-id",
  "base_sha256": "64 lowercase hex characters",
  "revision": 1,
  "parent_sha256": null,
  "intent": "owner intent",
  "intent_binding": {"intent_id": "intent-id", "sha256": "64 lowercase hex characters"},
  "expected_outcome": {"outcome_id": "outcome-id", "summary": "..."},
  "delegation": {
    "allowed_actions": ["outcome.adopt"],
    "adaptation": {
      "allow_target_revision": true,
      "mutable_obligations": [],
      "essential_obligations": [],
      "allowed_dispositions": ["retained"]
    }
  },
  "legacy_authority": [
    {
      "id": "legacy-id",
      "disposition": "retained",
      "source_sha256": "64 lowercase hex characters",
      "replacement_ref": null
    }
  ],
  "stop_policy": {
    "schema": "zap-stop-policy/1",
    "policy_id": "stop-policy-id",
    "revision": 1,
    "rules": []
  }
}
```

Legacy rows must cover every imported mandate exactly. `retained` and
`superseded` are resolved; `owner_decision_required` prevents activation.
`superseded` requires a replacement reference. Amendment supplies the full new
charter, whose `revision` is the active revision plus one and whose
`parent_sha256` is the exact active charter hash. Old assessments cannot admit
an action under the new revision.

`intent_binding` is optional only for legacy control-only charters. A full
domain charter binds the stable intent proposal ID and SHA-256 of B's canonical
immutable intent payload, including beneficiaries, values and constraints but
excluding mutable status/event metadata. Intent adoption checks that binding
plus the active charter ID/revision/hash. A new intent requires an owner
amendment; an outcome pivot does not change it.

The charter hash is `sha(packed(normalized_charter))`, using the shared tagged
canonical JSON encoder. Validation preserves omission of optional
`intent_binding`; it never inserts `null`, so a legacy caller hashes the exact
normalized charter shape it submitted. Activation and amendment use that hash.

`convert_legacy_stop_policy(legacy, *, policy_id, revision,
applies_to_actions)` prepares an explicit owner-reviewable
`zap-stop-policy/1` value from the old `zap-stop/1` unapproved probe. The old
`scope = run` is mapped to `campaign`, preserving the accepted whole-campaign
effect. Conversion does not activate or approve the result.

A stop rule contains `id`, `applies_to_actions`, `scope`, `timing`, and `when`.
Scopes are `campaign`, `branch`, or `run`; timing is `before_action` or
`before_next_action`. Expressions support `all`, `any`, `not`, `eq`, and
`failed_approaches`. Unsupported operators are refused. A matched owner rule is
sticky. Campaign pauses block every privileged action. Branch/run pauses
coexist and block only actions in the matching scope; unrelated work remains
eligible. A later campaign-wide match is added without deleting narrower
pause history and dominates it.

Approach outcomes are append-only. Only distinct `strategy_sha256` values with
the semantic outcome `failed` count in the current owner-selected epoch for the
same stable `problem_id`. `retry`, `provider_error`, `inconclusive`, and
`succeeded` never increase the failure count. Advancing an epoch changes the
counting boundary and never deletes history. Legacy core approaches are
included using their stable problem and strategy keys. A legacy unresolved
semantic outcome makes the affected assessment `needs_evidence`; it is never
treated as a clear zero count.

## Control events

`CONTROL_HANDLERS` contains these event kinds:

```text
control.charter-drafted
control.charter-activated
control.charter-amended
control.action-assessed
control.action-admitted
control.action-reservation-rebound
control.owner-stop-requested
control.pause-delivery-acknowledged
control.pause-safe-state-acknowledged
control.pause-resumed
control.action-exception-granted
control.approach-outcome-recorded
control.approach-epoch-advanced
```

Drafting is data-only. Activation, amendment, owner stop, resume, exception and
epoch advancement require owner authority. Assessment, approach outcome and
drain acknowledgements require coordinator or owner authority. Admission and
reservation rebind are service-internal.
`control.action-admitted` is service-internal. It consumes at most one exact
exception and creates a durable reservation bound to the action fingerprint,
product event ID, logical product command hash, charter and pause identity. Its
grant is valid only for the immediately following state revision.
`control.action-reservation-rebound` can refresh that grant after a crash or
unrelated append only for the same reserved logical command. Before rebinding,
the service records a fresh assessment against current policy, source captures,
approach history and active jobs; the exact product handler then checks the
current graph/resources again under ordinary CAS. A new stop, source
invalidation, charter amendment or changed pause identity blocks rebind. If the
product event is already committed, the service returns that event
idempotently. No external effect is claimed atomic with this journal sequence;
the runner reconciles its durable claim and receipt.

Pause records separately carry the evaluated policy result, delivery
acknowledgements, and actual-safe-state acknowledgements. `after_action` against
a matched `before_action` rule records `too_late`; it never claims prevention.
A matched `before_next_action` observed after the preceding action creates a
pause for upcoming work rather than calling that preceding action too late.
Resume requires the exact active pause ID and pause hash, completed delivery and
safe-state acknowledgements, and a new owner event. Repeating a resume or using
an exception twice is refused.

A delivery acknowledgement may use `already_terminal` only when the pause
captured that run's exact attempt/transport descriptor and a trusted runtime
observation already records the same natural `succeeded` or `failed` receipt
hash. It satisfies the delivery obligation while recording
`signal_delivered=false`. It never establishes the separate task safe
boundary. `stopped` and `interrupted` cannot masquerade as natural terminal
delivery.

## Application service and trust boundary

`ApplicationService(store, handlers, trust, host_principal=None, *,
action_kinds=None, data_kinds=(), observation_kinds=(), loader=load_store,
recorder=record)` accepts the fully composed handler registry explicitly. It
offers separate agent, host, trusted-observation and credentialed-control entry
points. The public agent entry point rejects every
privileged control kind and every privileged product action even if the command
contains `actor`, `owner`, `role` or similarly named fields. There is no
`--as-owner` equivalent.

Every composed non-control handler is classified explicitly as data-only or
mapped to one delegated action class or trusted observation. The three sets are
disjoint and service construction refuses an unclassified/conflicting
extension handler. Core data records remain data-only;
`plan.refined` is available through the legacy draft route only while no
charter is active, and becomes the `plan.lower` privileged action afterward.
An exact retry is recognized before current CAS only when its previously
committed command hash or durable product reservation matches. Changed bytes
under the same event ID are refused. Every genuinely new command still passes
the shared current-revision CAS before append.

Trusted observation methods are:

```python
submit_observation(command, *, credential_id, credential)
submit_host_observation(command)
```

`observation_kinds` is a fixed set supplied by the engine, never a role map in
caller data. The credentialed method requires a campaign-bound owner or
coordinator; the host method requires the configured campaign-bound owner or
coordinator principal. Reader, anonymous and agent routes refuse. Both methods
use the normal exact envelope, CAS and idempotency but do not require an active
charter or product-action admission, so transport started/result/stopped/
unknown-effect receipts can complete safe draining during pause. A trusted
effect adapter may also record genuine source/native-fact byte captures before
charter activation. These are observations of existing effects or captured
bytes; applicability/proof adjudication remains `evidence.adjudicate`, and an
observation never grants dispatch, acceptance or promotion.

Runtime `work.dispatch` and `verification.run` transitions use
`apply_host_action(command, action, assessment, *, exception_id=None)`.
Their subsequent transport receipts use `submit_host_observation`; this avoids
deadlocking drain evidence behind the pause that evidence is meant to resolve.

`CredentialAuthority` receives protected bindings from trusted startup. Each
opaque credential is bound to one credential ID, role, campaign and allowed
control kinds/action classes. It uses `hmac.compare_digest` for comparison;
`issue` uses `secrets.token_urlsafe`. Credential values never enter commands,
events, worker argv/environment, source or campaign storage. A configured host
principal is trusted only because the application host supplies it outside the
untrusted route.

The `reader` role is campaign-bound and must carry empty command/action scopes.
`authorize_read` lets a backend authenticate a viewer without sharing owner or
coordinator authority. Transport placement such as localhost is not caller
identity.

`CONTROL_EVENT_SCHEMAS` and `SERVICE_REQUEST_SCHEMAS` are machine-readable
descriptor mappings. `ApplicationService.route_descriptors()` combines them
with the instance's explicit data/action handler classification. These
surfaces expose required fields and trusted route metadata separately from
submitted actor data, and contain no credential values.

The supported threat boundary is an untrusted caller or worker input. It does
not claim protection from an operating-system administrator or another process
already able to read the same user's protected credential configuration.
