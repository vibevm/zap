#[test]
fn expensive_scope_add_uses_exact_owner_pause_and_effect_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dream-live-add.redb"))?;
    prepare_initial_lowering(&harness)?;
    let baseline_id = establish_baseline(&harness, "live-add")?;
    let assessment_id = ChangeAssessmentId::parse("assessment.dream-live-add")?;
    let draft = live_add_draft("dream.live-add", assessment_id.clone())?;
    harness.data(
        &DreamScopeChangeRequested {
            schema: DreamSchema::V1,
            operation: DreamOperation::Add,
            draft,
        },
        BasisBinding::NotApplicable,
        "command-dream-live-add-request",
    )?;
    let branch = dream(&harness, "dream.live-add")?;
    harness.owner(
        &DreamGrillDeclined {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        "command-dream-live-add-grill-decline",
    )?;
    let branch = dream(&harness, "dream.live-add")?;
    harness.internal(
        &DreamRecalculated {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-live-add-recalculate",
    )?;
    let projection = dream_view(&harness, &branch.dream_id)?.current_projection;
    assert!(projection.promotable);
    assert_eq!(projection.value.added_goal_count, 1);
    assert_eq!(projection.value.bounded_unknown_count, 1);
    let product = DreamApplied {
        schema: DreamSchema::V1,
        dream_id: branch.dream_id.clone(),
        expected_dream_revision: branch.revision,
        assessment_id: assessment_id.clone(),
        combined_charter: None,
        projection: DreamProjectionSeal::from(&projection),
    };
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &product)?;
    let alternative_id = ChangeAlternativeId::parse("alternative.dream-live-add")?;
    let effect_id = EffectId::parse("effect.dream-live-add")?;
    let product_event_id = EventId::parse("event:command-dream-live-add-apply")?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            assessment_id.clone(),
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    effect_id.clone(),
                    0,
                    EventKind::parse(DreamApplied::KIND)?,
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
        .ok_or_else(|| test_error("prepared dream effect missing"))?;
    let request = prepared
        .request()
        .effects()
        .first()
        .ok_or_else(|| test_error("prepared dream item missing"))?;
    let effect = change_effect(prepared, 0)?;
    let scope = derived_scope(
        prepared
            .affected_scope(0)
            .ok_or_else(|| test_error("dream affected scope missing"))?,
    );
    let mut assessment = assessment(
        "dream-live-add",
        baseline_id,
        alternative_id.clone(),
        effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    let six_hours = HoursMicros::new(6_000_000);
    assessment.alternatives[0].cost.expected_elapsed = Some(six_hours);
    assessment.alternatives[0].cost.elapsed_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    assessment.alternatives[0].cost.total_agent_hours = Some(six_hours);
    assessment.alternatives[0].cost.agent_hours_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    assessment.alternatives[0].cost.unknowns = vec![CostUnknown {
        unknown_id: text("dream-bounded-fog")?,
        category: CostCategoryKind::FogUncertainty,
        question: text("How much optional diagnostic refinement remains?")?,
        lower_bound: HoursMicros::ZERO,
        upper_bound: Some(HoursMicros::new(1_000_000)),
        material: true,
        resolution_action: text("Bound it during the selected implementation")?,
        evidence_refs: Vec::new(),
    }];
    assessment.comparison_reasons = vec![text(&format!(
        "Exact Dream projection {} carries measured consequences and bounded fog",
        projection.digest
    ))?];
    assessment.alternatives[0].basis = text(&format!(
        "Exact Dream projection {} and immutable effect payload",
        projection.digest
    ))?;
    harness.data(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-live-add-assessment",
    )?;
    let proposed = load_assessment(&harness, &assessment_id)?;
    assert_eq!(
        proposed.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    assert_eq!(proposed.recommendation, Recommendation::TakeProposal);
    assert_eq!(proposed.alternatives[0].cost.unknowns.len(), 1);
    let hold_id = HoldId::parse("hold.dream-live-add")?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment_id.clone(),
            hold_id: Some(hold_id.clone()),
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-live-add-adjudicate",
    )?;
    let adjudicated = load_assessment(&harness, &assessment_id)?;
    let selected = adjudicated.alternatives[0].clone();
    let selected_effect = selected.effects[0].clone();
    let decision_id = DecisionId::parse("decision.dream-live-add")?;
    harness.owner(
        &ChangeDecisionRecorded {
            decision: OwnerChangeDecisionRecord {
                decision_id: decision_id.clone(),
                assessment_id: assessment_id.clone(),
                assessment_digest: assessment_digest(&adjudicated)?,
                forecast_id: None,
                forecast_digest: None,
                policy_id: adjudicated.policy_id.clone(),
                policy_revision: adjudicated.policy_revision,
                recommended_alternative_id: selected.alternative_id.clone(),
                choice: OwnerChangeChoice::Approve,
                reason: text("Approve this exact six-hour Dream scope envelope")?,
                effect_fingerprints: vec![selected_effect.fingerprint()?],
                effect_preflight_digests: vec![
                    selected_effect
                        .preflight_digest
                        .ok_or_else(|| test_error("adjudicated Dream item digest missing"))?,
                ],
                revision: harness.store.head()?.checked_next()?,
            },
        },
        "command-dream-live-add-decision",
    )?;
    let pause_id = PauseId::parse("pause.dream-live-add")?;
    let pause_digest = PayloadDigest::hash(b"pause-dream-live-add");
    let charter = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<zap_domain::intent::CharterRecord>(&CharterId::parse("charter.one")?)?
        .ok_or_else(|| test_error("active charter missing"))?;
    harness.owner(
        &CampaignPaused {
            pause: PauseRecord {
                pause_id: pause_id.clone(),
                campaign_id: harness.identity.campaign_id.clone(),
                scope: PauseScope::Campaign(harness.identity.campaign_id.clone()),
                source: PauseSource::Owner,
                reason: text("Owner stop dominates Dream application")?,
                charter_revision: charter.revision,
                status: PauseStatus::Active,
                state_digest: pause_digest,
                revision: harness.store.head()?.checked_next()?,
            },
        },
        "command-dream-live-add-pause",
    )?;

    let paused_frame = support::frame(
        &harness.identity,
        &product,
        harness.store.head()?.checked_next()?,
        BasisBinding::Exact(selected_effect.relevant_before),
        "command-dream-live-add-apply",
    )?;
    prepare_admission(
        &harness,
        &adjudicated,
        &selected,
        &selected_effect,
        prepared,
        request,
        Some(decision_id.clone()),
        Some(hold_id.clone()),
        paused_frame.header().command_id().clone(),
        paused_frame.header().event_id().clone(),
        "command-dream-live-add-admission-paused",
    )?;
    assert_eq!(
        harness
            .execute_privileged(paused_frame)
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Paused)
    );
    assert_eq!(
        dream(&harness, "dream.live-add")?.status,
        DreamStatus::Ready
    );
    harness.owner(
        &PauseResumed {
            pause_id,
            expected_state_digest: pause_digest,
        },
        "command-dream-live-add-resume",
    )?;
    let product_frame = support::frame(
        &harness.identity,
        &product,
        harness.store.head()?.checked_next()?,
        BasisBinding::Exact(selected_effect.relevant_before),
        "command-dream-live-add-apply",
    )?;
    prepare_admission(
        &harness,
        &adjudicated,
        &selected,
        &selected_effect,
        prepared,
        request,
        Some(decision_id),
        Some(hold_id.clone()),
        product_frame.header().command_id().clone(),
        product_frame.header().event_id().clone(),
        "command-dream-live-add-admission-rebound",
    )?;
    assert_eq!(
        harness
            .execute_privileged(product_frame.clone())?
            .disposition(),
        CommitDisposition::Committed
    );
    assert_eq!(
        harness.execute_privileged(product_frame)?.disposition(),
        CommitDisposition::ExactRetry
    );
    let snapshot = harness.store.read(ReadAt::Current)?;
    let applied = snapshot
        .get_typed::<DreamApplicationRecord>(&branch.dream_id)?
        .ok_or_else(|| test_error("dream application record missing"))?;
    let strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or_else(|| test_error("applied strategy missing"))?;
    assert_eq!(
        dream(&harness, "dream.live-add")?.status,
        DreamStatus::Applied
    );
    let added_work_id = WorkId::parse("work.dream-live-add")?;
    assert!(
        strategy
            .nodes
            .iter()
            .any(|node| node.work_id == added_work_id)
    );
    assert_eq!(applied.applied_strategy_digest, strategy.semantic_digest);
    assert_eq!(applied.projection.projection_digest, projection.digest);
    drop(snapshot);
    harness.internal(
        &ChangeHoldResolved {
            hold_id: hold_id.clone(),
            forecast_id: None,
            forecast_digest: None,
            resolution: HoldResolution::ApprovedApplied,
            applied_effect_ids: vec![effect_id],
            safe_job_ids: Vec::new(),
        },
        BasisBinding::NotApplicable,
        "command-dream-live-add-hold-resolve",
    )?;
    assert_eq!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<ChangeHoldRecord>(&hold_id)?
            .ok_or_else(|| test_error("resolved Dream hold missing"))?
            .status,
        HoldStatus::Released
    );
    Ok(())
}

fn live_add_draft(
    dream_id: &str,
    assessment_id: ChangeAssessmentId,
) -> Result<DreamDraft, ZapError> {
    Ok(DreamDraft {
        dream_id: DreamId::parse(dream_id)?,
        base_strategic_revision: StrategicRevisionId::parse("strategy.one")?,
        summary: text("Add one exact valuable subgoal")?,
        attachment: DreamAttachmentRequest::Exact {
            attachment: DreamAttachment::Subgoal {
                parent_work_id: WorkId::parse("work.root")?,
            },
        },
        delta: DreamDelta {
            operations: vec![DreamDeltaOperation::Add(DreamAdd {
                node: StrategicNode {
                    work_id: WorkId::parse("work.dream-live-add")?,
                    title: text("Deliver the selected Dream subgoal")?,
                    obligation_ids: vec![ObligationId::parse("obligation.one")?],
                    depends_on: vec![WorkId::parse("work.root")?],
                    refinement_trigger: text("Lower after exact Dream admission")?,
                },
            })],
        },
        assumptions: vec![DreamAssumption {
            assumption_id: AssumptionId::parse("assumption.dream-live-add")?,
            statement: text("The subgoal adds owner value within the current outcome")?,
            state: AssumptionState::Supported,
            source_ids: vec![SourceId::parse("source.one")?],
        }],
        unknowns: vec![DreamUnknown {
            subject: SubjectRef::Resource(ResourceId::parse("resource.dream-live-add")?),
            question: text("How much bounded refinement remains?")?,
            resolution_action: text("Carry it into the exact economics estimate")?,
            disposition: DreamUnknownDisposition::BoundedForEconomics,
        }],
        alternatives: vec![DreamAlternative {
            alternative_id: ChangeAlternativeId::parse("alternative.dream-live-add")?,
            summary: text("Add and lower the exact subgoal")?,
            expected_value: text("High owner value")?,
            expected_cost: text("Six hours to verified result")?,
            factual_basis: vec![SourceId::parse("source.one")?],
        }],
        estimate: Some(assessment_id),
        required_charter_change: None,
    })
}

fn load_assessment(
    harness: &Harness,
    id: &ChangeAssessmentId,
) -> Result<ChangeAssessmentRecord, ZapError> {
    harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(id)?
        .ok_or_else(|| test_error("Dream assessment missing"))
}

fn change_effect(prepared: &PreparedEffectBundle, index: usize) -> Result<ChangeEffect, ZapError> {
    let request = prepared
        .request()
        .effects()
        .get(index)
        .ok_or_else(|| test_error("prepared Dream effect missing"))?;
    Ok(ChangeEffect {
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
    })
}

fn derived_scope(view: &AffectedScopeView) -> DerivedAffectedScope {
    DerivedAffectedScope {
        request_digest: view.request_digest,
        observed_revision: view.observed_revision,
        affected_work_ids: view.affected_work_ids.clone(),
        dependent_work_ids: view.dependent_work_ids.clone(),
        subjects: view.subjects.clone(),
        unknown_boundary: view.unknown_boundary.clone(),
        completeness: view.completeness,
        relevant_basis: view.relevant_basis,
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_admission(
    harness: &Harness,
    assessment: &ChangeAssessmentRecord,
    alternative: &ChangeAlternative,
    effect: &ChangeEffect,
    prepared: &PreparedEffectBundle,
    request: &EffectPreflightRequest,
    decision_id: Option<DecisionId>,
    hold_id: Option<HoldId>,
    command_id: CommandId,
    event_id: EventId,
    admission_command: &str,
) -> Result<(), ZapError> {
    let impact = ActionImpactRequest::new(
        ActionImpactRule::SemanticChange,
        effect
            .subjects
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
        effect.subjects.clone(),
    )?;
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        ActionClass::parse("plan.lower")?,
        effect.kind.clone(),
        event_id.clone(),
        effect.payload_digest,
        harness.store.head()?,
        ActionImpactClass::SemanticChange,
        Some(effect.relevant_before),
    )?
    .digest;
    harness.internal(
        &ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                change_id: assessment.change_id.clone(),
                assessment_id: assessment.assessment_id.clone(),
                assessment_digest: assessment_digest(assessment)?,
                forecast_id: None,
                forecast_digest: None,
                decision_id,
                alternative_id: alternative.alternative_id.clone(),
                effect_id: effect.effect_id.clone(),
                effect_index: effect.index,
                effect_fingerprint: effect.fingerprint()?,
                relevant_before: effect.relevant_before,
                action: ActionClass::parse("plan.lower")?,
                command_id,
                impact_digest,
                effect_item_digest: request
                    .effect_id()
                    .eq(&effect.effect_id)
                    .then_some(prepared.view().effects[0].stable_digest)
                    .ok_or_else(|| test_error("Dream item mismatch"))?,
                effect_preflight_digest: prepared.view().digest,
                payload_digest: effect.payload_digest,
                product_event_id: event_id,
                exception_id: None,
                hold_id,
                final_effect: true,
                applied_effect_ids: Vec::new(),
                applied: false,
                revision: Revision::new(1),
            },
        },
        BasisBinding::NotApplicable,
        admission_command,
    )?;
    Ok(())
}
