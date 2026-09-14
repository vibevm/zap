# Rust application usage guide {#root}

`guide r1`

This non-normative guide describes the [zap_app public exports](../../../../crates/zap-app/src/lib.rs). All 63 locally declared public types and traits are reachable through the crate root and cataloged once below. Public names do not imply that their concrete constructors are public; provider assembly paths are identified where that distinction matters.

The [storage contract](ZAP-RUST-STORAGE.xml), [backend contract](ZAP-BACKEND-API.md) and [CLI guide](ZAP-CLI.md) describe the existing service boundaries. Configured capability, pure preparation, trusted admission, actual external action and returned evidence remain separate. Data read from packets, responses or archives does not grant authority.

Types carried from core, wire, domain, runtime, API and store retain their owning contracts. This crate's root adds no cross-crate type reexports; the imported types visible inside public signatures are dependencies rather than additional local catalog entries.

## Compose registered reads and completion {#read-composition}

`guide r1`

Use [foundation_composition](../../../../crates/zap-app/src/composition.rs) to obtain the application's registered cells, records, queries, routes and providers. `foundation_composition_with_cross_domain` supplies the composition that includes the selected bundle and return providers. The resulting registries describe the configured implementation.

| Type | Role in this operation |
| --- | --- |
| `FoundationComposition` | Registered application operations and the providers used to evaluate them. |
| `ReadApplication` | Configured read-only machine surface for an existing store. |

[ReadApplication::open](../../../../crates/zap-app/src/composition.rs) binds an existing store to the read surface. Consume its machine capabilities, snapshot, events and query results through the read-port contract. `FoundationComposition::completion_evaluator` composes the shared completion providers; a completion read is separate from admitting a closure command.

## Open a service with explicit dependencies and limits {#service-lifecycle}

`guide r1`

[ApplicationServiceConfig::validate](../../../../crates/zap-app/src/runtime_service/config.rs) checks the configured store mode, profiles, trust and execution bounds. [ApplicationService::open_filesystem](../../../../crates/zap-app/src/runtime_service/application.rs) builds the filesystem adapters; `open` accepts explicitly supplied dependencies, and `open_existing_store` retains a matching open store handle.

| Type | Role in this operation |
| --- | --- |
| `ApplicationService` | Composed service for authenticated admission, runtime operations and machine reads. |
| `ApplicationServiceDependencies` | Explicit material, workspace, artifact and native-mailbox dependencies. |
| `ApplicationServiceConfig` | Configured state locations, trust, profiles, capacity and limits. |
| `ApplicationStoreMode` | Selection of an existing store or creation with an explicit identity. |
| `ApplicationLimits` | Bounds for in-flight work, prepared captures and service lifetimes. |

Keep store, endpoint, lease and packet-capture locations bound to the intended application. The `config_dir` argument is the material-adapter path base; other service path values are passed to their respective filesystem operations. Relative paths therefore retain their call-site working-directory meaning.

Use service preparation methods for bundle/comparison/overlay reads, then the appropriate authenticated submission method for mutation. A timeout can leave an Unknown submission; retain its identity and call `reconcile` before deciding whether to repeat it. See the [machine usage guide](ZAP-RUST-MACHINE-GUIDE.md) for returned status and preparation types.

## Bind credentials to distinct authority channels {#trusted-channels}

`guide r1`

The [trust configuration types](../../../../crates/zap-app/src/runtime_service/config.rs) name credential files and the controls/actions or issuer identities attached to each channel. The [configured bootstrap](../../../../crates/zap-app/src/runtime_service/trust.rs) reads those files and establishes the corresponding trusted handles.

| Type | Role in this operation |
| --- | --- |
| `CredentialChannelConfig` | Credential file and authorization reference for a credentialed channel. |
| `OwnerChannelConfig` | Owner credential channel and its configured controls. |
| `CoordinatorChannelConfig` | Coordinator credential channel and its configured actions. |
| `ProtectedIssuerConfig` | Configured issuer identity and credential source. |
| `TrustedObservationConfig` | Observation issuer bound to a harness and observation identity. |
| `ApplicationTrustConfig` | Complete separation of Owner, coordinator, data, observation and internal channels. |

Call the service's credential, agent-data or trusted-observation submission path with the matching configured credential. Request JSON, an actor label, worker role or credential ID alone does not establish authority. Credential bytes remain channel inputs rather than packet, event or archive content.

## Supply runtime reads, commands and eligibility {#runtime-composition}

`guide r2`

[ApplicationCampaignReadPort::new](../../../../crates/zap-app/src/runtime_service/read_port.rs) binds store, packet-resolution and completion providers for runtime consumption. [ApplicationRuntimeCommandFactory::new](../../../../crates/zap-app/src/runtime_service/factory.rs) supplies command construction for that store identity.

| Type | Role in this operation |
| --- | --- |
| `RuntimeCapacityConfig` | Configured resource, host, review and runtime-step limits. |
| `ApplicationCampaignReadPort` | Runtime view over the store, current packet selection and shared completion. |
| `ApplicationRuntimeCommandFactory` | Construction of registered runtime command frames for the application. |
| `ApplicationDispatchEligibilityProvider` | Current-state dispatch eligibility contribution. |

`ApplicationCampaignReadPort::frontier` reads the current Ready-work index in numeric `(order, WorkId)` order and returns a query-identity, store/base/revision and index-continuation-bound `PageCursor` when more work remains. Continue with that cursor; stale, foreign, wrong-query and incompatible-catalog continuations refuse. Before runtime discovery, the application reads the active campaign-pause and nonreleased global-hold indexes; resumed pauses and released holds do not consume a historical prefix, while an active campaign pause or a nonreleased hold covering all starts or an unknown effect refuses the start. The [dispatch eligibility provider](../../../../crates/zap-app/src/runtime_service/eligibility.rs) checks current work, contract and job bindings through the composed admission path. [Runtime step/run and native-driver methods](../../../../crates/zap-app/src/runtime_service/runtime.rs) consume these dependencies and the configured [capacity limits](../../../../crates/zap-app/src/runtime_service/config.rs). A scheduling result, prepared launch and observed host receipt remain distinct.

## Serve authenticated requests and retain endpoint ownership {#http-service}

`guide r1`

[ReadServer::from_config](../../../../crates/zap-app/src/server.rs) constructs the configured read server. `from_application_config` opens the complete application service, while `from_service` uses an existing service and checks that the configured store matches it. The server validates its loopback address and request/response, connection and timeout bounds before binding.

| Type | Role in this operation |
| --- | --- |
| `ReadServerConfig` | Existing-store, loopback, reader-credential and HTTP limit configuration. |
| `ApplicationServerConfig` | Combined protected-service and HTTP server configuration. |
| `ReadServer` | Bound listener and configured authenticated machine service. |
| `ApplicationEndpoint` | Published store/address/controller/service-instance identity. |

Use `local_addr` to obtain the actual bound address and `serve_until` with the cancellation input controlling the server lifetime. The published endpoint records the service instance; it is discovery data, not a credential.

The [backend contract](ZAP-BACKEND-API.md) defines route/request matching and channel requirements. `ApplicationService::recover_service_lease` checks exact lease and endpoint evidence; it does not repair history or decide an unknown external effect.

## Resolve current packets and retain prepared captures {#packet-resolution}

`guide r1`

Use [WorkerProfilePolicy::new](../../../../crates/zap-app/src/packet_resolution.rs) to bind desired profiles to the intended worker roles. `ApplicationPacketResolutionProvider::new` or `new_with_limit` combines those preferences with material and workspace providers. Actual capability resolution remains separate from the desired profile.

| Type | Role in this operation |
| --- | --- |
| `WorkerProfileBinding` | Desired worker profile associated with its configured principal identity. |
| `WorkerProfilePolicy` | Role-specific worker profile preferences. |
| `ApplicationPacketResolutionProvider` | Current packet resolution and captured-material verification. |
| `PreparedPacketCapture` | Command-bound prepared packet evidence and witness identities. |
| `PreparedPacketCaptureStore` | Filesystem retention and exact lookup of prepared captures. |

`prepare_claim_capture` binds captured material to an exact command and request. [PreparedPacketCapture::new and validate](../../../../crates/zap-app/src/packet_capture.rs) check that association; `PreparedPacketCaptureStore::create`, `publish` and `load` retain it across restart. `hydrate_capture` obtains artifact witnesses before the prepared evidence is reused.

Use `current_packet_selection` and `work_execution_view` for the current executable packet. Export verification methods distinguish current material from already captured historical material. A prepared capture is not an admitted runtime claim or proof of an actual host invocation.

## Assemble bounded filesystem material providers {#filesystem-materials}

`guide r1`

[build_filesystem_material_adapters](../../../../crates/zap-app/src/material_adapters/mod.rs) is the public construction path. It resolves configured material roots and artifact/archive locations against its explicit configuration directory, checks bindings and returns the provider bundle. Consume its `packet_materials`, `packet_workspaces`, `artifact_witness`, `bundle_artifacts` and `portable_bundles` getters.

| Type | Role in this operation |
| --- | --- |
| `FilesystemMaterialAdapterConfig` | Root, binding, artifact, archive and optional query configuration. |
| `FilesystemMaterialAdapters` | Constructed provider bundle exposed through public getters. |
| `MaterialRootConfig` | Named filesystem root used by material and workspace bindings. |
| `PacketMaterialSubjectConfig` | Source, rule or fork identity represented by a material binding. |
| `PacketMaterialBindingConfig` | Exact material identity associated with a configured root-relative path. |
| `FilesystemPacketMaterialProvider` | Filesystem-backed implementation of the packet material contract. |
| `ImmutableArtifactRepository` | Bounded immutable artifact and witness implementation used by the adapters. |
| `VibeQueryConfig` | Explicit executable, invocation identity, output bound and timeout for Vibe queries. |

The concrete material/workspace/repository/provider constructors are private to their implementation modules or crate. Use the public assembly function instead of inventing a public `new` for those exported types. [Material binding configuration](../../../../crates/zap-app/src/material_adapters/config.rs) supplies source identity, root and relative path; [path handling](../../../../crates/zap-app/src/material_adapters/paths.rs) checks the configured filesystem boundary.

When `vibe_query` is configured, adapter construction probes the selected executable's query help. Bound lookups use that executable and explicit offline query arguments, with configured output/time limits. This [Vibe query adapter](../../../../crates/zap-app/src/material_adapters/vibe_query.rs) verifies a specification binding; it is not a native model runner.

## Capture the declared workspace without claiming an OS sandbox {#workspace-manifests}

`guide r1`

The [workspace configuration](../../../../crates/zap-app/src/material_adapters/config.rs) describes named root access, planned operations, checks and outputs for the packet's bound workspace. The assembly function supplies the [FilesystemPacketWorkspaceProvider](../../../../crates/zap-app/src/material_adapters/workspace.rs), which captures and verifies the matching manifest.

| Type | Role in this operation |
| --- | --- |
| `WorkspaceRootAccess` | Declared read-only or read/write access category. |
| `WorkspaceRootGrant` | Access declaration for one named workspace root. |
| `WorkspaceOperationKind` | Declared read, create, replace or delete operation category. |
| `WorkspaceOperation` | Root-relative operation represented in the workspace contract. |
| `PacketWorkspaceBindingConfig` | Packet/work/harness binding with its workspace declarations. |
| `PacketWorkspaceManifestV1` | Captured typed workspace manifest and its stated isolation boundary. |
| `FilesystemPacketWorkspaceProvider` | Configured workspace capture and verification implementation. |

The provider validates the captured manifest and rejects a claim that this adapter provides an OS sandbox. The manifest records declared access and operations; it does not itself execute them or supply operating-system isolation.

## Prepare portable bundle closure and admit export {#bundle-closure}

`guide r1`

[BundleArtifactCapture::new](../../../../crates/zap-app/src/cross_domain/bundle.rs) creates a canonical semantic entry for the selected entry kind. The artifact provider captures or verifies that body; the bundle-closure provider combines required material with the packet/assignment meaning.

| Type | Role in this operation |
| --- | --- |
| `BundleArtifactProvider` | Capture, verification and reading contract for portable semantic entries. |
| `BundleArtifactCapture` | Canonical semantic body paired with its entry identity. |
| `PortablePacketBody` | Portable packet and its captured runtime claim. |
| `PortableAssignmentBody` | Portable task contract and runtime job meaning. |
| `BundleClosureProvider` | Preparation and verification contract for required bundle closure. |
| `ApplicationBundleClosureProvider` | Application implementation binding packet, artifact and historical evidence. |
| `ApplicationBundleExportedCell` | Registered export operation using the selected closure verifier. |
| `BundleExportArtifacts` | Artifact-reference extraction attached to bundle export. |

Construct `ApplicationBundleClosureProvider` with packet and artifact providers, attach the intended store once, and use its preparation and captured-evidence paths. Historical export preserves claim-time material rather than silently recapturing changed current files.

[ApplicationBundleExportedCell::new and cross_domain_cell_set](../../../../crates/zap-app/src/cross_domain/cells.rs) install the selected verifier and artifact extraction in the registered export operation. Prepared closure and artifact existence are inputs to admission, not an already accepted exported bundle.

## Resolve returned work before admitting it {#return-import}

`guide r1`

The [ReturnResolutionProvider](../../../../crates/zap-app/src/cross_domain/returns.rs) derives the affected request and resolves returned data against the current state. `ApplicationReturnResolutionProvider` produces the encounters, failed-approach information and import result needed by the application path.

| Type | Role in this operation |
| --- | --- |
| `ReturnResolutionProvider` | Affected-scope and resolution contract for returned bundle data. |
| `ApplicationReturnResolutionProvider` | Application implementation of return resolution. |
| `ResolvedReturnImport` | Resolved encounter, failure-history and import changes prepared for admission. |
| `ApplicationReturnImportedCell` | Registered import operation using the selected resolver. |
| `ApplicationReturnAffectedScope` | Affected-scope request adapter for the import operation. |
| `ReturnImportArtifacts` | Artifact-reference extraction attached to returned bundle import. |

[ApplicationReturnImportedCell::new](../../../../crates/zap-app/src/cross_domain/cells.rs) and the corresponding affected-scope/artifact adapters attach the selected resolver to the registered transition. Resolution is preparation; only admitted application changes persist its result. Returned candidates and findings retain their provenance instead of becoming accepted work merely by import.

## Publish and consume verified portable archives {#portable-archives}

`guide r1`

Obtain [PortableBundleArtifactProvider](../../../../crates/zap-app/src/material_adapters/bundle.rs) through the filesystem adapter bundle. `publish_archive` produces a publication result from the bound manifest; `verify_archive` or `verify_published_archive` reopens and verifies archive content.

| Type | Role in this operation |
| --- | --- |
| `PortableArchiveLimits` | Configured entry, manifest and total archive bounds. |
| `PortableBundleArtifactProvider` | Portable archive capture, publication and verification implementation. |
| `PublishedPortableBundle` | Publication receipt and its destination path. |
| `VerifiedPortableBundle` | Verified manifest, entries and archive identity. |
| `PortableBundleEntry` | Entry binding paired with its decoded body. |
| `PortableBundleEntryBody` | Typed material or semantic entry content. |

Use `VerifiedPortableBundle::entry` to select a typed entry. Material bytes, packet/assignment bodies, capability evidence, permission bindings and stop-rule data retain distinct meanings. Reading a permission or assignment entry does not grant its authority.

Configured [PortableArchiveLimits](../../../../crates/zap-app/src/material_adapters/config.rs) bound entries and their byte representations. The [application archive methods](../../../../crates/zap-app/src/runtime_service/archive.rs) expose publication, verification and bounded entry reads on the appropriate service paths.

## Import a bound legacy source without activating it {#legacy-application-import}

`guide r1`

[import_legacy](../../../../crates/zap-app/src/legacy_import/service.rs) consumes exact source/destination configuration and expected source hashes, then prepares a recoverable Rust destination. Consume the returned receipt's actual import and pointer-switch state.

| Type | Role in this operation |
| --- | --- |
| `LegacyImportConfig` | Exact source, destination, operation identities and expected source hashes. |
| `LegacyImportReceipt` | Observed import identity, counts, outcome and activation/switch state. |

This application projection path currently requires legacy revision zero; a nonzero legacy projection is refused before staging. The broader legacy reader and codec remain separate operations described by the [legacy usage guide](ZAP-RUST-LEGACY-GUIDE.md). Import does not activate the source charter or dispatch its tasks.

## Existing integration examples {#integration-examples}

`guide r1`

- [Application server](../../../../crates/zap-app/tests/application_server.rs): configured channels, canonical protected commands, exact service ownership and the included admission/completion scenarios.
- [Read server](../../../../crates/zap-app/tests/read_server.rs): authenticated loopback reads, snapshot/query/tail handling, resynchronization and cancellation.
- [Material adapters](../../../../crates/zap-app/tests/material_adapters.rs): overlapping packet material, retained historical capture and refusal of a path outside the configured root. The native Vibe query case is conditional on its configured fixture environment.
- [Packet-resolution service](../../../../crates/zap-app/tests/packet_resolution_service.rs): a real store composition tying packet capture, runtime claim, portable bundle and return processing together.
- [Deterministic completion](../../../../crates/zap-app/tests/deterministic_campaign_completion.rs): the explicit two-job algorithmic driver through the nonempty campaign acceptance/closure journey.
- [Legacy import](../../../../crates/zap-app/tests/legacy_import.rs): inactive projection, exact retry, source-preserving recovery and refusal of unrelated staging.

These sources are executable usage references with explicit fixtures, not fresh execution receipts or claims of actual model capability. Tests that return early without their optional environment input do not establish the omitted integration.

## Internal boundaries and constructor access {#internal-boundaries}

`guide r1`

The 63 cataloged declarations are publicly reexported. Their private support types remain implementation details: `HydratedPacketCapture` retains an artifact witness in [packet_resolution.rs](../../../../crates/zap-app/src/packet_resolution.rs), and `PreparedBundleEvidence` retains prepared export evidence in [cross_domain/bundle.rs](../../../../crates/zap-app/src/cross_domain/bundle.rs).

`VibeQueryAdapter` and `BoundedOutput` stay inside the material adapters. The public `VibeQueryConfig` and assembly function select that process boundary. The concrete filesystem provider/repository constructors are likewise private even though their types are exported; consume the configured provider getters.

`ServerState` in [server.rs](../../../../crates/zap-app/src/server.rs), the credential-bearing `ChannelSecret`, `AuthorityHandles` and `ConfiguredBootstrap` in [trust.rs](../../../../crates/zap-app/src/runtime_service/trust.rs), and lease/recovery records in [lease.rs](../../../../crates/zap-app/src/runtime_service/lease.rs) are not public construction APIs. Use the service and server configuration paths to establish their actual state.
