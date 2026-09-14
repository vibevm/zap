fn apply_ordinary_review(
    harness: &Harness,
    baseline_id: ChangeBaselineId,
) -> Result<ReviewReloweringBinding, Box<dyn std::error::Error>> {
    let review_id = ReviewId::parse("review.lowering")?;
    let work_id = WorkId::parse("work.leaf")?;
    let chosen = DecisionId::parse("decision.review-lowering")?;
    let review = AdaptiveReviewRecord {
        review_id: review_id.clone(),
        previous_review_id: None,
        captured_revision: harness.store.head()?,
        captured_intent_id: IntentId::parse("intent.one")?,
        captured_outcome_id: OutcomeId::parse("outcome.one")?,
        relevant_basis: RelevantBasisDigest::hash(b"pending-review-basis"),
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse(
            "A better implementation method is available",
        )?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Revalidate the same work identity")?,
            expected_value: ValueAssessment::High,
            feasibility: zap_domain::knowledge::Feasibility::Feasible,
            remaining_cost: BoundedText::parse("Bounded")?,
            risks: Vec::new(),
            unknowns: Vec::new(),
        }],
        chosen,
        decision: ReviewDecision::ReplaceMethod,
        transition: ReviewTransition {
            next_outcome_id: None,
            obligation_dispositions: Vec::new(),
            ownership_changes: Vec::new(),
            work_changes: vec![ReviewWorkChange {
                work_id: work_id.clone(),
                operation: ReviewWorkOperation::Revalidate,
                order: 1,
                successor_ids: Vec::new(),
                reason: BoundedText::parse("Retain identity and revise implementation meaning")?,
            }],
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation: Vec::new(),
            tradeoffs: Vec::new(),
            preserved_benefits: vec![BoundedText::parse("Same obligation and work lineage")?],
        },
        next_trigger: BoundedText::parse("Next material method change")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    };
    let mut proposal = ReviewProposed {
        schema: ReviewProposedSchema::V1,
        review,
    };
    let review_basis = DomainBasisProvider
        .relevant_basis(
            &harness.store.read(ReadAt::Current)?,
            &review_proposal_basis(&proposal)?,
        )?
        .digest;
    proposal.review.relevant_basis = review_basis;
    harness.data_with_basis(
        &proposal,
        harness.store.head()?,
        BasisBinding::Exact(review_basis),
        "command-review-propose",
    )?;
    let current_review_basis = DomainBasisProvider
        .relevant_basis(
            &harness.store.read(ReadAt::Current)?,
            &BasisRequest::new(BasisRequestInput {
                purpose: BasisPurpose::Mutation(EventKind::parse("domain.review-applied")?),
                roots: vec![SubjectRef::Review(review_id.clone())],
                policy: ContextRequirement::Required,
                capacity: ContextRequirement::NotApplicable,
                closure: ClosureRequirement::KnownGraph,
            })?,
        )?
        .digest;
    assert_eq!(current_review_basis, review_basis);

    let review_payload = ReviewApplied {
        schema: ReviewAppliedSchema::V1,
        review_id: review_id.clone(),
        expected_review_revision: Revision::new(1),
    };
    let product_kind = EventKind::parse(ReviewApplied::KIND)?;
    let product_event_id = EventId::parse("event:command-review-apply")?;
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &review_payload)?;
    let alternative_id = ChangeAlternativeId::parse("alternative.review")?;
    let assessment_id = ChangeAssessmentId::parse("assessment.review")?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            assessment_id,
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    EffectId::parse("effect.review")?,
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
        .ok_or("review effect missing")?;
    let view = prepared
        .view()
        .effects
        .first()
        .ok_or("review preflight missing")?;
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
    let scope_request = AffectedScopeRequest::new(effect.subjects.clone(), vec![work_id.clone()])?;
    let derived_scope = DomainAffectedScopeProvider
        .derive(&harness.store.read(ReadAt::Current)?, &scope_request)?;
    let assessment = assessment(
        "review",
        baseline_id,
        alternative_id.clone(),
        effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &derived_scope,
    )?;
    harness.data_with_basis(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-review-assessment",
    )?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment.assessment_id.clone(),
            hold_id: None,
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-review-adjudicate",
    )?;
    let adjudicated = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or("review assessment missing")?;
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
        harness.store.head()?,
        ActionImpactClass::SemanticChange,
        Some(effect.relevant_before),
    )?
    .digest;
    harness.internal(
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
                command_id: CommandId::parse("command-review-apply")?,
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
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command-review-admission",
    )?;
    harness.privileged(
        &review_payload,
        harness.store.head()?,
        BasisBinding::Exact(effect.relevant_before),
        "command-review-apply",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let work = snapshot
        .get_typed::<WorkRecord>(&work_id)?
        .ok_or("reviewed work missing")?;
    assert_eq!(work.state, zap_domain::seams::WorkState::Blocked);
    assert_eq!(work.revision, Revision::new(2));
    assert_eq!(work.validation_generation, 0);
    let key = ReviewReloweringKey {
        review_id,
        previous_lowering_id: LoweringId::parse("lowering.one")?,
    };
    let sidecar = snapshot
        .get_typed::<ReviewReloweringRecord>(&key)?
        .ok_or("review relowering sidecar missing")?;
    assert_eq!(sidecar.status, ReviewReloweringStatus::Pending);
    assert_eq!(sidecar.revision, Revision::new(1));
    assert!(sidecar.return_cause.is_none());
    Ok(ReviewReloweringBinding {
        key,
        record_revision: sidecar.revision,
        digest: sidecar.digest,
    })
}
