# ZAP adaptive domain internal API

The domain extension is a deterministic reducer family. It reads the immutable
MUP base and existing core fact/evidence records, and owns only
`state.extensions.domain`. It performs no source reads, process dispatch,
filesystem writes, clock reads, network calls, or semantic model calls.

## Public seam

`zaplib.domain` exports:

- `DOMAIN_HANDLERS`: immutable mapping of the 22 exact event kinds below.
- `DOMAIN_EVENT_SCHEMAS`: JSON-Schema-style exact payload descriptors for the
  same 21 kinds. Every object has `additionalProperties=false`.
- `DOMAIN_DATA_KINDS`: the three harmless proposal events.
- `DOMAIN_ACTION_KINDS`: every privileged event mapped to one frozen control
  action class. The module refuses to import if a handler is absent from, or
  appears in both, route collections.
- `DOMAIN_CAPABILITIES`: schema/version, event names and all delegated action
  classes used by domain or runtime consumers.
- `domain_state(state)`: detached domain projection. On a legacy store it
  derives the same view without mutating the supplied state.
- `domain_frontier(state)`: ordered ready leaves after parent suppression,
  inherited dependencies, current domain acceptance, explicit successor/drop
  resolution and open-deferral checks. It is not dispatch authority.
- `intent_fingerprint(payload)`: canonical SHA-256 of the exact
  `zap-domain/intent-proposed/1` payload. It includes schema, identity,
  revision, predecessor, summary, beneficiaries, values, constraints and
  source refs, and excludes later status/event metadata.
- `build_sparse_review_transition(state, request)`: expands one strict
  `zap-domain/sparse-review-transition/1` request into the exact full nested
  transition accepted by `domain.review-proposed`. It reads the active charter
  adaptation envelope, materializes a sorted retained row for every omitted
  active obligation, and validates every explicit disposition and reuse
  candidate. It creates no event and consumes no privilege.
- `SPARSE_REVIEW_TRANSITION_SCHEMA`: machine-readable exact input descriptor
  for that helper.
- `DOMAIN_OPERATIONS`: immutable operation registry. Its
  `domain.materialize-review-transition` row names the helper, input schema and
  `domain.review-proposed.transition` return shape.
- `current_acceptance_coverage(state)`: returns exact current central proof as
  `zap-domain/acceptance-coverage/1`, with the active outcome, sorted work,
  obligation, acceptance and integration IDs, plus per-integration obligation
  coverage. Historical or stale records are omitted.

Reducers call `control.require_action(state, action_class)`. The control
service authenticates and admits the caller before append; replay checks the
persisted one-revision grant. Editable actor, owner, role and authority labels
are never trusted.

Intent adoption additionally requires the active charter's exact
`intent_binding={intent_id, sha256}` and records the binding's charter ID,
revision and SHA-256. A later intent can be adopted only when an owner charter
amendment changes both charter revision/hash and binds the new exact intent
fingerprint. `adaptive.apply` may freely revise the expected outcome inside
the active adaptation envelope while retaining intent; it cannot change
beneficiaries, values or constraints under an unchanged charter.

## Projection

`extensions.domain.schema` is `zap-domain/1`. The projection stores a domain
revision, versioned intents and outcome revisions, active/original pointers,
first-class obligations, typed ownership, added work and work overlays,
successors, task-contract histories, adaptive reviews, evidence adjudications,
accepted stages, deferrals, work/integration acceptances, fact-promotion
metadata and one terminal closure. Evidence adjudication records also capture
the exact `{source_id, sha256}` identities current at adjudication. Selective
reuse records append-only evidence, stage, work-acceptance and integration
witnesses; original proof and acceptance rows remain unchanged.

Legacy MUP mandates become sourced active obligations with their stable IDs.
Every node acceptance criterion becomes
`acceptance:<node-id>:<zero-based-index>`. Imported task contracts are version
zero with their canonical hash. Imported accepted nodes remain visible as
`legacy_assertion` records tied to the immutable base; they do not become new
central acceptance or achieved-stage receipts.

## Exact event payloads and routes

Every row requires the listed `schema` and exactly the listed remaining
fields. Proposal rows are data-only. All other rows use the named action and
honor sticky pause.

| Event | Payload schema | Exact remaining fields | Route |
| --- | --- | --- | --- |
| `domain.intent-proposed` | `zap-domain/intent-proposed/1` | `intent_id, revision, previous_intent_id, summary, beneficiaries, values, constraints, source_refs` | data |
| `domain.intent-adopted` | `zap-domain/intent-adopted/1` | `intent_id` | `outcome.adopt` |
| `domain.outcome-proposed` | `zap-domain/outcome-proposed/1` | `outcome_id, revision, previous_outcome_id, intent_id, summary, benefits, guarantees, tradeoffs, obligations` | data |
| `domain.outcome-adopted` | `zap-domain/outcome-adopted/1` | `outcome_id, obligation_dispositions` | `outcome.adopt` |
| `domain.task-contract-replaced` | `zap-domain/task-contract-replaced/1` | `work_id, expected_version, contract` | `task.update` |
| `domain.work-revalidation-readied` | `zap-domain/work-revalidation-readied/1` | `work_id, review_id, job_id, from_generation, expected_state` | `plan.lower` |
| `domain.work-renamed` | `zap-domain/work-renamed/1` | `work_id, expected_title, new_title` | `plan.lower` |
| `domain.work-transitioned` | `zap-domain/work-transitioned/1` | `work_id, from_state, to_state, successor_ids` | `plan.lower` |
| `domain.work-dispatched` | `zap-domain/work-dispatched/1` | `work_id, from_state, job_id` | `work.dispatch` |
| `domain.plan-lowered` | `zap-domain/plan-lowered/1` | `parent_id, nodes, edges, coverage, contracts, integration_owner` | `plan.lower` |
| `domain.deferral-created` | `zap-domain/deferral-created/1` | `deferral_id, outcome_id, obligation_ids, work_ids, scope, reason, current_guarantees, responsible_party, closure_requirement` | `task.update` |
| `domain.deferral-transferred` | `zap-domain/deferral-transferred/1` | `deferral_id, from_responsible_party, to_responsible_party, to_work_ids, reason` | `task.update` |
| `domain.deferral-closed` | `zap-domain/deferral-closed/1` | `deferral_id, evidence_ids, reason` | `task.update` |
| `domain.deferral-inapplicable` | `zap-domain/deferral-inapplicable/1` | `deferral_id, outcome_id, reason` | `task.update` |
| `domain.evidence-adjudicated` | `zap-domain/evidence-adjudicated/1` | `evidence_id, expected_revision, disposition, applies_to, source_refs, method, limitations` | `evidence.adjudicate` |
| `domain.stage-accepted` | `zap-domain/stage-accepted/1` | `stage_acceptance_id, work_id, stage, outcome_id, evidence_ids, obligation_ids, scope, summary` | `stage.accept` |
| `domain.integration-accepted` | `zap-domain/integration-accepted/1` | `integration_id, work_id, child_work_ids, legacy_child_ids, outcome_id, evidence_ids, obligation_ids, summary` | `work.accept` |
| `domain.work-accepted` | `zap-domain/work-accepted/1` | `acceptance_id, work_id, outcome_id, stage_acceptance_id, evidence_ids, obligation_ids, integration_acceptance_ids, summary` | `work.accept` |
| `domain.fact-promotion-recorded` | `zap-domain/fact-promotion-recorded/1` | `promotion_id, fact_id, target_handle, content_sha256, evidence_ids, adapter_receipt, summary` | `fact.promote` |
| `domain.review-proposed` | `zap-domain/review-proposed/1` | `review_id, previous_review_id, signals, captures, knowledge, alternatives, chosen, decision, transition, next_trigger` | data |
| `domain.review-applied` | `zap-domain/review-applied/1` | `review_id, expected_domain_revision` | `adaptive.apply` |
| `domain.campaign-closed` | `zap-domain/campaign-closed/1` | `closure_id, classification, active_outcome_id, actual_benefit, obligation_results, acceptance_ids, integration_acceptance_ids, deferral_ids, promotion_ids, final_gate_evidence_ids, summary` | `campaign.close` |

`domain.work-transitioned` cannot enter `active`; dispatch has its own event and
action class. Dispatch requires ready frontier work, an open active outcome, a
current `zap-task-contract/1`, and at least one current obligation.

## Exact nested records

An outcome obligation is exactly
`{id, statement, essential, source_refs, owners}`. Each owner is exactly
`{work_id, role}` where role is `implementation`, `verification`, `integration`
or `acceptance`; at least one owner is required. The `essential` flag must
match the active owner policy rather than a producer assertion.

An obligation disposition is exactly
`{obligation_id, disposition, successor_ids, unmet_portion, reason}`.
Disposition is `retained`, `replaced`, `excluded` or `unattainable`.
`replaced` requires one or more successor IDs declared by the new outcome;
other dispositions prohibit successors. Every active prior obligation appears
once. Non-mutable and essential obligations must remain retained; changing the
charter is the only way to change that envelope.

A `zap-task-contract/1` is exactly
`{schema, contract_id, work_id, title, goal, read_subjects, write_subjects,
resources, steps, positive_cases, negative_cases, checks, acceptance,
safe_stop, integration_owner, delivery_route, required_stage, source_handles,
obligation_ids}`.
`delivery_route` is either `['direct']` or an ordered subset of prototype,
functional and productized. `required_stage` names the stage central acceptance
must prove. `obligation_ids` binds every current obligation owned by the work
exactly; contract replacement and lowering refuse an omitted or invented
obligation.

Each work has an implicit validation generation zero. The dedicated
`domain.work-revalidation-readied` event is valid only after an applied review
requested revalidation and the runtime's captured prior job reached its durable
`released` reconciliation state. It increments the generation and readies the
same work/problem identity. Evidence stores exact work-generation bindings;
stage, integration and work acceptance store their work generation. Older rows
with no generation field mean zero. Prior-generation proof remains historical
and cannot satisfy current acceptance, and re-adjudicating the same old evidence
identity cannot manufacture fresh proof.

A lowered node is exactly
`{id, parent, title, kind, state, order, depends_on, acceptance,
required_stage}` and must start planned inside the refined subtree. An edge is
`{node_id, depends_on}`. A coverage row is
`{obligation_id, assignments}` and each assignment is `{work_id, role}`.
Coverage names every current obligation owned by the parent exactly once,
gives every added work item an origin, preserves an acyclic parent/dependency
graph, and supplies exactly one task contract for every newly created leaf.
The same contract applies when a lowered work item is lowered again.

Evidence applicability is exactly
`{outcome_id, obligation_ids, work_ids, stage, scope}`. Verification method is
exactly `{argv, target, toolchain, environment, subjects, cases}`. An accepted
adjudication requires an existing core `observed_pass` or `observed_fail`, a
durable artifact reference, exact work subjects, explicit current source
applicability, and a complete known source closure. Positive stage, work,
integration and deferral closure additionally require `observed_pass`.
Unattainability may use centrally accepted `observed_fail` tied to captured
conditions. Promotion and final gate recheck current applicability. Producer
PASS, a declared maturity label and imported acceptance cannot create new
proof.

Stage, work and integration acceptance treat selected evidence as a coverage
set: every proof must match the claimed work/stage/outcome and pass, and their
obligation coverage union must include every requested obligation. Campaign
final-gate evidence uses the same union rule across independently scoped work
proofs. Omitting any obligation refuses; no individual check must overstate
that it proves unrelated obligations. Rejected or inapplicable adjudications
may record a known stale source identity, while only accepted proof requires a
current capture.

Review captures are exactly
`{base_sha256, zap_revision, domain_revision, intent_id, outcome_id,
policy_revision, source_captures, jobs}`. Source captures are
`{source_id, sha256}`; jobs are `{job_id, status, attempt_id}`. Knowledge is
exactly `{before, after, new_region_ids, affected_dependencies,
closure_complete}`. `before` is null for the first applied review or exactly
`{review_id, sha256}` referencing the prior applied review. `after` is exactly
`{region_ids, revision, sha256, regions}` from
`knowledge_snapshot(state, region_ids)`. Knowledge region events therefore
land before the review proposal; proposal and application both reject missing,
fabricated or changed selected regions. The snapshot hashes only selected
regions, so unrelated fog changes do not force a global replan. New questions
are the questions in the captured `new_region_ids`, preserving their D history
and lineage rather than copying unverifiable prose. An alternative is
`{id, description, value, feasibility, remaining_cost, risks, unknowns}` and
decision is `{kind, rationale}`.

Review transition is exactly
`{intent_id, outcome_id, obligation_dispositions, ownership_changes,
work_changes, preserved_evidence_ids, preserved_stage_acceptance_ids,
preserved_work_acceptance_ids, preserved_integration_acceptance_ids,
job_reconciliation, tradeoffs, preserved_benefits}`. Ownership change is
`{obligation_id, from_work_id, assignments, reason}`. Work change is
`{work_id, operation, order, successor_ids, reason}`. Job reconciliation is
`{job_id, action, safe_boundary, reason}`. Application requires unchanged
domain, policy, intent/outcome and captured jobs. It atomically records a
keep-route or pivot, obligation and ownership changes, work overlays and the
complete job reconciliation plan. It marks job effects as planned; runtime
adapters perform and acknowledge those effects separately.

The four preservation lists are explicit and closed over their proof graph.
Preserving a work acceptance requires its selected accepted evidence, achieved
stage and integration acceptances; preserving integration requires every
non-legacy child acceptance. A transfer witness binds the historical record
and adjudication event, source content hashes, core evidence hash, obligation
semantics, work subject, active task-contract version/hash and ownership. The
new and prior outcome must keep the same required guarantees. Changed source
content or applicability, a changed contract, revalidation, changed proof
ownership, a disposed covered obligation, a new obligation owned by that work,
or an unselected dependency refuses transfer. Later reads revalidate the
witness against current state. An outcome revision with no selected witnesses
leaves prior proof historical and unusable for current readiness or closure.

The sparse request is exactly
`{schema, intent_id, outcome_id, changed_dispositions, ownership_changes,
work_changes, preserved_evidence_ids, preserved_stage_acceptance_ids,
preserved_work_acceptance_ids, preserved_integration_acceptance_ids,
job_reconciliation, tradeoffs, preserved_benefits}`. `schema` is
`zap-domain/sparse-review-transition/1`. `changed_dispositions` contains only
the semantic changes chosen by the coordinator. The builder refuses duplicate
or non-current obligations, dispositions outside the active charter, changes
to immutable/essential obligations, missing replacement successors and false
reuse. It fills every other current obligation with a canonical retained row.
The caller places the returned full transition into an ordinary
`domain.review-proposed` payload; the sparse request is never journaled.

A closure obligation result is exactly
`{obligation_id, result, unmet_portion, successor_ids, evidence_ids}`. Result
is `accepted`, `retained_unmet`, `replaced`, `excluded` or `unattainable`.
Closure covers every historical obligation and every deferral, uses the active
outcome, current applicable final-gate evidence and existing acceptance,
integration and promotion identities. `original` and `revised` success require
no unmet active obligation and must match outcome history. `partial` and
`unreachable` retain a nonblank unmet portion for every active gap.

Fact promotion is metadata for a separately implemented effects adapter. Its
receipt and exact content hash are recorded after that adapter acts; the pure
reducer never claims it wrote the target artifact.
