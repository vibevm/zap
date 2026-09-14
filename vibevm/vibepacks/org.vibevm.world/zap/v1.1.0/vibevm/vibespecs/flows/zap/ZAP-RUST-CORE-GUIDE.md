# Rust core usage guide {#root}

`guide r2`

This non-normative guide describes acquisition and use of the [zap_core public API](../../../../crates/zap-core/src/lib.rs). Its catalogs cover all 313 locally declared or macro-emitted public types and traits, from registries and trusted service construction through agent capabilities, executable work, packet claims, candidate results and continuation views.

The [storage](ZAP-RUST-STORAGE.xml), [methodology](ZAP-METHODOLOGY.xml), [change-economics](ZAP-CHANGE-ECONOMICS.xml), [agent protocol](ZAP-AGENT-PROTOCOL.xml) and [runtime](ZAP-RUNTIME.xml) contracts remain authoritative. A name, constructed payload or deserialized historical view is separate from current capability, trusted admission and an actual effect.

Five wire-owned error types are reexported for convenience: `ErrorCode`, `ErrorDetail`, `FixSurface`, `RequirementRef` and `ZapError`. Their owning [wire error source](../../../../crates/zap-wire/src/error.rs) and usage contract remain separate from this core denominator.

## Bind store identity and register named capabilities {#registry-identities}

`guide r1`

[StoreIdentity and ReadAt](../../../../crates/zap-core/src/identity.rs) identify the store and requested read boundary. Use the concrete store/application constructors to establish the actual store; constructing an identity value alone neither opens a store nor grants access.

| Type | Role in this operation |
| --- | --- |
| `StoreIdentity` | Identity and epochs to which a configured store operation is bound. |
| `ReadAt` | Requested current or specified revision boundary. |
| `RecordFamily` | Validated name for a registered record family. |
| `IndexFamily` | Validated name for a derived index family. |
| `CapabilityId` | Validated capability identity. |
| `CapabilitySet` | Available capabilities exposed by composed registrations. |
| `RequiredCapabilities` | Required capability identities checked against the available set. |

Parse [RecordFamily, IndexFamily and CapabilityId](../../../../crates/zap-core/src/names.rs) through their own constructors. A valid spelling names a contract; registration establishes whether the configured implementation supplies it. [CapabilitySet and RequiredCapabilities](../../../../crates/zap-core/src/registry.rs) compose and check the selected capability set without inventing missing operations.

## Install trusted bindings at service construction {#trust-bootstrap}

`guide r1`

The [CommitServiceBuilder](../../../../crates/zap-core/src/commit/builder.rs) invokes the selected `TrustBootstrapSource` with a service-created [TrustRegistrar](../../../../crates/zap-core/src/trust.rs). Bind reader, Owner, coordinator, host, internal-protocol and data channels through that registrar. Its constructor is private; it is not a registrar a worker can fabricate from request data.

| Type | Role in this operation |
| --- | --- |
| `TrustBootstrapSource` | Application-supplied startup registration callback. |
| `EmptyTrustBootstrap` | Bootstrap that installs no trust bindings. |
| `TrustRegistrar` | Service-created capability for installing startup bindings. |
| `ControllerEpoch` | Validated controller epoch used to fence trusted scopes. |
| `OwnerScope` | Configured Owner controls for the bound campaign and epoch. |
| `CoordinatorScope` | Configured coordinator actions for the bound campaign and epoch. |
| `SecretInput` | Borrowed secret presented only to credential verification. |
| `SecretVerifier` | Configured verifier for presented secret input. |
| `TrustedHostBinding` | Startup binding for a host's permitted observation events. |
| `InternalProtocolBinding` | Startup binding for exact internal service events. |
| `AgentDataBinding` | Startup binding for non-authorizing data-proposal events. |

Use `ControllerEpoch::new`, `OwnerScope::new` and `CoordinatorScope::new` to validate startup scope inputs. These inputs become authority only when installed by the trusted application. `EmptyTrustBootstrap` installs no bindings. A [SecretVerifier](../../../../crates/zap-core/src/trust.rs) consumes a borrowed `SecretInput`; secret bytes stay outside serializable actor, packet and event records.

## Acquire and use exact authority handles {#authority-acquisition}

`guide r1`

Obtain the [CredentialAuthority](../../../../crates/zap-core/src/trust/authority.rs) from the constructed service. `authenticate` returns an `AuthenticatedPrincipal`; `authorize_read` returns a `ReaderGrant`. `BoundCredentialAuthority` is the service-owned implementation and has no public constructor.

| Type | Role in this operation |
| --- | --- |
| `CredentialAuthority` | Credential authentication and read-authorization boundary. |
| `BoundCredentialAuthority` | Service-owned authority bound to its installed trust registry. |
| `AuthenticatedPrincipal` | Credential-authenticated principal with installed scope. |
| `ReaderGrant` | Credential-derived read grant for a campaign. |
| `TrustedHostHandle` | Trusted issuer used to authorize exact observation frames. |
| `InternalProtocolHandle` | Trusted issuer used to authorize exact internal service frames. |
| `AgentDataIssuerHandle` | Issuer for exact data-proposal frames. |
| `TrustedObservationGrant` | Frame-bound trusted-observation grant. |
| `ServicePermit` | Frame-bound internal operation permit. |
| `AgentDataGrant` | Frame-bound non-authorizing proposal grant. |
| `PrincipalContext` | Borrowed authority context supplied to service admission. |

The registrar returns [TrustedHostHandle, InternalProtocolHandle and AgentDataIssuerHandle](../../../../crates/zap-core/src/trust.rs). Their `authorize` methods bind a grant or permit to the exact frame and permitted operation. Pass the resulting handle through the matching `PrincipalContext` route. A data-proposal grant is not Owner or coordinator authority.

## Interpret actor identity and recorded authority facts {#actor-authority-facts}

`guide r1`

The [authority types](../../../../crates/zap-core/src/authority.rs) keep execution responsibility, principal category and logical operation distinct. Use `ActorRef` for nonsecret attribution and separation checks, not as a credential.

| Type | Role in this operation |
| --- | --- |
| `WorkerRole` | Execution responsibility profile, separate from authority. |
| `PrincipalRole` | Category of the trusted principal. |
| `OperationRef` | Logical command, attempt or semantic operation identity. |
| `ActorRef` | Nonsecret actor and operation identity used for separation checks. |
| `AdmittedAuthority` | Recorded authority facts from the current admission representation. |
| `AdmittedAuthorityV1` | Historical schema-1 authority representation. |

Consume `AdmittedAuthority` through its actor, action, control and observation accessors after the supported service/replay path has established the record. Its service constructors are private. Deserializing historical authority data does not create a live principal or grant; `AdmittedAuthorityV1` retains the schema-1 representation.

## Register typed records before decoding them {#record-contracts}

`guide r1`

Implement the [RecordKey, VersionStamp and StoredRecord contracts](../../../../crates/zap-core/src/record.rs) for a record family. Register the concrete implementation through `RecordSet::single`, `register` or `compose`, then consume the registered decoder and descriptor.

| Type | Role in this operation |
| --- | --- |
| `RecordKey` | Canonical key encoding and ordering contract. |
| `VersionStamp` | Version encoding contract for optimistic record changes. |
| `StoredRecord` | Concrete record's identity, version, codec and index-contribution contract. |
| `RecordSet` | Registry of concrete record implementations. |
| `ErasedRecord` | Registry-decoded record exposed through the object-safe boundary. |
| `RecordDescriptor` | Registered key/value/version codec description. |

`ErasedRecord` is the object-safe representation produced by that registration. Use typed access through the state-reader extension when the concrete record type is known. A valid family name or arbitrary JSON body does not establish that its type matches the registered family.

## Acquire immutable views and continue raw record reads {#immutable-record-reads}

`guide r1`

The service passes [StateReader](../../../../crates/zap-core/src/state.rs) into reducers and providers; a store supplies `SnapshotRead` through `TransactionStore::read`. `StateReaderExt::get_typed` and `scan_typed` use the registered record decoder. `QuerySnapshot` adds the query epoch and limits. None of these read interfaces exposes product mutation methods.

| Type | Role in this operation |
| --- | --- |
| `StateReader` | Object-safe immutable access to registry-decoded records. |
| `StateReaderExt` | Typed convenience access over the immutable record boundary. |
| `QuerySnapshot` | Immutable state view with query epoch and limits. |
| `SnapshotRead` | Concrete backend's typed immutable snapshot interface. |
| `RecordCompleteness` | Complete, continuable or unknown raw record boundary. |
| `ErasedRecordPage` | Raw page of registry-decoded erased records. |
| `RecordPage` | Typed raw record page retaining its exact continuation key. |

Raw `RecordCompleteness::More` means another in-range raw record was observed. A More page is nonempty and ordered, with its exact last item key in `last_key`. Continue from an excluded last-key bound, using the encoded key or the matching typed key representation. `UnknownBoundary` cannot satisfy a consumer that requires complete-family knowledge.

The raw record boundary is separate from query `PageCursor` authority. `SnapshotRead::scan` cannot manufacture a query ID/cursor from a raw continuation and maps that truncated condition to public query `UnknownBoundary`. Use the appropriate raw or query interface rather than treating their completeness types as interchangeable.

## Execute registered queries and retain query cursors {#query-pages}

`guide r1`

Implement [QuerySpec](../../../../crates/zap-core/src/query.rs), register it in `QuerySet`, and use the registered execution path with canonical input and a `QuerySnapshot`. `ErasedQuery` bridges the concrete query to the registry; `ErasedQueryPage` carries its encoded results.

| Type | Role in this operation |
| --- | --- |
| `QuerySpec` | Concrete registered query input/output and execution contract. |
| `QuerySet` | Registry and execution entry point for concrete queries. |
| `ErasedQuery` | Object-safe adapter for a registered query. |
| `ErasedQueryPage` | Encoded query result from the registered adapter. |
| `QueryDescriptor` | Identity and contract description of a registered query. |
| `EncodedRecordKey` | Opaque encoded record key obtained through a key contract. |
| `KeyRange` | Typed record-key bounds. |
| `EncodedKeyRange` | The corresponding bounds after key encoding. |
| `PageLimit` | Requested page size validated against a maximum. |
| `QueryLimits` | Limits carried by a query snapshot. |
| `PageCursor` | Query-specific continuation identity at a recorded boundary. |
| `Completeness` | Public query completeness with a cursor when more data is known. |
| `Page` | Typed query result and its observed boundary. |

Use [EncodedRecordKey::from_key and EncodedKeyRange::from_typed](../../../../crates/zap-core/src/page.rs) for typed boundaries and `PageLimit::within` for the selected maximum. `from_registered_bytes` is the adapter boundary for already encoded keys, not proof of their registered meaning.

A public query `More` carries `PageCursor`, including the query and observed identity. Preserve that cursor through continuation. An unknown boundary is not an empty result or a synthesized cursor.

## Use bound index and traversal interfaces {#index-traversal}

`guide r1`

[IndexPartition::new](../../../../crates/zap-core/src/index_scan.rs) encodes the partition identity. `IndexCatalog::new` and `with_algorithms` describe the selected catalog; `IndexScanRequest::new` and `with_algorithm` bind a requested read to its query/catalog context. Consume the returned index cursor and completeness without treating a catalog value as proof that a backend has installed it.

| Type | Role in this operation |
| --- | --- |
| `IndexPartition` | Encoded partition identity for an index read. |
| `IndexCatalog` | Selected index families, catalog and query identity. |
| `IndexAlgorithm` | Algorithm identity associated with a family. |
| `IndexScanRequest` | Bound request for a partitioned index scan. |
| `IndexCursor` | Continuation identity for the index scan. |
| `IndexEntry` | One returned index row. |
| `IndexPage` | Index result and its bounded continuation state. |
| `DerivedTraversalState` | Store-supplied interface for one traversal generation. |
| `DerivedTraversalProgress` | Observed visited/frontier/quota/generation progress. |

The store supplies [DerivedTraversalState](../../../../crates/zap-core/src/traversal.rs) to the callback for an actual traversal generation. Use its queue, visited-set and quota operations and retain `DerivedTraversalProgress`; a caller-constructed progress value is not a committed generation.

## Inspect record history at its declared revision interval {#record-history}

`guide r1`

[QuerySnapshot::record_history](../../../../crates/zap-core/src/state.rs) reads a bounded historical interval when the backend supports that operation. Bind family/key selection, through-revision and cursor to the intended request; the default trait operation reports unsupported capability.

| Type | Role in this operation |
| --- | --- |
| `RecordHistoryRequest` | Selection and bounds for a historical record query. |
| `RecordHistoryCursor` | Continuation position within that history selection. |
| `HistoryMutationKind` | Historical insert, replace or remove category. |
| `RecordHistoryEntry` | Observed record change with its originating event and reason. |
| `RecordHistoryPage` | History entries and their completeness/next position. |

Consume returned mutations, before/after values and event/command provenance together. Continue using the supplied history cursor. A historical record entry describes a past mutation and does not change the current record or restore its former authority.

## Register typed transitions and their preflight hooks {#transition-registration}

`guide r1`

Implement [CommandPayload and TransitionCell](../../../../crates/zap-core/src/transition/cell.rs), using the payload's canonical contract and a [CellDescriptor](../../../../crates/zap-core/src/descriptor.rs). `CellRegistrationBuilder` attaches the selected hooks before `build`; `CellSet` and `RouteRegistry` compose the registered operations and exact routes.

| Type | Role in this operation |
| --- | --- |
| `CommandPayload` | Typed command kind and canonical payload contract. |
| `TransitionCell` | Pure typed transition invoked by the validated service path. |
| `CellDescriptorInput` | Inputs to the checked operation descriptor. |
| `CellDescriptor` | Checked route, codec, declared mutation and required-preflight description. |
| `CellRegistrationBuilder` | Composition of a typed cell and its selected hooks. |
| `CellSet` | Registry of composed transition cells. |
| `RouteRegistry` | Exact event-kind to authority-route registry. |
| `ErasedCommandPayload` | Object-safe decoded payload boundary. |
| `ErasedTransitionCell` | Object-safe registered transition interface. |
| `PayloadDispatchEligibility` | Payload-derived dispatch eligibility request. |
| `PayloadAffectedJobs` | Payload-derived affected-job request. |
| `PayloadAffectedScope` | Payload-derived affected-scope request. |
| `PayloadSafeJobs` | Payload-derived safe-job request. |

The [payload hook contracts](../../../../crates/zap-core/src/transition/payload.rs) request dispatch, affected-job, affected-scope or safe-job evaluation for that payload. Registration connects them to admission; implementing a trait alone does not authorize an operation.

`ErasedCommandPayload` and `ErasedTransitionCell` support registry dispatch. Implement ordinary typed cells instead of constructing the private erasure adapters.

## Consume validated commands inside the cell boundary {#validated-command-use}

`guide r1`

The service constructs [ValidatedHeader and ValidatedCommand](../../../../crates/zap-core/src/transition/cell.rs) after the registered checks. A cell receives the validated command in `apply` and reads its payload, header, admitted authority and required preflight observations.

| Type | Role in this operation |
| --- | --- |
| `ValidatedHeader` | Service-established header and admitted command context. |
| `ValidatedCommandPreflight` | Sealed preflight context retained for the validated command. |
| `ValidatedCommand` | Typed command wrapper supplied to the transition after validation. |

`ValidatedCommandPreflight` retains the admitted evaluation context. Its construction and sealing remain inside core. A deserialized preflight record or manually assembled payload cannot stand in for the validated wrapper passed to a reducer.

## Build changes without bypassing commit {#prepared-mutations}

`guide r1`

[ChangeSet::new, insert, replace and remove](../../../../crates/zap-core/src/change.rs) accumulate typed changes. Replacement/removal carries the expected version. The service prepares their encoded representation and checks them against the declared operation scope before storage commit.

| Type | Role in this operation |
| --- | --- |
| `ChangeSet` | Typed record changes accumulated by a transition. |
| `MutationKind` | Prepared insert, replace or remove operation category. |
| `PreparedRecordMutation` | Encoded, version-bound record change consumed by commit. |
| `RecordIndexRow` | Index contribution declared by a registered record. |
| `IndexMutationKind` | Prepared insertion or removal of an index row. |
| `PreparedIndexRow` | Encoded index mutation supplied to the store adapter. |

[RecordIndexRow::new or partitioned](../../../../crates/zap-core/src/record.rs) supplies a record's declared index contribution. [PreparedIndexRow](../../../../crates/zap-core/src/commit/index_plan.rs) is the prepared storage representation. Neither collecting changes nor constructing an index row is a database write.

## Construct the service and consume commit outcomes {#commit-service}

`guide r1`

[CommitServiceBuilder::new](../../../../crates/zap-core/src/commit/builder.rs) binds store identity, epochs and startup trust. Attach the actual cells, routes, records, queries and required providers, then call `build`. The builder checks the composition; empty or missing registrations do not become implemented capabilities.

| Type | Role in this operation |
| --- | --- |
| `CommitServiceBuilder` | Configured construction of the trusted commit service. |
| `CommitService` | Admission, preflight, transition and atomic commit orchestration. |
| `CommandPort` | Object-safe submission and reconciliation interface. |
| `CommitReceipt` | Commit result consumed with its exact command and store identity. |
| `CommitReceiptParts` | Validated storage/reconciliation inputs for receipt reconstruction. |
| `CommitDisposition` | New commit, exact retry or reconciled commit outcome. |
| `CommitStatus` | Durable committed, not-committed or unknown reconciliation status. |

Use [CommitService::execute](../../../../crates/zap-core/src/commit/service.rs) or `CommandPort::submit` with the obtained `PrincipalContext` and canonical frame. Consume `CommitReceipt` accessors and disposition; use reconciliation when the durable outcome needs checking.

`CommitReceipt::from_validated_parts` is a public store/reconciliation construction boundary. It does not independently execute the command or authenticate arbitrary supplied history. Preserve the distinction between a receipt returned by the supported service and a receipt-shaped value assembled by a caller.

## Implement the store transaction boundary {#transaction-binding}

`guide r1`

[TransactionStore](../../../../crates/zap-core/src/commit.rs) supplies concrete associated read/write handles. Its `transact` callback receives a write handle only through the service-provided `TransactionPermit`; the store binds it to its actual identity, head and nonce.

| Type | Role in this operation |
| --- | --- |
| `TransactionStore` | Concrete backend's permitted read/write transaction contract. |
| `AtomicWrite` | Write-side interface consuming a validated commit operation. |
| `TransactionNonce` | Store-supplied identity for the actual transaction binding. |
| `TransactionPermit` | Service-created permission to enter its store's write boundary. |
| `TransactionBinding` | Actual store, head and transaction identity bound by the adapter. |
| `ValidatedCommitIntent` | Fully checked commit operation supplied to atomic storage. |

[TransactionNonce::from_store_bytes and TransactionPermit::bind](../../../../crates/zap-core/src/commit/logical.rs) support that adapter boundary. The permit constructor is private. `AtomicWrite::apply_commit` consumes `ValidatedCommitIntent`, whose constructor is also private, so the ordinary caller cannot replace admission with an arbitrary prepared batch.

## Decode logical events and replay with the matching context {#history-replay}

`guide r1`

[decode_logical_event](../../../../crates/zap-core/src/commit/logical.rs) selects the supported schema from exact canonical event bytes. Retain its schema-specific authority, preflight and outcome records rather than reinterpreting older bytes under newer semantics.

| Type | Role in this operation |
| --- | --- |
| `LogicalEventV1` | Historical schema-1 logical event representation. |
| `LogicalEventV2` | Schema-2 logical event with current preflight/outcome representation. |
| `DecodedLogicalEvent` | Selected supported logical-event schema. |
| `ReplayProviders` | Providers required for the selected replay semantics. |
| `ReplayContext` | Checked cell/record/provider context for replay. |
| `ReplayedTransition` | Record and index changes rederived from the event. |

Build [ReplayContext](../../../../crates/zap-core/src/commit.rs) with the matching schema cell sets, record registry and providers, then use the supported replay functions. `ReplayedTransition` exposes the rederived record/index changes. Replaying historical data does not grant current execution authority or perform a host effect.

## Retain the explicit schema-1 admission boundary {#schema-one-admission}

`guide r1`

The [V1 admission contracts](../../../../crates/zap-core/src/commit.rs) remain separate from the current lifecycle. `AdmissionHookDescriptorV1::new` declares the historical hook scope; `ActionAdmissionObservationV1::new` encodes its payload and `decode` verifies that payload's identity.

| Type | Role in this operation |
| --- | --- |
| `ActionAdmissionRequestV1` | Historical privileged-action request representation. |
| `AdmissionHookDescriptorV1` | Historical hook identity and declared mutation scope. |
| `ActionAdmissionObservationV1` | Historical admission observation and encoded payload. |
| `ActionAdmissionProviderV1` | Configured schema-1 admission and application contract. |
| `Schema1ActionAdmissionReplay` | Replay contract for applying recorded schema-1 admission evidence. |

Use `ActionAdmissionProviderV1` only where that schema's live/service path is configured, and `Schema1ActionAdmissionReplay` for the matching historical application of recorded evidence. A V1 observation is not a current-schema authorization.

## Compose the complete blocker denominator {#completion-evaluation}

`guide r1`

Register concrete [CompletionBlockerProvider](../../../../crates/zap-core/src/completion.rs) implementations in `CompletionProviderSet`. `CompletionEvaluator::new` checks that the required providers are present; `view` evaluates their contributions against the same state boundary.

| Type | Role in this operation |
| --- | --- |
| `CompletionBlocker` | Typed reason that prevents completion. |
| `CompletionBlockerProvider` | One registered contribution to the shared blocker query. |
| `CompletionProviderSet` | Checked collection of named completion providers. |
| `CompletionEvaluator` | Composition enforcing the required provider denominator. |
| `CompletionView` | Completion result bound to the evaluated state. |

Consume the returned blockers and eligible flag. A clear frontier or accepted child list is not a substitute for the configured completion denominator, and an eligible view does not itself commit campaign closure.

## Request the relevant basis explicitly {#basis-request}

`guide r1`

[BasisRequest::new](../../../../crates/zap-core/src/basis/mod.rs) binds purpose, roots and policy/capacity/closure requirements. `PayloadBasisScope` derives that request for a concrete command; `BasisProvider::relevant_basis` computes it from the supplied state.

| Type | Role in this operation |
| --- | --- |
| `BasisPurpose` | Operation whose relevant basis is being requested. |
| `ContextRequirement` | Required or inapplicable contextual contribution. |
| `ClosureRequirement` | Requested knowledge-closure boundary. |
| `BasisRequestInput` | Inputs to a checked relevant-basis request. |
| `BasisRequest` | Normalized scoped request for relevant evidence. |
| `BasisProvider` | State-bound derivation of a request's relevant basis. |
| `PayloadBasisScope` | Payload adapter selecting the required basis scope. |

Select Required or NotApplicable context deliberately. An omitted requirement is not proof that no dependency or unknown boundary exists. The provider and request preserve the scope of the evidence used by admission.

## Retain captured fingerprints and closure knowledge {#basis-evidence}

`guide r1`

The [RelevantBasisInput family](../../../../crates/zap-core/src/basis/mod.rs) describes the captured contributions to the selected request. `RelevantBasis::new` normalizes and checks their representation and derives its identity. It does not discover missing semantic edges or establish the truth of arbitrary supplied fingerprints.

| Type | Role in this operation |
| --- | --- |
| `PolicyFingerprint` | Identity of the policy contribution. |
| `IntentFingerprint` | Identity of the relevant intent contribution. |
| `OutcomeFingerprint` | Identity of the relevant outcome contribution. |
| `SubjectFingerprint` | Identity/version contribution of a relevant subject. |
| `BasisDependencyEndpoint` | Typed endpoint represented in basis dependencies. |
| `DependencyFingerprint` | Identity of a relevant dependency contribution. |
| `ContractFingerprint` | Identity of the relevant contract contribution. |
| `EvidenceFingerprint` | Identity and applicability contribution of selected evidence. |
| `KnowledgeFingerprint` | Identity of relevant knowledge/uncertainty information. |
| `ResourceCapacityFingerprint` | Selected resource-capacity contribution. |
| `TeamCapacityFingerprint` | Selected team-capacity contribution. |
| `ClosureKnowledge` | Declared completeness or uncertainty of the known closure. |
| `RelevantBasisInput` | Captured contributions used to build the basis. |
| `RelevantBasis` | Checked scoped basis and its identity. |

Consume the resulting basis with its request and observed state. Closure knowledge remains explicit; matching a digest preserves byte identity of the selected evidence, not universal knowledge about unvisited parts of the project.

## Classify action impact against the same state {#action-impact}

`guide r2`

Use [ActionImpactRequest::new](../../../../crates/zap-core/src/admission/impact.rs) to bind the registered classification rule to its work/subject scope. `PayloadActionImpact` obtains that request from a typed payload, and `ActionImpactProvider::classify` evaluates it with the supplied state and action context.

| Type | Role in this operation |
| --- | --- |
| `ActionImpactRule` | Registered rule used to classify the operation. |
| `ActionImpactClass` | Derived baseline, progress, proof or semantic-change class. |
| `ActionImpactContext` | Current action/event/payload context supplied for classification. |
| `ActionImpactRequest` | Checked classification request and scope. |
| `ActionImpactView` | State-bound impact result and its identity. |
| `PayloadActionImpact` | Typed payload adapter for the impact request. |
| `ActionImpactProvider` | Configured classifier over the supplied immutable state. |

`ActionImpactView::new` constructs the bound classification result. Progress, proof, initial baseline and semantic change retain distinct meanings; selecting a label in data does not bypass the configured admission provider.

`InitialMilestonePlanOrSemantic` is the narrow bootstrap rule for canonical
milestone definition and first plan adoption. The domain classifier returns
`InitialBaseline` only for an exact active candidate strategy and outcome when
there is no adopted milestone plan, lowering, materialized Work, pending change
admission, hold, or known execution record. Further milestone creation or plan
adoption is a semantic change even when no Work has launched yet. Its affected
scope may name absent strategic Work and absent obligation owners only during
that verified initial state; ordinary affected-scope requests still reject
missing Work.

## Evaluate needs and enforce admission around product changes {#action-admission}

`guide r1`

The [ActionAdmissionProvider lifecycle](../../../../crates/zap-core/src/admission/protocol.rs) calls `needs`, evaluates the requested preflight, calls `admit`, applies admission changes and checks `verify_after` against the actual product outcome. The descriptor declares separate exempt/economic mutation scopes.

| Type | Role in this operation |
| --- | --- |
| `ActionBasis` | Request paired with its relevant basis. |
| `ActionAdmissionRequest` | Exact action, command and impact submitted to admission. |
| `ActionAdmissionNeeds` | Requested effect/scope/independence/safety preflight. |
| `ActionAdmissionPreflightRecord` | Serializable preflight observations retained in history. |
| `ActionAdmissionPreflight` | Service-supplied observations and sealed witnesses for this admission. |
| `ActionAdmissionBasis` | Exact exempt or economic basis of the admission result. |
| `AdmissionMutationScope` | Declared record and index families the hook may change. |
| `AdmissionHookDescriptor` | Hook identity, epoch and permitted mutation scopes. |
| `ActionAdmissionObservation` | Encoded and bound result of the admission decision. |
| `ActionProductOutcome` | Actual product effect/mutation/basis outcome checked after application. |
| `ActionAdmissionProvider` | Configured lifecycle around the privileged product write. |

`ActionAdmissionNeeds::new` identifies the selected effect, affected scopes, independence and safe-job requests. `ActionAdmissionObservation::new` binds its encoded observation to the impact and admission basis. These values remain evidence for the service lifecycle rather than authority obtained by constructing a DTO.

The service supplies `ActionAdmissionPreflight`; its constructor and witness seals are private. Use its accessors for the matching request or hold. Copying an `ActionAdmissionPreflightRecord` cannot manufacture the transaction-bound independence or safety witnesses.

## Construct alternatives for preparation {#effect-drafts}

`guide r1`

[EffectDraft::new, EffectBundleDraft::new and EffectComparisonDraft::new](../../../../crates/zap-core/src/effects/draft.rs) validate the submitted alternative structure. A NoOp uses its explicit basis rather than an ambiguous missing effect list.

| Type | Role in this operation |
| --- | --- |
| `EffectDraft` | One typed effect proposed for preparation. |
| `EffectBundleDraft` | Ordered alternative or explicit NoOp preparation input. |
| `EffectComparisonDraft` | Alternatives and context requirements for comparison. |
| `EffectPreflightRequestInput` | Inputs to one checked effect preflight request. |
| `EffectPreflightRequest` | Exact effect and declared basis/scope request. |
| `EffectBundleRequest` | Ordered checked effect requests or an explicit NoOp request. |

The [effect request constructors](../../../../crates/zap-core/src/effects.rs) bind exact payloads, predecessors, event identities and before/after basis declarations. Preparation derives and checks these against the registered contract; declaration alone neither applies an effect nor establishes its outcome.

## Share the pure scope and simulation contract {#effect-contracts}

`guide r1`

Register [EffectContract](../../../../crates/zap-core/src/effects.rs) with its transition. `scope` derives the effect's subject/basis/artifact requirements; `simulate` uses the same pure effect kernel against the supplied projected state. `PayloadEffectBundles` identifies additional effect-bundle preflight required by a payload.

| Type | Role in this operation |
| --- | --- |
| `EffectScope` | Checked scope returned by the registered effect contract. |
| `EffectScopeContext` | Core-supplied context for deriving scope. |
| `EffectSimulationContext` | Core-supplied projected-state context for simulation. |
| `EffectContract` | Pure scope and simulation implementation shared by preparation. |
| `PayloadEffectBundles` | Payload adapter requesting additional effect-bundle preflight. |

`EffectScopeContext` and `EffectSimulationContext` are supplied by core and have private constructors. Consume their observed identity, actor and relevant-basis/scope accessors; do not fabricate a simulation context to evade admission. Simulation remains separate from external I/O and product commit.

## Consume prepared results on their captured boundary {#prepared-effects}

`guide r1`

Acquire [PreparedEffectBundle and PreparedEffectComparison](../../../../crates/zap-core/src/effects.rs) through `CommitService::prepare_effect_bundle` or `prepare_effect_comparison`. Their constructors are private. For an overlay read on the same preparation, use the service's `with_prepared_effect_bundle` callback.

| Type | Role in this operation |
| --- | --- |
| `EffectPreflightView` | Derived result for one effect's preparation. |
| `EffectBundlePreflightView` | Derived ordered bundle result and identity. |
| `PreparedEffectBundle` | Service-produced bundle preparation at one captured boundary. |
| `PreparedEffectComparison` | Service-produced alternatives and comparison basis. |
| `CommandPreflightRecord` | Serializable command preflight observations retained with the command. |

Consume the request, observed revision, before/after views and affected scopes from that prepared result. Preparation does not consume approvals or commit mutations. Preserve its identities for the later checked path; a copied preflight record is not a new current preparation.

## Keep artifact witnesses and producer provenance distinct {#artifact-provenance}

`guide r1`

The [artifact contracts](../../../../crates/zap-core/src/artifact.rs) separate a payload's required artifact identities from the provider that opens/verifies them. `ArtifactWitnessProvider::prepare` supplies an `ArtifactWitnessGuard`; retain the guard through the service-defined boundary rather than substituting a path or digest-only assertion.

| Type | Role in this operation |
| --- | --- |
| `PayloadArtifacts` | Typed extraction of artifact identities required by a payload. |
| `ArtifactWitnessProvider` | Configured preparation of verified artifact witnesses. |
| `ArtifactWitnessGuard` | Retained witness lifetime and verified artifact identities. |
| `ProducerRef` | Actor/job/attempt/packet identity of the producer. |
| `CandidateProvenanceInput` | Inputs to the checked candidate provenance record. |
| `CandidateProvenanceRecord` | Durable candidate provenance consumed by later checks. |

[CandidateProvenanceRecord::new](../../../../crates/zap-core/src/candidate.rs) validates producer, subject, contract and artifact bindings supplied in `CandidateProvenanceInput`. It records provenance for later checks, not acceptance of the candidate or a live authority grant.

## Resolve desired profiles against observed support {#worker-profiles}

`guide r1`

Parse [ProviderName, ModelName and EffortName](../../../../crates/zap-core/src/agent/profiles.rs) through their generated constructors, then retain the Owner-selected `DesiredProfile` separately from resolution. `ResolvedProfile::new` checks that Exact agrees with the supplied actual values and that an unresolved result does not masquerade as a complete actual profile.

| Type | Role in this operation |
| --- | --- |
| `ProviderName` | Bounded provider spelling used by profile configuration. |
| `ModelName` | Bounded model spelling used by profile configuration. |
| `EffortName` | Bounded effort spelling used by profile configuration. |
| `DesiredProfile` | Owner-selected role/provider/model/effort preference. |
| `ProfileResolution` | Exact or explicitly unresolved/unsupported profile outcome. |
| `ResolvedProfile` | Observed resolution retained alongside its desired profile. |

Consume `resolution` or `is_exact` before preparing dispatch. A missing model, missing effort or unavailable host remains explicit; matching a model name does not establish authority or prove that a host invocation occurred.

## Describe observed host capabilities without inventing support {#host-capabilities}

`guide r1`

[ModelCapability::new](../../../../crates/zap-core/src/agent/capabilities.rs) validates a model's supported effort list. `AgentCapabilities::validate` checks the assembled capability representation and its evidence; `digest` identifies the complete observation, while `value_digest` compares capability values separately from refreshed observation provenance.

| Type | Role in this operation |
| --- | --- |
| `CapabilitySupport` | Supported, unsupported or unknown capability status. |
| `InstructionIsolation` | Observed instruction-inheritance/isolation boundary. |
| `LivenessCapability` | Available push/poll liveness mechanism or its absence. |
| `CancellationCapability` | Observed cancellation mechanism or unsupported/unknown state. |
| `GoalScope` | Goal scope supported or described by the host. |
| `GoalOperation` | Exact goal operation being requested. |
| `GoalOperationSupport` | Agent-callable, Owner-only or unavailable operation support. |
| `GoalOperationCapabilities` | Per-operation goal support description. |
| `GoalCapability` | Goal scope combined with its operation capabilities. |
| `AdapterIdentity` | Adapter/version/toolset identity attached to observations and handles. |
| `ModelCapability` | Observed model and its supported effort values. |
| `AgentCapabilities` | Captured host capability values and their observation provenance. |

Keep Unsupported and Unknown distinct. `GoalOperationCapabilities::support` selects support for the exact requested operation; it does not translate an unsupported update into a different action. Capability data is consumed from the configured host/evidence path, not promoted to observed support merely because its DTO can be constructed.

## Build a goal projection and plan its application {#goal-planning}

`guide r1`

[GoalProjection::build](../../../../crates/zap-core/src/agent/goals.rs) checks projection scope and assignment binding and computes its content identity. `validate` rederives that representation. `CharterRevision` identifies the charter version used by the projection; its construction is not a charter activation.

| Type | Role in this operation |
| --- | --- |
| `CharterRevision` | Charter version carried by a goal projection. |
| `GoalProjectionInput` | Inputs to the bounded goal projection builder. |
| `GoalProjection` | Checked goal content and its bound projection identity. |
| `GoalApplicationPlan` | Decision to invoke the operation or record its fallback state. |
| `GoalApplicationState` | Actual, manual-required, unsupported, unknown or stale application state. |

Use `plan_goal_application` with the observed capability, required scope and exact operation. Consume Invoke or the explicit recorded fallback state. An Invoke plan is still a plan; Applied requires an actual acknowledgment recorded through the trusted runtime path.

## Bind intent, external handle and observed dispatch receipt {#host-dispatch}

`guide r1`

Consume the configured [AgentHost](../../../../crates/zap-core/src/agent/host.rs) for capabilities, dispatch, observation and collection. [DispatchIntent::validate and digest](../../../../crates/zap-core/src/agent/dispatch.rs) bind the intended attempt to its host, profile, packet and contract; constructing an intent does not commit it or start a worker.

| Type | Role in this operation |
| --- | --- |
| `AgentHost` | Configured boundary for executable workers and their observed results. |
| `DispatchIntent` | Exact intended worker attempt and its packet/host binding. |
| `DispatchState` | Awaiting, submitted, starting, running or unavailable dispatch state. |
| `ExternalJobHandle` | Adapter-scoped external identity bound to the intended attempt. |
| `DriverProvenance` | Non-authorizing binding between driver observation and capability evidence. |
| `DispatchReceipt` | Observed dispatch state with its required handle binding. |
| `HostJobState` | State reported by the external host observation. |
| `JobObservation` | Host state plus activity/ownership evidence for that observation. |

The adapter supplies actual output to `ExternalJobHandle::new`. `DispatchReceipt::bind` checks state and handle presence against the intent, and `DriverProvenance::bind` associates an observation with captured capability evidence. These constructors validate representation and binding; they do not independently verify that an external tool ran.

AwaitingHarness and Unavailable have no external handle. Submitted, Starting and Running require a matching handle. Consume later `JobObservation` separately from the initial dispatch receipt, preserving unknown activity and ownership observations.

## Request stops and reconcile external outcomes {#host-stop-reconcile}

`guide r1`

Use the [host stop and reconciliation contracts](../../../../crates/zap-core/src/agent/dispatch.rs) through `AgentHost::request_stop` and `reconcile`. Preserve the effect and intent identities when interpreting the response.

| Type | Role in this operation |
| --- | --- |
| `StopMode` | Selected cooperative or exact-process stop mechanism. |
| `StopRequest` | Effect-bound stop request sent through the host boundary. |
| `StopDelivery` | Observed progress or uncertainty of stop delivery. |
| `StopReceipt` | Effect-bound stop-delivery observation. |
| `ReconciliationState` | Observed not-started, running, terminal or unknown outcome. |
| `ReconciliationObservation` | Reconciliation result bound to the dispatch intent and any retained receipt. |

Requested, Delivered, AlreadyTerminal and Unknown are different stop-delivery observations. A terminal reconciliation result does not by itself prove an application safe boundary, semantic acceptance or available host capacity. Reconcile unknown outcomes before deciding whether another effect is admissible.

## Consume candidate evidence without accepting it implicitly {#candidate-results}

`guide r1`

[CandidateResult::validate](../../../../crates/zap-core/src/agent/dispatch.rs) normalizes evidence references and rejects duplicate artifact, check or criterion identities. It deliberately retains unsatisfied criteria and unresolved findings. Use the dispatched result contract for the additional work/contract/basis checks.

| Type | Role in this operation |
| --- | --- |
| `ArtifactKind` | Reported artifact category. |
| `ArtifactRef` | Artifact identity, category and reported size. |
| `CriterionDisposition` | Reported satisfied or unsatisfied requirement outcome. |
| `CriterionResult` | One requirement's reported disposition and evidence references. |
| `CheckRef` | Verification identity and its observation reference. |
| `CandidateEffectState` | Reported external-effect state accompanying a candidate. |
| `CandidateResult` | Producer-bound candidate content, evidence and unresolved work. |

Artifact and verification references identify reported evidence; their presence does not prove the bytes, check outcome or effect state. Candidate collection and structural validation remain separate from artifact verification and semantic acceptance by the appropriate actor.

## Record the supported safe revalidation release {#revalidation-release}

`guide r1`

[WorkRevalidationReleaseRecord::new](../../../../crates/zap-core/src/agent/revalidation.rs) accepts the supported release combinations: Revalidate with Safe/Completed and a newer generation, or NotRequired with affirmative NoEffectProven evidence and an unchanged generation. Other reconciliation actions are not interchangeable with those release outcomes.

| Type | Role in this operation |
| --- | --- |
| `WorkRevalidationReleaseKey` | Exact review/job/attempt/request identity of the release. |
| `ReconciliationAction` | Selected reconciliation action to be represented by the appropriate operation. |
| `ReconciliationSafeState` | Observed safety category used when validating release. |
| `WorkRevalidationReleaseInput` | Inputs to the checked work-release constructor. |
| `WorkRevalidationReleaseRecord` | Checked release evidence consumed by revalidation readiness. |

Retain the exact review/job/attempt/request key and observed release revision. The constructed record is an input to its trusted recording path; it is not permission obtained from an arbitrary safety label.

## Retain the executable work contract and its lineage {#work-contract}

`guide r1`

The [work contract types](../../../../crates/zap-core/src/execution_views/work.rs) describe the captured work version, validation generation, obligations, resource needs, workspace, delivery and selected verification. Use `IntegrationOwner::parse` and the declared counter constructors when creating their values.

| Type | Role in this operation |
| --- | --- |
| `ContractVersion` | Version of the executable task contract. |
| `ValidationGeneration` | Generation under which work must be revalidated. |
| `MaturityStage` | Required or reported stage in the execution contract. |
| `IntegrationOwner` | Bounded integration-owner identity used by scheduling. |
| `ResourceClaim` | Named positive resource demand. |
| `WorkspaceMode` | Declared existing, isolated-worktree or temporary workspace mode. |
| `WorkspaceBinding` | Workspace identity tied to the store/base and declared mode. |
| `DeliveryRoute` | Selected native-harness or subprocess-adapter route. |
| `SafeStopContract` | Required safe boundary and optional verifier identity. |
| `AcceptanceCriterion` | Requirement and statement that work must satisfy. |
| `SourceFingerprint` | Source identity used by the contract or proof. |
| `VerificationPlan` | Selected check with its concrete target, inputs and execution context. |

`ContractVersion::new` and its transparent numeric deserializer both require a positive value; `ValidationGeneration::new` and its deserializer both permit zero. Positive values retain their exact numeric round trip. A workspace or delivery declaration also does not itself create an isolated checkout, execute an operation or authorize a transport fallback.

Consume verification plans with their actual source, subject, environment and toolchain bindings. A required maturity stage or acceptance criterion is an obligation, not evidence that the stage or criterion has already been satisfied.

## Read current work and packet selection before admission {#work-readiness}

`guide r2`

Obtain [CampaignReadPort](../../../../crates/zap-core/src/execution_views/work.rs) from the configured application. Its snapshot, frontier, work-execution, current-packet, readiness and completion methods provide distinct read views. Retain their revision/basis and cursor information. A `More` frontier continues from its `PageCursor`; `UnknownBoundary` is a refusal boundary rather than evidence that no later ready work exists.

| Type | Role in this operation |
| --- | --- |
| `FrontierRequest` | Revision and cursor request for a bounded frontier read. |
| `FrontierWorkView` | Compact work/version/obligation view used during selection. |
| `WorkExecutionView` | Full executable work contract presented to runtime consumers. |
| `ReadinessBlocker` | Typed reason current work cannot advance. |
| `ReadinessView` | Work/basis-bound blockers and derived ready flag. |
| `CurrentPacketSelection` | Unique current packet identity selected by a read. |
| `CampaignReadPort` | Configured immutable campaign views needed by the coordinator. |

`WorkExecutionView::validate` checks represented subjects, obligations, resources and source uniqueness. `ReadinessView::new` derives its ready flag from the supplied blocker set. The application must still derive those blockers from actual state.

`CurrentPacketSelection` is a read-only freshness/identity selection. The commit service resolves the packet again for the claim transaction; a selected packet or ready view is not a launch grant.

## Evaluate affected jobs while retaining independent state dimensions {#execution-observations}

`guide r1`

[WorkExecutionObservationRecord](../../../../crates/zap-core/src/execution_views/job.rs) exposes trusted current runtime state to domain admission. Its `validate` normalizes subject references; `is_active` includes both nonterminal execution and a Started/Unknown effect even after execution becomes terminal.

| Type | Role in this operation |
| --- | --- |
| `ExecutionState` | Recorded progress or uncertainty of execution. |
| `CollectionState` | Progress of result collection and candidate recording. |
| `EffectState` | Recorded progress or uncertainty of the external effect. |
| `SafeState` | Recorded safe-boundary or reconciliation state. |
| `AcceptanceState` | Separate unreviewed, rejected or accepted result state. |
| `WorkExecutionObservationRecord` | Job/contract-bound runtime observation used by admission. |
| `AffectedJobRequestInput` | Work/subject selection for the affected-job query. |
| `AffectedJobRequest` | Normalized affected-job request and identity. |
| `AffectedJobCompleteness` | Affirmative complete or unknown query boundary. |
| `AffectedJobView` | State-bound matching job observations and completeness. |
| `AffectedJobProvider` | Configured derivation of affected jobs on the supplied state. |

Build the requested scope with `AffectedJobRequest::build` and consume the configured `AffectedJobProvider::evaluate` result. `AffectedJobView::new` normalizes observations and rejects conflicting rows for one job identity. Complete-empty and Unknown are distinct results; do not infer completeness from an empty vector alone.

Execution, collection, effect, safety and acceptance have independent meanings. A terminal state or CandidateRecorded collection status does not imply safe state, acceptance or slot availability.

## Reevaluate the exact dispatch claim {#dispatch-eligibility}

`guide r1`

[DispatchEligibilityRequest::build](../../../../crates/zap-core/src/execution_views/eligibility.rs) binds work, contract, generation, basis and resource/subject declarations. The configured `DispatchEligibilityProvider` evaluates that request against the transaction pre-state.

| Type | Role in this operation |
| --- | --- |
| `DispatchEligibilityRequestInput` | Exact claim inputs that require reevaluation. |
| `DispatchEligibilityRequest` | Bound request for the dispatch gate. |
| `DispatchEligibilityBlocker` | Typed reason dispatch is not currently eligible. |
| `DispatchEligibilityView` | Request/revision-bound blockers and derived eligible flag. |
| `DispatchEligibilityProvider` | Configured transaction-state evaluator of dispatch eligibility. |

`DispatchEligibilityView::new` derives eligible from the returned blockers; custom deserialization also checks that agreement. Consume its request digest and observed revision with the result. A caller-supplied empty blocker list does not replace the provider's current-state evaluation.

## Derive affected scope with explicit uncertainty {#affected-scope}

`guide r1`

[AffectedScopeRequest::new](../../../../crates/zap-core/src/execution_views/affected_scope.rs) binds typed roots/direct work. The special initial-lowering constructor represents that exact request form; it is not a general permission to ignore missing work.

| Type | Role in this operation |
| --- | --- |
| `AffectedScopeRequest` | Checked typed-root/direct-work request for affected scope. |
| `AffectedScopeCompleteness` | Complete or explicitly incomplete knowledge boundary. |
| `DerivedAffectedScope` | Provider-derived work/subject closure before the combined view. |
| `AffectedScopeView` | Affected scope, related job observations and bound scope identity. |
| `AffectedScopeProvider` | State-bound affected-closure and independence evaluation contract. |

`AffectedScopeProvider::derive` obtains the actual closure. `DerivedAffectedScope::validate` checks direct/dependent separation and agreement between completeness and the unknown boundary. `AffectedScopeView::new` combines the derived scope with job observations; constructing the view does not prove the selected scope was complete.

## Acquire independence evidence for the exact hold and candidate {#independence-witness}

`guide r1`

[IndependenceRequest::new](../../../../crates/zap-core/src/execution_views/affected_scope.rs) binds the held scope, candidate request and relevant basis. `AffectedScopeProvider::assess_independence` supplies the result; `IndependenceView::new` represents the bound observation, including remaining unknowns.

| Type | Role in this operation |
| --- | --- |
| `IndependenceRequest` | Exact hold/candidate/basis request for independence evaluation. |
| `IndependenceView` | Reported independence result and its uncertainty/basis binding. |
| `IndependenceWitness` | Core-sealed independence observation for this admission boundary. |

`IndependenceWitness` is created inside core and acquired through the matching `ActionAdmissionPreflight::independence_for` accessor. Its transaction/service seals are private. A copied IndependenceView is not a witness for another transaction or service.

## Acquire safety evidence for the selected held executions {#safe-job-witness}

`guide r1`

[SafeJobRequest::new](../../../../crates/zap-core/src/execution_views/affected_scope.rs) selects exact-scope validation; `held_executions` binds the explicit held job identities. `HeldJobIdentity::from_observation` and `matches` preserve job, attempt, work, contract and validation generation.

| Type | Role in this operation |
| --- | --- |
| `SafeJobRequest` | Request for exact-scope or held-execution safety validation. |
| `SafeJobValidationMode` | Which safety-validation binding the request uses. |
| `HeldJobIdentity` | Exact execution/contract identity retained by the hold. |
| `SafeJobView` | Bound scope/job safety observation returned for the request. |
| `SafeJobWitness` | Core-sealed safety evidence for the current admission boundary. |

Consume `SafeJobView` with its mode, bound scope and reported all-safe result. Its constructor does not observe the host or prove safety. The corresponding `SafeJobWitness` has a private constructor and is obtained through the matching service/preflight accessor; a serialized view cannot substitute for that sealed witness.

## Capture packet inputs and retain stage-debt meaning {#packet-materials}

`guide r1`

The [material and workspace provider contracts](../../../../crates/zap-core/src/execution_views/packet/providers.rs) separate live capture from verification of already captured content. Use their capture/verify methods with the exact source/rule/fork or workspace request supplied by packet resolution.

| Type | Role in this operation |
| --- | --- |
| `PacketMaterialSubject` | Typed source, rule or fork identity requested for capture. |
| `PacketMaterialRequest` | Store/base/revision-bound request for that material. |
| `PacketMaterialProvider` | Live-capture and captured-material verification boundary. |
| `PacketWorkspaceRequest` | Exact packet/work/harness workspace capture request. |
| `PacketWorkspaceProvider` | Workspace capture and verification boundary. |
| `CapturedPacketMaterial` | Captured content artifact, length and token estimate. |
| `ResolvedPacketSource` | Source identity associated with its captured material. |
| `ResolvedPacketRule` | Requirement/source identity associated with captured rule material. |
| `ResolvedPacketFork` | Strategic fork identity associated with its captured material. |
| `CapturedPacketWorkspace` | Captured workspace binding and manifest artifact. |
| `ResolvedStageDebtDisposition` | Required, accepted or deferred status of one stage obligation. |
| `ResolvedStageDebt` | Work/stage obligation and its retained disposition. |

The [resolved material records](../../../../crates/zap-core/src/execution_views/packet.rs) retain source identities and content artifacts. Captured byte/token information describes the selected material; it does not prove semantic completeness or grant instruction authority.

Retain the Required, Accepted or Deferred disposition of each stage debt with its corresponding identity. Carrying a deferral is not discharging the underlying obligation.

## Seal resolved packet evidence for the claim transaction {#packet-claim-sealing}

`guide r1`

[PacketResolutionRequest::new](../../../../crates/zap-core/src/execution_views/packet.rs) binds packet, job, attempt, dispatch and effect identities. `PayloadPacketResolution` selects that request for the typed command. The configured provider receives the current state and the service-created `PacketResolutionContext`.

| Type | Role in this operation |
| --- | --- |
| `PacketResolutionRequest` | Exact packet-to-attempt resolution request and its identity. |
| `PayloadPacketResolution` | Typed payload adapter selecting the packet resolution request. |
| `ResolvedPacketIdentity` | Store, strategy, lowering and packet lineage of the resolved work. |
| `ExpectedProducer` | Principal/harness/role/capability binding expected from execution. |
| `RuntimeJobClaimRecord` | Serializable resolved work, material and execution-claim evidence. |
| `PacketResolutionContext` | Service-created command and sealing context supplied to the provider. |
| `PacketResolutionProvider` | Live and captured-replay resolution of the exact packet request. |
| `RuntimeJobClaim` | Transaction/service-sealed resolution wrapper acquired through the supplied context. |

`RuntimeJobClaimRecord::validate` checks the represented work/profile/material bindings and recomputes its digest. The context's `seal` produces `RuntimeJobClaim` for this transaction and service. Both the context constructor and claim seals are private; a deserialized record cannot manufacture that wrapper. The service still admits and commits the job-claim operation.

`PacketResolutionProvider::replay_captured` consumes retained historical evidence through the replay context instead of treating current recapture as the original input. `ResolvedPacketIdentity` retains strategy and lowering lineage alongside packet bytes; `ExpectedProducer` binds the later producer check. Neither packet resolution nor a committed job claim by itself proves actual host invocation.

## Bind and check the dispatched result contract {#candidate-contract}

`guide r1`

[CandidateResultTemplate::new](../../../../crates/zap-core/src/execution_views/packet/candidate.rs) defines the required criteria, checks, artifact categories, effect reporting and safe boundary. `CandidateResultContract::bind` attaches that template to the exact work, contract and relevant basis.

| Type | Role in this operation |
| --- | --- |
| `CandidateEffectPolicy` | Required absence or allowed reporting states for external effects. |
| `CandidateResultTemplate` | Checked result obligations independent of a particular work binding. |
| `CandidateResultContract` | Template bound to the dispatched work/contract/basis and used to check its candidate. |

`validate_candidate` checks the candidate against that bound shape and identity. It does not turn reported Satisfied criteria into verified facts, make an artifact exist, or accept the result. Preserve unsatisfied criteria and unknown/pending effects for the later verification and acceptance path.

## Resume from durable references and recheck current action {#resume-projection}

`guide r1`

[ResumeView::build](../../../../crates/zap-core/src/execution_views/resume.rs) normalizes the supplied durable references, checks campaign/revision and navigation consistency, and computes the projection identity. It is a projection builder, not a store reader that discovers missing decisions or host state.

| Type | Role in this operation |
| --- | --- |
| `AcceptedBoundary` | Previously recorded acceptance boundary presented for continuation. |
| `AssignmentRef` | Stable work/job/attempt/packet reference. |
| `QueryHandle` | Store/base/revision-bound navigation reference. |
| `AdmissibleAction` | Action category proposed by the derived continuation view. |
| `ResumeViewInput` | Durable references supplied to the resume projection builder. |
| `ResumeView` | Checked continuation projection and its identity. |

Consume its accepted boundary, assignments, pending effects, holds and next-action references through the configured read/runtime paths. Verify the current workspace, service and host observations before acting; a digest or an AdmissibleAction enum value does not create authority or prove that an older action remains admissible.

## Existing focused examples {#integration-examples}

`guide r1`

- [Trust boundary examples](../../../../crates/zap-core/src/trust/tests.rs): service-seal separation, exact data-proposal frames and distinct principals on one harness.
- [Typed transition examples](../../../../crates/zap-core/src/transition/tests.rs): payload decoding, registered action impact and deterministic typed transition/retry behavior.
- [Effect request example](../../../../crates/zap-core/src/effects/tests.rs): ordered effects retaining their own local basis domains.
- [Packet provider examples](../../../../crates/zap-core/src/execution_views/packet/providers.rs): live capture followed by verification through the material and workspace interfaces.
- [Packet-resolution callback](../../../../crates/zap-core/src/execution_views/packet.rs): the provider receives the service context used to seal resolved evidence.
- [Host observation interface](../../../../crates/zap-core/src/agent/host.rs): consuming a configured host's observation for the supplied handle.

These existing sources illustrate concrete contracts using explicit test fixtures. Internal fixture access is not a public constructor, and a source example is not a fresh execution receipt.

## Public acquisition and private implementation boundaries {#internal-boundaries}

`guide r1`

The inspected 313 local public declarations are reachable through the core export facades; no source-level public type was found stranded in a private module. Private constructors do not remove exported opaque types from the API: clients acquire those values through the trusted service or provider callbacks documented above.

The [trust registry](../../../../crates/zap-core/src/trust.rs) and credential bindings stay private. [Decoded payload/cell adapters](../../../../crates/zap-core/src/transition/cell.rs), [registered-record adapters](../../../../crates/zap-core/src/record.rs) and [query adapters](../../../../crates/zap-core/src/query.rs) implement erasure behind the public registries. They are not alternate admission or decoding APIs.

The private logical-event input structs in [commit/logical.rs](../../../../crates/zap-core/src/commit/logical.rs) validate schema-specific decoding; the logical-event input macro emits a private parser type. By contrast, the public name/profile/counter macros emit seven real public types and are included in the 313-type denominator.

## Public type coverage {#remaining-coverage}

`guide r1`

The catalogs map each of the 313 local public types to one use section: 187 foundation/service types and 126 agent/execution-view types. The latter source families contribute:

| Source family | Cataloged public types |
| --- | ---: |
| [Agent capabilities](../../../../crates/zap-core/src/agent/capabilities.rs) | 12 |
| [Agent dispatch, observation and result contracts](../../../../crates/zap-core/src/agent/dispatch.rs) | 20 |
| [Goal projection/application](../../../../crates/zap-core/src/agent/goals.rs) | 5 |
| [AgentHost](../../../../crates/zap-core/src/agent/host.rs) | 1 |
| [Profiles, including three generated name types](../../../../crates/zap-core/src/agent/profiles.rs) | 6 |
| [Revalidation release](../../../../crates/zap-core/src/agent/revalidation.rs) | 5 |
| [Affected-scope, independence and safe-job views](../../../../crates/zap-core/src/execution_views/affected_scope.rs) | 13 |
| [Dispatch eligibility views](../../../../crates/zap-core/src/execution_views/eligibility.rs) | 5 |
| [Execution observations and affected-job views](../../../../crates/zap-core/src/execution_views/job.rs) | 11 |
| [Packet resolution, material providers and result contracts](../../../../crates/zap-core/src/execution_views/packet.rs) | 23 |
| [Resume projections](../../../../crates/zap-core/src/execution_views/resume.rs) | 6 |
| [Work/readiness views, including two generated counters](../../../../crates/zap-core/src/execution_views/work.rs) | 19 |

Two additional generated registry-name types are cataloged with foundation identities. The five wire error reexports retain their owning API. Raw record continuation and query-cursor continuation retain the separate acquisition and interpretation paths described in their read sections.
