use super::*;

pub(super) fn apply_no_change_return(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    data: &AgentDataIssuerHandle,
    internal: &InternalProtocolHandle,
    bundle: &WeakBundleRecord,
    imported: &ReturnImportRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    let baseline_id = establish_baseline(service, store, identity, internal)?;
    let review_id = ReviewId::parse("review.return-no-change")?;
    let chosen = DecisionId::parse("decision.return-no-change")?;
    let mut review = AdaptiveReviewRecord {
        review_id: review_id.clone(),
        previous_review_id: None,
        captured_revision: store.head()?,
        captured_intent_id: IntentId::parse("intent.one")?,
        captured_outcome_id: OutcomeId::parse("outcome.one")?,
        relevant_basis: RelevantBasisDigest::hash(b"pending-return-review-basis"),
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse(
            "Offline observation preserves the current semantic route",
        )?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Keep the current lowering")?,
            expected_value: ValueAssessment::High,
            feasibility: zap_domain::knowledge::Feasibility::Feasible,
            remaining_cost: BoundedText::parse("No semantic change")?,
            risks: Vec::new(),
            unknowns: Vec::new(),
        }],
        chosen,
        decision: ReviewDecision::KeepRoute,
        transition: ReviewTransition {
            next_outcome_id: None,
            obligation_dispositions: Vec::new(),
            ownership_changes: Vec::new(),
            work_changes: Vec::new(),
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation: Vec::new(),
            tradeoffs: Vec::new(),
            preserved_benefits: vec![BoundedText::parse("Current packet lineage")?],
        },
        next_trigger: BoundedText::parse("A material return delta")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    };
    let proposal_for_basis = ReviewProposed {
        schema: ReviewProposedSchema::V1,
        review: review.clone(),
    };
    let review_basis = DomainBasisProvider
        .relevant_basis(
            &store.read(ReadAt::Current)?,
            &review_proposal_basis(&proposal_for_basis)?,
        )?
        .digest;
    review.relevant_basis = review_basis;
    let reassessment = ReturnReassessmentProposed {
        schema: ReturnReassessmentProposedSchema::V1,
        review,
        binding: ReturnDeltaBinding {
            source_bundle_id: bundle.bundle_id.clone(),
            return_digest: imported.return_digest,
            delta_digest: imported.delta_digest,
            prior_strategy_id: bundle.manifest.binding.strategy_id.clone(),
            prior_lowering_id: bundle.manifest.binding.lowering_id.clone(),
            affected_scope: imported.affected_scope,
            import_revision: imported.revision,
        },
        outcome: ReturnReassessmentOutcome::NoChange {
            reason: BoundedText::parse("The return contains only an applicable observation")?,
        },
    };
    let proposal_frame = frame_with_basis(
        identity,
        &reassessment,
        store.head()?,
        BasisBinding::Exact(review_basis),
        "command.return-reassessment.packet-resolution",
    )?;
    let proposal_grant = data.authorize(&proposal_frame)?;
    service.submit(PrincipalContext::AgentData(&proposal_grant), proposal_frame)?;

    apply_review(
        service,
        store,
        identity,
        data,
        internal,
        baseline_id,
        "return-no-change",
        review_id.clone(),
        review_basis,
    )?;

    let snapshot = store.read(ReadAt::Current)?;
    let final_import = snapshot
        .get_typed::<ReturnImportRecord>(&bundle.bundle_id)?
        .ok_or("resolved return import missing")?;
    let final_reassessment = snapshot
        .get_typed::<ReturnReassessmentRecord>(&review_id)?
        .ok_or("applied return reassessment missing")?;
    let final_review = snapshot
        .get_typed::<AdaptiveReviewRecord>(&review_id)?
        .ok_or("applied return review missing")?;
    let current_lowering = snapshot
        .get_typed::<LoweringRecord>(&bundle.manifest.binding.lowering_id)?
        .ok_or("current lowering missing after no-change reassessment")?;
    assert_eq!(final_import.resolution, ReturnResolutionState::NoChange);
    assert_eq!(final_import.reassessment_review_id, Some(review_id));
    assert_eq!(final_import.resolved_lowering_id, None);
    assert_eq!(final_reassessment.status, ReturnReassessmentStatus::Applied);
    assert_eq!(final_reassessment.revision, Revision::new(2));
    assert_eq!(final_review.status, ReviewStatus::Applied);
    assert_eq!(current_lowering.state, PlanningRevisionState::Current);
    Ok(())
}

fn establish_baseline(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    internal: &InternalProtocolHandle,
) -> Result<ChangeBaselineId, Box<dyn std::error::Error>> {
    let baseline_id = ChangeBaselineId::parse("baseline.return-no-change")?;
    let head = store.head()?;
    let payload = BaselineEstablished {
        baseline: ChangeBaselineRecord {
            baseline_id: baseline_id.clone(),
            base_digest: BaseDigest::hash(b"return-no-change-base"),
            committed_prefix_digest: PayloadDigest::hash(b"return-no-change-prefix"),
            committed_sequence: head,
            active_charter_digest: PayloadDigest::hash(b"active-charter"),
            active_intent_id: IntentId::parse("intent.one")?,
            active_outcome_id: OutcomeId::parse("outcome.one")?,
            active_outcome_digest: PayloadDigest::hash(b"active-outcome"),
            observed_plan_digest: PayloadDigest::hash(b"lowering.one"),
            change_policy_revision: Revision::new(1),
            revision: head.checked_next()?,
        },
    };
    submit_internal(
        service,
        internal,
        frame(
            identity,
            &payload,
            head,
            "command.baseline.return-no-change",
        )?,
        "economics.baseline:return-no-change",
    )?;
    Ok(baseline_id)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_review(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    data: &AgentDataIssuerHandle,
    internal: &InternalProtocolHandle,
    baseline_id: ChangeBaselineId,
    name: &str,
    review_id: ReviewId,
    review_basis: RelevantBasisDigest,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = ReviewApplied {
        schema: ReviewAppliedSchema::V1,
        review_id: review_id.clone(),
        expected_review_revision: Revision::new(1),
    };
    let product_kind = EventKind::parse(ReviewApplied::KIND)?;
    let product_event_id = EventId::parse(&format!("event.review-apply.{name}"))?;
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?;
    let alternative_id = ChangeAlternativeId::parse(&format!("alternative.{name}"))?;
    let comparison = service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            ChangeAssessmentId::parse(&format!("assessment.{name}"))?,
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    EffectId::parse(&format!("effect.{name}"))?,
                    0,
                    product_kind.clone(),
                    product_payload.clone(),
                    Vec::new(),
                    product_event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = comparison
        .alternatives()
        .first()
        .ok_or("prepared review alternative missing")?;
    let request = prepared
        .request()
        .effects()
        .first()
        .ok_or("prepared review effect missing")?;
    let view = prepared
        .view()
        .effects
        .first()
        .ok_or("prepared review preflight missing")?;
    let effect = ChangeEffect {
        effect_id: request.effect_id().clone(),
        index: request.index(),
        kind: request.kind().clone(),
        payload: request.payload().as_bytes().to_vec(),
        payload_digest: request.payload().digest(),
        subjects: request.declared_subjects().to_vec(),
        predecessors: request.predecessors().to_vec(),
        basis: request.basis().clone(),
        relevant_before: request.relevant_before(),
        relevant_after: request.declared_relevant_after(),
        product_event_id: request.product_event_id().clone(),
        preflight_digest: None,
    };
    let scope_request = AffectedScopeRequest::new(
        effect.subjects.clone(),
        effect
            .subjects
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
    )?;
    let scope =
        DomainAffectedScopeProvider.derive(&store.read(ReadAt::Current)?, &scope_request)?;
    let assessment = assessment(
        name,
        baseline_id,
        alternative_id.clone(),
        effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    let assessment_frame = frame_with_basis(
        identity,
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        &format!("command.assessment.{name}"),
    )?;
    let assessment_grant = data.authorize(&assessment_frame)?;
    service.submit(
        PrincipalContext::AgentData(&assessment_grant),
        assessment_frame,
    )?;
    submit_internal(
        service,
        internal,
        frame_with_basis(
            identity,
            &ChangeAssessmentAdjudicated {
                assessment_id: assessment.assessment_id.clone(),
                hold_id: None,
                drain_job_ids: Vec::new(),
                independence_basis: assessment.comparison_basis_digest,
                independent_effect_fingerprints: Vec::new(),
            },
            store.head()?,
            BasisBinding::Exact(assessment.comparison_basis_digest),
            &format!("command.adjudicate.{name}"),
        )?,
        &format!("economics.adjudicate:{name}"),
    )?;
    let adjudicated = store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or("adjudicated return assessment missing")?;
    let effect = adjudicated.alternatives[0].effects[0].clone();
    let impact = ActionImpactRequest::new(
        ActionImpactRule::SemanticChange,
        Vec::new(),
        vec![SubjectRef::Review(review_id.clone())],
    )?;
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        ActionClass::parse("adaptive.apply")?,
        product_kind,
        product_event_id.clone(),
        product_payload.digest(),
        store.head()?,
        ActionImpactClass::SemanticChange,
        Some(effect.relevant_before),
    )?
    .digest;
    submit_internal(
        service,
        internal,
        frame(
            identity,
            &ChangeAdmissionPrepared {
                admission: ChangeAdmissionRecord {
                    change_id: adjudicated.change_id.clone(),
                    assessment_id: adjudicated.assessment_id.clone(),
                    assessment_digest: assessment_digest(&adjudicated)?,
                    forecast_id: None,
                    forecast_digest: None,
                    decision_id: None,
                    alternative_id,
                    effect_id: effect.effect_id.clone(),
                    effect_index: 0,
                    effect_fingerprint: effect.fingerprint()?,
                    relevant_before: effect.relevant_before,
                    action: ActionClass::parse("adaptive.apply")?,
                    command_id: CommandId::parse(&format!("command.review-apply.{name}"))?,
                    impact_digest,
                    effect_item_digest: view.stable_digest,
                    effect_preflight_digest: prepared.view().digest,
                    payload_digest: product_payload.digest(),
                    product_event_id,
                    exception_id: None,
                    hold_id: None,
                    final_effect: true,
                    applied_effect_ids: Vec::new(),
                    applied: false,
                    revision: Revision::new(1),
                },
            },
            store.head()?,
            &format!("command.admission.{name}"),
        )?,
        &format!("economics.admission:{name}"),
    )?;
    let review_frame = frame_with_basis(
        identity,
        &payload,
        store.head()?,
        BasisBinding::Exact(review_basis),
        &format!("command.review-apply.{name}"),
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("coordinator.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    service.submit(PrincipalContext::Credentialed(&coordinator), review_frame)?;
    Ok(())
}

pub(super) fn submit_internal(
    service: &CommitService<RedbStore>,
    internal: &InternalProtocolHandle,
    frame: CanonicalCommandFrame,
    operation: &str,
) -> Result<(), ZapError> {
    let permit = internal.authorize(&frame, OperationId::parse(operation)?)?;
    service.submit(PrincipalContext::ServiceInternal(&permit), frame)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_assessment(
    name: &str,
    baseline_id: ChangeBaselineId,
    alternative_id: ChangeAlternativeId,
    effect: ChangeEffect,
    comparison_basis_request: BasisRequest,
    comparison_basis_digest: RelevantBasisDigest,
    scope: &DerivedAffectedScope,
) -> Result<ChangeAssessmentRecord, ZapError> {
    assessment(
        name,
        baseline_id,
        alternative_id,
        effect,
        comparison_basis_request,
        comparison_basis_digest,
        scope,
    )
}

include!("../../../zap-domain/tests/lowering_semantic_service/economics.rs");
