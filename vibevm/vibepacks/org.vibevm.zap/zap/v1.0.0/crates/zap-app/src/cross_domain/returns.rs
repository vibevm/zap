specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
);

use specmark::spec;
use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    AffectedScopeRequest, AffectedScopeView, BasisProvider, BasisPurpose, BasisRequest,
    BasisRequestInput, CandidateProvenanceRecord, ClosureRequirement, CollectionState,
    ContextRequirement, StateReader, StateReaderExt,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::lowering::{
    CounterDisposition, EncounterKind, EncounterRecord, FailedApproachKey, FailedApproachRecord,
    ReturnBundleInput, ReturnClassification, ReturnImportRecord, ReturnResolutionState,
    WeakBundleRecord, validate_return_bundle,
};
use zap_domain::owner_control::ApproachEpochRecord;
use zap_domain::seams::WorkState;
use zap_runtime::{AcceptanceState, CandidateResultRecord, RuntimeJobRecord};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ProblemId, Revision, SubjectRef, ZapError};

const RETURN_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN";

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#return-import")]
pub struct ResolvedReturnImport {
    pub encounters: Vec<EncounterRecord>,
    pub failed_approaches: Vec<FailedApproachRecord>,
    pub counter_updates: Vec<(Revision, ApproachEpochRecord)>,
    pub import: ReturnImportRecord,
}

/// ```
/// use zap_app::ReturnResolutionProvider;
/// use zap_core::StateReader;
/// fn affected(provider: &dyn ReturnResolutionProvider, state: &dyn StateReader, input: &zap_domain::lowering::ReturnBundleInput) -> Result<zap_core::AffectedScopeRequest, zap_wire::ZapError> {
///     provider.affected_request(state, input)
/// }
/// ```
pub trait ReturnResolutionProvider: Send + Sync + 'static {
    fn affected_request(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
    ) -> Result<AffectedScopeRequest, ZapError>;

    fn resolve(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
        affected: &AffectedScopeView,
    ) -> Result<ResolvedReturnImport, ZapError>;
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#return-import")]
pub struct ApplicationReturnResolutionProvider;

impl ReturnResolutionProvider for ApplicationReturnResolutionProvider {
    fn affected_request(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
    ) -> Result<AffectedScopeRequest, ZapError> {
        let bundle = ready_bundle(state, input)?;
        let mut work_ids = Vec::new();
        let mut roots = Vec::new();
        let lowering = state
            .get_typed::<zap_domain::lowering::LoweringRecord>(
                &bundle.manifest.binding.lowering_id,
            )?
            .ok_or_else(|| return_missing("return lowering is missing"))?;
        roots.push(SubjectRef::Work(lowering.target));
        for attempt in &bundle.manifest.attempts {
            let job = state
                .get_typed::<RuntimeJobRecord>(&attempt.job_id)?
                .ok_or_else(|| return_missing("return attempt job is missing"))?;
            work_ids.push(job.work_id.clone());
            roots.push(SubjectRef::Work(job.work_id));
            roots.extend(job.read_subjects);
            roots.extend(job.write_subjects);
        }
        roots.extend(
            input
                .delta
                .encounters
                .iter()
                .map(|row| SubjectRef::Job(row.job_id.clone())),
        );
        work_ids.sort();
        work_ids.dedup();
        roots.sort();
        roots.dedup();
        AffectedScopeRequest::new(roots, work_ids)
    }

    fn resolve(
        &self,
        state: &dyn StateReader,
        input: &ReturnBundleInput,
        affected: &AffectedScopeView,
    ) -> Result<ResolvedReturnImport, ZapError> {
        let bundle = ready_bundle(state, input)?;
        let request = self.affected_request(state, input)?;
        if affected.request_digest != request.request_digest()
            || affected.observed_revision != state.revision()
            || !affected.unknown_boundary.is_empty()
        {
            return Err(return_conflict(
                "return affected closure is stale or incomplete",
            ));
        }
        let attempts = bundle
            .manifest
            .attempts
            .iter()
            .map(|attempt| (attempt.job_id.clone(), attempt))
            .collect::<BTreeMap<_, _>>();
        let plan_current = current_plan(state, &bundle)?;
        let mut classifications = Vec::new();
        let mut records = Vec::new();
        let mut failed = Vec::new();
        let mut counters: BTreeMap<ProblemId, (Revision, ApproachEpochRecord)> = BTreeMap::new();
        for encounter in &input.delta.encounters {
            let attempt = attempts
                .get(&encounter.job_id)
                .ok_or_else(|| return_conflict("encounter job is outside bundle attempts"))?;
            if encounter.attempt_id != attempt.attempt_id
                || encounter.packet_id != attempt.packet_id
                || encounter.producer != attempt.producer
            {
                return Err(return_conflict("encounter attempt or producer is foreign"));
            }
            let classification = match encounter.kind {
                EncounterKind::CandidateProduced => {
                    classify_candidate(state, encounter, attempt, plan_current)?
                }
                EncounterKind::Contradiction => ReturnClassification::Contradiction,
                EncounterKind::Failure => ReturnClassification::Failure,
                EncounterKind::MissingCapability | EncounterKind::UnresolvedQuestion
                    if encounter.effect_state == zap_core::CandidateEffectState::Unknown =>
                {
                    ReturnClassification::NewUnknown
                }
                EncounterKind::AttemptStarted
                | EncounterKind::ForkSelected
                | EncounterKind::Observation
                    if plan_current =>
                {
                    ReturnClassification::ApplicableObservation
                }
                _ => ReturnClassification::StaleReviewInput,
            };
            classifications.push((encounter.encounter_id.clone(), classification.clone()));
            match state.get_typed::<EncounterRecord>(&encounter.encounter_id)? {
                Some(existing)
                    if existing.source_bundle_id == input.source_bundle_id
                        && existing.encounter == *encounter => {}
                Some(_) => return Err(return_conflict("encounter identity was reused")),
                None => records.push(EncounterRecord {
                    encounter_id: encounter.encounter_id.clone(),
                    source_bundle_id: input.source_bundle_id.clone(),
                    delta_digest: input.delta.digest,
                    sequence: encounter.sequence,
                    encounter: encounter.clone(),
                    classification,
                    revision: Revision::new(1),
                }),
            }
            if encounter.kind == EncounterKind::Failure {
                let approach = encounter
                    .approach
                    .as_ref()
                    .ok_or_else(|| return_conflict("failure lacks approach identity"))?;
                let key = FailedApproachKey {
                    problem_id: approach.problem_id.clone(),
                    epoch: approach.epoch,
                    approach_digest: approach.approach_digest,
                };
                if state.get_typed::<FailedApproachRecord>(&key)?.is_some()
                    || failed
                        .iter()
                        .any(|row: &FailedApproachRecord| row.key == key)
                {
                    continue;
                }
                let epoch = state.get_typed::<ApproachEpochRecord>(&approach.problem_id)?;
                let disposition = if epoch
                    .as_ref()
                    .is_some_and(|row| row.epoch == approach.epoch)
                {
                    let current =
                        epoch.ok_or_else(|| return_missing("approach epoch disappeared"))?;
                    let mut updated = counters
                        .get(&current.problem_id)
                        .map_or_else(|| current.clone(), |(_, row)| row.clone());
                    updated.failed_approaches = updated
                        .failed_approaches
                        .checked_add(1)
                        .ok_or_else(|| return_conflict("approach counter overflowed"))?;
                    updated.revision = updated.revision.checked_next()?;
                    counters.insert(current.problem_id.clone(), (current.revision, updated));
                    CounterDisposition::Counted
                } else {
                    CounterDisposition::PendingOwnerEpoch
                };
                failed.push(FailedApproachRecord {
                    key,
                    source_bundle_id: input.source_bundle_id.clone(),
                    encounter_id: encounter.encounter_id.clone(),
                    attempt_id: encounter.attempt_id.clone(),
                    disposition,
                    revision: Revision::new(1),
                });
            }
        }
        let final_sequence = input.delta.encounters.last().map(|row| row.sequence);
        let import = ReturnImportRecord {
            source_bundle_id: input.source_bundle_id.clone(),
            return_digest: input.digest,
            delta_digest: input.delta.digest,
            first_sequence: input.delta.first_sequence,
            final_sequence,
            final_digest: input.delta.final_digest,
            imported_encounters: input
                .delta
                .encounters
                .iter()
                .map(|row| row.encounter_id.clone())
                .collect(),
            classifications,
            affected_scope: affected.digest,
            affected_work_ids: affected.affected_work_ids.clone(),
            dependent_work_ids: affected.dependent_work_ids.clone(),
            affected_subjects: affected.subjects.clone(),
            unknown_boundary: affected.unknown_boundary.clone(),
            reassessment_review_id: None,
            resolved_lowering_id: None,
            resolution: ReturnResolutionState::AwaitingReassessment,
            revision: Revision::new(1),
        };
        Ok(ResolvedReturnImport {
            encounters: records,
            failed_approaches: failed,
            counter_updates: counters.into_values().collect(),
            import,
        })
    }
}

fn ready_bundle(
    state: &dyn StateReader,
    input: &ReturnBundleInput,
) -> Result<WeakBundleRecord, ZapError> {
    let bundle = state
        .get_typed::<WeakBundleRecord>(&input.source_bundle_id)?
        .ok_or_else(|| return_missing("return source bundle is missing"))?;
    if bundle.status != zap_domain::lowering::BundleStatus::Ready {
        return Err(return_conflict("return source bundle is not ready"));
    }
    validate_return_bundle(&bundle.manifest, input)?;
    Ok(bundle)
}

fn current_plan(state: &dyn StateReader, bundle: &WeakBundleRecord) -> Result<bool, ZapError> {
    let lowering = state
        .get_typed::<zap_domain::lowering::LoweringRecord>(&bundle.manifest.binding.lowering_id)?;
    let strategy = state.get_typed::<zap_domain::lowering::StrategicPlanRecord>(
        &bundle.manifest.binding.strategy_id,
    )?;
    let lowering_current = lowering.is_some_and(|row| {
        row.state == zap_domain::lowering::PlanningRevisionState::Current
            && row.revision == bundle.manifest.binding.lowering_revision
            && row.semantic_digest == bundle.manifest.binding.lowering_semantic_digest
    });
    let strategy_current = strategy.is_some_and(|row| {
        row.state == zap_domain::lowering::PlanningRevisionState::Current
            && row.revision == bundle.manifest.binding.strategy_revision
            && row.semantic_digest == bundle.manifest.binding.strategy_semantic_digest
    });
    let mut packets_current = true;
    for binding in &bundle.manifest.packets {
        let Some(packet) =
            state.get_typed::<zap_domain::lowering::WorkerPacketRecord>(&binding.packet_id)?
        else {
            packets_current = false;
            break;
        };
        let Some(work) = state.get_typed::<WorkRecord>(&binding.work_id)? else {
            packets_current = false;
            break;
        };
        let Some(contract) = state.get_typed::<TaskContractRecord>(&binding.contract_id)? else {
            packets_current = false;
            break;
        };
        let attempt = bundle
            .manifest
            .attempts
            .iter()
            .find(|attempt| attempt.packet_id == binding.packet_id);
        let basis = DomainBasisProvider.relevant_basis(
            state,
            &BasisRequest::new(BasisRequestInput {
                purpose: BasisPurpose::Dispatch(binding.work_id.clone()),
                roots: vec![SubjectRef::Work(binding.work_id.clone())],
                policy: ContextRequirement::Required,
                capacity: ContextRequirement::NotApplicable,
                closure: ClosureRequirement::KnownGraph,
            })?,
        )?;
        if packet.state != zap_domain::lowering::PacketState::Current
            || packet.packet_digest != binding.packet_digest
            || packet.work_id != binding.work_id
            || packet.contract_id != binding.contract_id
            || packet.contract_version != binding.contract_version
            || packet.contract_digest != binding.contract_digest
            || work.state != WorkState::Active
            || work.validation_generation != packet.validation_generation
            || attempt.is_none_or(|attempt| work.active_job.as_ref() != Some(&attempt.job_id))
            || !contract.active
            || contract.work_id != binding.work_id
            || contract.version != binding.contract_version
            || contract.contract_digest != binding.contract_digest
            || basis.digest != binding.relevant_basis
        {
            packets_current = false;
            break;
        }
    }
    Ok(lowering_current && strategy_current && packets_current)
}

fn classify_candidate(
    state: &dyn StateReader,
    encounter: &zap_domain::lowering::OfflineEncounter,
    attempt: &zap_domain::lowering::BundleAttemptBinding,
    plan_current: bool,
) -> Result<ReturnClassification, ZapError> {
    let candidate_id = encounter
        .candidate_id
        .as_ref()
        .ok_or_else(|| return_conflict("candidate encounter lacks candidate identity"))?;
    let job = state
        .get_typed::<RuntimeJobRecord>(&attempt.job_id)?
        .ok_or_else(|| return_missing("candidate runtime job is missing"))?;
    let result = state
        .get_typed::<CandidateResultRecord>(candidate_id)?
        .ok_or_else(|| return_missing("candidate result is missing"))?;
    let provenance = state
        .get_typed::<CandidateProvenanceRecord>(candidate_id)?
        .ok_or_else(|| return_missing("candidate provenance is missing"))?;
    let result_artifacts = result
        .candidate
        .artifacts
        .iter()
        .map(|row| row.digest)
        .collect::<BTreeSet<_>>();
    let encounter_artifacts = encounter.artifacts.iter().copied().collect::<BTreeSet<_>>();
    let provenance_artifacts = provenance
        .artifacts()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if !job.execution.is_terminal()
        || job.collection != CollectionState::CandidateRecorded
        || job.acceptance != AcceptanceState::Unreviewed
        || job.candidate_id.as_ref() != Some(candidate_id)
        || result.candidate.producer != attempt.producer
        || provenance.producer() != &attempt.producer
        || result.candidate.producer.job_id != attempt.job_id
        || result.candidate.producer.attempt_id != attempt.attempt_id
        || result.candidate.producer.packet_id != attempt.packet_id
        || result.candidate.contract_id != job.contract_id
        || result.candidate.contract_digest != job.contract_digest
        || result.candidate.relevant_basis != job.relevant_basis
        || provenance.contract_id() != &job.contract_id
        || provenance.contract_digest() != job.contract_digest
        || provenance.relevant_basis() != job.relevant_basis
        || provenance.observation() != &result.candidate.terminal_observation
        || provenance_artifacts != result_artifacts
        || result_artifacts != encounter_artifacts
    {
        return Err(return_conflict(
            "candidate result or provenance does not match actual attempt",
        ));
    }
    Ok(if plan_current {
        ReturnClassification::ApplicableCandidate {
            candidate_id: candidate_id.clone(),
        }
    } else {
        ReturnClassification::StaleReviewInput
    })
}

fn return_conflict(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        RETURN_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn return_missing(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        RETURN_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
