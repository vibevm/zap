use super::change_admission_milestone_support::{MilestoneSeed, milestone_seed};
use super::*;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::economics::*;
use zap_domain::seams::{
    DeliveryRoute, DomainMutation, MaturityStage, TaskContract, WorkKind, WorkState, WorkType,
};
use zap_store::RedbStore;

const SEED_KIND: &str = "test.change-admission-seed";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Seed {
    charter: CharterRecord,
    baseline: ChangeBaselineRecord,
    policy: ChangePolicyRecord,
    work: Vec<WorkRecord>,
    contract: TaskContractRecord,
    milestones: MilestoneSeed,
}

impl CanonicalEncode for Seed {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for Seed {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for Seed {
    const KIND: &'static str = SEED_KIND;
}

struct SeedCell;

impl TransitionCell for SeedCell {
    type Payload = Seed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut records = vec![
            RecordFamily::parse(CharterRecord::FAMILY)?,
            RecordFamily::parse(ChangeBaselineRecord::FAMILY)?,
            RecordFamily::parse(ChangePolicyRecord::FAMILY)?,
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(TaskContractRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::intent::OutcomeRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::control::ObligationRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::knowledge::SourceRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::lowering::StrategicPlanRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::milestones::MilestoneRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::milestones::MilestoneRevisionRecord::FAMILY)?,
            RecordFamily::parse(
                zap_domain::milestone_planning::MilestonePlanProposalRecord::FAMILY,
            )?,
            RecordFamily::parse(zap_domain::milestone_planning::MilestonePlanStateRecord::FAMILY)?,
        ];
        records.sort();
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(SEED_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_indexes: zap_domain::viewer_index_families_for_records(&records)?,
            affected_records: records,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        changes.insert(command.payload().charter.clone())?;
        changes.insert(command.payload().baseline.clone())?;
        changes.insert(command.payload().policy.clone())?;
        for work in &command.payload().work {
            changes.insert(work.clone())?;
        }
        changes.insert(command.payload().contract.clone())?;
        command.payload().milestones.insert(changes)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

struct SeedTrust {
    identity: StoreIdentity,
    handle: Arc<OnceLock<InternalProtocolHandle>>,
}

impl TrustBootstrapSource for SeedTrust {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal.change-admission-seed")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(SEED_KIND)?]),
        })?;
        self.handle
            .set(handle)
            .map_err(|_| ZapError::unsupported_operation())
    }
}

pub(super) fn seed(path: &std::path::Path, identity: &StoreIdentity) -> Result<(), ZapError> {
    seed_with_profile(path, identity, false)
}

pub(super) fn seed_with_profile(
    path: &std::path::Path,
    identity: &StoreIdentity,
    current_strategy: bool,
) -> Result<(), ZapError> {
    let records = zap_domain::record_set()?;
    let store = RedbStore::create(path, identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let mut index_families = zap_domain::viewer_graph_index_families()?;
    index_families.extend(zap_core::affected_job_index_families()?);
    index_families.extend(zap_runtime::runtime_index_families()?);
    index_families.sort();
    index_families.dedup();
    let mut index_algorithms = zap_domain::viewer_index_algorithms()?;
    index_algorithms.extend(zap_core::affected_job_index_algorithms()?);
    index_algorithms.extend(zap_runtime::runtime_index_algorithms()?);
    index_algorithms.sort();
    store.rebuild_indexes_v2(index_families, index_algorithms, Revision::GENESIS)?;
    let cells = CellSet::single(SeedCell)?;
    let routes = RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal);
    let handle = Arc::new(OnceLock::new());
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(SeedTrust {
            identity: identity.clone(),
            handle: handle.clone(),
        }),
    )
    .cells(cells)
    .records(records)
    .routes(routes)
    .build()?;
    let policy = ChangePolicyRecord::default_policy()?;
    let charter_digest = PayloadDigest::hash(b"charter.http-ready");
    let payload = Seed {
        charter: CharterRecord {
            charter_id: CharterId::parse("charter.http-ready")?,
            policy_id: policy.policy_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            revision: Revision::new(1),
            parent_digest: None,
            intent_id: IntentId::parse("intent.http-ready")?,
            intent_digest: PayloadDigest::hash(b"intent.http-ready"),
            expected_outcome_id: OutcomeId::parse("outcome.http-ready")?,
            allowed_actions: vec![ActionClass::parse("plan.lower")?],
            mutable_obligations: Vec::new(),
            essential_obligations: Vec::new(),
            allowed_dispositions: vec![ObligationDisposition::Retained],
            completion_duty_policy: CompletionDutyPolicy {
                final_gate: CharterDutyAuthority::NoDutyAllowed,
                promotion: CharterDutyAuthority::NoDutyAllowed,
            },
            status: LifecycleStatus::Active,
            digest: charter_digest,
        },
        baseline: ChangeBaselineRecord {
            baseline_id: ChangeBaselineId::parse("baseline.http-ready")?,
            base_digest: BaseDigest::hash(b"base.http-ready"),
            committed_prefix_digest: PayloadDigest::hash(b"prefix.http-ready"),
            committed_sequence: Revision::GENESIS,
            active_charter_digest: charter_digest,
            active_intent_id: IntentId::parse("intent.http-ready")?,
            active_outcome_id: OutcomeId::parse("outcome.http-ready")?,
            active_outcome_digest: PayloadDigest::hash(b"outcome.http-ready"),
            observed_plan_digest: PayloadDigest::hash(b"plan.http-ready"),
            change_policy_revision: policy.revision,
            revision: Revision::new(1),
        },
        policy,
        work: vec![
            WorkRecord {
                work_id: WorkId::parse("work.change-admission")?,
                parent_id: None,
                title: BoundedText::parse("Change admission work")?,
                kind: WorkKind::Atom,
                work_type: WorkType::Change,
                state: WorkState::Ready,
                order: 1,
                depends_on: Vec::new(),
                acceptance: vec![BoundedText::parse("May be dropped")?],
                required_stage: MaturityStage::Functional,
                validation_generation: 0,
                active_job: None,
                revision: Revision::new(1),
            },
            WorkRecord {
                work_id: WorkId::parse("work.change-admission-successor")?,
                parent_id: None,
                title: BoundedText::parse("Successor work")?,
                kind: WorkKind::Atom,
                work_type: WorkType::Change,
                state: WorkState::Planned,
                order: 2,
                depends_on: Vec::new(),
                acceptance: vec![BoundedText::parse("Succeeds retired work")?],
                required_stage: MaturityStage::Functional,
                validation_generation: 0,
                active_job: None,
                revision: Revision::new(1),
            },
        ],
        contract: resource_contract()?,
        milestones: milestone_seed(current_strategy)?,
    };
    let frame = CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse("command.change-admission-seed")?,
            event_id: EventId::parse("event.change-admission-seed")?,
            expected_revision: Revision::GENESIS,
            kind: EventKind::parse(SEED_KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Seed change admission HTTP fixture")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?,
    )?;
    let permit = handle
        .get()
        .ok_or_else(ZapError::unsupported_operation)?
        .authorize(
            &frame,
            OperationId::parse("operation.change-admission-seed")?,
        )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), frame)?;
    drop(service);
    drop(store);
    Ok(())
}

fn resource_contract() -> Result<TaskContractRecord, ZapError> {
    let work_id = WorkId::parse("work.change-admission")?;
    let contract_id = ContractId::parse("contract.change-admission")?;
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: ContractDigest::hash(b"contract.change-admission"),
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: BoundedText::parse("Execute the synthetic plan change")?,
            goal: BoundedText::parse("Exercise public change admission")?,
            read_subjects: Vec::new(),
            write_subjects: vec![SubjectRef::Work(work_id.clone())],
            resources: vec![ResourceId::parse("resource.quicklens-fixture")?],
            steps: vec![BoundedText::parse("Prepare and admit the selected effect")?],
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: vec![BoundedText::parse("The selected effect is admitted")?],
            safe_stop: BoundedText::parse("No product effect has started")?,
            integration_owner: work_id,
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: vec![SourceId::parse("source.http-ready")?],
            obligation_ids: vec![ObligationId::parse("obligation.http-ready")?],
        },
    })
}

pub(super) fn assessment(
    name: &str,
    prepared: &zap_api::PreparedEffectComparisonView,
    scope: &AffectedScopeView,
    cost: HoursMicros,
) -> Result<ChangeAssessmentRecord, ZapError> {
    let bundle = &prepared.alternatives[0];
    let items = &bundle.request.effects;
    let mut scope_roots = items
        .iter()
        .flat_map(|item| item.declared_subjects.iter().cloned())
        .collect::<Vec<_>>();
    scope_roots.sort();
    scope_roots.dedup();
    let zero = HoursInterval::new(HoursMicros::ZERO, Some(HoursMicros::ZERO))?;
    let categories = CostCategoryKind::ALL
        .into_iter()
        .map(|category| {
            Ok(CostCategory {
                category,
                applicability: Applicability::Included,
                agent_hours: zero,
                elapsed: zero,
                consequence: ConsequenceBand::Negligible,
                basis: BoundedText::parse("No extra fixture cost")?,
                evidence_refs: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    let policy = ChangePolicyRecord::default_policy()?;
    Ok(ChangeAssessmentRecord {
        assessment_id: ChangeAssessmentId::parse(&format!("assessment.{name}"))?,
        change_id: ChangeId::parse(&format!("change.{name}"))?,
        baseline_id: ChangeBaselineId::parse("baseline.http-ready")?,
        summary: BoundedText::parse("Drop one fixture work item")?,
        necessity: ChangeNecessity {
            class: NecessityClass::OptionalImprovement,
            obligation_ids: Vec::new(),
            constraint_refs: Vec::new(),
            problem: BoundedText::parse("Exercise public orchestration")?,
            basis: BoundedText::parse("Generated fixture")?,
            evidence_refs: Vec::new(),
        },
        affected_work_ids: scope.affected_work_ids.clone(),
        dependent_work_ids: scope.dependent_work_ids.clone(),
        affected_subjects: scope.subjects.clone(),
        scope_roots: scope_roots.clone(),
        scope_direct_work_ids: scope_roots
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
        affected_scope_digest: None,
        unknown_impact: scope.unknown_boundary.clone(),
        comparison_basis_request: prepared.basis_request.clone(),
        comparison_basis_digest: prepared.relevant_basis,
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.revision,
        policy_digest: policy.digest()?,
        team_model: TeamCapacityModel {
            model_id: BoundedText::parse("team.http-ready")?,
            profile_digest: PayloadDigest::hash(b"team.http-ready"),
            executor_classes: vec![ExecutorCapacity {
                class_id: BoundedText::parse("test")?,
                capability_ids: vec![BoundedText::parse("rust")?],
                nominal_capacity: 1,
            }],
            nominal_parallelism: 1,
            resource_capacities: Vec::new(),
            scheduling_assumptions: vec![BoundedText::parse("One test executor")?],
            evidence_refs: Vec::new(),
        },
        alternatives: vec![
            ChangeAlternative {
                alternative_id: bundle.request.alternative_id.clone(),
                kind: AlternativeKind::Proposal,
                summary: BoundedText::parse("Drop work")?,
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
                    expected_elapsed: Some(cost),
                    elapsed_interval: HoursInterval::new(cost, Some(cost))?,
                    expected_passive_wait: Some(HoursMicros::ZERO),
                    passive_wait_interval: zero,
                    total_agent_hours: Some(cost),
                    agent_hours_interval: HoursInterval::new(cost, Some(cost))?,
                    precision: CostPrecision::BoundedEstimate,
                    consequence: ConsequenceBand::Negligible,
                    categories: categories.clone(),
                    unknowns: Vec::new(),
                    excluded_costs: Vec::new(),
                    attribution_summary: BoundedText::parse("One product effect")?,
                },
                feasibility: Feasibility::Feasible,
                effects: items
                    .iter()
                    .map(|item| ChangeEffect {
                        effect_id: item.effect_id.clone(),
                        index: item.index,
                        kind: item.kind.clone(),
                        payload: item.payload.canonical_json.clone(),
                        payload_digest: PayloadDigest::hash(&item.payload.canonical_json),
                        subjects: item.declared_subjects.clone(),
                        predecessors: item.predecessors.clone(),
                        basis: item.basis.clone(),
                        relevant_before: item.relevant_before,
                        relevant_after: item.declared_relevant_after,
                        product_event_id: item.product_event_id.clone(),
                        preflight_digest: None,
                    })
                    .collect(),
                no_op_basis_request: None,
                basis: BoundedText::parse("Exact prepared effect")?,
                evidence_refs: Vec::new(),
            },
            ChangeAlternative {
                alternative_id: ChangeAlternativeId::parse(&format!("alternative.{name}-no-op"))?,
                kind: AlternativeKind::NoOp,
                summary: BoundedText::parse("Keep current work state")?,
                solves_mandatory_problem: false,
                preserved_obligations: Vec::new(),
                sacrificed_obligations: Vec::new(),
                utility: UtilityAssessment {
                    overall: UtilityBand::Low,
                    owner_benefit: UtilityBand::Low,
                    risk_reduction: UtilityBand::Low,
                    urgency: UtilityBand::Low,
                    strategic_optionality: UtilityBand::Low,
                    reversibility: UtilityBand::High,
                    confidence: ConfidenceBand::High,
                    basis: BoundedText::parse("No change")?,
                    evidence_refs: Vec::new(),
                },
                cost: IncrementalCost {
                    expected_elapsed: Some(HoursMicros::ZERO),
                    elapsed_interval: zero,
                    expected_passive_wait: Some(HoursMicros::ZERO),
                    passive_wait_interval: zero,
                    total_agent_hours: Some(HoursMicros::ZERO),
                    agent_hours_interval: zero,
                    precision: CostPrecision::BoundedEstimate,
                    consequence: ConsequenceBand::Negligible,
                    categories,
                    unknowns: Vec::new(),
                    excluded_costs: Vec::new(),
                    attribution_summary: BoundedText::parse("No-op")?,
                },
                feasibility: Feasibility::Feasible,
                effects: Vec::new(),
                no_op_basis_request: Some(prepared.basis_request.clone()),
                basis: BoundedText::parse("Current state remains")?,
                evidence_refs: Vec::new(),
            },
        ],
        recommended_alternative_id: Some(bundle.request.alternative_id.clone()),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::Automatic,
        comparison_reasons: vec![BoundedText::parse("Bounded automatic fixture")?],
        estimation: EstimationUsage {
            elapsed: HoursMicros::new(100_000),
            agent_hours: HoursMicros::new(100_000),
            stopped_because: EstimationStop::Sufficient,
            assumptions: vec![BoundedText::parse("Stable fixture")?],
            evidence_refs: Vec::new(),
        },
        hold_id: None,
        adjudicated: false,
        resolved: false,
        revision: Revision::new(1),
    })
}

pub(super) fn read_assessment(
    address: std::net::SocketAddr,
    id: &ChangeAssessmentId,
) -> Result<ChangeAssessmentRecord, Box<dyn std::error::Error>> {
    read_record(address, id)
}

pub(super) fn read_plan_state(
    address: std::net::SocketAddr,
    id: &OutcomeId,
) -> Result<zap_domain::milestone_planning::MilestonePlanStateRecord, Box<dyn std::error::Error>> {
    read_record(address, id)
}

fn read_record<T, K>(
    address: std::net::SocketAddr,
    key: &K,
) -> Result<T, Box<dyn std::error::Error>>
where
    T: CanonicalDecode + StoredRecord,
    K: RecordKey,
{
    let response = super::send(
        address,
        "POST",
        "/v1/prepare/projected-record",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareProjectedRecord {
            request: zap_api::PrepareProjectedRecordRequest {
                at: zap_api::PreparationRead::Current,
                actor: None,
                draft: zap_api::EffectBundleDraftInput {
                    alternative_id: ChangeAlternativeId::parse("alternative.http-record-read")?,
                    committed_prefix: Vec::new(),
                    effects: Vec::new(),
                    no_op_basis: Some(BasisRequest::new(BasisRequestInput {
                        purpose: BasisPurpose::Completion,
                        roots: Vec::new(),
                        policy: ContextRequirement::NotApplicable,
                        capacity: ContextRequirement::NotApplicable,
                        closure: ClosureRequirement::KnownGraph,
                    })?),
                },
                record: zap_api::ProjectedRecordSelector {
                    family: RecordFamily::parse(T::FAMILY)?,
                    key: key.encode_key()?,
                },
            },
        })?,
    )?;
    if super::status(&response)? != 200 {
        return Err(String::from_utf8_lossy(super::body(&response)?)
            .into_owned()
            .into());
    }
    let MachineResponse::ProjectedRecord(view) = serde_json::from_slice(super::body(&response)?)?
    else {
        return Err("wrong projected-record response".into());
    };
    let bytes = view.canonical_value.ok_or("projected record is missing")?;
    Ok(T::decode_canonical(
        &CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?,
    )?)
}
