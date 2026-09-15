# Rust runtime usage guide {#root}

`guide r1`

This non-normative guide describes the public `zap_runtime` API by lifecycle operation. The catalogs cover its 107 externally reachable, locally declared types and traits. The [crate exports](../../../../crates/zap-runtime/src/lib.rs) provide their public paths; internal module names are not client import paths.

The [runtime contract](ZAP-RUNTIME.xml), [agent protocol](ZAP-AGENT-PROTOCOL.xml) and [storage contract](ZAP-RUST-STORAGE.xml) govern authority, durable state and evidence. Pure calculations, constructed payloads, admitted transitions, actual host effects and returned receipts are separate boundaries. The runtime coordinates a cooperating host; it does not turn a declared capability into an observed invocation.

Records and payloads are consumed through their concrete validation and registered service paths. A serde value or record constructor alone does not establish authority, persist state or accept work. Use the appropriate trusted handles supplied by the configured application.

The core-owned `AcceptanceState`, `CollectionState`, `EffectState`, `ExecutionState` and `SafeState` enums are reexported for convenience. Their declarations and methods remain in the [core execution-state source](../../../../crates/zap-core/src/execution_views/job.rs); they are not additional local runtime types.

## Record capability evidence for an exact host identity {#capability-evidence}

`guide r1`

Use [CapabilityCache::new, record and get](../../../../crates/zap-runtime/src/capability_cache.rs) with the complete effective host identity. The cache reports identity changes and keeps contradictory observations pending instead of silently replacing current evidence. It is an in-memory utility; durable capability history uses the registered records and transition.

| Type | Role in this operation |
| --- | --- |
| `CapabilityCacheKey` | Effective host, toolset and configuration identity used for cache lookup. |
| `CachedCapability` | An observation paired with the identity under which it was obtained. |
| `CacheDisposition` | Outcome of inserting, refreshing or retaining a contradictory observation. |
| `CapabilityCache` | In-memory current and pending capability observations. |
| `CapabilityObservationRecord` | Durable captured host capability evidence. |
| `CapabilityCurrentState` | Current, pending-adjudication or unknown capability selection state. |
| `CapabilityCurrentRecord` | Durable selection of current and pending observations for one harness. |
| `CapabilityObservedPayload` | Host evidence submitted to the capability observation operation. |
| `CapabilityObservedCell` | Registered transition that checks and records that evidence. |

[CapabilityObservedCell](../../../../crates/zap-runtime/src/capability_goal.rs) records supplied host observations through the trusted-observation route. Its [observation and current-selection records](../../../../crates/zap-runtime/src/records/observations.rs) distinguish retained evidence from the observation selected for current use. A desired provider, model or effort is a preference; supported and resolved capability follows observed metadata and configured policy.

## Project goals and record their actual application {#goal-projection}

`guide r1`

[campaign_goal and assignment_goal](../../../../crates/zap-runtime/src/goals.rs) render projections from supplied campaign or work context. A campaign umbrella refers to assignments, stop conditions and resume navigation; it does not concatenate every work body.

| Type | Role in this operation |
| --- | --- |
| `CampaignGoalInput` | Inputs to a campaign-scoped goal projection. |
| `GoalProjectionRecord` | Durable generated projection and its identity. |
| `GoalApplicationRecord` | Recorded application state for a particular projection. |
| `GoalProjectionRecordedPayload` | Projection submitted for durable recording. |
| `GoalProjectionRecordedCell` | Registered operation that stores the projection. |
| `GoalFallbackRecordedPayload` | Requested goal operation and capability evidence for fallback planning. |
| `GoalFallbackRecordedCell` | Registered operation that records the supported fallback state. |
| `GoalAcknowledgedPayload` | Observed acknowledgment of a specific goal operation. |
| `GoalAcknowledgedCell` | Registered operation that checks acknowledgment provenance. |

Use the [goal payloads and cells](../../../../crates/zap-runtime/src/capability_goal.rs) to record a projection, the applicable fallback, or an actual acknowledgment. The [goal records](../../../../crates/zap-runtime/src/records/goals.rs) keep projected content separate from its application state. Manual-required or unsupported capability does not become an applied host goal merely because content was generated.

## Select work using readiness, claims and capacity {#scheduling-claims}

`guide r1`

Construct [RuntimeClaim](../../../../crates/zap-runtime/src/claims.rs) through its validating constructor, then supply candidates, active occupancy and explicit capacities to `select_maximal_ready`. Its deterministic greedy selection is maximal under that ordering, not a claim of globally optimal scheduling.

| Type | Role in this operation |
| --- | --- |
| `RuntimeClaim` | Subjects, resources and ownership used to assess one candidate's occupancy. |
| `SchedulingCandidate` | Ordered candidate paired with its complete claim. |
| `ActiveClaim` | Existing occupancy considered independently of new candidate order. |
| `SchedulingCapacity` | Explicit resource, host, integration and review capacities. |
| `ClaimRefusal` | Reason a candidate conflicts with claims or available capacity. |
| `Selection` | Selected keys and recorded claim refusals. |
| `HostCapacityKey` | Typed native-harness or subprocess-adapter capacity identity. |
| `SchedulerRefusal` | Readiness or claim reason that excludes runtime work. |
| `ScheduledSet` | Selected WorkIds and their excluded peers' reasons. |

[select_runtime_ready](../../../../crates/zap-runtime/src/scheduler.rs) first checks the supplied work/readiness bindings, then applies claim selection. Retain refusal reasons for subsequent decisions. A selected WorkId is a scheduling result, not an admitted job, a launch ticket or accepted work. A subprocess capacity category does not authorize a silent transport fallback.

## Compose the coordinator with trusted ports {#coordinator-ports}

`guide r2`

[Coordinator::new](../../../../crates/zap-runtime/src/coordinator.rs) binds durable read and command ports, a local mailbox, a command factory and capacity inputs. Call `step` with established principal handles and consume the returned step outcome. The factory constructs the registered command frames required by this composition; supplying an implementation does not confer authority.

| Type | Role in this operation |
| --- | --- |
| `RuntimeCommandFactory` | Application-supplied construction of registered runtime command frames. |
| `CoordinatorPrincipals` | Borrowed trusted principal handles used for distinct coordinator routes. |
| `Coordinator` | Bounded step orchestration over injected ports and mailbox. |
| `CoordinatorStep` | Outcome telling the caller what the step recorded or is waiting for. |
| `RuntimeAffectedJobProvider` | Runtime contribution to the affected-job view used by admission. |

Wait, idle and completion-eligible outcomes have different meanings. Before returning one, the coordinator consumes the complete indexed relevant-job set and continues the ready-work frontier past refused pages. `Idle` therefore follows a complete empty/exhausted view; an unknown frontier, exhausted aggregate work budget, or missing/stale/incompatible runtime index is a typed refusal. `CompletionEligible` does not itself record campaign closure. The [affected-job provider](../../../../crates/zap-runtime/src/registration/affected_jobs.rs) supplies runtime observations to the composed admission path; use its returned completeness rather than inferring independence from missing rows.

## Create jobs from sealed packets and consume state observations {#job-state}

`guide r1`

The [JobClaimPayload](../../../../crates/zap-runtime/src/transitions.rs) supplies a packet-resolution request and operation identities. [JobClaimCell](../../../../crates/zap-runtime/src/transitions/job_claim.rs) uses the admitted resolution to derive the runtime job; the caller does not substitute arbitrary job meaning for a sealed packet.

| Type | Role in this operation |
| --- | --- |
| `RuntimeJobRecord` | Durable job identity, captured contract and execution state. |
| `JobClaimPayload` | Packet-resolution request and identities for a new job claim. |
| `JobClaimCell` | Registered transition deriving and recording a claim. |
| `RuntimeTransitionOutput` | Job and execution result returned by runtime transition operations. |
| `ExecutionObservation` | Pure input to the supported execution-state transition table. |
| `JobObservationRecordedPayload` | Bound driver observation proposed for the current job revision. |
| `JobObservationRecordedCell` | Registered operation that validates and records the observation. |

[RuntimeJobRecord::validate](../../../../crates/zap-runtime/src/records.rs) checks the record's identity and contract bindings. [Job observation ingress](../../../../crates/zap-runtime/src/job_ingress.rs) consumes bound driver evidence, while [transition_execution](../../../../crates/zap-runtime/src/state.rs) implements the pure state transition table. Constructing an observation or evaluating that table alone does not persist a transition.

## Prepare a native launch and record its actual receipt {#native-dispatch}

`guide r1`

Create [NativeBridge](../../../../crates/zap-runtime/src/native_bridge.rs) from captured capabilities and a bridge observation. It supplies the sealed local mailbox used by the coordinator. Queuing an intent can return AwaitingHarness or Unavailable; neither outcome proves that an external worker started.

| Type | Role in this operation |
| --- | --- |
| `LocalAgentMailbox` | Sealed host interface restricted to local mailbox operations. |
| `NativeBridge` | Local rendezvous for pending intents and bound host observations. |
| `NativeLaunchTicket` | Single-use pickup proof produced at the launch boundary. |
| `NativeDriverAuthority` | Privileged or internal authority handle used for launch preparation. |
| `NativeDriverStep` | Authorization, ready-to-invoke or already-receipted preparation outcome. |
| `NativeDriverCoordinator` | Store-backed orchestration immediately around host invocation. |
| `DispatchAuthorizePayload` | Current job and eligibility request for authorization. |
| `DispatchAuthorizeCell` | Registered authorization transition. |
| `DispatchConsumePayload` | Request to consume the current dispatch authorization. |
| `DispatchConsumeCell` | Registered consumption transition before ticket pickup. |
| `DispatchReceiptPayload` | Actual dispatch receipt and provenance submitted for recording. |
| `DispatchReceiptCell` | Registered receipt validation and persistence transition. |
| `PreEffectAuthorizationState` | Lifecycle of an authorization, including unknown external effect. |
| `PreEffectAuthorizationRecord` | Durable authorization bound to a dispatch, actor and observed revision. |

[NativeDriverCoordinator::prepare_launch](../../../../crates/zap-runtime/src/driver.rs) advances through registered authorization and consumption using the appropriate trusted authority. Only the resulting single-use ticket reaches the narrow host invocation boundary. The cooperating host performs the invocation and returns its actual handle and provenance; `record_receipt` then uses the registered receipt route.

The [authorization and receipt payloads](../../../../crates/zap-runtime/src/transitions.rs), their [authorization cells](../../../../crates/zap-runtime/src/transitions/authorization.rs), [receipt cell](../../../../crates/zap-runtime/src/transitions/receipt.rs), and [authorization records](../../../../crates/zap-runtime/src/records/recovery.rs) retain that separation. A populated authorization record supplied as data is not a trusted service grant.

## Validate agent messages without promoting them to authority {#agent-messages}

`guide r1`

[AgentMessage::validate](../../../../crates/zap-runtime/src/protocol.rs) checks causal predecessors, profile roles and the selected message payload. `kind` is derived from the payload variant. Call validation when consuming an assembled message; ordinary deserialization is not a substitute for all of those checks.

| Type | Role in this operation |
| --- | --- |
| `AgentMessageKind` | Discriminant identifying the protocol message category. |
| `AgentMessagePayload` | Typed message content with variant-specific meaning. |
| `AgentMessage` | Envelope binding message content to packet and operation lineage. |

Keep desired and resolved profiles distinct and retain packet, attempt and causal identity. A heartbeat, candidate, verification message and safe-stop receipt communicate different evidence. The message is input to the appropriate trusted handling path, not an authorization or an acceptance decision.

## Record native spawn outcomes and release bounded retries {#native-spawn-recovery}

`guide r1`

Use the [native spawn observation family](../../../../crates/zap-runtime/src/spawn_recovery.rs) to distinguish Started, proven NotStarted and Unknown. Observations bind stable command identity, exact job/dispatch and the consumed authorization revision. The registered observation cell records recovery state through the service.

| Type | Role in this operation |
| --- | --- |
| `NativeSlotCapacityObservation` | Independent observation of available, unavailable or unknown capacity. |
| `NativeSpawnFailureClass` | Classification accompanying a refusal or unknown spawn outcome. |
| `NativeSpawnOutcome` | Started, NotStarted or Unknown result supplied by the host boundary. |
| `NativeSpawnObservation` | Recorded outcome and its recovery/release observations. |
| `NativeSpawnRecoveryRecord` | Durable sequence of observations for one dispatch. |
| `NativeSpawnObservedPayload` | Exact spawn observation submitted to the service. |
| `NativeSpawnObservedCell` | Registered operation that records the bound recovery observation. |
| `NativeSpawnRetryReleasedPayload` | Later capacity and time observation requesting retry release. |
| `NativeSpawnRetryReleasedCell` | Registered retry-release operation. |

[NativeSpawnRetryReleasedCell](../../../../crates/zap-runtime/src/spawn_recovery/release.rs) handles the later release observation. A due wait and a separately observed available slot are distinct from terminal job state. Unknown outcomes remain reconciliation work; they are not permission to retry a possibly completed launch. Preserve the recorded history across process or account replacement.

## Preserve attempt history and reconcile before retry {#retry-reconciliation}

`guide r1`

[RetryHistory::new and record](../../../../crates/zap-runtime/src/retry.rs) maintain outcomes for a stable job and reject conflicting reuse of an attempt identity. Its retry predicates evaluate the recorded condition and supplied observations; they do not obtain new host evidence or issue an effect.

| Type | Role in this operation |
| --- | --- |
| `WaitClass` | Recorded reason an operation cannot currently advance. |
| `BackoffBasis` | Evidence or policy basis used to choose a retry boundary. |
| `RetryCondition` | Specific condition that can release the recorded wait. |
| `AttemptOutcome` | Retained outcome of one identified attempt. |
| `RetryHistory` | Ordered attempt history and retry-condition evaluation. |
| `RetryHistoryRecord` | Durable attempt history and its release observation. |
| `RuntimeWaitRecord` | Durable waiting condition associated with runtime work. |
| `RetryRecordedPayload` | History and optional wait submitted for persistence. |
| `RetryRecordedCell` | Registered operation that records retry history. |
| `RetryReleasedPayload` | Observed inputs requesting release of a persisted retry. |
| `RetryReleasedCell` | Registered operation that checks the retry release. |
| `ReconciliationRecord` | Durable host reconciliation observation for a dispatch. |
| `ReconciliationRecordedPayload` | Bound observation submitted for reconciliation recording. |
| `ReconciliationRecordedCell` | Registered reconciliation transition. |

Persist history and waits through the [retry payloads](../../../../crates/zap-runtime/src/runtime_updates.rs) and [registered retry cells](../../../../crates/zap-runtime/src/runtime_updates/retry.rs). [ReconciliationRecordedCell](../../../../crates/zap-runtime/src/reconciliation.rs) consumes exact host observations. The [durable records](../../../../crates/zap-runtime/src/records/recovery.rs) retain these outcomes independently of the current agent context.

For resumed collection, `NativeDriverCoordinator::restore_dispatch` and its recovered observation/candidate methods reconstruct the mailbox from saved state. Restoration does not issue another authorization or consume another launch ticket. Provider waits remain distinct from architectural failure.

## Coalesce heartbeats and persist useful checkpoints {#liveness-checkpoints}

`guide r1`

[LivenessTable::new and observe](../../../../crates/zap-runtime/src/liveness.rs) provide in-memory heartbeat coalescing. Read its disposition to distinguish an initial observation, another heartbeat and a changed useful checkpoint. Reading the table does not create a semantic event.

| Type | Role in this operation |
| --- | --- |
| `LivenessRecord` | Coalesced observations and the latest useful checkpoint identity. |
| `LivenessDisposition` | Meaning of the latest coalescing operation. |
| `LivenessTable` | In-memory liveness observations keyed by the caller's identity. |
| `RuntimeLivenessRecord` | Durable job-bound liveness and checkpoint record. |
| `LivenessBoundaryPayload` | Liveness boundary with driver provenance submitted for recording. |
| `LivenessBoundaryCell` | Registered operation that persists the boundary. |

For durable boundaries, submit the [LivenessBoundaryPayload](../../../../crates/zap-runtime/src/runtime_updates.rs) through [LivenessBoundaryCell](../../../../crates/zap-runtime/src/runtime_updates/observations.rs), consuming the resulting [RuntimeLivenessRecord](../../../../crates/zap-runtime/src/records/observations.rs). A heartbeat establishes observed liveness only; it does not prove completion, safety or continued liveness after that observation.

## Collect candidates and retain useful work during repair {#candidate-repair}

`guide r1`

[CandidateRecordedCell](../../../../crates/zap-runtime/src/job_ingress.rs) checks candidate producer, packet, contract and driver bindings before storing a [CandidateResultRecord](../../../../crates/zap-runtime/src/records.rs). A stored candidate is not semantic acceptance by the coordinator.

| Type | Role in this operation |
| --- | --- |
| `CandidateRecordedPayload` | Candidate and driver provenance submitted for a job revision. |
| `CandidateRecordedCell` | Registered operation recording a validated candidate. |
| `CandidateResultRecord` | Durable candidate content awaiting the appropriate acceptance process. |
| `RepairDisposition` | Same-basis repair or a wait for changed input/profile. |
| `MalformedCandidateRecord` | Preserved malformed-result evidence and useful artifacts. |
| `MalformedCandidateRecordedPayload` | Malformed-result record and provenance submitted for persistence. |
| `MalformedCandidateRecordedCell` | Registered operation that records the repair boundary. |

When output is malformed, [MalformedCandidateRecord::new](../../../../crates/zap-runtime/src/repair.rs) retains artifact identities and derives the bounded repair disposition from the repair count. The [malformed-result payload](../../../../crates/zap-runtime/src/runtime_updates.rs) and [recording cell](../../../../crates/zap-runtime/src/runtime_updates/observations.rs) preserve that evidence through the trusted route. A format failure does not erase successful transport artifacts or justify repeating the underlying effect.

## Record verification claims and consume scoped results {#verification-evidence}

`guide r1`

[VerificationRecord::validate](../../../../crates/zap-runtime/src/records/observations.rs) checks consistency between a verification claim/result and its observation. Select the actual verification scope; a general check is not automatically evidence for a particular safe boundary.

| Type | Role in this operation |
| --- | --- |
| `VerificationState` | Claimed, Passed or Failed verification state. |
| `VerificationScope` | General verification or an exact safe-boundary scope. |
| `VerificationRecord` | Durable verification claim or observed result. |
| `VerificationClaimedPayload` | Verification claim submitted for admission. |
| `VerificationClaimedCell` | Registered operation that records a valid verification claim. |
| `VerificationResultPayload` | Observed result and its artifact/provenance references. |
| `VerificationResultCell` | Registered operation that validates and records the result. |

The [verification payloads](../../../../crates/zap-runtime/src/runtime_updates.rs) are handled by the [claim and result cells](../../../../crates/zap-runtime/src/runtime_updates/verification.rs). Claiming a check and recording its trusted result are separate operations. A Passed result is evidence for its bound check and scope, not whole-work or campaign acceptance.

## Record stop delivery, safe state and revalidation release {#stop-safe-release}

`guide r1`

The [stop and release operations](../../../../crates/zap-runtime/src/stop_ingress.rs) separate requesting a stop, observing its delivery, proving a safe boundary and releasing work for revalidation. Their payloads retain the target job or release identity and the applicable observation provenance.

| Type | Role in this operation |
| --- | --- |
| `StopRequestedPayload` | Exact job and stop request submitted to the internal service route. |
| `StopRequestedCell` | Registered operation that records the stop request. |
| `StopDeliveryRecordedPayload` | Observed stop-delivery receipt and provenance. |
| `StopDeliveryRecordedCell` | Registered operation that records verified delivery evidence. |
| `SafeStateRecordedPayload` | Claimed safe state with its observation and verification references. |
| `SafeStateRecordedCell` | Registered operation that checks and records the safe boundary. |
| `WorkRevalidationReleasedPayload` | Exact work-release evidence and driver provenance. |
| `WorkRevalidationReleasedCell` | Registered operation recording a justified revalidation release. |

Use the driver's stop-delivery and safe-state recording methods with actual evidence. A requested stop, delivered message, terminal process and proven safe state are separate facts. Work revalidation release consumes its own bound release evidence; none of these DTOs is an Owner resume decision or a new launch permission.

## Executable integration examples {#integration-examples}

`guide r1`

These existing entrypoints show construction and consumption with explicit fixture configuration:

- [Runtime contracts](../../../../crates/zap-runtime/tests/runtime_contracts.rs): readiness and claim selection, subject/resource conflicts, unknown native delivery, retry-history round-trip, goal capability and cache handling, heartbeat coalescing and malformed-result repair.
- [Runtime persistence](../../../../crates/zap-runtime/tests/runtime_persistence.rs): the registered store/service composition, sealed-packet claim refusal, and the included recovery, native-retry and candidate journeys.
- [Consumed-launch recovery](../../../../crates/zap-runtime/tests/runtime_persistence/recovery_test.rs): current pause checks, recovery of consumed authorization as Unknown, and scoped safe-state evidence.
- [Known-refusal retry](../../../../crates/zap-runtime/tests/runtime_persistence/native_retry_test.rs): persisted waits, renewed launch gates, late-start uncertainty and collection after mailbox restoration.
- [Candidate and stop provenance](../../../../crates/zap-runtime/tests/runtime_persistence/candidate_test.rs): receipt/stop/safe observations and candidate-binding checks. Recorded candidate state remains separate from acceptance.

These sources are integration references, not fresh execution receipts. Their fixture capabilities and supplied host observations do not by themselves demonstrate an actual model/provider invocation.

## Internal boundaries behind the public API {#internal-boundaries}

`guide r1`

Two source-level public declarations are intentionally unreachable through the exported API and are excluded from the 107-type catalogs:

- [native_bridge.rs](../../../../crates/zap-runtime/src/native_bridge.rs) contains `sealed::Sealed` inside a private module. It prevents external implementations from presenting arbitrary remote I/O as `LocalAgentMailbox`. Clients use the exported mailbox contract with its supported `NativeBridge` implementation; the source includes compile-fail examples for the private seal.
- [registration.rs](../../../../crates/zap-runtime/src/registration.rs) contains `RuntimeCompletionProvider` without reexporting the concrete type. The public `completion_provider_set` factory supplies the runtime contribution to the shared completion machinery. Clients consume that factory result rather than constructing the hidden provider.

These implementation boundaries do not add public constructors or waive the service's authority, completion or recovery checks.
