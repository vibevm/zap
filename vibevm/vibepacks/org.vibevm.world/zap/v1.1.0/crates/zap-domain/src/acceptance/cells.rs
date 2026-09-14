specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE");

use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, PayloadBasisScope, StateReader, StateReaderExt, StoredRecord,
    TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, ErrorCode, ErrorDetail, FixSurface, RouteClass, SubjectRef, ZapError,
};

use crate::acceptance::*;
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::{
    ClosureStatus, SourceApplicabilityRecord, SourceApplicabilityStatus, SourceCaptureStatus,
    SourceRecord, SourceScope, current_proof_set,
};
use crate::seams::{
    DomainMutation, ObligationStatus, TypedActionImpact, cell_descriptor, impl_command_payload,
    refuse, scan_all,
};

const ACCEPTANCE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE";
const EVIDENCE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#FACT-EVIDENCE-DISTINCTION";

impl_command_payload!(EvidenceAdjudicated, "domain.evidence-adjudicated");
impl_command_payload!(StageAccepted, "domain.stage-accepted");
impl_command_payload!(IntegrationAccepted, "domain.integration-accepted");
impl_command_payload!(WorkAccepted, "domain.work-accepted");

fn actor<P: CommandPayload>(
    command: &ValidatedCommand<P>,
) -> Result<&zap_core::ActorRef, ZapError> {
    command.authority().actor().ok_or_else(|| {
        ZapError::from_static(
            ErrorCode::Unauthorized,
            ACCEPTANCE_REQ,
            "acceptance requires an admitted non-internal actor",
            FixSurface::Authority,
            ErrorDetail::None,
        )
    })
}

fn exact_basis(basis: &BasisBinding) -> Result<zap_wire::RelevantBasisDigest, ZapError> {
    match basis {
        BasisBinding::Exact(digest) => Ok(*digest),
        BasisBinding::NotApplicable => refuse(
            ErrorCode::StaleBasis,
            ACCEPTANCE_REQ,
            "acceptance command requires its registered exact local basis",
            FixSurface::Command,
            ErrorDetail::None,
        ),
    }
}

fn mutation<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct EvidenceAdjudicatedCell;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct EvidenceAdjudicationBasisScope;

impl PayloadBasisScope<EvidenceAdjudicated> for EvidenceAdjudicationBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &EvidenceAdjudicated,
    ) -> Result<BasisRequest, ZapError> {
        let mut roots: BTreeSet<_> = payload.method.subjects.iter().cloned().collect();
        roots.insert(SubjectRef::Outcome(payload.applies_to.outcome_id.clone()));
        roots.extend(
            payload
                .applies_to
                .obligation_ids
                .iter()
                .cloned()
                .map(SubjectRef::Obligation),
        );
        roots.extend(
            payload
                .applies_to
                .work_ids
                .iter()
                .cloned()
                .map(SubjectRef::Work),
        );
        roots.extend(
            payload
                .source_captures
                .iter()
                .map(|capture| SubjectRef::Source(capture.source_id.clone())),
        );
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Verification(payload.verification_id.clone()),
            roots: roots.into_iter().collect(),
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}
impl TransitionCell for EvidenceAdjudicatedCell {
    type Payload = EvidenceAdjudicated;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("evidence.adjudicate")?),
            &[EvidenceAdjudicationRecord::FAMILY],
            EVIDENCE_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let provenance = current_candidate(
            state,
            &payload.candidate_id,
            &payload.applies_to.work_ids,
            &payload.method.subjects,
            Some(payload.observation.artifact),
            CandidateReviewPhase::UnderReview,
        )?;
        let acceptor = actor(command)?;
        let current = state.get_typed::<EvidenceAdjudicationRecord>(&payload.evidence_id)?;
        let mut generations = Vec::new();
        for work_id in &payload.applies_to.work_ids {
            let work = state.get_typed::<WorkRecord>(work_id)?.ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    EVIDENCE_REQ,
                    "evidence work is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
            generations.push(WorkGeneration {
                work_id: work_id.clone(),
                generation: work.validation_generation,
            });
        }
        generations.sort();
        let sources: BTreeMap<_, _> = scan_all::<SourceRecord>(state)?
            .into_iter()
            .map(|row| (row.source_id.clone(), row))
            .collect();
        let assessments: BTreeMap<_, _> = scan_all::<SourceApplicabilityRecord>(state)?
            .into_iter()
            .map(|row| (row.source_id.clone(), row))
            .collect();
        let captures_match = payload.source_captures.len() == payload.observation.source_ids.len()
            && payload.source_captures.iter().all(|capture| {
                sources.get(&capture.source_id).is_some_and(|source| {
                    source.capture_status == SourceCaptureStatus::Current
                        && !matches!(source.scope, SourceScope::Unassessed)
                        && source.current.digest == capture.digest
                        && payload
                            .observation
                            .source_ids
                            .binary_search(&capture.source_id)
                            .is_ok()
                        && assessments
                            .get(&capture.source_id)
                            .is_none_or(|assessment| {
                                assessment.source_digest == capture.digest
                                    && assessment.status == SourceApplicabilityStatus::Applicable
                                    && assessment.closure_status == ClosureStatus::Complete
                            })
                })
            })
            && provenance
                .artifacts()
                .binary_search(&payload.observation.artifact)
                .is_ok();
        let record = adjudicate_evidence(
            payload,
            current.as_ref(),
            generations,
            captures_match,
            exact_basis(command.header().basis())?,
            provenance.producer(),
            acceptor,
        )?;
        if let Some(current) = current {
            changes.replace(current.revision, record)?;
        } else {
            changes.insert(record)?;
        }
        mutation(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct StageAcceptedCell;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stage-acceptance")]
pub struct StageAcceptanceBasisScope;

impl PayloadBasisScope<StageAccepted> for StageAcceptanceBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &StageAccepted,
    ) -> Result<BasisRequest, ZapError> {
        acceptance_basis_request(
            StageAccepted::KIND,
            &payload.outcome_id,
            &payload.work_id,
            &payload.obligation_ids,
            &payload.evidence_ids,
        )
    }
}

impl TransitionCell for StageAcceptedCell {
    type Payload = StageAccepted;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("stage.accept")?),
            &[StageAcceptanceRecord::FAMILY],
            ACCEPTANCE_REQ,
            false,
        )
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let provenance = current_candidate(
            state,
            &payload.candidate_id,
            std::slice::from_ref(&payload.work_id),
            &[SubjectRef::Work(payload.work_id.clone())],
            None,
            CandidateReviewPhase::UnderReview,
        )?;
        let work = state
            .get_typed::<WorkRecord>(&payload.work_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    ACCEPTANCE_REQ,
                    "stage work is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let obligations = owned_obligations(state, &payload.work_id)?;
        let evidence = current_proof_set(state, &payload.evidence_ids)?;
        let record = accept_stage(
            payload,
            &work,
            &obligations,
            &evidence,
            provenance.producer(),
            actor(command)?,
        )?;
        changes.insert(record)?;
        mutation(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct IntegrationAcceptedCell;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#integration-acceptance"
)]
pub struct IntegrationAcceptanceBasisScope;

impl PayloadBasisScope<IntegrationAccepted> for IntegrationAcceptanceBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &IntegrationAccepted,
    ) -> Result<BasisRequest, ZapError> {
        acceptance_basis_request(
            IntegrationAccepted::KIND,
            &payload.outcome_id,
            &payload.work_id,
            &payload.obligation_ids,
            &payload.evidence_ids,
        )
    }
}

impl TransitionCell for IntegrationAcceptedCell {
    type Payload = IntegrationAccepted;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("work.accept")?),
            &[IntegrationAcceptanceRecord::FAMILY],
            ACCEPTANCE_REQ,
            false,
        )
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let provenance = current_candidate(
            state,
            &payload.candidate_id,
            std::slice::from_ref(&payload.work_id),
            &[SubjectRef::Work(payload.work_id.clone())],
            None,
            CandidateReviewPhase::UnderReview,
        )?;
        let work = state
            .get_typed::<WorkRecord>(&payload.work_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    ACCEPTANCE_REQ,
                    "integration work is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let all_work = scan_all::<WorkRecord>(state)?;
        let children = all_work
            .iter()
            .filter(|row| row.parent_id.as_ref() == Some(&payload.work_id))
            .map(|row| row.work_id.clone())
            .collect();
        let accepted_children = scan_all::<WorkAcceptanceRecord>(state)?
            .into_iter()
            .filter(|row| row.outcome_id == payload.outcome_id)
            .map(|row| row.work_id)
            .collect();
        let record = accept_integration(
            payload,
            &work,
            &children,
            &accepted_children,
            &current_proof_set(state, &payload.evidence_ids)?,
            provenance.producer(),
            actor(command)?,
        )?;
        changes.insert(record)?;
        mutation(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct WorkAcceptedCell;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-acceptance")]
pub struct WorkAcceptanceBasisScope;

impl PayloadBasisScope<WorkAccepted> for WorkAcceptanceBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &WorkAccepted,
    ) -> Result<BasisRequest, ZapError> {
        acceptance_basis_request(
            WorkAccepted::KIND,
            &payload.outcome_id,
            &payload.work_id,
            &payload.obligation_ids,
            &payload.evidence_ids,
        )
    }
}

impl TransitionCell for WorkAcceptedCell {
    type Payload = WorkAccepted;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("work.accept")?),
            &[WorkAcceptanceRecord::FAMILY, WorkRecord::FAMILY],
            ACCEPTANCE_REQ,
            false,
        )
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let provenance = current_candidate(
            state,
            &payload.candidate_id,
            std::slice::from_ref(&payload.work_id),
            &[SubjectRef::Work(payload.work_id.clone())],
            None,
            CandidateReviewPhase::UnderReview,
        )?;
        let work = state
            .get_typed::<WorkRecord>(&payload.work_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    ACCEPTANCE_REQ,
                    "candidate work is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let contract = scan_all::<TaskContractRecord>(state)?
            .into_iter()
            .find(|row| row.work_id == payload.work_id && row.active)
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    ACCEPTANCE_REQ,
                    "active task contract is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        if provenance.contract_id() != &contract.contract_id
            || provenance.contract_digest() != contract.contract_digest
        {
            return refuse(
                ErrorCode::StaleBasis,
                ACCEPTANCE_REQ,
                "candidate provenance does not match the active task contract",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        let stage = state
            .get_typed::<StageAcceptanceRecord>(&payload.stage_acceptance_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    ACCEPTANCE_REQ,
                    "stage acceptance is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let integrations: BTreeMap<_, _> = scan_all::<IntegrationAcceptanceRecord>(state)?
            .into_iter()
            .map(|row| (row.integration_id.clone(), row))
            .collect();
        let all_work = scan_all::<WorkRecord>(state)?;
        let children = all_work
            .iter()
            .filter(|row| row.parent_id.as_ref() == Some(&payload.work_id))
            .map(|row| row.work_id.clone())
            .collect();
        let dependencies = work.depends_on.iter().cloned().collect();
        let accepted_dependencies = scan_all::<WorkAcceptanceRecord>(state)?
            .into_iter()
            .filter(|row| row.outcome_id == payload.outcome_id)
            .map(|row| row.work_id)
            .collect();
        let evidence = current_proof_set(state, &payload.evidence_ids)?;
        let obligations = owned_obligations(state, &payload.work_id)?;
        let context = WorkAcceptanceContext {
            work: &work,
            contract: &contract,
            stage: &stage,
            integrations: &integrations,
            evidence: &evidence,
            owned_obligations: &obligations,
            accepted_dependencies: &accepted_dependencies,
            dependencies: &dependencies,
            children: &children,
            producer: provenance.producer(),
            acceptor: actor(command)?,
        };
        let (acceptance, accepted_work) = accept_work(payload, context)?;
        changes.insert(acceptance)?;
        changes.replace(work.revision, accepted_work)?;
        mutation(command)
    }
}

fn owned_obligations(
    state: &dyn StateReader,
    work_id: &zap_wire::WorkId,
) -> Result<BTreeSet<zap_wire::ObligationId>, ZapError> {
    Ok(scan_all::<ObligationRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.status == ObligationStatus::Active
                && row.owners.iter().any(|owner| owner.work_id == *work_id)
        })
        .map(|row| row.obligation_id)
        .collect())
}

fn acceptance_basis_request(
    kind: &str,
    outcome_id: &zap_wire::OutcomeId,
    work_id: &zap_wire::WorkId,
    obligation_ids: &[zap_wire::ObligationId],
    evidence_ids: &[zap_wire::EvidenceId],
) -> Result<BasisRequest, ZapError> {
    let mut roots = BTreeSet::from([
        SubjectRef::Outcome(outcome_id.clone()),
        SubjectRef::Work(work_id.clone()),
    ]);
    roots.extend(obligation_ids.iter().cloned().map(SubjectRef::Obligation));
    roots.extend(evidence_ids.iter().cloned().map(SubjectRef::Evidence));
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse(kind)?),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(EvidenceAdjudicatedCell)
            .basis(EvidenceAdjudicationBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &EvidenceAdjudicated| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    payload.applies_to.work_ids.clone(),
                    vec![SubjectRef::Evidence(payload.evidence_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(StageAcceptedCell)
            .basis(StageAcceptanceBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &StageAccepted| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    vec![payload.work_id.clone()],
                    vec![SubjectRef::Work(payload.work_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(IntegrationAcceptedCell)
            .basis(IntegrationAcceptanceBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &IntegrationAccepted| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    vec![payload.work_id.clone()],
                    vec![SubjectRef::Work(payload.work_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(WorkAcceptedCell)
            .basis(WorkAcceptanceBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &WorkAccepted| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    vec![payload.work_id.clone()],
                    vec![SubjectRef::Work(payload.work_id.clone())],
                )
            }))?
            .build()?,
    ])
}
