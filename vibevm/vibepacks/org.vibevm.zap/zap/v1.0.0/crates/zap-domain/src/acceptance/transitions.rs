use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_core::{
    ActorRef, ChangeSet, CommandPayload, CompletionView, ProducerRef, StateReader, StoredRecord,
    TransitionCell, ValidatedCommand,
};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, Revision, WorkId, ZapError};

use crate::acceptance::{
    CampaignClosed, ClosureRecord, EvidenceAdjudicated, EvidenceAdjudicationRecord,
    FactPromotionRecorded, IntegrationAcceptanceRecord, IntegrationAccepted, PromotionRecord,
    ProofClaim, StageAcceptanceRecord, StageAccepted, WorkAcceptanceRecord, WorkAccepted,
    WorkGeneration, validate_collective_proof, validate_producer_acceptor,
};
use crate::control::{TaskContractRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::seams::{
    DomainMutation, EvidenceDisposition, LifecycleStatus, ProofApplicability, WorkState,
    cell_descriptor, impl_command_payload, refuse, scan_all, sorted_unique, sorted_unique_nonempty,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE");

const EVIDENCE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FACT-EVIDENCE-DISTINCTION";
const ACCEPTANCE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE";
const CLOSURE_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-CLOSURE";

pub fn adjudicate_evidence(
    payload: &EvidenceAdjudicated,
    current: Option<&EvidenceAdjudicationRecord>,
    work_generations: Vec<WorkGeneration>,
    source_closure_current: bool,
    relevant_basis: zap_wire::RelevantBasisDigest,
    producer: &ProducerRef,
    acceptor: &ActorRef,
) -> Result<EvidenceAdjudicationRecord, ZapError> {
    validate_producer_acceptor(producer, acceptor)?;
    let actual_revision = current.map_or(Revision::GENESIS, |row| row.revision);
    if payload.expected_revision != actual_revision
        || payload.observation.evidence_id != payload.evidence_id
        || !sorted_unique_nonempty(&payload.applies_to.obligation_ids)
        || !sorted_unique_nonempty(&payload.applies_to.work_ids)
        || !sorted_unique_nonempty(&payload.source_captures)
        || !sorted_unique_nonempty(&work_generations)
        || payload
            .applies_to
            .work_ids
            .iter()
            .any(|id| payload.observation.work_ids.binary_search(id).is_err())
        || payload.source_captures.iter().any(|capture| {
            payload
                .observation
                .source_ids
                .binary_search(&capture.source_id)
                .is_err()
        })
        || payload.disposition == EvidenceDisposition::Accepted && !source_closure_current
    {
        return refuse(
            ErrorCode::NeedsEvidence,
            EVIDENCE_REQ,
            "adjudication must bind the exact evidence, current source closure and work generations",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    Ok(EvidenceAdjudicationRecord {
        evidence_id: payload.evidence_id.clone(),
        candidate_id: payload.candidate_id.clone(),
        verification_id: payload.verification_id.clone(),
        revision: actual_revision.checked_next()?,
        disposition: payload.disposition,
        applicability: if source_closure_current {
            ProofApplicability::Current
        } else {
            ProofApplicability::Unknown
        },
        relevant_basis,
        applies_to: payload.applies_to.clone(),
        source_captures: payload.source_captures.clone(),
        method: payload.method.clone(),
        limitations: payload.limitations.clone(),
        observation: payload.observation.clone(),
        validation_generations: work_generations,
    })
}

pub fn accept_stage(
    payload: &StageAccepted,
    work: &WorkRecord,
    owned_obligations: &BTreeSet<zap_wire::ObligationId>,
    evidence: &crate::knowledge::CurrentProofSet,
    producer: &ProducerRef,
    acceptor: &ActorRef,
) -> Result<StageAcceptanceRecord, ZapError> {
    validate_producer_acceptor(producer, acceptor)?;
    if payload.work_id != work.work_id
        || !sorted_unique_nonempty(&payload.evidence_ids)
        || !sorted_unique_nonempty(&payload.obligation_ids)
        || payload
            .obligation_ids
            .iter()
            .any(|id| !owned_obligations.contains(id))
    {
        return refuse(
            ErrorCode::Conflict,
            ACCEPTANCE_REQ,
            "stage acceptance must name current work obligations and evidence exactly",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    validate_collective_proof(
        &payload.evidence_ids,
        evidence,
        ProofClaim {
            outcome_id: &payload.outcome_id,
            work_id: &payload.work_id,
            stage: Some(payload.stage),
            obligation_ids: &payload.obligation_ids,
            validation_generation: work.validation_generation,
            require_pass: true,
        },
    )?;
    Ok(StageAcceptanceRecord {
        stage_acceptance_id: payload.stage_acceptance_id.clone(),
        candidate_id: payload.candidate_id.clone(),
        work_id: payload.work_id.clone(),
        generation: work.validation_generation,
        stage: payload.stage,
        outcome_id: payload.outcome_id.clone(),
        evidence_ids: payload.evidence_ids.clone(),
        obligation_ids: payload.obligation_ids.clone(),
        scope: payload.scope.clone(),
        summary: payload.summary.clone(),
        revision: Revision::new(1),
    })
}

pub fn accept_integration(
    payload: &IntegrationAccepted,
    work: &WorkRecord,
    exact_children: &BTreeSet<WorkId>,
    current_child_acceptances: &BTreeSet<WorkId>,
    evidence: &crate::knowledge::CurrentProofSet,
    producer: &ProducerRef,
    acceptor: &ActorRef,
) -> Result<IntegrationAcceptanceRecord, ZapError> {
    validate_producer_acceptor(producer, acceptor)?;
    let claimed: BTreeSet<_> = payload.child_work_ids.iter().cloned().collect();
    let legacy: BTreeSet<_> = payload.legacy_child_ids.iter().cloned().collect();
    if payload.work_id != work.work_id
        || claimed != *exact_children
        || !legacy.is_subset(exact_children)
        || exact_children
            .difference(&legacy)
            .any(|id| !current_child_acceptances.contains(id))
    {
        return refuse(
            ErrorCode::Conflict,
            ACCEPTANCE_REQ,
            "integration acceptance must cover every direct child and every non-legacy child must be currently accepted",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    validate_collective_proof(
        &payload.evidence_ids,
        evidence,
        ProofClaim {
            outcome_id: &payload.outcome_id,
            work_id: &payload.work_id,
            stage: None,
            obligation_ids: &payload.obligation_ids,
            validation_generation: work.validation_generation,
            require_pass: true,
        },
    )?;
    Ok(IntegrationAcceptanceRecord {
        integration_id: payload.integration_id.clone(),
        candidate_id: payload.candidate_id.clone(),
        work_id: payload.work_id.clone(),
        generation: work.validation_generation,
        child_work_ids: payload.child_work_ids.clone(),
        legacy_child_ids: payload.legacy_child_ids.clone(),
        outcome_id: payload.outcome_id.clone(),
        evidence_ids: payload.evidence_ids.clone(),
        obligation_ids: payload.obligation_ids.clone(),
        summary: payload.summary.clone(),
        revision: Revision::new(1),
    })
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-acceptance")]
pub struct WorkAcceptanceContext<'a> {
    pub work: &'a WorkRecord,
    pub contract: &'a TaskContractRecord,
    pub stage: &'a StageAcceptanceRecord,
    pub integrations: &'a BTreeMap<zap_wire::IntegrationAcceptanceId, IntegrationAcceptanceRecord>,
    pub evidence: &'a crate::knowledge::CurrentProofSet,
    pub owned_obligations: &'a BTreeSet<zap_wire::ObligationId>,
    pub accepted_dependencies: &'a BTreeSet<WorkId>,
    pub dependencies: &'a BTreeSet<WorkId>,
    pub children: &'a BTreeSet<WorkId>,
    pub producer: &'a ProducerRef,
    pub acceptor: &'a ActorRef,
}

pub fn accept_work(
    payload: &WorkAccepted,
    context: WorkAcceptanceContext<'_>,
) -> Result<(WorkAcceptanceRecord, WorkRecord), ZapError> {
    let work = context.work;
    let obligations: BTreeSet<_> = payload.obligation_ids.iter().cloned().collect();
    if payload.work_id != work.work_id
        || work.state != WorkState::Candidate
        || payload.outcome_id != context.stage.outcome_id
        || payload.stage_acceptance_id != context.stage.stage_acceptance_id
        || context.stage.generation != work.validation_generation
        || context.stage.stage != context.contract.contract.required_stage
        || obligations != *context.owned_obligations
        || !context
            .dependencies
            .is_subset(context.accepted_dependencies)
        || validate_producer_acceptor(context.producer, context.acceptor).is_err()
    {
        return refuse(
            ErrorCode::Unauthorized,
            ACCEPTANCE_REQ,
            "work acceptance requires independent authority, current proof, contract, dependencies and obligation coverage",
            FixSurface::Authority,
            ErrorDetail::None,
        );
    }
    if !context.children.is_empty() {
        let integrated = payload.integration_acceptance_ids.iter().any(|id| {
            context.integrations.get(id).is_some_and(|row| {
                row.work_id == work.work_id
                    && row.generation == work.validation_generation
                    && row.child_work_ids.iter().cloned().collect::<BTreeSet<_>>()
                        == *context.children
            })
        });
        if !integrated {
            return refuse(
                ErrorCode::Conflict,
                ACCEPTANCE_REQ,
                "parent work lacks current integration acceptance for all direct children",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
    }
    validate_collective_proof(
        &payload.evidence_ids,
        context.evidence,
        ProofClaim {
            outcome_id: &payload.outcome_id,
            work_id: &payload.work_id,
            stage: None,
            obligation_ids: &payload.obligation_ids,
            validation_generation: work.validation_generation,
            require_pass: true,
        },
    )?;
    let record = WorkAcceptanceRecord {
        acceptance_id: payload.acceptance_id.clone(),
        candidate_id: payload.candidate_id.clone(),
        work_id: payload.work_id.clone(),
        generation: work.validation_generation,
        outcome_id: payload.outcome_id.clone(),
        stage_acceptance_id: payload.stage_acceptance_id.clone(),
        evidence_ids: payload.evidence_ids.clone(),
        obligation_ids: payload.obligation_ids.clone(),
        integration_acceptance_ids: payload.integration_acceptance_ids.clone(),
        contract_version: context.contract.version,
        summary: payload.summary.clone(),
        revision: Revision::new(1),
    };
    let mut accepted_work = work.clone();
    accepted_work.state = WorkState::Accepted;
    accepted_work.active_job = None;
    accepted_work.revision = accepted_work.revision.checked_next()?;
    Ok((record, accepted_work))
}

pub fn record_promotion(
    payload: &FactPromotionRecorded,
    fact: &crate::knowledge::FactRecord,
    evidence: &crate::knowledge::ScopedEvidenceWitness,
) -> Result<PromotionRecord, ZapError> {
    let claimed: BTreeSet<_> = payload.evidence_ids.iter().cloned().collect();
    let fact_evidence: BTreeSet<_> = fact.evidence_refs.iter().cloned().collect();
    if fact.fact_id != payload.fact_id
        || fact.epistemic_status != crate::knowledge::EpistemicStatus::Observed
        || fact.acceptance_status != crate::knowledge::FactAcceptanceStatus::Accepted
        || fact.source_applicability != crate::knowledge::SourceApplicabilityStatus::Applicable
        || fact.subject_refs.binary_search(&payload.target).is_err()
        || claimed != fact_evidence
        || !evidence.matches(&payload.evidence_ids, payload.basis)
    {
        return refuse(
            ErrorCode::NeedsEvidence,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#PROMOTE-FACTS",
            "fact promotion requires the observed fact's exact centrally accepted evidence set",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    Ok(PromotionRecord {
        promotion_id: payload.promotion_id.clone(),
        fact_id: payload.fact_id.clone(),
        target: payload.target.clone(),
        content_digest: payload.content_digest,
        evidence_ids: payload.evidence_ids.clone(),
        adapter_receipt: payload.adapter_receipt.clone(),
        summary: payload.summary.clone(),
        revision: Revision::new(1),
    })
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-CLOSURE")]
pub fn close_campaign(
    payload: &CampaignClosed,
    completion: Option<&CompletionView>,
    outcome: &OutcomeRecord,
    promotions: &BTreeMap<zap_wire::PromotionId, PromotionRecord>,
) -> Result<ClosureRecord, ZapError> {
    let Some(view) = completion else {
        return refuse(
            ErrorCode::InternalInvariant,
            CLOSURE_REQ,
            "campaign close did not receive the configured shared completion view",
            FixSurface::Configuration,
            ErrorDetail::None,
        );
    };
    let expected_final: BTreeSet<_> = outcome
        .required_final_gate_evidence_ids
        .iter()
        .cloned()
        .collect();
    let claimed_final: BTreeSet<_> = payload.final_gate_evidence_ids.iter().cloned().collect();
    let promoted_subjects: BTreeSet<_> = payload
        .promotion_ids
        .iter()
        .filter_map(|id| promotions.get(id).map(|row| row.target.clone()))
        .collect();
    let expected_promotions: BTreeSet<_> = outcome.required_promotions.iter().cloned().collect();
    if !view.eligible
        || !view.blockers.is_empty()
        || view.outcome_id.as_ref() != Some(&payload.active_outcome_id)
        || outcome.outcome_id != payload.active_outcome_id
        || outcome.status != LifecycleStatus::Active
        || claimed_final != expected_final
        || promoted_subjects != expected_promotions
        || !sorted_unique(&payload.obligation_results)
        || !sorted_unique(&payload.acceptance_ids)
        || !sorted_unique(&payload.integration_acceptance_ids)
        || !sorted_unique(&payload.deferral_ids)
        || !sorted_unique(&payload.promotion_ids)
        || !sorted_unique(&payload.final_gate_evidence_ids)
    {
        return refuse(
            ErrorCode::Conflict,
            CLOSURE_REQ,
            "campaign close requires the eligible shared completion view and exact sorted proof identities",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(ClosureRecord {
        closure_id: payload.closure_id.clone(),
        classification: payload.classification,
        active_outcome_id: payload.active_outcome_id.clone(),
        actual_benefit: payload.actual_benefit.clone(),
        obligation_results: payload.obligation_results.clone(),
        acceptance_ids: payload.acceptance_ids.clone(),
        integration_acceptance_ids: payload.integration_acceptance_ids.clone(),
        deferral_ids: payload.deferral_ids.clone(),
        promotion_ids: payload.promotion_ids.clone(),
        final_gate_evidence_ids: payload.final_gate_evidence_ids.clone(),
        summary: payload.summary.clone(),
        revision: Revision::new(1),
    })
}

impl_command_payload!(CampaignClosed, "domain.campaign-closed");

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct CampaignClosedCell;

impl TransitionCell for CampaignClosedCell {
    type Payload = CampaignClosed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            zap_wire::RouteClass::Privileged(zap_wire::ActionClass::parse("campaign.close")?),
            &[ClosureRecord::FAMILY],
            CLOSURE_REQ,
            true,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        if !scan_all::<ClosureRecord>(state)?.is_empty() {
            return refuse(
                ErrorCode::Conflict,
                CLOSURE_REQ,
                "campaign already has a terminal closure",
                FixSurface::Command,
                ErrorDetail::None,
            );
        }
        let mut active = scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .filter(|row| row.status == LifecycleStatus::Active);
        let outcome = active.next().ok_or_else(|| {
            ZapError::from_static(
                ErrorCode::MissingReference,
                CLOSURE_REQ,
                "active outcome is missing",
                FixSurface::Store,
                ErrorDetail::None,
            )
        })?;
        if active.next().is_some() {
            return refuse(
                ErrorCode::InternalInvariant,
                CLOSURE_REQ,
                "more than one outcome is active",
                FixSurface::Store,
                ErrorDetail::None,
            );
        }
        let promotions = scan_all::<PromotionRecord>(state)?
            .into_iter()
            .map(|row| (row.promotion_id.clone(), row))
            .collect();
        let closure = close_campaign(
            command.payload(),
            command.completion(),
            &outcome,
            &promotions,
        )?;
        changes.insert(closure)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}
