#[test]
fn exact_default_threshold_and_total_optional_matrix() -> Result<(), zap_wire::ZapError> {
    let policy = ChangePolicyRecord::default_policy()?;
    let below = evaluate_assessment(
        &assessment(
            Some(3_900_000),
            UtilityBand::High,
            NecessityClass::OptionalImprovement,
        )?,
        &policy,
    )?;
    let exact = evaluate_assessment(
        &assessment(
            Some(4_000_000),
            UtilityBand::High,
            NecessityClass::OptionalImprovement,
        )?,
        &policy,
    )?;
    let above = evaluate_assessment(
        &assessment(
            Some(4_000_001),
            UtilityBand::High,
            NecessityClass::OptionalImprovement,
        )?,
        &policy,
    )?;
    assert_eq!(below.admission, AdmissionDisposition::Automatic);
    assert_eq!(exact.admission, AdmissionDisposition::Automatic);
    assert_eq!(above.recommendation, Recommendation::TakeProposal);
    assert_eq!(above.admission, AdmissionDisposition::OwnerDecisionRequired);
    assert!(above.creates_hold);

    let low_value = evaluate_assessment(
        &assessment(
            Some(6_000_000),
            UtilityBand::Low,
            NecessityClass::OptionalImprovement,
        )?,
        &policy,
    )?;
    assert_eq!(low_value.recommendation, Recommendation::ContinueBaseline);
    assert_eq!(low_value.admission, AdmissionDisposition::Blocked);
    assert!(!low_value.creates_hold);

    let unknown = evaluate_assessment(
        &assessment(None, UtilityBand::High, NecessityClass::OptionalImprovement)?,
        &policy,
    )?;
    assert_eq!(unknown.recommendation, Recommendation::TakeProposal);
    assert_eq!(
        unknown.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );

    let mandatory = evaluate_assessment(
        &assessment(
            Some(6_000_000),
            UtilityBand::Moderate,
            NecessityClass::MandatoryProblem,
        )?,
        &policy,
    )?;
    assert_eq!(mandatory.recommendation, Recommendation::TakeProposal);
    assert_eq!(
        mandatory.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );

    let mut with_cheaper = assessment(
        Some(6_000_000),
        UtilityBand::Moderate,
        NecessityClass::MandatoryProblem,
    )?;
    let mut cheaper = alternative(
        "alternative.cheaper",
        AlternativeKind::CheaperAlternative,
        Some(3_000_000),
        UtilityBand::Moderate,
        true,
    )?;
    cheaper.preserved_obligations = vec![ObligationId::parse("obligation.one")?];
    with_cheaper.alternatives.push(cheaper);
    let preferred = evaluate_assessment(&with_cheaper, &policy)?;
    assert_eq!(preferred.recommendation, Recommendation::PreferAlternative);
    assert_eq!(
        preferred.recommended_alternative_id,
        Some(ChangeAlternativeId::parse("alternative.cheaper")?),
    );
    assert_eq!(preferred.admission, AdmissionDisposition::Automatic);
    Ok(())
}

#[test]
fn cumulative_forecast_requires_exact_actual_plus_remaining() -> Result<(), zap_wire::ZapError> {
    let totals = |elapsed: u64| -> Result<ForecastTotals, zap_wire::ZapError> {
        Ok(ForecastTotals {
            elapsed: Some(HoursMicros::new(elapsed)),
            elapsed_interval: interval(elapsed)?,
            passive_wait: Some(HoursMicros::ZERO),
            passive_wait_interval: interval(0)?,
            agent_hours: Some(HoursMicros::new(elapsed)),
            agent_hours_interval: interval(elapsed)?,
        })
    };
    let mut forecast = CostForecastRecord {
        forecast_id: zap_wire::CostForecastId::parse("forecast.one")?,
        assessment_id: ChangeAssessmentId::parse("assessment.one")?,
        previous_forecast_id: None,
        trigger: ForecastTrigger::EffectCompleted,
        original_baseline_id: ChangeBaselineId::parse("baseline.one")?,
        completed_effect_ids: vec![EffectId::parse("effect.0")?],
        team_model_digest: PayloadDigest::hash(b"team"),
        cumulative_actual: totals(2_000_000)?,
        remaining_estimate: totals(3_000_000)?,
        total_to_verified: totals(5_000_000)?,
        relevant_basis: RelevantBasisDigest::hash(b"basis"),
        unknowns: Vec::new(),
        evidence_refs: Vec::new(),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::OwnerDecisionRequired,
        hold_id: Some(HoldId::parse("hold.one")?),
        adjudicated: true,
        revision: Revision::new(1),
    };
    validate_forecast(&forecast)?;
    forecast.total_to_verified = totals(3_000_000)?;
    assert!(validate_forecast(&forecast).is_err());
    Ok(())
}

#[test]
fn forecast_unknown_handling_honors_each_active_policy_mode() -> Result<(), zap_wire::ZapError> {
    let known = |value: u64| -> Result<ForecastTotals, zap_wire::ZapError> {
        Ok(ForecastTotals {
            elapsed: Some(HoursMicros::new(value)),
            elapsed_interval: interval(value)?,
            passive_wait: Some(HoursMicros::ZERO),
            passive_wait_interval: interval(0)?,
            agent_hours: Some(HoursMicros::new(value)),
            agent_hours_interval: interval(value)?,
        })
    };
    let mut forecast = CostForecastRecord {
        forecast_id: zap_wire::CostForecastId::parse("forecast.policy")?,
        assessment_id: ChangeAssessmentId::parse("assessment.one")?,
        previous_forecast_id: None,
        trigger: ForecastTrigger::EstimateCorrected,
        original_baseline_id: ChangeBaselineId::parse("baseline.one")?,
        completed_effect_ids: Vec::new(),
        team_model_digest: PayloadDigest::hash(b"team"),
        cumulative_actual: known(1_000_000)?,
        remaining_estimate: known(2_000_000)?,
        total_to_verified: known(3_000_000)?,
        relevant_basis: RelevantBasisDigest::hash(b"basis"),
        unknowns: Vec::new(),
        evidence_refs: Vec::new(),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::Automatic,
        hold_id: None,
        adjudicated: false,
        revision: Revision::new(1),
    };
    forecast.remaining_estimate.elapsed = None;
    forecast.remaining_estimate.elapsed_interval = HoursInterval::new(
        HoursMicros::new(1_000_000),
        Some(HoursMicros::new(2_000_000)),
    )?;
    forecast.total_to_verified.elapsed = None;
    forecast.total_to_verified.elapsed_interval = HoursInterval::new(
        HoursMicros::new(2_000_000),
        Some(HoursMicros::new(3_000_000)),
    )?;
    validate_forecast(&forecast)?;

    let mut policy = ChangePolicyRecord::default_policy()?;
    policy.unknown_cost = UnknownCostHandling::OwnerIfThresholdPossible;
    assert!(forecast_requires_owner(&forecast, &policy));
    policy.unknown_cost = UnknownCostHandling::OwnerIfExpectedUnknown;
    assert!(forecast_requires_owner(&forecast, &policy));
    policy.unknown_cost = UnknownCostHandling::OwnerIfUnbounded;
    assert!(!forecast_requires_owner(&forecast, &policy));

    forecast.total_to_verified.elapsed = Some(HoursMicros::new(3_000_000));
    forecast.total_to_verified.elapsed_interval = HoursInterval::new(
        HoursMicros::new(2_000_000),
        Some(HoursMicros::new(6_000_000)),
    )?;
    policy.unknown_cost = UnknownCostHandling::OwnerIfThresholdPossible;
    assert!(forecast_requires_owner(&forecast, &policy));
    policy.unknown_cost = UnknownCostHandling::OwnerIfExpectedUnknown;
    assert!(!forecast_requires_owner(&forecast, &policy));
    policy.unknown_cost = UnknownCostHandling::OwnerIfUnbounded;
    assert!(!forecast_requires_owner(&forecast, &policy));
    Ok(())
}

struct TestErased<R: StoredRecord> {
    descriptor: RecordDescriptor,
    value: R,
}

impl<R: StoredRecord> ErasedRecord for TestErased<R> {
    fn descriptor(&self) -> &RecordDescriptor {
        &self.descriptor
    }
    fn as_any(&self) -> &dyn Any {
        &self.value
    }
    fn key_bytes(&self) -> Result<Vec<u8>, zap_wire::ZapError> {
        self.value.key().encode_key()
    }
    fn version_bytes(&self) -> Vec<u8> {
        self.value.version().encode_version()
    }
    fn value_bytes(&self) -> Result<Vec<u8>, zap_wire::ZapError> {
        Ok(self
            .value
            .encode_canonical(CodecEpoch::CURRENT)?
            .as_bytes()
            .to_vec())
    }
    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, zap_wire::ZapError> {
        self.value.index_rows()
    }
}

struct TestState {
    identity: StoreIdentity,
    records: Vec<Arc<dyn ErasedRecord>>,
}

impl TestState {
    fn new() -> Result<Self, zap_wire::ZapError> {
        Ok(Self {
            identity: StoreIdentity {
                store_id: StoreId::parse("store.test")?,
                campaign_id: CampaignId::parse("campaign.test")?,
                base_id: BaseId::parse("base.test")?,
                store_epoch: StoreEpoch::ZAP2,
                codec_epoch: CodecEpoch::CURRENT,
                reducer_epoch: ReducerEpoch::new(1)?,
            },
            records: Vec::new(),
        })
    }
    fn with<R: StoredRecord>(mut self, value: R) -> Result<Self, zap_wire::ZapError> {
        self.records.push(Arc::new(TestErased {
            descriptor: R::descriptor()?,
            value,
        }));
        Ok(self)
    }
}

impl StateReader for TestState {
    fn identity(&self) -> StoreIdentity {
        self.identity.clone()
    }
    fn revision(&self) -> Revision {
        Revision::new(10)
    }
    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, zap_wire::ZapError> {
        Ok(self
            .records
            .iter()
            .find(|row| {
                row.descriptor().family == *family
                    && row.key_bytes().is_ok_and(|bytes| bytes == key.as_bytes())
            })
            .cloned())
    }
    fn scan_erased(
        &self,
        family: &RecordFamily,
        _range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, zap_wire::ZapError> {
        let items = self
            .records
            .iter()
            .filter(|row| row.descriptor().family == *family)
            .take(limit.get() as usize)
            .cloned()
            .collect();
        Ok(ErasedRecordPage {
            items,
            completeness: RecordCompleteness::Complete,
            last_key: None,
        })
    }
}

#[test]
fn shared_economics_provider_blocks_live_decision_hold_and_unknown_effect()
-> Result<(), zap_wire::ZapError> {
    let mut assessment = assessment(
        Some(6_000_000),
        UtilityBand::High,
        NecessityClass::OptionalImprovement,
    )?;
    assessment.recommended_alternative_id =
        Some(ChangeAlternativeId::parse("alternative.proposal")?);
    assessment.recommendation = Recommendation::TakeProposal;
    assessment.admission = AdmissionDisposition::OwnerDecisionRequired;
    assessment.hold_id = Some(HoldId::parse("hold.one")?);
    assessment.adjudicated = true;
    let hold = ChangeHoldRecord {
        hold_id: HoldId::parse("hold.one")?,
        assessment_id: assessment.assessment_id.clone(),
        forecast_id: None,
        policy_id: PolicyId::parse("change-policy:default")?,
        status: HoldStatus::Active,
        affected_work_ids: Vec::new(),
        dependent_work_ids: Vec::new(),
        subject_ids: Vec::new(),
        scope_roots: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
        scope_direct_work_ids: vec![WorkId::parse("work.fixture")?],
        affected_scope_digest: zap_wire::AffectedScopeDigest::hash(b"scope.one"),
        unknown_boundary: Vec::new(),
        closure_complete: true,
        hold_all_starts: false,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: zap_core::SafeJobValidationMode::ExactScope,
        held_jobs: Vec::new(),
        unknown_effect_ids: vec![EffectId::parse("external.effect")?],
        independence_basis: assessment.comparison_basis_digest,
        decision_id: None,
        revision: Revision::new(2),
    };
    let state = TestState::new()?
        .with(assessment.clone())?
        .with(hold.clone())?;
    let blockers = economics_blockers(&state)?;
    assert!(blockers.contains(&CompletionBlocker::PendingOwnerDecision(
        assessment.change_id.clone()
    )));
    assert!(blockers.contains(&CompletionBlocker::PendingSelectedChange(
        assessment.change_id.clone()
    )));
    assert!(blockers.contains(&CompletionBlocker::ActiveHold(HoldId::parse("hold.one")?)));
    assert!(
        blockers.contains(&CompletionBlocker::UnknownExternalEffect(EffectId::parse(
            "external.effect"
        )?))
    );
    let evaluator = CompletionEvaluator::new(
        zap_domain::economics_completion_provider_set()?,
        vec![zap_wire::CompletionProviderId::parse("zap.economics")?],
    )?;
    let shared_view = evaluator.view(&state)?;
    assert!(!shared_view.eligible);
    assert!(
        shared_view
            .blockers
            .contains(&CompletionBlocker::ActiveHold(HoldId::parse("hold.one")?))
    );

    let rejected = OwnerChangeDecisionRecord {
        decision_id: DecisionId::parse("decision.reject")?,
        assessment_id: assessment.assessment_id.clone(),
        assessment_digest: assessment_digest(&assessment)?,
        forecast_id: None,
        forecast_digest: None,
        policy_id: PolicyId::parse("change-policy:default")?,
        policy_revision: Revision::new(1),
        recommended_alternative_id: ChangeAlternativeId::parse("alternative.proposal")?,
        choice: OwnerChangeChoice::Reject,
        reason: text("Continue the approved baseline")?,
        effect_fingerprints: Vec::new(),
        effect_preflight_digests: Vec::new(),
        revision: Revision::new(3),
    };
    let mut released = hold;
    released.status = HoldStatus::Released;
    let resolved = TestState::new()?
        .with(assessment)?
        .with(released)?
        .with(rejected)?;
    assert!(economics_blockers(&resolved)?.is_empty());
    Ok(())
}

#[test]
fn owner_control_routes_and_unknown_stop_inputs_are_explicit() -> Result<(), zap_wire::ZapError> {
    let cells = zap_domain::cell_set()?;
    let paused = cells
        .descriptor(&EventKind::parse("control.campaign-paused")?)
        .ok_or_else(|| {
            zap_wire::ZapError::from_static(
                zap_wire::ErrorCode::MissingReference,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#WHOLE-CAMPAIGN-STOP",
                "campaign pause cell is missing",
                zap_wire::FixSurface::Configuration,
                zap_wire::ErrorDetail::None,
            )
        })?;
    assert_eq!(
        paused.route(),
        &zap_wire::RouteClass::OwnerControl(zap_wire::ControlClass::CampaignStop),
    );
    assert_eq!(
        cells
            .descriptor(&EventKind::parse("control.pause-resumed")?)
            .map(zap_core::CellDescriptor::route),
        Some(&zap_wire::RouteClass::OwnerControl(
            zap_wire::ControlClass::PauseResume,
        )),
    );
    assert_eq!(
        cells
            .descriptor(&EventKind::parse("control.stop-rule-triggered")?)
            .map(zap_core::CellDescriptor::route),
        Some(&zap_wire::RouteClass::ServiceInternal),
    );
    assert_eq!(
        cells
            .descriptor(&EventKind::parse(
                "economics.change-assessment-adjudicated",
            )?)
            .map(zap_core::CellDescriptor::route),
        Some(&zap_wire::RouteClass::ServiceInternal),
    );

    let problem = zap_wire::ProblemId::parse("problem.one")?;
    let rule = StopRuleExpression::FailedApproachesAtLeast {
        problem_id: problem.clone(),
        count: 2,
    };
    let unknown = StopFacts {
        failed_approaches: BTreeMap::new(),
        active_holds: Some(0),
        unknown_effects: Some(0),
        present_evidence: BTreeSet::new(),
    };
    assert_eq!(
        evaluate_stop_rule(&rule, &unknown)
            .err()
            .map(|error| error.code),
        Some(zap_wire::ErrorCode::NeedsEvidence),
    );
    let mut known = unknown;
    known.failed_approaches.insert(problem, 2);
    assert_eq!(evaluate_stop_rule(&rule, &known)?, RuleResult::Triggered);
    Ok(())
}
