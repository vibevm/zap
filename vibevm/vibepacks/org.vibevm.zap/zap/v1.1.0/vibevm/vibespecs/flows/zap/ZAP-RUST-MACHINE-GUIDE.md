# Rust machine API usage guide {#root}

`guide r5`

This non-normative guide groups the public `zap_api` types by client operation. Its catalogs cover all 54 locally declared types and traits exposed by the [crate exports](../../../../crates/zap-api/src/lib.rs). Three runtime-owned reexports are identified separately below.

The [backend contract](ZAP-BACKEND-API.md), [CLI guide](ZAP-CLI.md), [runtime contract](ZAP-RUNTIME.xml) and [agent protocol](ZAP-AGENT-PROTOCOL.xml) describe the configured service and its authority boundaries. Constructing or deserializing a DTO establishes neither configured capability nor permission to execute it. Preparation, admission, an external invocation and its receipt remain separate observations.

Source links below identify existing operations and examples. They do not report newly executed checks or claim that constructing a response is evidence of a real service result.

## Discover and call the read surface {#read-surface}

`guide r1`

Use the configured port's capabilities to select a supported operation, then call [execute_read](../../../../crates/zap-api/src/surface.rs) through `MachineReadPort`. Match the returned `MachineResponse` to the requested operation. The request enum also represents service operations that this read dispatcher refuses.

| Type | Role in this operation |
| --- | --- |
| `MachineRequest` | Tagged request selecting a machine operation. |
| `MachineResponse` | Tagged result returned for a completed machine operation. |
| `SurfaceCapabilities` | Advertised operations and query availability for the inspected surface. |
| `MachineReadPort` | Typed read interface supplied by the configured application. |

`SurfaceCapabilities::default` describes the unconfigured read surface, with no command operations or query IDs. The API crate's [registration functions](../../../../crates/zap-api/src/registration.rs) likewise return empty query and capability sets; the presence of a DTO is not a registration.

## Send canonical queries and interpret bounded pages {#query-pages}

`guide r1`

[QueryInput::canonical](../../../../crates/zap-api/src/surface.rs) validates the exact bytes for their codec before a query reaches the read port. Obtain the query ID and the selected query's continuation contract from the configured capabilities and query definition.

| Type | Role in this operation |
| --- | --- |
| `QueryInput` | Codec and byte input awaiting canonical validation. |
| `QueryPage` | Returned query items bound to a store, revision and query epoch. |
| `PageCompleteness` | Distinguishes a complete page result, more data and an unknown boundary. |

Deserializing `QueryInput` does not itself perform canonical validation. Consume item bytes using the selected query's concrete result contract, and preserve its continuation information. `UnknownBoundary` does not mean an empty or fully explored result.

The [backend read routes](ZAP-BACKEND-API.md) describe bounded query responses.

## Follow snapshots and committed event history {#snapshot-events}

`guide r1`

Read a [SnapshotView and EventPage](../../../../crates/zap-api/src/surface.rs) from the service and retain the returned store identity and committed boundary. Continue using the returned cursor instead of inventing sequence positions from the number of displayed events.

| Type | Role in this operation |
| --- | --- |
| `SnapshotView` | Logical and physical snapshot information for one observed boundary. |
| `EventCursor` | Store-bound position for continued event reading. |
| `EventSummary` | Event identity and the historical metadata available for that event. |
| `EventAuthority` | Distinguishes schema-1 and schema-2 historical authority representations. |
| `EventActionAdmission` | Distinguishes schema-1 and schema-2 admission observations. |
| `PhysicalSchemaView` | Physical store representation reported by the snapshot. |
| `PhysicalProjectionAlgorithmView` | Physical projection algorithm reported independently of logical history. |
| `EventPage` | Committed event page with resume and optional next-page cursors. |

Optional historical metadata remains optional; its absence is not an invented authority or admission record. A physical schema value is not a package release version. These public views can be deserialized or constructed as DTOs, so their presence alone does not authenticate a claimed historical event.

The [backend consistency description](ZAP-BACKEND-API.md) explains snapshot/tail boundaries and typed cursor conflicts. It also describes the bounded stream response and reconnect behavior.

## Rebuild indexes and continue a bounded traversal {#index-traversal}

`guide r1`

Submit [index and traversal requests](../../../../crates/zap-api/src/surface.rs) through a configured service channel that supports them. `execute_read` does not dispatch these maintenance operations.

| Type | Role in this operation |
| --- | --- |
| `IndexRebuildRequest` | Request to rebuild derived indexes at an expected store revision. |
| `IndexRebuildView` | Catalog and row count returned by the rebuild operation. |
| `AffectedTraversalBeginRequest` | Starts a traversal session with its focus and work budgets. |
| `AffectedTraversalContinueRequest` | Continues an existing session at an expected generation. |
| `AffectedTraversalCancelRequest` | Selects the traversal session to cancel. |
| `AffectedTraversalView` | Returned traversal progress, items and continuation or repair state. |

Carry session identity and returned generation into the next operation. Interpret completion, quota requests, repair information, exact retry and cleanup state independently. Cancellation is not evidence that every cleanup row has already been removed.

The [storage navigation contract](ZAP-RUST-STORAGE.xml) supplies the semantic boundary.

## Submit a command and reconcile an unknown outcome {#command-reconciliation}

`guide r1`

[ProtectedCommand::canonical](../../../../crates/zap-api/src/commands.rs) constructs the canonical frame from its validated header, reason and canonical payload. The configured command channel still performs authority, current-state and admission checks.

| Type | Role in this operation |
| --- | --- |
| `ProtectedCommand` | Submitted command wrapper whose frame can be canonicalized. |
| `CommandFrameInput` | Typed header, reason and payload input to frame construction. |
| `ReconcileRequest` | Exact command identity whose durable outcome is requested. |
| `CommitReceiptView` | Machine representation of a service commit receipt. |
| `CommitDispositionView` | Distinguishes a new commit, exact retry and reconciled commit. |
| `SubmissionStatusView` | Distinguishes committed, not committed and still unknown outcomes. |

`CommitReceiptView::from` converts an actual core receipt to its machine representation. Constructing the view manually does not create that receipt or authenticate its origin.

When the service returns `Unknown`, retain the command ID and digest and use reconciliation before deciding whether to submit again. Unknown is not a refusal, a successful commit or permission to repeat an external effect. The [backend submission behavior](ZAP-BACKEND-API.md) explains why processing may continue after the response deadline.

`ProtectedCommand` contains no credential. Authentication belongs to the configured channel, and an actor or role label in request data does not supply it.

## Prepare effect alternatives without applying them {#effect-preparation}

`guide r1`

Build the [effect draft inputs](../../../../crates/zap-api/src/commands.rs) for the selected read boundary. Their `into_core` methods validate canonical payloads and delegate construction to the core draft types. Use the preparation route to obtain the actual prepared result.

| Type | Role in this operation |
| --- | --- |
| `PreparationRead` | Selects current or historical state for preparation. |
| `EffectDraftInput` | Input for one effect draft before core validation. |
| `EffectBundleDraftInput` | Input for an ordered alternative or its explicit NoOp basis. |
| `EffectComparisonDraftInput` | Input for comparing alternatives under declared context requirements. |
| `PrepareBundleRequest` | Machine request to prepare one effect bundle. |
| `PrepareComparisonRequest` | Machine request to prepare an alternatives comparison. |
| `EffectPreflightRequestView` | Machine view of one derived core effect request. |
| `EffectBundleRequestView` | Machine view of the complete core bundle request. |
| `PreparedEffectBundleView` | Prepared bundle and affected-scope observations from its captured state. |
| `PreparedEffectComparisonView` | Prepared alternatives and their comparison basis. |

The existing `From` implementations convert core requests and prepared results into these views. They provide a path for consuming actual preparation output without reconstructing its fields from an assumption about the current store.

Preserve returned request bytes, digests and observed boundaries through assessment. NoOp is an explicit preparation form, not a missing bundle. The optional actor in a preparation request remains data for the operation; it does not grant execution authority.

The [CLI preparation description](ZAP-CLI.md) distinguishes preparation from writes, approval consumption and external effects.

## Inspect one record from a prepared overlay {#projected-record}

`guide r1`

Use a [PrepareProjectedRecordRequest](../../../../crates/zap-api/src/commands.rs) when the consumer needs one record from the prepared bundle's final overlay. Keep the selector and the returned preparation boundary together.

| Type | Role in this operation |
| --- | --- |
| `ProjectedRecordSelector` | Typed family and encoded key of the requested record. |
| `PrepareProjectedRecordRequest` | Prepares a bundle and requests one projected record from it. |
| `ProjectedRecordView` | Optional canonical record value and the preparation that produced the view. |

`canonical_value` being absent represents absence in that projected view; it does not establish what exists in a later live revision. Decode present bytes using the selected record contract. Reading the overlay neither applies the bundle nor authorizes its effects.

For an exact current-record read, use `PreparationRead::Current` and an
`EffectBundleDraftInput` with an empty committed prefix, no effects and an explicit
NoOp basis. The existing empty-root recipe uses `BasisPurpose::Completion`,
NotApplicable policy/capacity and the required closure mode, then selects the registered
family and its canonical `RecordKey` bytes. The basis is derived on the same immutable
snapshot but is independent of the selector; an empty Completion basis can perform
global/budgeted basis work and is not a cheap universal record-read promise.

Selector keys are 1–4096 bytes. A present result carries the complete typed canonical
record value and exact observed revision; the NoOp call adds no effect, admission or
store revision. A missing key in a valid family returns `None`. An absent key under an
unknown family can also return `None`, so absence alone does not prove registration.
This method has no separate backend record-value byte cap; configured HTTP response
bounds remain a transport concern.

The [backend preparation route](ZAP-BACKEND-API.md) names this read-only operation.

## Cooperate with the native driver and inspect recovery {#native-runtime}

`guide r1`

The [native-driver request family](../../../../crates/zap-api/src/commands.rs) connects a cooperating host to persisted runtime operations. Select an operation supported by the configured service, retain exact job/dispatch identities, and interpret returned recovery state before attempting another action.

| Type | Role in this operation |
| --- | --- |
| `NativeDriverRequest` | Requests intent reading, launch preparation, restoration, observations, candidates or retry release. |
| `NativeLaunchReadyView` | Prepared intent and authorization revision returned before the actual host invocation. |
| `NativeRecoveryStatusView` | Durable identity and observed state used to choose a recovery action. |
| `RuntimeInspectRequest` | Bounded runtime or job inspection request. |
| `RuntimeView` | Runtime state and detail returned at an observed store revision. |

`NativeSlotCapacityObservation`, `NativeSpawnFailureClass` and `NativeSpawnOutcome` are reexported from the owning [runtime public API](../../../../crates/zap-runtime/src/lib.rs); they are not new API-crate types. Their meaning follows the [runtime contract](ZAP-RUNTIME.xml) and [agent protocol](ZAP-AGENT-PROTOCOL.xml).

Prepared launch is not an observed native invocation. A known refusal and an unknown outcome require different recovery handling. Terminal task state does not establish available slot capacity. Restoration and candidate collection operate on persisted identities; they do not implicitly authorize another launch.

The [CLI](ZAP-CLI.md) relies on a cooperating host to perform native invocations and return their observed receipts.

## Read verified portable archive contents {#portable-archive}

`guide r1`

Use the [archive requests](../../../../crates/zap-api/src/commands.rs) to distinguish trusted publication, archive verification and a bounded entry read. Interpret the returned archive identity before using entry contents.

| Type | Role in this operation |
| --- | --- |
| `BundleArchiveRequest` | Selects the bundle for archive publication or verification. |
| `PortableEntryKindView` | Selects the logical category of an archive entry. |
| `BundleEntryReadRequest` | Requests one entry with an explicit byte bound. |
| `PortableEntryBodyFormat` | Distinguishes raw material from canonical JSON entry content. |
| `BundleArchiveView` | Verified or published archive identity and any associated submission result. |
| `BundleEntryView` | Returned entry bytes with their archive and content identities. |

Use the declared body format and concrete entry contract when interpreting returned bytes. A successful entry read supplies data, not authority to execute an embedded assignment or apply a returned result.

The [backend archive description](ZAP-BACKEND-API.md) explains configured bounds, immutable claim-time material and archive-only entry reading. Publication, verification, a possible unknown submission result and entry consumption remain separate operations.

## Executable integration examples {#integration-examples}

`guide r3`

These existing test entrypoints show how the public types are used with configured applications:

- [Application server journey](../../../../crates/zap-app/tests/application_server.rs): configured service channels, canonical protected commands and authenticated HTTP helpers for the protected-route and completion scenarios.
- [Packet and bundle journey](../../../../crates/zap-app/tests/packet_resolution_service.rs): lowered packet material, a sealed runtime claim, captured-source identity and prepared portable bundles within one service composition.
- [Compiled binary journeys](../../../../crates/zap-cli/tests/binary_server.rs): a typed revision-difference query and authenticated snapshot read against a temporary store, followed by configured index maintenance and a missing-traversal refusal.
- [Change-admission fixture writer](../../../../crates/zap-app/tests/application_server/change_admission_fixture.rs): set `ZAP_CHANGE_ADMISSION_FIXTURE_DIR` to a new empty directory and optionally set `ZAP_CHANGE_ADMISSION_FIXTURE_PROFILE=current`, then run the ignored `write_isolated_change_admission_fixture` test. Start its ordinary `application-server.json` with `zap serve-runtime`; discover the loopback address in `application.endpoint.json`. The generated `fixture.json` names synthetic role credential files and seeded strategy, Work, milestone, plan and resource identities without copying bearer values.

The examples use explicit fixture configuration. Their source is a construction and integration reference; an execution receipt establishes which checks actually ran against particular source and configuration.

## Prepare and record a composite successor candidate {#composite-successor}

`guide r1`

`PrepareCompositeSuccessorRequest` binds one operation, store revision, a
nonempty ordered precursor bundle and a codec-2 successor plan intent. Every
precursor must decode as the registered `MilestoneCreated` or
`MilestoneRevised` payload named by its effect kind. Preparation projects those
effects without committing them, derives and validates the plan against that
projected state, and returns the normalized plan, precursor preflight, request
digest and internal candidate-recording reconciliation identity.

The client persists that complete prepared response before calling the
credentialed record operation. The record operation authenticates only the
configured data-proposal channel, repeats preparation, and refuses any changed
request or preparation. It commits one dormant plan-proposal record and returns
the ordinary submission union. A lost response is resolved through
`/v1/reconcile` and an exact record retry. The candidate commit does not create
or revise milestone heads and does not adopt a plan. Those changes remain the
ordered privileged products of one later comparison and assessment.

## Advance one selected change admission {#change-admission-orchestration}

`guide r3`

`ChangeAdmissionAdvanceRequest` binds one stable operation, exact store and
revision, configured privileged action, proposed assessment and selected
alternative, the comparison draft to re-run, and the next
protected product command without submitting that command.
`source_assessment_digest` remains the pre-adjudication retry identity;
`assessment_digest` is initially equal to it and changes to the exact digest
returned by `OwnerDecisionRequired` when the held operation resumes. Optional
decision and exception identities are references to existing exact records,
not authority claims. Ordered multi-effect changes retain their one aggregate
assessment and advance one effect at a time with the exact applied prefix.

The authenticated service requires a configured coordinator credential whose
action scope matches the selected registered effects. Owner credentials retain
their separate exact decision/control role and do not gain action scope. The
service derives and commits `ChangeAssessmentAdjudicated` and,
when allowed, `ChangeAdmissionPrepared` through its private internal handle.
Comparison preparation returns one backend-derived aggregate affected scope per
alternative, so callers do not reproduce closure logic. Orchestration derives
the action-impact digest through the registered cell and provider; it is not a
caller claim. The response distinguishes `Ready` and `OwnerDecisionRequired` and returns stable
internal command receipts. A lost HTTP response leaves the client uncertain;
retry the byte-equivalent operation request, which reconciles both internal
stage identities before advancing and returns `ExactRetry` receipts without a
duplicate store revision. It never applies the product command. Reader/data
credentials refuse, and no request field can create Owner/coordinator authority.
