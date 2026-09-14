use super::*;
use reassessment::{apply_review, build_assessment, submit_internal};

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_return_relowering(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    data: &AgentDataIssuerHandle,
    internal: &InternalProtocolHandle,
    bundle: &WeakBundleRecord,
    imported: &ReturnImportRecord,
    candidate_id: &CandidateId,
) -> Result<(), Box<dyn std::error::Error>> {
    let review_id = ReviewId::parse("review.return-relower")?;
    let work_id = WorkId::parse("work.leaf")?;
    let target = WorkId::parse("work.root")?;
    let chosen = DecisionId::parse("decision.return-relower")?;
    let mut review = AdaptiveReviewRecord {
        review_id: review_id.clone(),
        previous_review_id: Some(ReviewId::parse("review.return-no-change")?),
        captured_revision: store.head()?,
        captured_intent_id: IntentId::parse("intent.one")?,
        captured_outcome_id: OutcomeId::parse("outcome.one")?,
        relevant_basis: RelevantBasisDigest::hash(b"pending-return-relower-basis"),
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse(
            "Offline failure requires a bounded method replacement",
        )?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Revalidate the affected work identity")?,
            expected_value: ValueAssessment::High,
            feasibility: zap_domain::knowledge::Feasibility::Feasible,
            remaining_cost: BoundedText::parse("One bounded relowering")?,
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
                reason: BoundedText::parse("Replace the contradicted implementation method")?,
            }],
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation: Vec::new(),
            tradeoffs: Vec::new(),
            preserved_benefits: vec![BoundedText::parse("Preserve candidate history")?],
        },
        next_trigger: BoundedText::parse("Next material return delta")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    };
    let proposal = ReviewProposed {
        schema: ReviewProposedSchema::V1,
        review: review.clone(),
    };
    let review_basis = DomainBasisProvider
        .relevant_basis(
            &store.read(ReadAt::Current)?,
            &review_proposal_basis(&proposal)?,
        )?
        .digest;
    review.relevant_basis = review_basis;
    let payload = ReturnReassessmentProposed {
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
        outcome: ReturnReassessmentOutcome::Relower {
            target: target.clone(),
            changed_work_ids: vec![work_id.clone()],
            changed_subjects: vec![SubjectRef::Work(work_id.clone())],
            reason: BoundedText::parse("The failed approach changes the implementation contract")?,
        },
    };
    let proposal_frame = frame_with_basis(
        identity,
        &payload,
        store.head()?,
        BasisBinding::Exact(review_basis),
        "command.return-reassessment.packet-relower",
    )?;
    let grant = data.authorize(&proposal_frame)?;
    service.submit(PrincipalContext::AgentData(&grant), proposal_frame)?;
    apply_review(
        service,
        store,
        identity,
        data,
        internal,
        ChangeBaselineId::parse("baseline.return-no-change")?,
        "return-relower",
        review_id.clone(),
        review_basis,
    )?;

    let snapshot = store.read(ReadAt::Current)?;
    let applied_import = snapshot
        .get_typed::<ReturnImportRecord>(&bundle.bundle_id)?
        .ok_or("return import missing after relowering review")?;
    let applied_reassessment = snapshot
        .get_typed::<ReturnReassessmentRecord>(&review_id)?
        .ok_or("return reassessment missing after review")?;
    let key = ReviewReloweringKey {
        review_id: review_id.clone(),
        previous_lowering_id: bundle.manifest.binding.lowering_id.clone(),
    };
    let sidecar = snapshot
        .get_typed::<ReviewReloweringRecord>(&key)?
        .ok_or("return review relowering sidecar missing")?;
    assert_eq!(
        applied_import.resolution,
        ReturnResolutionState::ReloweringRequired
    );
    assert_eq!(
        applied_reassessment.status,
        ReturnReassessmentStatus::Applied
    );
    assert_eq!(sidecar.status, ReviewReloweringStatus::Pending);
    assert_eq!(sidecar.return_cause, Some(payload.binding));
    let return_cause = sidecar
        .return_cause
        .as_ref()
        .ok_or("return cause missing")?;
    assert_eq!(
        applied_import.reassessment_review_id.as_ref(),
        Some(&review_id)
    );
    assert_eq!(applied_import.return_digest, return_cause.return_digest);
    assert_eq!(applied_import.delta_digest, return_cause.delta_digest);
    assert_eq!(applied_import.affected_scope, return_cause.affected_scope);
    assert_eq!(applied_reassessment.binding, *return_cause);
    let ReturnReassessmentOutcome::Relower {
        target,
        changed_work_ids,
        changed_subjects,
        ..
    } = &applied_reassessment.outcome
    else {
        return Err("relowering reassessment outcome changed".into());
    };
    assert_eq!(target, &WorkId::parse("work.root")?);
    assert!(changed_work_ids.iter().all(|id| {
        applied_import.affected_work_ids.contains(id)
            || applied_import.dependent_work_ids.contains(id)
    }));
    assert!(
        changed_subjects
            .iter()
            .all(|subject| applied_import.affected_subjects.contains(subject))
    );
    assert!(
        sidecar
            .work
            .iter()
            .any(|row| row.current_candidate_ids.contains(candidate_id))
    );
    let binding = ReviewReloweringBinding {
        key,
        record_revision: sidecar.revision,
        digest: sidecar.digest,
    };
    drop(snapshot);

    let lowering = second_lowering(store, binding)?;
    submit_second_lowering(service, store, identity, data, internal, lowering)?;
    assert_consumed(store, bundle, &review_id, candidate_id)
}

fn second_lowering(
    store: &RedbStore,
    review_cause: ReviewReloweringBinding,
) -> Result<LoweringApplied, Box<dyn std::error::Error>> {
    let snapshot = store.read(ReadAt::Current)?;
    let first = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or("first lowering missing")?;
    let strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or("strategy missing")?;
    let mut work = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or("work missing")?;
    let mut contract = snapshot
        .get_typed::<zap_domain::control::TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or("contract missing")?;
    let obligation = snapshot
        .get_typed::<zap_domain::control::ObligationRecord>(&ObligationId::parse(
            "obligation.one",
        )?)?
        .ok_or("obligation missing")?;
    drop(snapshot);
    work.title = BoundedText::parse("Implement the return-corrected lowering")?;
    work.state = WorkState::Planned;
    work.active_job = None;
    work.validation_generation = work
        .validation_generation
        .checked_add(1)
        .ok_or("generation overflow")?;
    work.revision = work.revision.checked_next()?;
    contract.version = contract.version.checked_next()?;
    contract.contract.goal = BoundedText::parse("Produce a return-corrected typed candidate")?;
    contract.contract_digest = ContractDigest::hash(
        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &contract.contract)?.as_bytes(),
    );
    let mut lowering = first.clone();
    lowering.lowering_id = LoweringId::parse("lowering.return-two")?;
    lowering.previous = Some(first.lowering_id.clone());
    lowering.state = PlanningRevisionState::Candidate;
    lowering.review_cause = Some(review_cause.clone());
    lowering.revision = first.revision.checked_next()?;
    let binding = lowering
        .work
        .first_mut()
        .ok_or("lowering work binding missing")?;
    let LoweredNodeExecution::Executable {
        contract_version,
        contract_digest,
        validation_generation,
        ..
    } = &mut binding.execution
    else {
        return Err("lowering work is not executable".into());
    };
    *contract_version = contract.version;
    *contract_digest = contract.contract_digest;
    *validation_generation = work.validation_generation;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Lowering(lowering.lowering_id.clone()),
        roots: vec![
            SubjectRef::Work(lowering.target.clone()),
            SubjectRef::Obligation(obligation.obligation_id.clone()),
            SubjectRef::Source(SourceId::parse("source.one")?),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    lowering.relevant_basis = DomainBasisProvider
        .relevant_basis(&store.read(ReadAt::Current)?, &request)?
        .digest;
    lowering.semantic_digest = lowering_digest(&lowering)?;
    Ok(LoweringApplied {
        schema: LoweringAppliedSchema::V1,
        strategy_id: strategy.strategic_revision_id,
        expected_strategy_revision: strategy.revision,
        lowering,
        graph: LoweredGraph {
            parent_id: WorkId::parse("work.root")?,
            root: None,
            nodes: vec![work],
            coverage: vec![zap_domain::seams::ObligationAssignment {
                obligation_id: obligation.obligation_id,
                assignments: obligation.owners,
            }],
            contracts: vec![contract],
            integration_owner: WorkId::parse("work.leaf")?,
        },
        review_cause: Some(review_cause),
    })
}

#[allow(clippy::too_many_arguments)]
fn submit_second_lowering(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    data: &AgentDataIssuerHandle,
    internal: &InternalProtocolHandle,
    payload: LoweringApplied,
) -> Result<(), Box<dyn std::error::Error>> {
    let kind = EventKind::parse(LoweringApplied::KIND)?;
    let event_id = EventId::parse("event.lowering.return-two")?;
    let canonical = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?;
    let alternative_id = ChangeAlternativeId::parse("alternative.lowering.return-two")?;
    let comparison = service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            ChangeAssessmentId::parse("assessment.lowering.return-two")?,
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    EffectId::parse("effect.lowering.return-two")?,
                    0,
                    kind.clone(),
                    canonical.clone(),
                    Vec::new(),
                    event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = &comparison.alternatives()[0];
    let request = &prepared.request().effects()[0];
    let view = &prepared.view().effects[0];
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
    let assessment = build_assessment(
        "lowering.return-two",
        ChangeBaselineId::parse("baseline.return-no-change")?,
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
        "command.assessment.lowering.return-two",
    )?;
    let grant = data.authorize(&assessment_frame)?;
    service.submit(PrincipalContext::AgentData(&grant), assessment_frame)?;
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
            "command.adjudicate.lowering.return-two",
        )?,
        "economics.adjudicate:lowering.return-two",
    )?;
    let adjudicated = store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or("adjudicated lowering assessment missing")?;
    let effect = adjudicated.alternatives[0].effects[0].clone();
    let impact = ActionImpactRequest::new(
        ActionImpactRule::InitialLoweringOrSemantic {
            strategy_id: payload.strategy_id.clone(),
            target: payload.lowering.target.clone(),
        },
        vec![payload.lowering.target.clone()],
        vec![SubjectRef::Work(payload.lowering.target.clone())],
    )?;
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        ActionClass::parse("plan.lower")?,
        kind,
        event_id.clone(),
        canonical.digest(),
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
                    action: ActionClass::parse("plan.lower")?,
                    command_id: CommandId::parse("command.lowering.return-two")?,
                    impact_digest,
                    effect_item_digest: view.stable_digest,
                    effect_preflight_digest: prepared.view().digest,
                    payload_digest: canonical.digest(),
                    product_event_id: event_id,
                    exception_id: None,
                    hold_id: None,
                    final_effect: true,
                    applied_effect_ids: Vec::new(),
                    applied: false,
                    revision: Revision::new(1),
                },
            },
            store.head()?,
            "command.admission.lowering.return-two",
        )?,
        "economics.admission:lowering.return-two",
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("coordinator.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    service.submit(
        PrincipalContext::Credentialed(&coordinator),
        frame_with_basis(
            identity,
            &payload,
            store.head()?,
            BasisBinding::Exact(payload.lowering.relevant_basis),
            "command.lowering.return-two",
        )?,
    )?;
    Ok(())
}

fn assert_consumed(
    store: &RedbStore,
    bundle: &WeakBundleRecord,
    review_id: &ReviewId,
    candidate_id: &CandidateId,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = store.read(ReadAt::Current)?;
    let import = snapshot
        .get_typed::<ReturnImportRecord>(&bundle.bundle_id)?
        .ok_or("consumed return import missing")?;
    let reassessment = snapshot
        .get_typed::<ReturnReassessmentRecord>(review_id)?
        .ok_or("consumed return reassessment missing")?;
    let sidecar = snapshot
        .get_typed::<ReviewReloweringRecord>(&ReviewReloweringKey {
            review_id: review_id.clone(),
            previous_lowering_id: bundle.manifest.binding.lowering_id.clone(),
        })?
        .ok_or("consumed review sidecar missing")?;
    let current = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.return-two")?)?
        .ok_or("return lowering missing")?;
    let prior = snapshot
        .get_typed::<LoweringRecord>(&bundle.manifest.binding.lowering_id)?
        .ok_or("prior lowering history missing")?;
    let work = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or("relowered work missing")?;
    assert_eq!(import.resolution, ReturnResolutionState::ReloweringApplied);
    assert_eq!(
        import.resolved_lowering_id,
        Some(current.lowering_id.clone())
    );
    assert_eq!(reassessment.status, ReturnReassessmentStatus::Consumed);
    assert_eq!(sidecar.status, ReviewReloweringStatus::Consumed);
    assert_eq!(sidecar.consumed_by, Some(current.lowering_id.clone()));
    assert_eq!(current.state, PlanningRevisionState::Current);
    assert_eq!(prior.state, PlanningRevisionState::Superseded);
    assert_eq!(work.validation_generation, 1);
    assert_eq!(work.active_job, None);
    assert!(
        snapshot
            .get_typed::<CandidateProvenanceRecord>(candidate_id)?
            .is_some()
    );
    assert!(
        snapshot
            .get_typed::<zap_runtime::CandidateResultRecord>(candidate_id)?
            .is_some()
    );
    Ok(())
}
