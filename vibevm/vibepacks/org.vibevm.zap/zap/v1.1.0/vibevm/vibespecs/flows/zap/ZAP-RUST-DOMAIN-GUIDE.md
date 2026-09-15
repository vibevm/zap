# Rust domain usage guide {#root}

`guide r3`

This non-normative guide describes the [zap_domain public modules](../../../../crates/zap-domain/src/lib.rs). Its original catalogs cover the 454 externally reachable local types of the 1.0 baseline across charter/intent/outcome, work and obligations, knowledge and proof, adaptive review, economics, acceptance, Owner control, strategic lowering, detached returns, Dreamer and viewer operations. The 1.1 map-work assessment additions appear in their section below; strategic projection types are covered by the [strategic-map guide](ZAP-STRATEGIC-MAP-GUIDE.md).

Paths in the catalogs are relative to `zap_domain`, so `intent::CharterRecord` means the public module path rather than a private implementation module. The original inventory distinguishes 437 literal declarations and 67 macro emissions: 454 unique public types and 50 private-module implementation declarations. These are baseline counts, not the total after the 1.1 additions. Offline lowering types also have public `lowering::offline` aliases; each unique type is cataloged once at its shorter public path.

Pure helper functions validate or derive values; they do not persist changes or confer authority. Use the configured core/application service and actual domain registrations for admission and commit. A constructed payload, proposed record, returned candidate and accepted result remain separate.

Generated schema enums select their declared wire tag. Their Rust variant is named `V1` even when a particular declared wire tag ends in `/2`; use the exact payload's enum and serialized contract rather than inferring a protocol version from the variant name.

The [methodology](ZAP-METHODOLOGY.xml), [adaptive cycle](ZAP-ADAPTIVE-CYCLE.xml), [data contract](ZAP-DATA-AND-VIEWER.xml), [economics](ZAP-CHANGE-ECONOMICS.xml), [lowering/Dreamer](ZAP-LOWERING-AND-DREAMER.xml), [agent protocol](ZAP-AGENT-PROTOCOL.xml), [runtime](ZAP-RUNTIME.xml) and [storage](ZAP-RUST-STORAGE.xml) contracts remain authoritative.

## Read domain state without conflating its dimensions {#domain-state}

`guide r1`

The [shared domain model](../../../../crates/zap-domain/src/seams/model.rs) separates hierarchy, work purpose, state and maturity. The [WorkRecord](../../../../crates/zap-domain/src/control/records.rs) is read from the registered store; assembling an equivalent record does not create work or advance its state.

| Public type | Role in this operation |
| --- | --- |
| `seams::LifecycleStatus` | Proposed, active or superseded lifecycle state. |
| `seams::WorkKind` | Position/category of work in the hierarchy. |
| `seams::WorkType` | Purpose of work such as evidence, decision or change. |
| `seams::WorkState` | Current domain work state. |
| `seams::MaturityStage` | Domain prototype, functional or productized maturity. |
| `seams::DeliveryRoute` | Direct or staged maturity delivery route. |
| `seams::DomainMutation` | Revision result returned by registered domain changes. |
| `control::WorkRecord` | Stored work identity, relationships, contract state and generation. |

`seams::DeliveryRoute::is_valid` checks Direct or strictly ordered staged delivery. This is a maturity route, distinct from the transport `zap_core::DeliveryRoute`. Likewise domain Prototype/Functional/Productized stages differ from the core execution-stage vocabulary. A `DomainMutation` result records the resulting revision, not campaign acceptance.

## Draft and activate an exact charter {#charter-authority}

`guide r1`

[draft_charter, activate_charter and amend_charter](../../../../crates/zap-domain/src/intent/transitions.rs) validate pure transformations of [charter records and payloads](../../../../crates/zap-domain/src/intent/records.rs). Submit the corresponding payload through the configured registered service to persist the transformation and enforce its Owner authority.

| Public type | Role in this operation |
| --- | --- |
| `intent::CharterRecord` | Stored charter scope and its revision/digest binding. |
| `intent::CharterDrafted` | Proposed charter input to the registered drafting operation. |
| `intent::CharterActivated` | Exact draft identity selected for activation. |
| `intent::CharterAmended` | Proposed replacement charter with its predecessor binding. |
| `intent::CharterDraftedSchema` | Wire schema tag for the charter draft payload. |
| `intent::CharterActivatedSchema` | Wire schema tag for charter activation. |
| `intent::CharterAmendedSchema` | Wire schema tag for charter amendment. |
| `seams::CharterBinding` | Charter/intent identity carried by an adopted record. |
| `seams::CompletionDutyPolicy` | Charter policy for final-gate and promotion obligations. |
| `seams::CharterDutyAuthority` | Required duty or charter permission for a justified NoDuty disposition. |
| `seams::CharterRecordView` | Borrowed charter facts used for duty authorization checks. |
| `seams::CompletionDutyDisposition` | Required duty or exact charter-bound NoDuty claim. |

Completion-duty dispositions bind to the exact charter policy and revision. `matches_items` checks the represented duty set and `authorized_by` checks its charter view; a copied charter ID or NoDuty label does not supply Owner authorization. The [schema enums](../../../../crates/zap-domain/src/intent/payloads.rs) select the exact declared wire tag.

## Preserve intent proposals separately from adoption {#intent-adoption}

`guide r1`

Use [propose_intent and adopt_intent](../../../../crates/zap-domain/src/intent/transitions.rs) with the exact predecessor and active charter context. The helper returns proposed/adopted record values; the registered service supplies current-state and authority checks before persistence.

| Public type | Role in this operation |
| --- | --- |
| `intent::IntentRecord` | Stored intent revision and its adopted charter binding. |
| `intent::IntentProposed` | Proposed intent content and predecessor identity. |
| `intent::IntentAdopted` | Input selecting the intent for adoption. |
| `intent::IntentProposedSchema` | Wire schema tag for intent proposal. |
| `intent::IntentAdoptedSchema` | Wire schema tag for intent adoption. |

An intent revision retains its predecessor, source references and Owner binding. A proposal does not become active solely because its summary resembles the charter's intent.

## Adopt an outcome while conserving obligations {#outcome-adoption}

`guide r1`

[propose_outcome and adopt_outcome](../../../../crates/zap-domain/src/intent/transitions.rs) preserve the link to intent and the prior outcome. Outcome adoption accounts for the existing active obligations and applies the charter's allowed dispositions.

| Public type | Role in this operation |
| --- | --- |
| `intent::OutcomeRecord` | Stored outcome revision, guarantees and obligation dispositions. |
| `intent::ProposedObligation` | Obligation proposed as part of an outcome. |
| `intent::OutcomeProposed` | Outcome proposal and its required duties. |
| `intent::OutcomeAdopted` | Exact adoption request and prior-obligation dispositions. |
| `intent::OutcomeAdoption` | Complete outcome/obligation changes derived by adoption. |
| `intent::OutcomeProposedSchema` | Wire schema tag for outcome proposal. |
| `intent::OutcomeAdoptedSchema` | Wire schema tag for outcome adoption. |

Consume `OutcomeAdoption` as the complete pure result: predecessor/current outcomes plus retained or created obligations. Persisting only the new outcome would lose the conservation changes. Benefits, guarantees, final-gate and promotion duties remain part of the selected outcome's contract.

## Retain ownership and coverage when refining work {#obligation-coverage}

`guide r1`

The [obligation model](../../../../crates/zap-domain/src/seams/model.rs) distinguishes status, disposition and the role of each owner. Retained, replaced, excluded and unattainable obligations require their explicit record-level disposition; owner roles do not themselves confer acceptance authority.

| Public type | Role in this operation |
| --- | --- |
| `control::ObligationRecord` | Stored obligation and its outcome/ownership history. |
| `seams::ObligationDisposition` | Meaning of an explicit obligation disposition. |
| `seams::ObligationStatus` | Stored active/replaced/excluded/unattainable state. |
| `seams::OwnershipRole` | Implementation, verification, integration or acceptance responsibility. |
| `seams::ObligationOwner` | Work identity paired with its responsibility. |
| `seams::ObligationDispositionRow` | One obligation's justified disposition and successors. |
| `seams::ObligationAssignment` | Explicit work-role assignments for an obligation. |
| `control::PlanLowered` | Control-layer graph/contract/coverage refinement payload. |
| `control::PlanLoweredSchema` | Exact wire schema for that control-layer lowering payload. |

[validate_lowered_graph](../../../../crates/zap-domain/src/control/validation.rs) and [lower_plan](../../../../crates/zap-domain/src/control/transitions.rs) check the proposed graph and parent coverage with the supplied records. `PlanLowered` is the control-layer payload family; the richer strategic lowering lifecycle is a separate domain family. Use the configured operation registry rather than inferring compatibility from a similarly named payload.

## Validate and replace the executable task contract {#task-contract}

`guide r1`

[validate_task_contract](../../../../crates/zap-domain/src/control/validation.rs) checks the shared [TaskContract](../../../../crates/zap-domain/src/seams/model.rs). `replace_task_contract` in [control transitions](../../../../crates/zap-domain/src/control/transitions.rs) binds the current active version and exact obligation coverage before producing old/new records.

| Public type | Role in this operation |
| --- | --- |
| `seams::TaskContract` | Executable work contract and its obligations. |
| `seams::VerificationMethod` | Declared check invocation and its material context. |
| `control::TaskContractRecord` | Stored contract version and active-state identity. |
| `control::TaskContractReplaced` | Exact contract replacement input. |
| `control::ContractReplacedSchema` | Wire schema tag for contract replacement. |

The contract retains read/write subjects, checks, acceptance, safe stop, sources and maturity duties. A verification method describes intended execution; it is not a verification receipt.

## Apply the dedicated work transition path {#work-transitions}

`guide r1`

[rename_work, transition_work and dispatch_work](../../../../crates/zap-domain/src/control/transitions.rs) validate their distinct input/state bindings. The generic transition helper does not replace the dedicated dispatch or acceptance path.

| Public type | Role in this operation |
| --- | --- |
| `control::WorkRenamed` | Exact title-change payload. |
| `control::WorkRenamedSchema` | Wire schema tag for work rename. |
| `control::WorkTransitioned` | Explicit current/next domain state payload. |
| `control::WorkTransitionedSchema` | Wire schema tag for generic work transition. |
| `control::WorkDispatched` | Dedicated work dispatch transition payload. |
| `control::WorkDispatchedSchema` | Wire schema tag for work dispatch. |
| `control::ReadinessBlocker` | Domain-only readiness refusal reason. |

[readiness_blockers](../../../../crates/zap-domain/src/control/validation.rs) reports this domain layer's outcome, obligation, contract, dependency and deferral blockers. The composed runtime/admission path also enforces its own stop, economics, capability and resource checks; a clear domain-only list is not a launch permit.

## Consume applied-review evidence before readying work {#revalidation-ready}

`guide r1`

[ready_revalidation](../../../../crates/zap-domain/src/control/transitions.rs) requires an `AppliedRevalidationWitness` in addition to the exact work/generation payload. That witness has a crate-private constructor in [seams/proof.rs](../../../../crates/zap-domain/src/seams/proof.rs); callers do not manufacture it from a numeric generation.

| Public type | Role in this operation |
| --- | --- |
| `control::WorkRevalidationReadied` | Exact work/state/generation request for revalidation readiness. |
| `control::RevalidationReadiedSchema` | Wire schema tag for revalidation readiness. |
| `seams::AppliedRevalidationWitness` | Internally acquired evidence of the applied review's safe release. |

Use the registered revalidation operation so the applied review and release evidence are derived from current state. Readying work for another validation generation is separate from declaring the old result applicable or accepted.

## Transfer or close explicit deferrals {#deferral-lifecycle}

`guide r1`

[create_deferral, transfer_deferral, close_deferral and mark_deferral_inapplicable](../../../../crates/zap-domain/src/control/transitions.rs) preserve responsible work and closure duties. They consume exact current records and the relevant proof/obligation context.

| Public type | Role in this operation |
| --- | --- |
| `control::DeferralRecord` | Stored deferred duty, responsibility and closure evidence. |
| `seams::DeferralStatus` | Open, closed or inapplicable deferral state. |
| `control::DeferralCreated` | New explicit deferral payload. |
| `control::DeferralCreatedSchema` | Wire schema tag for deferral creation. |
| `control::DeferralTransferred` | Responsibility-transfer payload. |
| `control::DeferralTransferredSchema` | Wire schema tag for deferral transfer. |
| `control::DeferralClosed` | Evidence-bound closure payload. |
| `control::DeferralClosedSchema` | Wire schema tag for deferral closure. |
| `control::DeferralInapplicable` | Checked inapplicability payload. |
| `control::DeferralInapplicableSchema` | Wire schema tag for deferral inapplicability. |

A transfer keeps the debt; closure requires its selected proof; inapplicability is a checked disposition rather than deletion. Use the corresponding [payload schema](../../../../crates/zap-domain/src/control/payloads.rs) and registered service route.

## Keep observation, applicability and adjudication distinct {#evidence-adjudication}

`guide r1`

The [evidence model](../../../../crates/zap-domain/src/seams/model.rs) distinguishes observed pass/fail from accepted/rejected/inapplicable disposition. Preserve the [source and proof-applicability identity](../../../../crates/zap-domain/src/seams/proof.rs) alongside the claim.

| Public type | Role in this operation |
| --- | --- |
| `seams::EvidenceApplicability` | Outcome/work/obligation/stage scope claimed for evidence. |
| `seams::EvidenceDisposition` | Accepted, rejected or inapplicable adjudication category. |
| `seams::EvidenceResult` | Observed pass or fail result. |
| `seams::EvidenceObservation` | Observed artifact result and its work/source references. |
| `seams::SourceCapture` | Source identity retained for proof applicability. |
| `seams::ProofApplicability` | Current, stale or unknown proof applicability. |
| `acceptance::EvidenceAdjudicationRecord` | Stored adjudication bound to candidate, source and work generation. |
| `acceptance::WorkGeneration` | Work identity paired with the evaluated generation. |
| `acceptance::CandidateReviewRecord` | Candidate/version boundary opened for review. |
| `acceptance::ProofClaim` | Borrowed scope and pass requirement for collective proof checking. |
| `acceptance::EvidenceAdjudicated` | Evidence adjudication payload. |
| `acceptance::EvidenceAdjudicatedSchema` | Exact wire tag for evidence adjudication. |
| `acceptance::EvidenceAdjudicationBasisScope` | Registered basis-request adapter for adjudication. |

[adjudicate_evidence](../../../../crates/zap-domain/src/acceptance/transitions.rs) and [validate_collective_proof](../../../../crates/zap-domain/src/acceptance/validation.rs) consume actual producer/acceptor context and current scoped proof. A `ProofClaim`, observed pass or list of evidence IDs is not already an accepted witness.

The registered [evidence basis hook](../../../../crates/zap-domain/src/acceptance/cells.rs) binds adjudication to its relevant current state. `CandidateReviewRecord` and work generations preserve which runtime candidate is under review.

## Accept an exact maturity-stage result {#stage-acceptance}

`guide r1`

[accept_stage](../../../../crates/zap-domain/src/acceptance/transitions.rs) checks the stage claim against owned obligations, current proof and producer/acceptor separation. The registered basis adapter supplies its relevant-state request.

| Public type | Role in this operation |
| --- | --- |
| `seams::StageClaim` | Work/stage/outcome scope submitted for acceptance. |
| `acceptance::StageAccepted` | Stage-acceptance payload. |
| `acceptance::StageAcceptedSchema` | Wire schema tag for stage acceptance. |
| `acceptance::StageAcceptanceRecord` | Stored accepted stage and proof binding. |
| `acceptance::StageAcceptanceBasisScope` | Relevant-basis adapter attached to the registered stage operation. |

The stage record applies to its work generation and outcome. It does not erase remaining productization duties or automatically accept the whole work.

## Accept the exact integrated child set {#integration-acceptance}

`guide r1`

[accept_integration](../../../../crates/zap-domain/src/acceptance/transitions.rs) consumes the exact child set, current child acceptance and scoped evidence. The registered integration basis adapter is part of that service path.

| Public type | Role in this operation |
| --- | --- |
| `acceptance::IntegrationAccepted` | Exact child-set integration acceptance payload. |
| `acceptance::IntegrationAcceptedSchema` | Wire schema tag for integration acceptance. |
| `acceptance::IntegrationAcceptanceRecord` | Stored integration result and its evidence. |
| `acceptance::IntegrationAcceptanceBasisScope` | Relevant-basis adapter for the integration operation. |

Preserve current and legacy child identities as represented by the record. A list of completed child tasks does not substitute for this integration acceptance.

## Accept work with current context and independent authority {#work-acceptance}

`guide r1`

[accept_work](../../../../crates/zap-domain/src/acceptance/transitions.rs) consumes `WorkAcceptanceContext`: the current work, contract, stage, integrations, proof and producer/acceptor facts. Its pure result includes both the acceptance record and changed work record.

| Public type | Role in this operation |
| --- | --- |
| `acceptance::WorkAccepted` | Work-acceptance payload and claimed obligation coverage. |
| `acceptance::WorkAcceptedSchema` | Wire schema tag for work acceptance. |
| `acceptance::WorkAcceptanceContext` | Borrowed current facts required by the acceptance helper. |
| `acceptance::WorkAcceptanceRecord` | Stored work acceptance and its stage/integration/proof references. |
| `acceptance::WorkAcceptanceBasisScope` | Relevant-basis adapter for the registered work-acceptance path. |

The registered operation rederives that context and enforces its authority before persistence. The producer's exact operation cannot accept its own result. Constructing context data or calling a pure helper does not itself write accepted state.

## Promote required facts and record the actual completion outcome {#promotion-closure}

`guide r1`

[record_promotion and close_campaign](../../../../crates/zap-domain/src/acceptance/transitions.rs) require their actual fact/evidence and shared-completion context. A promotion record refers to the permanent target and adapter receipt; writing the DTO does not publish those target bytes.

| Public type | Role in this operation |
| --- | --- |
| `seams::RequiredPromotion` | Fact/subject promotion duty retained by the outcome. |
| `seams::ClosureClassification` | Original, revised, partial or unreachable completion classification. |
| `seams::ClosureObligationResultKind` | Disposition of one obligation in the closure outcome. |
| `seams::ClosureObligationResult` | Obligation result with unmet portion, successors and evidence. |
| `seams::AcceptedProofSet` | Acceptance/evidence/deferral/promotion references used by closure. |
| `seams::KnowledgeBoundary` | Declared completeness and explicit unknown subjects. |
| `acceptance::FactPromotionRecorded` | Permanent fact-promotion receipt payload. |
| `acceptance::PromotionRecordedSchema` | Wire schema tag for promotion recording. |
| `acceptance::PromotionRecord` | Stored permanent-target promotion evidence. |
| `acceptance::CampaignClosed` | Actual completion outcome submitted for recording. |
| `acceptance::CampaignClosedSchema` | Wire schema tag for campaign closure. |
| `acceptance::ClosureRecord` | Stored completion outcome and remaining/disposed obligations. |
| `acceptance::CampaignClosedCell` | Registered transition checking and recording closure. |

`CampaignClosedCell` is publicly exposed, but the normal domain [cell_set](../../../../crates/zap-domain/src/registration.rs) composes it with the selected impact/closure path. Closure preserves obligation dispositions and whether the result is original, revised, partial or unreachable.

Consume `AcceptedProofSet` and `KnowledgeBoundary` as supplied evidence representations. `KnowledgeBoundary::is_truthful` checks the represented complete/unknown relationship; it does not discover missing knowledge or turn receipt IDs into proof.

## Apply scoped pauses and exact resume decisions {#pause-resume}

`guide r1`

The [pause records](../../../../crates/zap-domain/src/owner_control/records.rs) retain campaign/work/subject scope and the source of the pause. Submit [CampaignPaused or PauseResumed](../../../../crates/zap-domain/src/owner_control/cells.rs) through the configured control route with the exact current binding.

| Public type | Role in this operation |
| --- | --- |
| `owner_control::PauseScope` | Campaign, selected work or selected subject pause scope. |
| `owner_control::PauseSource` | Owner, stop-rule or change-hold origin. |
| `owner_control::PauseStatus` | Active or resumed pause state. |
| `owner_control::PauseRecord` | Stored scoped pause and exact state identity. |
| `owner_control::CampaignPaused` | Payload requesting the scoped pause operation. |
| `owner_control::PauseResumed` | Exact pause/state resume payload. |

Resuming requires the recorded pause/state digest; a missing heartbeat, resumed agent context or changed plan does not remove an active pause. Distinguish Owner, stop-rule and change-hold sources when interpreting the record.

## Evaluate explicit stop rules with observed facts {#stop-rule-evaluation}

`guide r1`

[validate_stop_rule and evaluate_stop_rule](../../../../crates/zap-domain/src/owner_control/rules.rs) interpret the supported expression over supplied `StopFacts`. Missing observations produce NeedsEvidence where required; they are not silently false.

| Public type | Role in this operation |
| --- | --- |
| `owner_control::StopRuleExpression` | Supported typed stop condition. |
| `owner_control::StopRuleRecord` | Stored condition and its active revision. |
| `owner_control::StopFacts` | Observed inputs supplied to rule evaluation. |
| `owner_control::RuleResult` | Clear, triggered or needs-evidence evaluation result. |
| `owner_control::StopRuleRecorded` | Rule-recording payload. |
| `owner_control::StopRuleTriggered` | Exact rule revision, facts and pause submitted for recording. |

The [record and trigger payloads](../../../../crates/zap-domain/src/owner_control/cells.rs) carry rule identity, revision and the pause to record. Evaluating an expression does not itself pause the store; the registered operation verifies and persists that result.

## Preserve one-use exceptions and approach history {#exceptions-approaches}

`guide r1`

The [exception and approach records](../../../../crates/zap-domain/src/owner_control/records.rs) bind a permitted exception to an exact command and keep the problem's approach epoch/failure count. The [grant and epoch-advance operations](../../../../crates/zap-domain/src/owner_control/cells.rs) require their configured Owner route.

| Public type | Role in this operation |
| --- | --- |
| `owner_control::ActionExceptionRecord` | Exact command-bound exception and consumption state. |
| `owner_control::ActionExceptionGranted` | Owner exception-grant payload. |
| `owner_control::ApproachEpochRecord` | Persisted problem epoch and failed-approach history. |
| `owner_control::ApproachEpochAdvanced` | Owner-authorized approach-epoch change payload. |

An exception is not a general resume; consumption and current pause matching remain part of admission. Process, model or account replacement does not justify resetting approach history.

## Record exact economic decisions and retain control blockers {#owner-economic-decisions}

`guide r1`

The [Owner decision record](../../../../crates/zap-domain/src/owner_control/records.rs) binds assessment, forecast, selected alternative and effect fingerprints. [ChangeDecisionRecorded and ChangePolicyActivated](../../../../crates/zap-domain/src/owner_control/cells.rs) persist the chosen decision/policy through their configured control routes.

| Public type | Role in this operation |
| --- | --- |
| `owner_control::OwnerChangeChoice` | Approve, reject, revise or defer decision category. |
| `owner_control::OwnerChangeDecisionRecord` | Exact assessment/forecast/alternative/effect decision binding. |
| `owner_control::ChangeDecisionRecorded` | Payload recording that Owner decision. |
| `owner_control::ChangePolicyActivated` | Exact policy revision/digest activation payload. |
| `owner_control::ControlCompletionProvider` | Registered control contribution to completion blockers. |

[ControlCompletionProvider::new and control_blockers](../../../../crates/zap-domain/src/owner_control/completion.rs) contribute active control blockers to the shared completion evaluation. A positive economic decision does not remove unrelated Owner stop conditions or bypass later effect admission.

## Read preserved legacy constraints without activating them {#legacy-projection}

`guide r1`

The [legacy projection records](../../../../crates/zap-domain/src/legacy_projection/model.rs) retain original mandates, node metadata and task constraints alongside current typed work. `LegacyProjectionBundle::counts` counts the represented collection; it does not validate an unrelated source or activate its authority.

| Public type | Role in this operation |
| --- | --- |
| `legacy_projection::LegacyMandateRecord` | Preserved legacy mandate and its source representation. |
| `legacy_projection::LegacyNodeMetadataRecord` | Historical node identities and metadata retained beside typed work. |
| `legacy_projection::LegacyTaskConstraintRecord` | Preserved task source and execution constraints. |
| `legacy_projection::LegacyProjectionBundle` | Typed work/contract/obligation and legacy-record projection set. |
| `legacy_projection::LegacyProjectionCounts` | Counts of the represented projection collection. |
| `legacy_projection::LegacyProjectionLookup` | Typed selection for the legacy projection query. |
| `legacy_projection::LegacyProjectionQueryInput` | Query input containing that selection. |
| `legacy_projection::LegacyProjectionQueryResult` | Typed projected record or count result. |

The [legacy projection query](../../../../crates/zap-domain/src/legacy_projection/query.rs) is obtained through the public domain query registry. Select a typed lookup and consume the corresponding result. The concrete query implementation is private; import configuration and preservation belong to the application/legacy paths.

## Consume typed viewer data rather than presentation-derived state {#viewer-results}

`guide r1`

The [viewer model](../../../../crates/zap-domain/src/viewer_queries/model.rs) provides typed entity identity/detail and bounded operation input/output. Use the registered query matching the selected operation and inspect the result's actual operation, scope and revision.

| Public type | Role in this operation |
| --- | --- |
| `viewer_queries::ViewerNodeId` | Typed identity of a viewable domain entity. |
| `viewer_queries::ViewerDetail` | Concrete record detail for the selected entity kind. |
| `viewer_queries::ViewerNode` | Summarized node and its visible relations. |
| `viewer_queries::ViewerOperation` | Requested domain viewer operation. |
| `viewer_queries::ViewerInput` | Focus, filter, revision and continuation query input. |
| `viewer_queries::ViewerResult` | Bounded nodes/changes and operation metadata returned to the client. |

Viewer data describes records and relations for a client; it does not prescribe a canvas, infer acceptance from color or perform semantic reassessment when opened.

## Preserve exact viewer continuation state {#viewer-continuation}

`guide r1`

The [viewer continuation types](../../../../crates/zap-domain/src/viewer_queries/model.rs) retain request identity, index positions and traversal expansion state. Reuse the returned cursor with the corresponding operation instead of constructing a continuation from the displayed node count.

| Public type | Role in this operation |
| --- | --- |
| `viewer_queries::ViewerCursor` | Operation/revision-bound viewer continuation. |
| `viewer_queries::ViewerIndexContinuation` | Positions within the relevant index scans. |
| `viewer_queries::ViewerTraversalContinuation` | Frontier, visited and unfinished traversal state. |
| `viewer_queries::ViewerExpansionContinuation` | Per-node expansion position retained across continuation. |

Empty filtered results can still carry continuation. Preserve stale/unknown/completeness distinctions; a partially visited region is not a fully explored empty graph.

## Interpret record history with its before and after values {#viewer-history}

`guide r1`

[ViewerHistoricalValue and ViewerHistoryEntry](../../../../crates/zap-domain/src/viewer_queries/model.rs) retain change identity, provenance and available record detail or stored summary. Consume both ends of the change and the selected history interval.

| Public type | Role in this operation |
| --- | --- |
| `viewer_queries::ViewerHistoricalValue` | Available historical detail or explicitly summarized stored value. |
| `viewer_queries::ViewerHistoryEntry` | Record change and its historical before/after provenance. |

A summary digest is not full record content. A history result describes the past and does not make old evidence applicable to the current work.

## Advance a bounded affected traversal {#viewer-traversal}

`guide r1`

[advance_affected_traversal](../../../../crates/zap-domain/src/viewer_queries/traversal.rs) consumes the store-supplied `DerivedTraversalState` with explicit node/edge budgets. Use `encode_affected_step` and `decode_affected_step` for the returned step's canonical representation.

| Public type | Role in this operation |
| --- | --- |
| `viewer_queries::AffectedTraversalQuota` | Required state quota and repair information. |
| `viewer_queries::AffectedTraversalStep` | Bounded traversal result, observed work and progress. |

Consume reported progress, completeness and quota requirements separately. A quota request is not completion, and observed row counters are not a universal performance guarantee.

## Select the concrete registered viewer operation {#viewer-query-adapters}

`guide r1`

The [public viewer query adapters](../../../../crates/zap-domain/src/viewer_queries/mod.rs) implement `QuerySpec` with shared ViewerInput/ViewerResult contracts. They are emitted by the viewer-query macro and are real public types, not names inferred from documentation.

| Public type | Role in this operation |
| --- | --- |
| `viewer_queries::NodeQuery` | Registered exact-node viewer operation. |
| `viewer_queries::DetailQuery` | Registered typed-detail viewer operation. |
| `viewer_queries::SearchQuery` | Registered viewer search operation. |
| `viewer_queries::AncestorsQuery` | Registered ancestor lookup operation. |
| `viewer_queries::ChildrenQuery` | Registered child lookup operation. |
| `viewer_queries::DependentsQuery` | Registered dependent lookup operation. |
| `viewer_queries::FrontierQuery` | Registered frontier viewer operation. |
| `viewer_queries::WhyBlockedQuery` | Registered blocker-explanation operation. |
| `viewer_queries::AffectedQuery` | Registered affected-subgraph operation. |
| `viewer_queries::DiffQuery` | Registered revision-difference operation. |
| `viewer_queries::HistoryQuery` | Registered record-history viewer operation. |

Use the public domain `query_set` for the configured collection, or register the selected adapter through the core query registry. The operation-specific adapter selects behavior; constructing its unit value does not execute a query or establish a dataset.

## Record exact source captures and later recaptures {#source-capture}

`guide r1`

[record_source and recapture_source](../../../../crates/zap-domain/src/knowledge/transitions.rs) validate supplied capture facts and derive source records. File access belongs to the configured source adapter; these helpers do not read a locator or prove arbitrary supplied bytes.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::SourceKind` | Ordinary file or native VibeVM specification source category. |
| `knowledge::SourceScope` | Unassessed, project or explicit-subject source scope. |
| `knowledge::SourceCaptureStatus` | Current, changed or unavailable source observation state. |
| `knowledge::SourceVersion` | Observed content identity and size for one source version. |
| `knowledge::SourceRecord` | Stored source identity, capture history and current state. |
| `knowledge::SourceCaptureInput` | Capture facts supplied to source recording. |
| `knowledge::SourceRecorded` | Initial source recording payload. |
| `knowledge::SourceRecordedSchema` | Exact source-recording wire tag. |
| `knowledge::SourceRecaptured` | Recapture input bound to the prior source digest. |
| `knowledge::SourceRecapturedSchema` | Exact source-recapture wire tag. |

Submit the [capture payloads](../../../../crates/zap-domain/src/knowledge/payloads.rs) through the registered operation. Retain earlier versions and distinguish Current, Changed and Unavailable; an unassessed source scope is not a project-wide applicability grant.

## Separate proposed source observations from trusted recording {#source-observations}

`guide r1`

[validate_observation, record_observation_candidate and observe_source](../../../../crates/zap-domain/src/knowledge/transitions.rs) distinguish the proposed observation from updating the tracked source. Unavailable observations retain their explicit detail without pretending a digest was observed.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::SourceObservation` | Observed source status with its available content evidence. |
| `knowledge::SourceObservationProposed` | Candidate observation and supporting claim/artifacts. |
| `knowledge::SourceObservationProposedSchema` | Wire tag for the observation proposal. |
| `knowledge::SourceObservationCandidateRecord` | Stored proposed observation awaiting its proper disposition. |
| `knowledge::SourceObserved` | Observation submitted to the source-update operation. |
| `knowledge::SourceObservedSchema` | Wire tag for source observation recording. |

The proposed observation's claim and artifacts are evidence candidates. The registered observation path checks the appropriate authority and existing source identity before changing durable state.

## Keep normative facts, observations and adjudication distinct {#fact-lifecycle}

`guide r1`

[record_native_facts](../../../../crates/zap-domain/src/knowledge/transitions.rs) derives native fact records from the captured native source. [propose_fact and adjudicate_fact](../../../../crates/zap-domain/src/knowledge/review_transitions.rs) handle other fact proposals and their scoped evidence/applicability checks.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::FactOrigin` | Native specification, observation or semantic-assessment origin. |
| `knowledge::EpistemicStatus` | Normative-only, observed, invalidated or unknown knowledge state. |
| `knowledge::FactAcceptanceStatus` | Unassessed, accepted or rejected adjudication state. |
| `knowledge::FactRecord` | Stored fact meaning, provenance and independent status dimensions. |
| `knowledge::NativeFactInput` | Native fact content supplied with its captured source. |
| `knowledge::NativeFactsRecorded` | Native source/fact recording payload. |
| `knowledge::NativeFactsRecordedSchema` | Exact native-fact recording wire tag. |
| `knowledge::FactProposed` | Fact proposal and its source/evidence references. |
| `knowledge::FactProposedSchema` | Wire tag for fact proposal. |
| `knowledge::FactAdjudicated` | Revision-bound fact adjudication payload. |
| `knowledge::FactAdjudicatedSchema` | Wire tag for fact adjudication. |

A native specification statement can remain NormativeOnly; its text does not prove observed implementation. Epistemic and acceptance statuses are separate. Adjudication consumes the scoped witness supplied by the registered path rather than a caller-invented list of evidence IDs.

## Record typed relations and derive known invalidation {#knowledge-relations}

`guide r1`

Parse [KnowledgeEdgeId](../../../../crates/zap-domain/src/knowledge/identifiers.rs), retain typed endpoints, and use [record_dependency and invalidation_closure](../../../../crates/zap-domain/src/knowledge/transitions.rs) with the actual known records.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::KnowledgeEdgeId` | Validated identity of a knowledge relation. |
| `knowledge::KnowledgeEndpoint` | Typed endpoint in the knowledge graph. |
| `knowledge::DependencyRelation` | Meaning of the recorded directed relation. |
| `knowledge::KnowledgeDependencyRecord` | Stored prerequisite/dependent relation. |
| `knowledge::DependencyRecorded` | Relation-recording payload. |
| `knowledge::DependencyRecordedSchema` | Exact dependency-recording wire tag. |
| `knowledge::InvalidationClosure` | Known affected endpoints and closure incompleteness. |

Endpoint conversion is partial: a knowledge Fact endpoint has no `SubjectRef` representation. Preserve that distinction rather than guessing an equivalent subject. The invalidation closure follows known relations and exposes incompleteness; it cannot discover missing semantic dependencies.

## Assess closure and source applicability with scoped proof {#knowledge-applicability}

`guide r1`

[assess_closure, assess_applicability and current_applicability](../../../../crates/zap-domain/src/knowledge/transitions.rs) bind the assessment to the expected revision, source digest and relevant evidence. Complete, incomplete and unknown closure remain distinct.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::ClosureStatus` | Complete, incomplete or unknown knowledge closure. |
| `knowledge::KnowledgeClosureRecord` | Stored closure assessment and its evidence/basis. |
| `knowledge::ClosureAssessed` | Exact closure-assessment input. |
| `knowledge::ClosureAssessedSchema` | Closure-assessment wire tag. |
| `knowledge::SourceApplicabilityStatus` | Applicable, not applicable, stale or unknown source use. |
| `knowledge::SourceApplicabilityRecord` | Source/digest-bound applicability assessment. |
| `knowledge::ApplicabilityAssessed` | Applicability-assessment payload. |
| `knowledge::ApplicabilityAssessedSchema` | Applicability-assessment wire tag. |
| `knowledge::ApplicabilityView` | Combined applicability result with stale/unknown references. |

Consume stale and unknown references from the applicability view. An unrelated source change need not invalidate a correctly bounded assessment, while an incomplete known closure is not complete applicability.

## Refine uncertainty without equating progress with fewer regions {#unknown-regions}

`guide r1`

Parse [RegionId](../../../../crates/zap-domain/src/knowledge/identifiers.rs) and use [region transition, relevance, split and merge helpers](../../../../crates/zap-domain/src/knowledge/region_transitions.rs) with current records. Transitions and relevance decisions that require evidence receive the appropriate scoped witness.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::RegionId` | Validated unknown-region identity. |
| `knowledge::RegionState` | Unexamined, bounded, evidenced or invalidated knowledge state. |
| `knowledge::RegionRelevance` | Relevant, irrelevant or unknown relationship to current work. |
| `knowledge::RegionRecord` | Stored region question, scope, state and history. |
| `knowledge::NewRegion` | Proposed child or merged region. |
| `knowledge::RegionTransitioned` | Exact region-state transition payload. |
| `knowledge::RegionTransitionedSchema` | Region-transition wire tag. |
| `knowledge::RegionRelevanceSet` | Evidence-bound relevance-change payload. |
| `knowledge::RegionRelevanceSetSchema` | Region-relevance wire tag. |
| `knowledge::RegionSplit` | Parent and proposed children for a region split. |
| `knowledge::RegionSplitSchema` | Region-split wire tag. |
| `knowledge::RegionMerged` | Source regions and proposed merged region. |
| `knowledge::RegionMergedSchema` | Region-merge wire tag. |

Splitting may increase the number of unknown regions while improving knowledge. Relevance and epistemic state are independent; marking a region irrelevant is not permission to expand the charter or erase its history.

## Preserve semantic assessments as sourced proposals {#semantic-assessment}

`guide r1`

[propose_semantic_assessment](../../../../crates/zap-domain/src/knowledge/region_transitions.rs) validates the proposed record against known premises. It does not invoke a model or certify the conclusion's truth.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::SemanticAssessmentRecord` | Sourced semantic conclusion proposed against a relevant basis. |
| `knowledge::Feasibility` | Feasible, infeasible or unknown assessment in the knowledge model. |
| `knowledge::SemanticAssessmentProposed` | Semantic-assessment proposal payload. |
| `knowledge::SemanticAssessmentProposedSchema` | Exact semantic-assessment wire tag. |

Retain the supplied relevant basis, premise identities and feasibility assessment. `knowledge::Feasibility` belongs to this knowledge model; it is distinct from the similarly named economic alternative type.

## Propose and apply a complete adaptive transition {#adaptive-review}

`guide r1`

[review_proposal_basis and propose_review](../../../../crates/zap-domain/src/knowledge/review_transitions.rs) bind the review to captured intent, outcome, sources and regions. Its transition preserves explicit obligation, ownership, work, proof, deferral and live-job dispositions.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::AdaptiveReviewRecord` | Captured review basis, alternatives, chosen decision and planned changes. |
| `knowledge::ReviewAlternative` | Candidate route with value, feasibility, cost and risk rationale. |
| `knowledge::ValueAssessment` | Qualitative value assessment retained by the review. |
| `knowledge::ReviewDecision` | Selected adaptive-review decision category. |
| `knowledge::ReviewStatus` | Proposed, applied or stale review state. |
| `knowledge::ReviewTransition` | Complete obligation/work/proof/deferral/job transition plan. |
| `knowledge::OwnershipChange` | Exact obligation-ownership change and reason. |
| `knowledge::ReviewWorkOperation` | Selected work-change category. |
| `knowledge::ReviewWorkChange` | Work identity and the planned operation/successors. |
| `knowledge::DeferralReviewAction` | Retain, transfer, close or mark a deferral inapplicable. |
| `knowledge::DeferralReviewDisposition` | Exact deferral change and supporting evidence. |
| `knowledge::ReconciliationAction` | Knowledge-review plan for continuing, draining or revalidating live work. |
| `knowledge::JobReconciliationPlan` | Bound job/attempt/work reconciliation plan. |
| `knowledge::RegionSnapshot` | Captured region revision used by the review. |
| `knowledge::ProofReuseRecord` | Explicit mapping of retained acceptance/evidence across outcome change. |
| `knowledge::ReviewProposed` | Adaptive-review proposal payload. |
| `knowledge::ReviewProposedSchema` | Exact review-proposal wire tag. |
| `knowledge::ReviewApplied` | Exact review/revision application payload. |
| `knowledge::ReviewAppliedSchema` | Exact review-application wire tag. |

Use the registered ReviewApplied operation for the full application. `apply_review_marker`, `build_proof_reuse`, `expand_sparse_dispositions` and `reconcile_work_change` are bounded helpers, not substitutes for the whole transaction and its economics/stop gates.

A review can retain, research, reorder or revise the route within authority. Reused proof remains an explicit mapping; a revalidation or reconciliation plan is not already a safe release or an accepted result.

## Consume internally acquired current proof witnesses {#current-proof}

`guide r1`

[CurrentProofSet and ScopedEvidenceWitness](../../../../crates/zap-domain/src/knowledge/proof.rs) are exported opaque types with private fields and crate-private acquisition helpers. The registered domain path obtains them from current evidence, source, work-generation and basis context.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::CurrentProofSet` | Internally derived set of currently applicable adjudicated proof. |
| `knowledge::ScopedEvidenceWitness` | Internally acquired evidence witness for an exact assessment scope/basis. |

Use the corresponding registered proof-consuming operations. Arbitrary accepted-ID DTOs cannot construct these witnesses, and calling a pure acceptance helper requires the actual opaque proof context rather than copied evidence rows.

## Provide scoped basis and inspect knowledge summaries {#domain-basis}

`guide r1`

Compose the [DomainBasisProvider](../../../../crates/zap-domain/src/knowledge/basis.rs) as the core BasisProvider implementation. It derives the requested relevant basis and validates proposed scope against the supplied state and actual registrations.

| Public type | Role in this operation |
| --- | --- |
| `knowledge::DomainBasisProvider` | Domain implementation of relevant-basis and scope validation. |
| `knowledge::KnowledgeSummaryScope` | All or explicitly selected subject scope for a summary. |
| `knowledge::KnowledgeSummaryInput` | Knowledge-summary query input. |
| `knowledge::KnowledgeSummary` | Bounded knowledge counts and stale/incomplete information. |

The [knowledge-summary input/result](../../../../crates/zap-domain/src/knowledge/queries.rs) is consumed through the public domain query registry. Counts and unknown-region information describe the selected scope; they do not imply that all semantic relationships are known.

## Represent incremental cost and uncertainty in fixed-point hours {#cost-representation}

`guide r1`

[HoursMicros](../../../../crates/zap-domain/src/economics/model.rs) represents millionths of an hour, not microseconds. Use its checked arithmetic and `HoursInterval::new` for ordered bounds. `IncrementalCost::validate` checks expectations, intervals and category accounting.

| Public type | Role in this operation |
| --- | --- |
| `economics::HoursMicros` | Fixed-point micro-hour value with checked arithmetic. |
| `economics::HoursInterval` | Lower and optional upper cost/time bound. |
| `economics::CostBand` | Qualitative assessed cost band. |
| `economics::CostPrecision` | Measured, bounded-estimate or order-of-magnitude precision. |
| `economics::ConsequenceBand` | Qualitative consequence band for a cost category. |
| `economics::CostCategoryKind` | Implementation, verification and other accounted cost category. |
| `economics::Applicability` | Included, not applicable or unknown category disposition. |
| `economics::CostCategory` | Cost-category estimate and its justification. |
| `economics::CostUnknown` | Explicit unanswered cost question and bounds. |
| `economics::ExcludedCostKind` | Retained approved baseline or sunk-cost exclusion category. |
| `economics::ExcludedCost` | Justified exclusion from incremental cost. |
| `economics::IncrementalCost` | Elapsed, passive-wait and agent-hour estimates with category accounting. |

Retain measured versus estimated precision, unknown costs and excluded approved-baseline/sunk costs. A missing upper bound is not zero cost. Cost bands describe the policy's assessment inputs rather than an independently measured prediction.

## Retain benefit, necessity and team assumptions {#utility-necessity-capacity}

`guide r1`

The [utility, necessity and capacity model](../../../../crates/zap-domain/src/economics/model.rs) records the rationale used by economic comparison. `ChangeNecessity::validate` checks mandatory grounding; `TeamCapacityModel::validate` and `digest` preserve the selected capacity assumptions.

| Public type | Role in this operation |
| --- | --- |
| `economics::UtilityBand` | Qualitative utility magnitude. |
| `economics::ConfidenceBand` | Confidence retained with an assessment. |
| `economics::UtilityAssessment` | Benefit/risk/urgency and related value judgments. |
| `economics::NecessityClass` | Mandatory problem, safeguard or optional improvement category. |
| `economics::ChangeNecessity` | Grounded reason the proposed change is necessary. |
| `economics::ExecutorCapacity` | Executor-class assumptions for estimation. |
| `economics::ResourceCapacity` | Named nominal resource capacity. |
| `economics::TeamCapacityModel` | Selected team/parallelism/resource assumptions and identity. |

Nominal parallelism and executor/resource capacity are estimation inputs, not observed free execution slots. Utility and confidence remain explicit judgments with evidence instead of an unexplained numeric score.

## Compare exact proposed effects with alternatives and NoOp {#economic-alternatives}

`guide r1`

[ChangeEffect::validate and fingerprint](../../../../crates/zap-domain/src/economics/model.rs) bind canonical payload, event, scope and before/after information. `ChangeAlternative::validate` checks the proposed effect sequence and its cost representation.

| Public type | Role in this operation |
| --- | --- |
| `economics::ChangeEffect` | Exact proposed effect and its canonical preparation identities. |
| `economics::AlternativeKind` | Proposal, cheaper alternative or NoOp classification. |
| `economics::Feasibility` | Economic alternative's feasible/infeasible/unknown assessment. |
| `economics::ChangeAlternative` | Compared alternative, obligations, value, cost and proposed effects. |

Retain the proposal, cheaper alternative and explicit NoOp as distinct choices. Alternative feasibility and a claimed mandatory-problem solution are assessment data; actual effect preparation/admission supplies the checked execution boundary.

## Establish a policy-bound approved baseline {#economic-policy-baseline}

`guide r1`

[ChangePolicyRecord::default_policy and validate](../../../../crates/zap-domain/src/economics/records.rs) provide the existing default and validate its bounds. The default threshold is four hours; policy activation is a separate Owner-control operation.

| Public type | Role in this operation |
| --- | --- |
| `economics::UnknownCostHandling` | Policy for deciding when cost uncertainty requires the Owner. |
| `economics::UnknownImpactHandling` | Policy for holding unproven-independent work or all starts. |
| `economics::ChangePolicyRecord` | Versioned economic thresholds and estimation policy. |
| `economics::ChangeBaselineRecord` | Exact approved history/charter/outcome comparison baseline. |
| `economics::BaselineEstablished` | Baseline-establishment payload. |
| `economics::ChangePolicyProposed` | New economic policy proposal payload. |

Use the registered [BaselineEstablished and ChangePolicyProposed payloads](../../../../crates/zap-domain/src/economics/cells.rs). The baseline binds the approved history and current charter/intent/outcome; it prevents already approved work from being silently charged as a new change.

## Evaluate an assessment before recording its decision {#assessment-decision}

`guide r1`

[evaluate_assessment](../../../../crates/zap-domain/src/economics/decision.rs) validates the supplied necessity, team, alternatives and rationale against the policy. Consume its recommendation and admission disposition separately.

| Public type | Role in this operation |
| --- | --- |
| `economics::Recommendation` | Take proposal, prefer alternative, retain baseline or investigate unknowns. |
| `economics::AdmissionDisposition` | Automatic, Owner-decision-required or blocked assessment outcome. |
| `economics::EstimationUsage` | Recorded estimation time, assumptions and evidence. |
| `economics::EstimationStop` | Sufficiency, budget or evidence boundary that ended estimation. |
| `economics::ChangeAssessmentRecord` | Bound baseline, scope, alternatives and adjudication state. |
| `economics::AssessmentDecision` | Pure economic recommendation/admission/hold result. |
| `economics::ChangeAssessmentProposed` | Assessment proposal payload. |
| `economics::ChangeAssessmentAdjudicated` | Exact assessment adjudication payload. |

Record the proposal/adjudication through the registered [assessment payloads](../../../../crates/zap-domain/src/economics/cells.rs). Automatic eligibility is not an executed change; an OwnerDecisionRequired result does not consume a future approval. Preserve estimation effort and why investigation stopped.

## Refresh cumulative cost without resetting the baseline {#cost-forecast}

`guide r1`

[validate_forecast and forecast_requires_owner](../../../../crates/zap-domain/src/economics/decision.rs) consume cumulative actuals plus remaining-to-verified estimates under the current policy. `forecast_digest` identifies the corresponding [record](../../../../crates/zap-domain/src/economics/records.rs).

| Public type | Role in this operation |
| --- | --- |
| `economics::ForecastTotals` | Elapsed, passive-wait and agent-hour totals with intervals. |
| `economics::ForecastTrigger` | Material reason for recalculating the forecast. |
| `economics::CostForecastRecord` | Cumulative/remaining forecast bound to the original assessment baseline. |
| `economics::CostForecastRefreshed` | Forecast refresh payload. |
| `economics::CostForecastAdjudicated` | Exact forecast adjudication payload. |

Use the registered refresh/adjudication operations and retain the previous forecast, original baseline and completed effect identities. Splitting work or restarting a session does not reset cumulative assessment.

## Retain affected holds until their checked resolution {#economic-holds}

`guide r1`

[change_hold_guard](../../../../crates/zap-domain/src/economics/holds.rs) consumes the exact preflight scope and independence evidence. Active economic holding includes every status other than Released, not only the enum variant Active.

| Public type | Role in this operation |
| --- | --- |
| `economics::HoldStatus` | Holding, applying, restoring, deferred or released economic state. |
| `economics::ChangeHoldRecord` | Exact held scope, decision, affected jobs and recovery evidence. |
| `economics::HoldGuardStatus` | Clear, held or unproven guard outcome. |
| `economics::ChangeHoldGuard` | Matched hold/scope result consumed by admission. |
| `economics::HoldResolution` | Selected hold-resolution category. |
| `economics::ChangeHoldResolved` | Exact resolution payload. |
| `economics::EconomicsCompletionProvider` | Registered economic contribution to shared completion blockers. |

The registered [ChangeHoldResolved operation](../../../../crates/zap-domain/src/economics/cells.rs) and [economics completion provider](../../../../crates/zap-domain/src/economics/completion.rs) preserve unresolved decisions, applying effects and unknown outcomes as appropriate blockers. Owner pauses retain their separate precedence.

## Apply admitted economic changes through the common gate {#economic-admission}

`guide r1`

[ChangeControlAdmissionProvider::new](../../../../crates/zap-domain/src/economics/admission.rs) supplies the registered admission lifecycle around the actual product write. [DomainActionImpactProvider](../../../../crates/zap-domain/src/economics/impact.rs) classifies the operation and [DomainAffectedScopeProvider](../../../../crates/zap-domain/src/economics/affected_scope.rs) derives its affected scope.

| Public type | Role in this operation |
| --- | --- |
| `economics::ChangeAdmissionRecord` | Exact economic admission and effect-consumption state. |
| `economics::ChangeAdmissionPrepared` | Admission-preparation payload. |
| `economics::ChangeControlAdmissionProvider` | Configured current admission lifecycle with explicit historical compatibility. |
| `economics::DomainActionImpactProvider` | Domain operation-impact classifier. |
| `economics::DomainAffectedScopeProvider` | Domain affected-scope and independence provider. |

Use the registered ChangeAdmissionPrepared operation with the exact assessment, forecast, selected effect and Owner decision where required. A constructed admission record does not consume approval or apply the effect. Current control/hold checks and the actual after-outcome remain part of the same service path.

## Add optional descriptive assessment without changing work {#map-work-assessment}

`guide r1`

The [map assessment module](../../../../crates/zap-domain/src/map_assessment/mod.rs)
stores one optional descriptive record for an existing materialized WorkId.
Use `work_assessment_basis` on the current snapshot before proposing the
record, then preserve the returned fingerprint in
`MapWorkAssessmentProposed`. The registered DataProposal cell checks that
fingerprint again and uses `expected_assessment_revision` as exact record CAS.

| Public type | Role in this operation |
| --- | --- |
| `map_assessment::MapAssessmentGrade` | Independent low, medium, high or unassessed complexity/difficulty grade. |
| `map_assessment::MapAssessmentConfidence` | Explicit confidence in the descriptive uncertainty assessment. |
| `map_assessment::MapAssessmentFreshness` | Current, stale or unavailable classification against current Work/contract data. |
| `map_assessment::MapWorkEstimate` | One hours range, precision, source and assumptions for a single effort/time dimension. |
| `map_assessment::MapComplexityAssessment` | Structural complexity grade and rationale. |
| `map_assessment::MapDifficultyAssessment` | Executor-relative difficulty, rationale and executor/knowledge assumptions. |
| `map_assessment::MapUncertaintyAssessment` | Confidence, rationale and explicit unknowns. |
| `map_assessment::MapWorkAssessmentContent` | Optional label/explanation, independent estimates, grades, uncertainty and evidence. |
| `map_assessment::MapWorkAssessmentRecord` | Revisioned descriptive metadata keyed by the underlying WorkId and source fingerprint. |
| `map_assessment::MapWorkAssessmentProposed` | Versioned DataProposal payload with source fingerprint and record CAS. |
| `map_assessment::MapWorkAssessmentProposedSchema` | Exact proposal wire tag. |

`MapWorkEstimate` keeps remaining agent hours, elapsed duration and passive
wait in separate optional fields. Missing estimates remain absent, and an
unknown upper bound stays unknown. Assessed difficulty requires its executor
and knowledge assumptions; unassessed metadata may retain partial context.
Evidence IDs must name existing evidence records, but their presence does not
turn the assessment into acceptance or observed capability.

Use `work_assessment_freshness` when presenting stored metadata. A Work or
selected active-contract change makes the record stale; an unrelated commit
does not. The selector preserves the established complete indexed
last-active-contract policy. Reading or writing this record never changes the
Work, contract, authority, completion or dispatch state.

The module's `record_set`, `cell_set` and `route_set` functions compose the
storage and DataProposal surface. `MAP_WORK_ASSESSMENT_PROPOSED_KIND` is the
event kind for an AgentData binding; knowing that string does not grant the
binding or authorize a command.

## Preserve strategy independently of its lowered work {#strategic-plan}

`guide r1`

[validate_strategy and strategy_digest](../../../../crates/zap-domain/src/lowering/transitions.rs) validate and identify the proposed [strategic record](../../../../crates/zap-domain/src/lowering/records.rs). Submit StrategyProposed through the configured registration; a Candidate strategy is not automatically Current.

| Public type | Role in this operation |
| --- | --- |
| `lowering::PlanningRevisionState` | Candidate, current or superseded planning revision. |
| `lowering::StrategicNode` | Strategic work node and its obligations/dependencies. |
| `lowering::StrategicPlanRecord` | Stored strategic revision and its semantic identity. |
| `lowering::StrategyProposed` | Strategic-plan proposal payload. |
| `lowering::StrategyProposedSchema` | Exact strategy-proposal wire tag. |

Retain intent/outcome lineage, obligations, prepared forks and refinement triggers. Lowering derives executable work from that strategy without replacing the strategic source with a worker packet.

## Lower strategy with complete obligation and stage-debt coverage {#lowered-graph}

`guide r1`

The [lowered graph model](../../../../crates/zap-domain/src/lowering/model.rs) links executable contracts and container nodes to the parent strategy. Preserve implementation/verification/integration traces, exact outcome dispositions, stage debt, deferrals and unresolved horizons.

| Public type | Role in this operation |
| --- | --- |
| `lowering::ObligationRoute` | Active, successor-bound or inapplicable obligation route. |
| `lowering::OutcomeDispositionBinding` | Exact charter/outcome authority behind a disposition. |
| `lowering::ObligationTrace` | Implementation/verification/integration coverage for an obligation. |
| `lowering::StageDebtDisposition` | Required, accepted or deferred stage duty. |
| `lowering::StageDebt` | Work/stage duty and its explicit disposition. |
| `lowering::LoweredWork` | Lowered executable contract and verification declaration. |
| `lowering::RuleSourceBinding` | Requirement tied to its exact source identity. |
| `lowering::LoweredNodeExecution` | Container or explicitly bound executable-node meaning. |
| `lowering::LoweredWorkBinding` | Work/parent/dependency identity with its execution binding. |
| `lowering::LoweredGraph` | Proposed work, contracts and parent-obligation coverage. |
| `lowering::DeferralRoute` | Retained, transferred or inapplicable deferral path. |
| `lowering::DeferralTrace` | Deferral identity and its selected path. |
| `lowering::VerificationSelection` | Selected affected checks and justified proof reuse. |
| `lowering::BoundedHorizon` | Unrefined subject/question and its next refinement trigger. |
| `lowering::LoweringRecord` | Stored strategic lowering and its conserved obligations. |
| `lowering::LoweringApplied` | Semantic lowering application payload. |
| `lowering::LoweringAppliedSchema` | Exact lowering-application wire tag. |

Use the registered LoweringApplied path for semantic lowering. The [LoweringRecord](../../../../crates/zap-domain/src/lowering/records.rs) and `lowering_digest` retain its lineage and meaning. A named acceptance or deferral reference is not itself proof that a duty was discharged, and a container has no implicit executable contract.

## Bind relowering to the applied review's exact work changes {#review-relowering}

`guide r1`

The [review-relowering model](../../../../crates/zap-domain/src/lowering/model.rs) and [record](../../../../crates/zap-domain/src/lowering/records.rs) preserve the applied review, prior lowering, work versions and candidate/job context.

| Public type | Role in this operation |
| --- | --- |
| `lowering::ReviewReloweringKey` | Applied review and preceding lowering identity. |
| `lowering::ReviewWorkCas` | Exact pre/post-review work state and execution bindings. |
| `lowering::ReviewReloweringStatus` | Pending or consumed relowering state. |
| `lowering::ReviewReloweringBinding` | Revision/digest reference to the relowering record. |
| `lowering::ReviewReloweringRecord` | Stored review-caused relowering boundary and consumption history. |

Consume the pending binding through the registered lowering operation and retain its Consumed state and consuming lowering identity. Reconstructing the same shape does not authorize relowering against changed work or discard prior candidate/effect evidence.

## Choose within a prepared fork's current conditions {#prepared-forks}

`guide r1`

[select_fork](../../../../crates/zap-domain/src/lowering/transitions.rs) evaluates the selected alternative against the [prepared fork](../../../../crates/zap-domain/src/lowering/model.rs). Consume Selected, EvidenceRequired or Refused; Unknown conditions are not True.

| Public type | Role in this operation |
| --- | --- |
| `lowering::TruthValue` | True, false or unknown condition result. |
| `lowering::ForkCondition` | Named condition with its evidence request when needed. |
| `lowering::ForkAlternative` | Prepared alternative, conditions and tradeoffs. |
| `lowering::PreparedFork` | Delegated choice envelope and diagnostic/safe-stop context. |
| `lowering::ForkSelection` | Pure selected/evidence-required/refused choice outcome. |
| `lowering::ForkIdRef` | Stored reference to a prepared fork identity. |
| `lowering::ForkBinding` | Fork identity paired with its captured semantic digest. |

Keep the delegated alternatives, rejection conditions, recommendation and safe-stop boundary with the fork. Selecting a branch value does not persist the choice, expand authority or turn dormant alternatives into mandatory active work.

## Derive execution responsibility from the actual task {#worker-routing}

`guide r1`

[route_role](../../../../crates/zap-domain/src/lowering/transitions.rs) maps the supplied role assessment to algorithmic work or a worker role. [derive_packet_role](../../../../crates/zap-domain/src/lowering/packets/derive.rs) derives packet responsibility from its current work/strategy context.

| Public type | Role in this operation |
| --- | --- |
| `lowering::Consequence` | Assessed consequence of an error. |
| `lowering::VerificationCost` | Assessed cost of checking the result. |
| `lowering::RoleAssessment` | Task characteristics used by responsibility routing. |
| `lowering::RouteDecision` | Algorithmic execution or a selected worker responsibility. |

Consequence and verification cost support routing; they do not grant authority, establish model capability or justify silently changing a requested model. Preserve desired and resolved profiles in the packet's later contract.

## Assemble bounded context without changing admitted meaning {#packet-assembly}

`guide r1`

[assemble_packet](../../../../crates/zap-domain/src/lowering/packets.rs) checks subjects, provenance, required material, profile binding and optional abstraction. It deduplicates compatible fragments and preserves typed omissions; mandatory overflow is refused rather than silently truncated.

| Public type | Role in this operation |
| --- | --- |
| `lowering::FragmentClass` | Protocol, assignment, rule, source, example or full-boot fragment category. |
| `lowering::FragmentAvailability` | Available, unavailable, unknown or authorized-excluded material state. |
| `lowering::FragmentUse` | Instruction-authorized or data-only use of a fragment. |
| `lowering::PacketFragment` | Content identity, provenance, reason and retrieval metadata. |
| `lowering::ContextOmission` | Typed explanation and retrieval path for omitted context. |
| `lowering::HiddenConstraint` | Constraint class that an abstraction must not conceal. |
| `lowering::AbstractionMap` | Concrete/abstract mapping, preserved invariants and reconstruction checks. |
| `lowering::PacketAssembly` | Input to checked context assembly for a work/lowering binding. |
| `lowering::AssembledPacket` | Included context, omissions and the assembled packet identity. |
| `lowering::PacketState` | Current or superseded packet state. |
| `lowering::WorkerPacketRecord` | Stored rendered packet and executable-work lineage. |
| `lowering::CurrentWorkerPacket` | Current packet plus revalidated supporting planning context. |
| `lowering::PacketRendered` | Packet-rendering payload. |
| `lowering::PacketRenderedSchema` | Exact packet-rendering wire tag. |

Use the registered PacketRendered operation to persist the [worker packet](../../../../crates/zap-domain/src/lowering/records.rs). [current_worker_packet](../../../../crates/zap-domain/src/lowering/packets/current.rs) rechecks its current strategy/lowering/work bindings. Rendering is separate from semantic lowering, an admitted job claim and actual invocation.

Instruction use requires its explicit authority binding. Unloaded, unavailable, unknown and excluded context remain distinct. An abstraction map retains its invariants, omitted properties and reconstruction checks instead of concealing authority or consumer constraints.

## Inspect the stored planning bundle {#planning-view}

`guide r1`

The [bundle-view query contract](../../../../crates/zap-domain/src/lowering/queries.rs) exposes the stored planning view through the domain query registry. Use the returned strategy/lowering/packet relationships to navigate current state.

| Public type | Role in this operation |
| --- | --- |
| `lowering::BundleViewInput` | Selection input for the planning-bundle view. |
| `lowering::BundleView` | Returned strategic/lowering/packet planning view. |

The concrete query implementation is private; the public input/result types do not execute the query or grant permission to apply the inspected plan.

## Discover the active planning context {#active-context-query}

`guide r2`

Call the registered `zap.planning.active-context.v1` query with an empty
`lowering::ActiveContextInput`. Its single complete item binds `store_id`,
`campaign_id`, `base_id` and `revision`, then returns tagged references for the
active outcome, current strategy and adopted milestone plan. An empty campaign
uses `active_outcome: { "state": "uninitialized" }`; an adopted outcome without
a current strategy or plan uses explicit `absent` states.

The query reads the active-outcome and current-strategy derived indexes with a
two-row uniqueness bound, then loads the selected records by typed key. A stale
or unrebuilt index is `Unavailable`, and ambiguous active/current rows refuse.
An adopted plan whose proposal or strategy record is missing, stale or
mismatched remains visible as `needs_reassessment` with the authoritative
`plan_key`, plan-state revision and stable gap codes. The independently verified
outcome and current strategy stay available in the same item. Rebuild the
configured store's registered index catalog before retrying an unavailable
query. Discovery does not select a campaign, mutate a plan or grant admission.

| Public type | Role in this operation |
| --- | --- |
| `lowering::ActiveContextInput` | Closed empty input for configured-service discovery. |
| `lowering::ActiveContextSnapshot` | Exact store/campaign/base/revision binding. |
| `lowering::ActiveOutcomeRef` | Uninitialized or exact active outcome identity and revision. |
| `lowering::CurrentStrategyRef` | Absent or exact current strategic revision and record revision. |
| `lowering::AdoptedMilestonePlanGap` | Stable reassessment reason code for an adopted binding. |
| `lowering::AdoptedMilestonePlanRef` | Absent, present, or needs-reassessment plan-state reference. |
| `lowering::ActiveContextView` | One coherent snapshot-bound discovery result. |

## Seal a portable execution manifest with exact bindings {#portable-manifest}

`guide r1`

[WeakBundleManifest::seal and entries_digest](../../../../crates/zap-domain/src/lowering/offline/journal.rs) validate and identify the [portable bindings](../../../../crates/zap-domain/src/lowering/offline/model.rs). Preserve strategy, packet, attempt, source/rule/fork and permission/stop-rule identity together.

| Public type | Role in this operation |
| --- | --- |
| `lowering::BundleStrategyBinding` | Strategy/outcome/base identity carried by the bundle. |
| `lowering::BundlePacketBinding` | Exact packet and work contract included for execution. |
| `lowering::BundleAttemptBinding` | Permitted job/attempt identity carried by the bundle. |
| `lowering::BundleSourceBinding` | Captured source identity and artifact binding. |
| `lowering::BundleRuleBinding` | Captured requirement/source rule binding. |
| `lowering::BundleForkBinding` | Prepared fork identity and retained choice envelope. |
| `lowering::CharterPermissionBinding` | Delegated charter permission data retained for execution checks. |
| `lowering::StopRuleBinding` | Source stop-rule identity and binding. |
| `lowering::BundleEntryKind` | Typed portable entry category. |
| `lowering::BundleEntryBinding` | Entry identity, content binding and representation. |
| `lowering::WeakBundleManifest` | Complete portable execution envelope and its digest. |

Sealing computes consistency and content identity; it is not a signature, credential or new authority grant. A portable permission binding describes the delegated source envelope without giving an importer permission to activate unrelated work.

## Separate required closure, export and archive publication {#bundle-publication}

`guide r1`

[BundleClosureRequest::new](../../../../crates/zap-domain/src/lowering/offline/model.rs) validates a bounded request for the required bundle contents. The application closure provider derives the actual record; the registered export and archive-publication operations persist their respective results.

| Public type | Role in this operation |
| --- | --- |
| `lowering::BundleClosureRequest` | Bounded exact-content closure request. |
| `lowering::BundleClosureRecord` | Derived closure content required by bundle export. |
| `lowering::BundleStatus` | Stored lifecycle state of the portable bundle. |
| `lowering::BundleArchiveReceipt` | Observed publication and archive identity. |
| `lowering::WeakBundleRecord` | Stored bundle manifest, status and publication references. |
| `lowering::BundleExported` | Bundle-export payload. |
| `lowering::BundleExportedSchema` | Exact bundle-export wire tag. |
| `lowering::BundleArchivePublished` | Archive-publication recording payload. |
| `lowering::BundleArchivePublishedSchema` | Exact archive-publication wire tag. |

Prepared closure is not a published archive. Retain the stored bundle status and actual archive receipt, and verify its binding before external execution. Public payloads and schema tags are defined in [offline payloads](../../../../crates/zap-domain/src/lowering/offline/payloads.rs).

## Build an encounter delta without resetting operation history {#offline-encounters}

`guide r1`

[OfflineEncounter::seal](../../../../crates/zap-domain/src/lowering/offline/model.rs) binds one encounter's evidence. [OfflineEncounterJournal::new, append and finish](../../../../crates/zap-domain/src/lowering/offline/journal.rs) validate its manifest membership, sequence and previous-digest chain and produce a delta.

| Public type | Role in this operation |
| --- | --- |
| `lowering::EncounterKind` | Typed observation, finding or failure category. |
| `lowering::EncounterApproachBinding` | Problem/approach identity associated with the encounter. |
| `lowering::OfflineEncounter` | Sealed job/attempt-bound encounter and its evidence. |
| `lowering::EncounterDelta` | Ordered returned encounter history and its identity. |
| `lowering::OfflineEncounterJournal` | Checked builder of manifest-bound encounter deltas. |
| `lowering::EncounterRecord` | Stored imported encounter and its provenance. |

This journal is a builder over retained data, not an automatic filesystem writer. Persist/export its result through the selected surrounding workflow. An encounter records findings or failure history; it is not accepted product work simply because it was returned.

## Validate and classify the returned execution envelope {#returned-bundle}

`guide r1`

[seal_return_bundle, validate_return_bundle and validate_encounter_delta](../../../../crates/zap-domain/src/lowering/offline/validation.rs) check source-manifest, base, archive and encounter bindings. Use the actual application return-resolution provider for current-state classification and the registered import operation for persistence.

| Public type | Role in this operation |
| --- | --- |
| `lowering::ReturnArchiveReceipt` | Returned archive and delta identity. |
| `lowering::ReturnBundleInput` | Bound source envelope, returned history and archive receipt. |
| `lowering::ReturnClassification` | Classification of returned work against its current context. |
| `lowering::ReturnResolutionState` | Resolution progress for the returned data. |
| `lowering::ReturnImportRecord` | Stored return identity, classification and resolution state. |
| `lowering::ReturnImported` | Return-import payload. |
| `lowering::ReturnImportedSchema` | Exact return-import wire tag. |

An internally consistent return may still require reassessment against changed strategy or inputs. Import preserves unresolved findings and candidates instead of overwriting newer state or automatically accepting their claims.

## Preserve failed-approach counters across detached work {#returned-approaches}

`guide r1`

The [failed-approach model](../../../../crates/zap-domain/src/lowering/offline/model.rs) and [record](../../../../crates/zap-domain/src/lowering/offline/records.rs) retain the source problem/approach identity and counter disposition used during return processing.

| Public type | Role in this operation |
| --- | --- |
| `lowering::FailedApproachKey` | Stable identity for one returned failed approach. |
| `lowering::CounterDisposition` | Selected treatment of the returned approach counter. |
| `lowering::FailedApproachRecord` | Stored failed-approach evidence and counter handling. |

Use the registered return-resolution path to derive these updates. Re-import, account change or detached execution does not grant a fresh attempt history or permission to count the same failure repeatedly.

## Bind reassessment and relowering to the returned delta {#return-reassessment}

`guide r1`

[reassessment_digest and return_affected_roots](../../../../crates/zap-domain/src/lowering/offline/validation.rs) identify the returned input and its known scope. Record ReturnReassessmentProposed through the registered operation.

| Public type | Role in this operation |
| --- | --- |
| `lowering::ReturnDeltaBinding` | Exact return/delta identity carried into a subsequent review. |
| `lowering::ReturnReassessmentOutcome` | Selected reassessment disposition for the returned work. |
| `lowering::ReturnReassessmentStatus` | Current status of the return reassessment. |
| `lowering::ReturnReassessmentRecord` | Stored reassessment binding and outcome. |
| `lowering::ReturnReassessmentProposed` | Return-reassessment proposal payload. |
| `lowering::ReturnReassessmentProposedSchema` | Exact return-reassessment wire tag. |

Retain the exact delta binding, reassessment outcome/status and resulting record. A reassessment proposal is not applied relowering; consuming the corresponding review/relowering boundary remains a separate checked operation.

## Start a hypothetical branch or an explicit scope request {#dream-intent}

`guide r1`

The [Dreamer intent and attachment model](../../../../crates/zap-domain/src/dreamer/model.rs) distinguishes Hypothetical from ExplicitScopeChange and exact placement from an unresolved attachment. Submit the matching [start/request payload](../../../../crates/zap-domain/src/dreamer/payloads.rs) through the registered operation.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::DreamOperation` | Requested add, remove, move or replace operation. |
| `dreamer::DreamApplicationDisposition` | Recorded applied-operation category. |
| `dreamer::DreamIntent` | Hypothetical exploration or explicit scope-change intent. |
| `dreamer::DreamAttachment` | Exact strategy-root or subgoal placement. |
| `dreamer::DreamAttachmentRequest` | Exact or explicitly ambiguous requested placement. |
| `dreamer::DreamAttachmentState` | Resolved placement or retained question/candidates. |
| `dreamer::DreamStatus` | Exploration/readiness/staleness/application lifecycle state. |
| `dreamer::DreamDraft` | Base strategy, requested delta and exploration context. |
| `dreamer::DreamBranchRecord` | Stored isolated Dreamer branch and its captured base. |
| `dreamer::DreamExplorationStarted` | Hypothetical exploration start payload. |
| `dreamer::DreamScopeChangeRequested` | Explicit scope-change request payload. |
| `dreamer::DreamSchema` | Exact schema discriminator shared by Dreamer payloads. |

An ambiguous attachment retains its question and candidate placements. A DreamBranchRecord stores the exploration separately from current strategy; neither Exploring nor Ready means the proposed delta has been applied.

## Persist factual discovery and Owner choices separately {#dream-grill}

`guide r1`

Use [GrillQuestionId::new](../../../../crates/zap-domain/src/dreamer/model.rs) and the typed question/answer forms to retain placement, factual discovery and Owner preferences. The [question/answer/completion payloads](../../../../crates/zap-domain/src/dreamer/payloads.rs) bind the exact dream revision.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::GrillQuestionId` | Question identity created by the checked constructor. |
| `dreamer::GrillQuestionKind` | Placement, factual-discovery or Owner-preference category. |
| `dreamer::GrillChoice` | Named choice and its stated consequence. |
| `dreamer::GrillQuestion` | Persisted question, choices, recommendation and source references. |
| `dreamer::GrillAnswerValue` | Factual answer or explicit Owner-choice content. |
| `dreamer::GrillAnswer` | Answer bound to its question and recorded actor. |
| `dreamer::GrillState` | Offered, declined, in-progress or completed grill state. |
| `dreamer::DreamGrillQuestionSaved` | Question-recording payload. |
| `dreamer::DreamFactAnswered` | Sourced factual-answer payload. |
| `dreamer::DreamOwnerAnswered` | Owner-choice/placement-answer payload. |
| `dreamer::DreamGrillDeclined` | Exact grill-decline payload. |
| `dreamer::DreamGrillCompleted` | Exact transcript-completion payload. |

Factual answers retain sources; Owner choices retain the choice and reason through the appropriate authority route. Declining or completing the grill records that decision/transcript state and does not itself apply the dream.

## Retain assumptions, unknowns and alternatives {#dream-assumptions}

`guide r1`

The [Dreamer exploration records](../../../../crates/zap-domain/src/dreamer/model.rs) keep assumptions, unknown questions, their dispositions and alternatives explicit. They provide inputs for projection and discussion rather than executable work by default.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::AssumptionState` | Recorded state of a dream assumption. |
| `dreamer::DreamAssumption` | Assumption and supporting exploration context. |
| `dreamer::DreamUnknown` | Explicit unknown retained by the dream. |
| `dreamer::DreamUnknownDisposition` | Selected treatment of an unknown in the projection. |
| `dreamer::DreamAlternative` | Alternative considered during Dreamer exploration. |

Retain uncertainty when recalculating the branch. A disposition or confidence supplied in data is not newly observed evidence, and an unexplored alternative is not automatically a completion blocker.

## Represent the hypothetical operation explicitly {#dream-delta}

`guide r1`

The [delta types](../../../../crates/zap-domain/src/dreamer/model.rs) represent Add, Remove, Move and Replace with the appropriate node, attachment or removal plan. `DreamDeltaOperation::operation` derives the category from the actual variant; `delta_digest` identifies the delta representation.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::DreamAdd` | Proposed strategic node addition. |
| `dreamer::DreamMove` | Existing work and its proposed new attachment. |
| `dreamer::DreamReplace` | Removed work, replacement node and conservation plan. |
| `dreamer::DreamDeltaOperation` | Typed operation in the proposed delta. |
| `dreamer::DreamDelta` | Ordered proposed Dreamer operations. |

Constructing a delta or computing its digest does not mutate strategy. Projection, approval and the exact application operation remain separate.

## Conserve obligations and evidence when removing or replacing work {#dream-removal}

`guide r1`

The [removal plan](../../../../crates/zap-domain/src/dreamer/model.rs) accounts for obligation successors, dependent prerequisites, evidence, artifacts, stage debt, deferrals and external work. Projection and application check these dispositions against actual scope.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::ObligationRemovalDisposition` | Successor work for an affected obligation. |
| `dreamer::DependentRemovalDisposition` | Replacement prerequisite for an affected dependent. |
| `dreamer::EvidenceRemovalDisposition` | Retention disposition of affected evidence. |
| `dreamer::ArtifactRemovalDisposition` | Retention disposition of affected artifacts. |
| `dreamer::DreamRetention` | Supported retention policy for existing evidence/artifacts. |
| `dreamer::StageDebtRemovalDisposition` | Successor responsible for the affected stage debt. |
| `dreamer::DeferralRemovalDisposition` | Successor responsible for the affected deferral. |
| `dreamer::ExternalEffectRemovalDisposition` | External job requiring explicit conservation/reconciliation. |
| `dreamer::DreamRemovalPlan` | Complete proposed removal/conservation envelope. |

`DreamRetention` currently represents retention; it is not an arbitrary deletion policy. A proposed removal does not erase accepted evidence or dispose of a running/unknown external effect.

## Recalculate an isolated projection before application {#dream-projection}

`guide r1`

[project_dream](../../../../crates/zap-domain/src/dreamer/projection.rs) computes the branch projection against supplied current state. Use the registered DreamRecalculated operation to store it and the [DreamView query](../../../../crates/zap-domain/src/dreamer/queries.rs) to inspect the resulting branch/projection.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::DreamProjection` | Derived scope, preserved evidence, uncertainty and application readiness. |
| `dreamer::DreamValueProjection` | Represented value-related counts in the projection. |
| `dreamer::DreamCostProjection` | Affected/revalidation/reconciliation counts and assessment reference. |
| `dreamer::DreamBurdenProjection` | Lowering, stage-debt, deferral and artifact burden counts. |
| `dreamer::DreamProjectionSeal` | Exact projection identity retained for application checks. |
| `dreamer::DreamProjectionRecord` | Stored recalculated projection. |
| `dreamer::DreamRecalculated` | Exact recalculation payload. |
| `dreamer::DreamViewInput` | Dream-view query selection. |
| `dreamer::DreamView` | Returned branch and projection view. |

Value, cost and burden projections expose counts and assessment references, not independently measured utility or elapsed time. `DreamProjectionSeal::from` copies exact projection identity for later checks; this serializable seal is not a trusted authority witness.

Consume stale/rebased/promotable information with the current revision. Projecting or viewing a promotable branch does not apply its changes.

## Apply only the exact authorized projection {#dream-application}

`guide r1`

The [DreamApplied and combined Owner-decision payloads](../../../../crates/zap-domain/src/dreamer/payloads.rs) bind the dream revision, projection and economic/charter decision. `project_dream_for_combined` is a distinct projection path requiring the explicit charter-change requirement.

| Public type | Role in this operation |
| --- | --- |
| `dreamer::CharterChangeRequirement` | Exact charter expansion required by the proposed dream. |
| `dreamer::CombinedCharterBinding` | Original/replacement charter identities bound to the combined decision. |
| `dreamer::DreamCombinedOwnerDecision` | Combined charter/economics Owner-decision payload. |
| `dreamer::DreamCombinedAuthorizationRecord` | Stored exact authorization for the combined application envelope. |
| `dreamer::DreamApplied` | Exact projection application payload. |
| `dreamer::DreamApplicationRecord` | Recorded prior/applied strategy and application disposition. |

Use the registered application and combined-decision operations. Preserve [authorization/application records](../../../../crates/zap-domain/src/dreamer/records.rs) with the exact prior/applied strategy and effect bindings. A CombinedCharterBinding DTO does not manufacture Owner approval, and a projection cannot bypass later stale-state, economics or stop checks.

## Select the actual registered domain surface {#domain-registration}

`guide r1`

The [root registration functions](../../../../crates/zap-domain/src/registration.rs) supply record, cell, route and query sets. Routes derive from the actual cell descriptors; a public payload type alone does not advertise an installed operation. The completion-provider factory includes domain, control and economics contributions, with application/runtime composition supplying its other required providers.

Use the public index-family/algorithm registration helpers when composing the corresponding record/query surface. A valid family name does not prove its catalog is installed. Registration, an executed operation and its acceptance evidence remain distinct.

## Existing focused examples {#integration-examples}

`guide r1`

- [Domain contracts](../../../../crates/zap-domain/tests/domain_contracts.rs): registered surface truth, dedicated dispatch/acceptance, exact duty authorization and producer/acceptor separation.
- [Knowledge service](../../../../crates/zap-domain/tests/knowledge_service.rs): current sources and proof, adaptive/revalidation paths and viewer scenarios.
- [Economics service](../../../../crates/zap-domain/tests/economics_service.rs): exact decisions, pauses, holds, effect application and completion behavior.
- [Lowering service](../../../../crates/zap-domain/tests/lowering_service.rs): strategy, executable lowering and packet lineage through registered operations.
- [Offline lowering](../../../../crates/zap-domain/tests/lowering_offline.rs): manifest and encounter delta construction with exact return bindings.
- [Dreamer service](../../../../crates/zap-domain/tests/dreamer_service.rs): isolated projection, grill decisions, exact application and removal/combined-decision scenarios.
- [Map assessment](../../../../crates/zap-domain/tests/map_assessment.rs): source-bound descriptive metadata, exact CAS/retry, stale/unavailable classification and cold reopen.

These sources are executable usage references with explicit fixtures, not new execution receipts or acceptance of a different source revision. Internal test construction does not make private service helpers public APIs.

## Private implementation declarations {#internal-boundaries}

`guide r1`

The following source-level public declarations are behind private module facades. Consumers obtain their behavior through the registered cell/query/completion factories or the exported admission provider, not by inventing imports or constructors:

| Implementation source | Internal declarations |
| --- | --- |
| [queries.rs](../../../../crates/zap-domain/src/queries.rs) | `DomainCompletionProvider` |
| [acceptance/cells.rs](../../../../crates/zap-domain/src/acceptance/cells.rs) | `EvidenceAdjudicatedCell`, `StageAcceptedCell`, `IntegrationAcceptedCell`, `WorkAcceptedCell` |
| [dreamer/queries.rs](../../../../crates/zap-domain/src/dreamer/queries.rs) | `DreamViewQuery` |
| [economics/admission_cells.rs](../../../../crates/zap-domain/src/economics/admission_cells.rs) | `ChangeAdmissionPreparedCell`, `ChangeHoldResolvedCell` |
| [economics/admission_v1.rs](../../../../crates/zap-domain/src/economics/admission_v1.rs) | `Schema1ChangeControlAdmissionProvider` |
| [economics/assessment_cells.rs](../../../../crates/zap-domain/src/economics/assessment_cells.rs) | `BaselineEstablishedCell`, `ChangePolicyProposedCell`, `ChangeAssessmentProposedCell`, `ChangeAssessmentAdjudicatedCell` |
| [economics/forecast_cells.rs](../../../../crates/zap-domain/src/economics/forecast_cells.rs) | `CostForecastRefreshedCell`, `CostForecastAdjudicatedCell` |
| [economics/preflight.rs](../../../../crates/zap-domain/src/economics/preflight.rs) | `AssessmentProposalBasis`, `AssessmentAdjudicationBasis`, `AssessmentEffectBundles`, `AssessmentAffectedScope`, `AdmissionEffectBundle`, `HoldSafeJobs`, `ForecastAffectedScope`, `ForecastAdjudicationBasis` |
| [intent/cells.rs](../../../../crates/zap-domain/src/intent/cells.rs) | `CharterDraftedCell`, `CharterActivatedCell`, `CharterAmendedCell`, `IntentProposedCell`, `IntentAdoptedCell`, `OutcomeProposedCell`, `OutcomeAdoptedCell` |
| [knowledge/queries.rs](../../../../crates/zap-domain/src/knowledge/queries.rs) | `KnowledgeSummaryQuery` |
| [legacy_projection/query.rs](../../../../crates/zap-domain/src/legacy_projection/query.rs) | `LegacyProjectionQuery` |
| [lowering/cells.rs](../../../../crates/zap-domain/src/lowering/cells.rs) | `StrategyProposedCell`, `LoweringBasisScope`, `LoweringAppliedCell`, `LoweringEffectContract`, `PacketRenderedCell`, `PacketBasisScope` |
| [lowering/queries.rs](../../../../crates/zap-domain/src/lowering/queries.rs) | `BundleViewQuery` |
| [lowering/offline/cells.rs](../../../../crates/zap-domain/src/lowering/offline/cells.rs) | `BundleArchivePublishedCell`, `BundleArchiveArtifacts`, `ReturnReassessmentProposedCell` |
| [owner_control/cells.rs](../../../../crates/zap-domain/src/owner_control/cells.rs) | `StopRuleTriggeredCell`, `CampaignPausedCell`, `PauseResumedCell`, `StopRuleRecordedCell`, `ActionExceptionGrantedCell`, `ApproachEpochAdvancedCell`, `ChangePolicyActivatedCell`, `ChangeDecisionRecordedCell` |

The seven generated Owner cells are included in this internal denominator. By contrast, the generated schema enums, knowledge IDs and viewer-query adapters that cross public facades are real public types. Other crate-private support types and witness constructors remain implementation details; their existence does not grant authority.

## Public type coverage {#remaining-coverage}

`guide r1`

The catalogs map every unique public type once. The following module counts total 494; additional offline aliases do not represent additional types:

| Public module | Cataloged unique types |
| --- | ---: |
| [acceptance](../../../../crates/zap-domain/src/acceptance/mod.rs) | 27 |
| [control](../../../../crates/zap-domain/src/control/mod.rs) | 25 |
| [dreamer](../../../../crates/zap-domain/src/dreamer/mod.rs) | 58 |
| [economics](../../../../crates/zap-domain/src/economics/mod.rs) | 55 |
| [intent](../../../../crates/zap-domain/src/intent/mod.rs) | 19 |
| [knowledge](../../../../crates/zap-domain/src/knowledge/mod.rs) | 85 |
| [legacy_projection](../../../../crates/zap-domain/src/legacy_projection/mod.rs) | 8 |
| [lowering](../../../../crates/zap-domain/src/lowering/mod.rs) | 96 |
| [map_assessment](../../../../crates/zap-domain/src/map_assessment/mod.rs) | 11 |
| [owner_control](../../../../crates/zap-domain/src/owner_control/mod.rs) | 21 |
| [seams](../../../../crates/zap-domain/src/seams/mod.rs) | 35 |
| [strategic_map](../../../../crates/zap-domain/src/strategic_map/mod.rs) | 29 |
| [viewer_queries](../../../../crates/zap-domain/src/viewer_queries/mod.rs) | 25 |

Of the 67 macro-emitted declarations, 47 payload-schema enums, two knowledge IDs and 11 viewer-query adapters are public. The seven generated Owner cell types stay behind the private implementation facade listed above.
