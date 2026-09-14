fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store.economics")?,
        campaign_id: CampaignId::parse("campaign.economics")?,
        base_id: BaseId::parse("base.economics")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn audit_schema2(
    store: &RedbStore,
    cells: &CellSet,
    records: &RecordSet,
    provider: &ChangeControlAdmissionProvider,
) -> Result<zap_store::AuditReport, ZapError> {
    audit_schema2_with_scope(store, cells, records, provider, &TestAffectedScopeProvider)
}

fn audit_schema2_with_scope(
    store: &RedbStore,
    cells: &CellSet,
    records: &RecordSet,
    provider: &ChangeControlAdmissionProvider,
    scope: &dyn AffectedScopeProvider,
) -> Result<zap_store::AuditReport, ZapError> {
    let impact = DomainActionImpactProvider;
    let basis = FixedBasisProvider;
    let jobs = EmptyAffectedJobs;
    let context = ReplayContext::new(
        cells,
        cells,
        records,
        ReplayProviders {
            schema1_admission: None,
            action_impact: Some(&impact),
            action_admission: Some(provider),
            basis: Some(&basis),
            affected_scope: Some(scope),
            affected_jobs: Some(&jobs),
            packet_resolution: None,
            dispatch_eligibility: None,
        },
    )?;
    store.audit_with_replay_context(cells, &context)
}

fn frame<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    command_id: &str,
    event_id: &str,
    revision: Revision,
    basis: BasisBinding,
    change: Option<ChangeId>,
    payload: &P,
) -> Result<CanonicalCommandFrame, ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: identity.store_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        base_id: identity.base_id.clone(),
        command_id: CommandId::parse(command_id)?,
        event_id: EventId::parse(event_id)?,
        expected_revision: revision,
        kind: EventKind::parse(P::KIND)?,
        causes: Vec::new(),
        basis,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("Focused economics service fixture")?,
        evidence: Vec::new(),
        decision: None,
        change,
    })?;
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?;
    CanonicalCommandFrame::new(header, reason, payload)
}

fn semantic_impact_digest(
    action: &str,
    kind: &EventKind,
    event_id: &EventId,
    payload_digest: PayloadDigest,
    basis: RelevantBasisDigest,
    work_ids: Vec<WorkId>,
    subjects: Vec<SubjectRef>,
) -> Result<ActionImpactDigest, ZapError> {
    let request = ActionImpactRequest::new(ActionImpactRule::SemanticChange, work_ids, subjects)?;
    Ok(ActionImpactView::new(
        request.request_digest(),
        ActionClass::parse(action)?,
        kind.clone(),
        event_id.clone(),
        payload_digest,
        Revision::GENESIS,
        ActionImpactClass::SemanticChange,
        Some(basis),
    )?
    .digest)
}

fn exact_interval(value: u64) -> Result<HoursInterval, ZapError> {
    HoursInterval::new(HoursMicros::new(value), Some(HoursMicros::new(value)))
}

fn fixture_assessment(
    effect: ChangeEffect,
    _basis: RelevantBasisDigest,
) -> Result<ChangeAssessmentRecord, ZapError> {
    let zero = exact_interval(0)?;
    let categories = CostCategoryKind::ALL
        .into_iter()
        .map(|category| {
            Ok(CostCategory {
                category,
                applicability: Applicability::Included,
                agent_hours: zero,
                elapsed: zero,
                consequence: ConsequenceBand::Negligible,
                basis: BoundedText::parse("No additional category cost in fixture")?,
                evidence_refs: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    let policy = ChangePolicyRecord::default_policy()?;
    let assessment_id = ChangeAssessmentId::parse("assessment.service")?;
    let comparison_basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::ChangeAssessment(assessment_id.clone()),
        roots: effect.basis.roots().to_vec(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let comparison_basis_digest = RelevantBasis::new(RelevantBasisInput {
        purpose: comparison_basis_request.purpose().clone(),
        store: identity()?,
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
    })?
    .digest;
    Ok(ChangeAssessmentRecord {
        assessment_id,
        change_id: ChangeId::parse("change.service")?,
        baseline_id: ChangeBaselineId::parse("baseline.service")?,
        summary: BoundedText::parse("Exact automatic product effect")?,
        necessity: ChangeNecessity {
            class: NecessityClass::OptionalImprovement,
            obligation_ids: Vec::new(),
            constraint_refs: Vec::new(),
            problem: BoundedText::parse("Exercise atomic admission")?,
            basis: BoundedText::parse("Focused service fixture")?,
            evidence_refs: Vec::new(),
        },
        affected_work_ids: vec![WorkId::parse("work.service")?],
        dependent_work_ids: Vec::new(),
        affected_subjects: vec![SubjectRef::Work(WorkId::parse("work.service")?)],
        scope_roots: vec![SubjectRef::Work(WorkId::parse("work.service")?)],
        scope_direct_work_ids: vec![WorkId::parse("work.service")?],
        affected_scope_digest: None,
        unknown_impact: Vec::new(),
        comparison_basis_request,
        comparison_basis_digest,
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.revision,
        policy_digest: policy.digest()?,
        team_model: TeamCapacityModel {
            model_id: BoundedText::parse("team.service")?,
            profile_digest: PayloadDigest::hash(b"team-service"),
            executor_classes: vec![ExecutorCapacity {
                class_id: BoundedText::parse("middle")?,
                capability_ids: vec![BoundedText::parse("rust")?],
                nominal_capacity: 1,
            }],
            nominal_parallelism: 1,
            resource_capacities: Vec::new(),
            scheduling_assumptions: vec![BoundedText::parse("One executor")?],
            evidence_refs: Vec::new(),
        },
        alternatives: vec![ChangeAlternative {
            alternative_id: ChangeAlternativeId::parse("alternative.service")?,
            kind: AlternativeKind::Proposal,
            summary: BoundedText::parse("Apply product payload")?,
            solves_mandatory_problem: true,
            preserved_obligations: Vec::new(),
            sacrificed_obligations: Vec::new(),
            utility: UtilityAssessment {
                overall: UtilityBand::High,
                owner_benefit: UtilityBand::High,
                risk_reduction: UtilityBand::Moderate,
                urgency: UtilityBand::Moderate,
                strategic_optionality: UtilityBand::Moderate,
                reversibility: UtilityBand::High,
                confidence: ConfidenceBand::High,
                basis: BoundedText::parse("High-value fixture")?,
                evidence_refs: Vec::new(),
            },
            cost: IncrementalCost {
                expected_elapsed: Some(HoursMicros::new(1_000_000)),
                elapsed_interval: exact_interval(1_000_000)?,
                expected_passive_wait: Some(HoursMicros::ZERO),
                passive_wait_interval: zero,
                total_agent_hours: Some(HoursMicros::new(1_000_000)),
                agent_hours_interval: exact_interval(1_000_000)?,
                precision: CostPrecision::BoundedEstimate,
                consequence: ConsequenceBand::Negligible,
                categories,
                unknowns: Vec::new(),
                excluded_costs: Vec::new(),
                attribution_summary: BoundedText::parse("One incremental product effect")?,
            },
            feasibility: Feasibility::Feasible,
            effects: vec![effect],
            no_op_basis_request: None,
            basis: BoundedText::parse("Exact current basis")?,
            evidence_refs: Vec::new(),
        }],
        recommended_alternative_id: Some(ChangeAlternativeId::parse("alternative.service")?),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::Automatic,
        comparison_reasons: vec![BoundedText::parse("High-value bounded effect")?],
        estimation: EstimationUsage {
            elapsed: HoursMicros::new(100_000),
            agent_hours: HoursMicros::new(100_000),
            stopped_because: EstimationStop::Sufficient,
            assumptions: vec![BoundedText::parse("Stable fixture")?],
            evidence_refs: Vec::new(),
        },
        hold_id: None,
        adjudicated: true,
        resolved: false,
        revision: Revision::new(1),
    })
}

fn rebind_fixture_comparison(assessment: &mut ChangeAssessmentRecord) -> Result<(), ZapError> {
    let mut roots = assessment
        .alternatives
        .iter()
        .filter(|alternative| alternative.feasibility == Feasibility::Feasible)
        .flat_map(|alternative| &alternative.effects)
        .flat_map(|effect| effect.basis.roots().iter().cloned())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::ChangeAssessment(assessment.assessment_id.clone()),
        roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    for alternative in &mut assessment.alternatives {
        if alternative.kind == AlternativeKind::NoOp {
            alternative.no_op_basis_request = Some(request.clone());
        }
    }
    assessment.comparison_basis_digest = RelevantBasis::new(RelevantBasisInput {
        purpose: request.purpose().clone(),
        store: identity()?,
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
    })?
    .digest;
    assessment.comparison_basis_request = request;
    Ok(())
}

fn initialize_domain_indexes(store: &RedbStore) -> Result<(), ZapError> {
    let mut families = zap_domain::viewer_graph_index_families()?;
    families.extend(zap_core::affected_job_index_families()?);
    families.sort();
    families.dedup();
    let mut algorithms = zap_domain::viewer_index_algorithms()?;
    algorithms.extend(zap_core::affected_job_index_algorithms()?);
    algorithms.sort();
    store.rebuild_indexes_v2(families, algorithms, Revision::GENESIS)?;
    Ok(())
}

fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        REQ,
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
