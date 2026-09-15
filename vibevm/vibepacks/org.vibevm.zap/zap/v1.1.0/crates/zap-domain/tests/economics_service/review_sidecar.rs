#[test]
fn review_applied_simulation_matches_product_and_persists_relowering_sidecar()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_id = WorkId::parse("work.review-effect")?;
    let review_id = ReviewId::parse("review.effect")?;
    let lowering_id = LoweringId::parse("lowering.review-effect")?;
    let intent_id = IntentId::parse("intent.review-effect")?;
    let outcome_id = OutcomeId::parse("outcome.review-effect")?;
    let review_kind = EventKind::parse("domain.review-applied")?;
    let basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(review_kind.clone()),
        roots: vec![SubjectRef::Review(review_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let basis = RelevantBasis::new(RelevantBasisInput {
        purpose: basis_request.purpose().clone(),
        store: identity.clone(),
        observed_revision: Revision::GENESIS,
        policy: None,
        intent: None,
        outcome: None,
        subjects: Vec::new(),
        dependencies: Vec::new(),
        contracts: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        knowledge: Vec::new(),
        capacity: None,
        closure: ClosureKnowledge::Complete,
    })?;
    let work = WorkRecord {
        work_id: work_id.clone(),
        parent_id: None,
        title: BoundedText::parse("Review effect work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Ready,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("Relower after review")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    };
    let chosen = DecisionId::parse("decision.review-effect-route")?;
    let review = AdaptiveReviewRecord {
        review_id: review_id.clone(),
        previous_review_id: None,
        captured_revision: Revision::new(1),
        captured_intent_id: intent_id.clone(),
        captured_outcome_id: outcome_id.clone(),
        relevant_basis: basis.digest,
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse("Method needs replacement")?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Revalidate through a new lowering")?,
            expected_value: ValueAssessment::High,
            feasibility: ReviewFeasibility::Feasible,
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
                reason: BoundedText::parse("Require semantic relowering")?,
            }],
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation: Vec::new(),
            tradeoffs: Vec::new(),
            preserved_benefits: vec![BoundedText::parse("Keep accepted intent")?],
        },
        next_trigger: BoundedText::parse("Lower the replacement method")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    };
    let lowering = LoweringRecord {
        lowering_id: lowering_id.clone(),
        previous: None,
        strategic_revision_id: StrategicRevisionId::parse("strategy.review-effect")?,
        target: work_id.clone(),
        relevant_basis: basis.digest,
        source_captures: Vec::new(),
        work: vec![LoweredWorkBinding {
            work_id: work_id.clone(),
            parent_id: work_id.clone(),
            depends_on: Vec::new(),
            execution: LoweredNodeExecution::Container,
        }],
        obligations: Vec::new(),
        stage_debt: Vec::new(),
        deferrals: Vec::new(),
        forks: Vec::new(),
        verification: VerificationSelection {
            plans: Vec::new(),
            affected_subjects: vec![SubjectRef::Work(work_id.clone())],
            consumer_subjects: Vec::new(),
            negative_cases: Vec::new(),
            reused_evidence: Vec::new(),
            full_panel_reason: None,
            mutation_reason: None,
        },
        unresolved_horizons: Vec::new(),
        review_cause: None,
        state: PlanningRevisionState::Current,
        semantic_digest: PayloadDigest::hash(b"current-review-lowering"),
        revision: Revision::new(1),
    };
    let review_payload = ReviewApplied {
        schema: ReviewAppliedSchema::V1,
        review_id: review_id.clone(),
        expected_review_revision: review.revision,
    };
    let review_frame = frame(
        &identity,
        "command.review-effect",
        "event.review-effect",
        Revision::new(4),
        BasisBinding::Exact(basis.digest),
        Some(ChangeId::parse("change.review-effect")?),
        &review_payload,
    )?;
    let review_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &review_payload)?;
    let mut effect_subjects = vec![
        SubjectRef::Review(review_id.clone()),
        SubjectRef::Work(work_id.clone()),
    ];
    effect_subjects.sort();
    let effect = ChangeEffect {
        effect_id: EffectId::parse("effect.review-applied")?,
        index: 0,
        kind: review_kind,
        payload: review_bytes.as_bytes().to_vec(),
        payload_digest: review_bytes.digest(),
        subjects: effect_subjects.clone(),
        predecessors: Vec::new(),
        basis: basis_request.clone(),
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: review_frame.header().event_id().clone(),
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect.clone(), basis.digest)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.review-effect")?;
    assessment.change_id = ChangeId::parse("change.review-effect")?;
    assessment.affected_work_ids = vec![work_id.clone()];
    assessment.affected_subjects = effect_subjects.clone();
    assessment.scope_roots = effect_subjects;
    assessment.scope_direct_work_ids = vec![work_id.clone()];
    assessment.affected_scope_digest = None;
    assessment.adjudicated = false;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.review-no-op")?;
    no_op.kind = AlternativeKind::NoOp;
    no_op.solves_mandatory_problem = false;
    no_op.utility.overall = UtilityBand::Negligible;
    no_op.utility.owner_benefit = UtilityBand::Negligible;
    no_op.cost.expected_elapsed = Some(HoursMicros::ZERO);
    no_op.cost.elapsed_interval = exact_interval(0)?;
    no_op.cost.total_agent_hours = Some(HoursMicros::ZERO);
    no_op.cost.agent_hours_interval = exact_interval(0)?;
    no_op.no_op_basis_request = Some(no_op.effects[0].basis.clone());
    no_op.effects.clear();
    assessment.alternatives.push(no_op);
    rebind_fixture_comparison(&mut assessment)?;

    let intent = IntentRecord {
        intent_id: intent_id.clone(),
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: BoundedText::parse("Review effect intent")?,
        beneficiaries: vec![BoundedText::parse("Owner")?],
        values: vec![BoundedText::parse("Safe relowering")?],
        constraints: Vec::new(),
        source_refs: Vec::new(),
        status: LifecycleStatus::Active,
        fingerprint: PayloadDigest::hash(b"intent.review-effect"),
        owner_binding: None,
    };
    let outcome = OutcomeRecord {
        outcome_id: outcome_id.clone(),
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: intent_id.clone(),
        summary: BoundedText::parse("Review effect outcome")?,
        benefits: vec![BoundedText::parse("Current route")?],
        guarantees: vec![BoundedText::parse("Review is applied")?],
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: CompletionDutyDisposition::Required,
        promotion_disposition: CompletionDutyDisposition::Required,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    };
    let charter = CharterRecord {
        charter_id: CharterId::parse("charter.review-effect")?,
        policy_id: PolicyId::parse("policy.review-effect")?,
        campaign_id: identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: intent_id.clone(),
        intent_digest: intent.fingerprint,
        expected_outcome_id: outcome_id,
        allowed_actions: vec![ActionClass::parse("adaptive.apply")?],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::Required,
            promotion: CharterDutyAuthority::Required,
        },
        status: LifecycleStatus::Active,
        digest: PayloadDigest::hash(b"charter.review-effect"),
    };

    let seed = SeedPayload {
        charters: vec![charter],
        intents: vec![intent],
        outcomes: vec![outcome],
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: vec![assessment.clone()],
        admissions: Vec::new(),
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: vec![work],
        work_replacements: Vec::new(),
        reviews: vec![review],
        lowerings: vec![lowering],
        sources: Vec::new(),
    };
    let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedCell)?])?;
    let records = zap_domain::record_set()?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
    ])?;
    let store = RedbStore::create(root.path().join("review-effect.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal_slot = Arc::new(OnceLock::new());
    let provider = Arc::new(ChangeControlAdmissionProvider::new()?);
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(TestBootstrap {
            identity: identity.clone(),
            internal: internal_slot.clone(),
            trusted: Arc::new(OnceLock::new()),
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(provider.clone())
    .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("review internal handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.review-seed",
        "event.review-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(
            &internal.authorize(&seed_frame, OperationId::parse("operation.review-seed")?)?,
        ),
        seed_frame,
    )?;
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: None,
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.review-adjudicate",
        "event.review-adjudicate",
        Revision::new(1),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &adjudication_frame,
            OperationId::parse("operation.review-adjudicate")?,
        )?),
        adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("review assessment missing"))?;
    let effect = adjudicated.alternatives[0].effects[0].clone();
    drop(snapshot);
    let admission = ChangeAdmissionPrepared {
        admission: ChangeAdmissionRecord {
            change_id: adjudicated.change_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: assessment_digest(&adjudicated)?,
            forecast_id: None,
            forecast_digest: None,
            decision_id: None,
            alternative_id: ChangeAlternativeId::parse("alternative.service")?,
            effect_id: effect.effect_id.clone(),
            effect_index: 0,
            effect_fingerprint: effect.fingerprint()?,
            relevant_before: basis.digest,
            action: ActionClass::parse("adaptive.apply")?,
            command_id: review_frame.header().command_id().clone(),
            impact_digest: semantic_impact_digest(
                "adaptive.apply",
                &effect.kind,
                &effect.product_event_id,
                effect.payload_digest,
                basis.digest,
                Vec::new(),
                vec![SubjectRef::Review(review_id.clone())],
            )?,
            effect_item_digest: EffectItemDigest::hash(b"replaced-by-cell"),
            effect_preflight_digest: EffectPreflightDigest::hash(b"replaced-by-cell"),
            payload_digest: review_frame.payload().digest(),
            product_event_id: review_frame.header().event_id().clone(),
            exception_id: None,
            hold_id: None,
            final_effect: true,
            applied_effect_ids: Vec::new(),
            applied: false,
            revision: Revision::new(1),
        },
    };
    let admission_frame = frame(
        &identity,
        "command.review-admission",
        "event.review-admission",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &admission,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &admission_frame,
            OperationId::parse("operation.review-admission")?,
        )?),
        admission_frame,
    )?;
    let unrelated = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: Vec::new(),
        admissions: Vec::new(),
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: Vec::new(),
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let unrelated_frame = frame(
        &identity,
        "command.review-unrelated-churn",
        "event.review-unrelated-churn",
        Revision::new(3),
        BasisBinding::NotApplicable,
        None,
        &unrelated,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &unrelated_frame,
            OperationId::parse("operation.review-unrelated-churn")?,
        )?),
        unrelated_frame,
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    service.execute(PrincipalContext::Credentialed(&coordinator), review_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    assert_eq!(
        snapshot
            .get::<AdaptiveReviewRecord>(&review_id)?
            .ok_or_else(|| test_error("applied review missing"))?
            .status,
        ReviewStatus::Applied,
    );
    let changed_work = snapshot
        .get::<WorkRecord>(&work_id)?
        .ok_or_else(|| test_error("review work missing"))?;
    assert_eq!(changed_work.state, WorkState::Blocked);
    assert_eq!(changed_work.validation_generation, 0);
    let sidecar = snapshot
        .get::<ReviewReloweringRecord>(&ReviewReloweringKey {
            review_id,
            previous_lowering_id: lowering_id,
        })?
        .ok_or_else(|| test_error("review relowering sidecar missing"))?;
    assert_eq!(sidecar.status, ReviewReloweringStatus::Pending);
    assert_eq!(sidecar.revision, Revision::new(1));
    assert_eq!(sidecar.work.len(), 1);
    assert_eq!(sidecar.work[0].pre_review_revision, Revision::new(1));
    assert_eq!(sidecar.work[0].post_review_revision, Revision::new(2));
    drop(snapshot);
    audit_schema2_with_scope(
        &store,
        &cells,
        &records,
        provider.as_ref(),
        &DomainAffectedScopeProvider,
    )?;
    Ok(())
}
