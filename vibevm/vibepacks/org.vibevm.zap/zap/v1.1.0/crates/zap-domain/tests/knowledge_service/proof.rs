use tempfile::tempdir;
use zap_core::{
    ActorRef, BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput,
    CandidateProvenanceInput, CandidateProvenanceRecord, ClosureRequirement, CompletionBlocker,
    CompletionEvaluator, ContextRequirement, EffectState, ExecutionState, HistoryMutationKind,
    OperationRef, PrincipalRole, ProducerRef, ReadAt, SafeState, StateReader, TransactionStore,
    ValidationGeneration, WorkExecutionObservationRecord,
};
use zap_domain::acceptance::{
    CandidateReviewRecord, EvidenceAdjudicated, EvidenceAdjudicatedSchema, StageAccepted,
    StageAcceptedSchema,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::intent::OutcomeRecord;
use zap_domain::knowledge::{
    ApplicabilityAssessed, ApplicabilityAssessedSchema, ClosureAssessed, ClosureAssessedSchema,
    ClosureStatus, DependencyRecorded, DependencyRecordedSchema, DependencyRelation,
    DomainBasisProvider, EpistemicStatus, FactAcceptanceStatus, FactAdjudicated,
    FactAdjudicatedSchema, FactOrigin, FactRecord, KnowledgeEdgeId, KnowledgeEndpoint,
    SourceApplicabilityRecord, SourceApplicabilityStatus, SourceCaptureInput, SourceKind,
    SourceRecaptured, SourceRecapturedSchema, SourceRecord, SourceScope, record_source,
};
use zap_domain::seams::{
    CompletionDutyDisposition, DeliveryRoute, EvidenceApplicability, EvidenceDisposition,
    EvidenceObservation, EvidenceResult, LifecycleStatus, MaturityStage, ObligationDisposition,
    ObligationOwner, ObligationStatus, OwnershipRole, SourceCapture, TaskContract,
    VerificationMethod, WorkKind, WorkState, WorkType,
};
use zap_domain::viewer_queries::{
    ViewerDetail, ViewerHistoricalValue, ViewerInput, ViewerNodeId, ViewerResult,
};
use zap_wire::{
    ArtifactDigest, AttemptId, BasisBinding, BoundedText, CandidateId, CanonicalEncode,
    CanonicalPayload, CodecEpoch, CompletionProviderId, ContractDigest, ContractId, EventKind,
    EvidenceId, JobId, ObligationId, ObservationRef, OutcomeId, PacketDigest, PacketId,
    PayloadDigest, PrincipalId, QueryId, RelevantBasisDigest, Revision, SourceDigest, SourceId,
    StageAcceptanceId, SubjectRef, VerificationId, WorkId,
};

use super::support::*;

include!("proof/scenario.rs");
pub(super) fn current_basis(
    harness: &Harness,
    request: BasisRequest,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    Ok(DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}

fn mutation_basis(
    harness: &Harness,
    kind: &str,
    roots: Vec<SubjectRef>,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    current_basis(
        harness,
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
            roots,
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
    )
}

fn completion_has_missing(
    harness: &Harness,
    evidence_id: &EvidenceId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let evaluator = CompletionEvaluator::new(
        zap_domain::completion_provider_set()?,
        vec![CompletionProviderId::parse("zap.domain")?],
    )?;
    Ok(evaluator.view(&snapshot)?.blockers.iter().any(
        |blocker| matches!(blocker, CompletionBlocker::MissingFinalGate(id) if id == evidence_id),
    ))
}

struct ReviewedCandidateInput<'a> {
    id: &'a str,
    work: &'a WorkRecord,
    contract: &'a TaskContractRecord,
    outcome: &'a OutcomeRecord,
    source: &'a SourceRecord,
    artifact: ArtifactDigest,
    producer_basis: RelevantBasisDigest,
}

fn reviewed_candidate(
    harness: &Harness,
    input: ReviewedCandidateInput<'_>,
) -> Result<
    (
        CandidateProvenanceInput,
        CandidateReviewRecord,
        WorkExecutionObservationRecord,
    ),
    Box<dyn std::error::Error>,
> {
    let ReviewedCandidateInput {
        id,
        work,
        contract,
        outcome,
        source,
        artifact,
        producer_basis,
    } = input;
    let candidate_id = CandidateId::parse(id)?;
    let attempt_id = AttemptId::parse(&format!("attempt-{id}"))?;
    let job_id = JobId::parse(&format!("job-{id}"))?;
    let packet_id = PacketId::parse(&format!("packet-{id}"))?;
    let mut subjects = vec![
        SubjectRef::Work(work.work_id.clone()),
        SubjectRef::Source(source.source_id.clone()),
    ];
    subjects.sort();
    let input = CandidateProvenanceInput {
        candidate_id: candidate_id.clone(),
        producer: ProducerRef {
            actor: ActorRef {
                principal_id: PrincipalId::parse(&format!("worker-{id}"))?,
                operation: OperationRef::Attempt(attempt_id.clone()),
                role: PrincipalRole::Worker,
            },
            job_id: job_id.clone(),
            attempt_id: attempt_id.clone(),
            packet_id: packet_id.clone(),
        },
        subjects: subjects.clone(),
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        relevant_basis: producer_basis,
        artifacts: vec![artifact],
        observation: ObservationRef::parse(&format!("observation-{id}"))?,
        revision: Revision::new(1),
    };
    let provenance = CandidateProvenanceRecord::new(input.clone())?;
    let applicability_basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::CandidateReview(candidate_id.clone()),
        roots: vec![
            SubjectRef::Outcome(outcome.outcome_id.clone()),
            SubjectRef::Work(work.work_id.clone()),
            SubjectRef::Contract(contract.contract_id.clone()),
            SubjectRef::Source(source.source_id.clone()),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let applicability_basis = current_basis(harness, applicability_basis_request.clone())?;
    let encoded = provenance.encode_canonical(CodecEpoch::CURRENT)?;
    let review = CandidateReviewRecord {
        candidate_id,
        work_id: work.work_id.clone(),
        job_id: job_id.clone(),
        attempt_id: attempt_id.clone(),
        packet_id,
        packet_digest: PacketDigest::hash(format!("packet-{id}").as_bytes()),
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version,
        contract_digest: contract.contract_digest,
        validation_generation: work.validation_generation,
        producer_basis,
        provenance_digest: PayloadDigest::hash(encoded.as_bytes()),
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        rule_sources: Vec::new(),
        applicability_basis_request,
        applicability_basis,
        revision: Revision::new(1),
    };
    let job = WorkExecutionObservationRecord {
        job_id,
        attempt_id,
        work_id: work.work_id.clone(),
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        validation_generation: ValidationGeneration::new(work.validation_generation)?,
        subjects,
        execution: ExecutionState::Succeeded,
        effect: EffectState::NotStarted,
        safe_state: SafeState::Completed,
        revision: Revision::new(1),
    };
    Ok((input, review, job))
}

fn stage_payload(
    id: &str,
    candidate_id: &str,
    work: &WorkRecord,
    obligation: &ObligationRecord,
    outcome: &OutcomeRecord,
    evidence_id: &EvidenceId,
) -> Result<StageAccepted, zap_wire::ZapError> {
    Ok(StageAccepted {
        schema: StageAcceptedSchema::V1,
        stage_acceptance_id: StageAcceptanceId::parse(id)?,
        candidate_id: CandidateId::parse(candidate_id)?,
        work_id: work.work_id.clone(),
        stage: MaturityStage::Functional,
        outcome_id: outcome.outcome_id.clone(),
        evidence_ids: vec![evidence_id.clone()],
        obligation_ids: vec![obligation.obligation_id.clone()],
        scope: BoundedText::parse("Current proof acceptance")?,
        summary: BoundedText::parse("Accepted current evidence")?,
    })
}

pub(super) fn work(id: &str, depends_on: Vec<WorkId>) -> Result<WorkRecord, zap_wire::ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(id)?,
        kind: WorkKind::Atom,
        work_type: WorkType::Verification,
        state: WorkState::Ready,
        order: 1,
        depends_on,
        acceptance: vec![BoundedText::parse("verified")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}

pub(super) fn scoped_source(
    id: &str,
    work_id: &WorkId,
) -> Result<SourceRecord, zap_wire::ZapError> {
    record_source(&SourceCaptureInput {
        source_id: SourceId::parse(id)?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse(id)?,
        content_digest: SourceDigest::hash(id.as_bytes()),
        byte_len: id.len() as u64,
        scope: SourceScope::Subjects(vec![SubjectRef::Work(work_id.clone())]),
        observation: ObservationRef::parse(&format!("observation-{id}"))?,
    })
}

pub(super) fn outcome() -> Result<OutcomeRecord, zap_wire::ZapError> {
    Ok(OutcomeRecord {
        outcome_id: OutcomeId::parse("outcome-proof")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: zap_wire::IntentId::parse("intent-proof")?,
        summary: BoundedText::parse("Proof outcome")?,
        benefits: vec![BoundedText::parse("Correct proof")?],
        guarantees: vec![BoundedText::parse("Scoped evidence")?],
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: CompletionDutyDisposition::Required,
        promotion_disposition: CompletionDutyDisposition::Required,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    })
}

pub(super) fn obligation(
    id: &str,
    work_id: &WorkId,
) -> Result<ObligationRecord, zap_wire::ZapError> {
    Ok(ObligationRecord {
        obligation_id: ObligationId::parse(id)?,
        created_for_outcome: OutcomeId::parse("outcome-proof")?,
        current_outcomes: vec![OutcomeId::parse("outcome-proof")?],
        statement: BoundedText::parse(id)?,
        essential: false,
        owners: vec![ObligationOwner {
            work_id: work_id.clone(),
            role: OwnershipRole::Implementation,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    })
}

pub(super) fn contract(
    id: &str,
    work_id: &WorkId,
    source_id: &SourceId,
    obligation_id: &ObligationId,
) -> Result<TaskContractRecord, zap_wire::ZapError> {
    let contract_id = ContractId::parse(id)?;
    let digest = ContractDigest::hash(id.as_bytes());
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: digest,
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: BoundedText::parse(id)?,
            goal: BoundedText::parse("Verify scoped proof")?,
            read_subjects: vec![SubjectRef::Source(source_id.clone())],
            write_subjects: vec![SubjectRef::Work(work_id.clone())],
            resources: Vec::new(),
            steps: vec![BoundedText::parse("check")?],
            positive_cases: vec![BoundedText::parse("passes")?],
            negative_cases: vec![BoundedText::parse("fails")?],
            checks: vec![method(work_id)?],
            acceptance: vec![BoundedText::parse("accepted")?],
            safe_stop: BoundedText::parse("stop")?,
            integration_owner: work_id.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: vec![source_id.clone()],
            obligation_ids: vec![obligation_id.clone()],
        },
    })
}

fn applicable(source: &SourceRecord) -> Result<SourceApplicabilityRecord, zap_wire::ZapError> {
    Ok(SourceApplicabilityRecord {
        source_id: source.source_id.clone(),
        source_digest: source.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: source.scope.clone(),
        evidence_refs: vec![EvidenceId::parse("evidence-seeded-applicability")?],
        closure_status: ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"seeded-applicability"),
        revision: Revision::new(1),
    })
}

fn method(work_id: &WorkId) -> Result<VerificationMethod, zap_wire::ZapError> {
    Ok(VerificationMethod {
        argv: vec![BoundedText::parse("verify")?],
        target: BoundedText::parse("target")?,
        toolchain: BoundedText::parse("toolchain")?,
        environment: BoundedText::parse("environment")?,
        subjects: vec![SubjectRef::Work(work_id.clone())],
        cases: vec![BoundedText::parse("case")?],
    })
}

fn viewer_query(
    snapshot: &dyn zap_core::QuerySnapshot,
    id: &str,
    input: ViewerInput,
) -> Result<ViewerResult, zap_wire::ZapError> {
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &input)?;
    let page = zap_domain::query_set()?.execute(&QueryId::parse(id)?, snapshot, &payload)?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
        .decode_json()
}
