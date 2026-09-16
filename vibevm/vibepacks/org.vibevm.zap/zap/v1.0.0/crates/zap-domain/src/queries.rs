specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPLETION-PREDICATE");

use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_core::{
    CandidateProvenanceRecord, CompletionBlocker, CompletionBlockerProvider, StateReader,
    StateReaderExt,
};
use zap_wire::{CompletionProviderId, ZapError};

use crate::acceptance::{IntegrationAcceptanceRecord, PromotionRecord, WorkAcceptanceRecord};
use crate::control::{DeferralRecord, ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    EpistemicStatus, FactAcceptanceStatus, FactRecord, ProofReuseRecord, SourceApplicabilityStatus,
    current_proof_index,
};
use crate::seams::{DeferralStatus, LifecycleStatus, ObligationStatus, WorkState, scan_all};

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct DomainCompletionProvider {
    id: CompletionProviderId,
}

impl DomainCompletionProvider {
    pub fn new() -> Result<Self, ZapError> {
        Ok(Self {
            id: CompletionProviderId::parse("zap.domain")?,
        })
    }
}

impl CompletionBlockerProvider for DomainCompletionProvider {
    fn id(&self) -> CompletionProviderId {
        self.id.clone()
    }

    fn active_outcome(
        &self,
        state: &dyn StateReader,
    ) -> Result<Option<zap_wire::OutcomeId>, ZapError> {
        active_outcome(state)
    }

    fn blockers(&self, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
        domain_blockers(state)
    }
}

pub fn domain_blockers(state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
    let Some(active_outcome_id) = active_outcome(state)? else {
        return Ok(Vec::new());
    };
    let outcome = state
        .get_typed::<OutcomeRecord>(&active_outcome_id)?
        .ok_or_else(active_outcome_invariant)?;

    let obligations: Vec<_> = scan_all::<ObligationRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.status == ObligationStatus::Active
                && row
                    .current_outcomes
                    .binary_search(&outcome.outcome_id)
                    .is_ok()
        })
        .collect();
    let work = scan_all::<WorkRecord>(state)?;
    let contracts = scan_all::<TaskContractRecord>(state)?;
    let acceptances = scan_all::<WorkAcceptanceRecord>(state)?;
    let integrations = scan_all::<IntegrationAcceptanceRecord>(state)?;
    let deferrals = scan_all::<DeferralRecord>(state)?;
    let promotions = scan_all::<PromotionRecord>(state)?;
    let provenances = scan_all::<CandidateProvenanceRecord>(state)?;
    let reuse = scan_all::<ProofReuseRecord>(state)?;
    let facts: BTreeMap<_, _> = scan_all::<FactRecord>(state)?
        .into_iter()
        .map(|row| (row.fact_id.clone(), row))
        .collect();

    let work_by_id: BTreeMap<_, _> = work.iter().map(|row| (&row.work_id, row)).collect();
    let active_contracts: BTreeMap<_, _> = contracts
        .into_iter()
        .filter(|row| row.active)
        .map(|row| (row.work_id.clone(), row))
        .collect();
    let provenance_by_id: BTreeMap<_, _> = provenances
        .into_iter()
        .map(|row| (row.candidate_id().clone(), row))
        .collect();
    let current_proofs = current_proof_index(state)?;
    let current_evidence: BTreeSet<_> = current_proofs
        .values()
        .filter(|row| {
            let direct = row.applies_to.outcome_id == outcome.outcome_id;
            let transferred = reuse.iter().any(|witness| {
                witness.to_outcome_id == outcome.outcome_id
                    && witness.evidence_ids.binary_search(&row.evidence_id).is_ok()
                    && row.applies_to.outcome_id == witness.from_outcome_id
                    && witness
                        .source_captures
                        .iter()
                        .all(|capture| row.source_captures.binary_search(capture).is_ok())
            });
            direct || transferred
        })
        .map(|row| row.evidence_id.clone())
        .collect();
    let current_acceptances: BTreeMap<_, _> = acceptances
        .into_iter()
        .filter(|row| {
            let accepted_for_outcome = row.outcome_id == outcome.outcome_id
                || reuse.iter().any(|witness| {
                    witness.to_outcome_id == outcome.outcome_id
                        && witness.from_outcome_id == row.outcome_id
                        && witness
                            .work_acceptance_ids
                            .binary_search(&row.acceptance_id)
                            .is_ok()
                });
            acceptance_is_current(
                row,
                accepted_for_outcome,
                &work_by_id,
                &active_contracts,
                &provenance_by_id,
                &current_evidence,
                &obligations,
            )
        })
        .map(|row| (row.work_id.clone(), row))
        .collect();
    let active_obligation_ids: BTreeSet<_> = obligations
        .iter()
        .map(|row| row.obligation_id.clone())
        .collect();
    let mut blockers = Vec::new();
    for obligation in &obligations {
        let covered = !obligation.owners.is_empty()
            && obligation.owners.iter().all(|owner| {
                current_acceptances
                    .get(&owner.work_id)
                    .is_some_and(|acceptance| {
                        acceptance
                            .obligation_ids
                            .binary_search(&obligation.obligation_id)
                            .is_ok()
                    })
            });
        if !covered {
            blockers.push(CompletionBlocker::ActiveObligation(
                obligation.obligation_id.clone(),
            ));
        }
    }

    let selected_work: Vec<_> = work
        .iter()
        .filter(|row| {
            !matches!(row.state, WorkState::Dropped | WorkState::Superseded)
                && obligations.iter().any(|obligation| {
                    obligation
                        .owners
                        .iter()
                        .any(|owner| owner.work_id == row.work_id)
                })
        })
        .collect();
    for row in &selected_work {
        let accepted = current_acceptances
            .get(&row.work_id)
            .is_some_and(|acceptance| acceptance.generation == row.validation_generation);
        if !accepted {
            blockers.push(CompletionBlocker::MissingWorkAcceptance(
                row.work_id.clone(),
            ));
        }
        let children: BTreeSet<_> = work
            .iter()
            .filter(|child| child.parent_id.as_ref() == Some(&row.work_id))
            .map(|child| child.work_id.clone())
            .collect();
        if !children.is_empty()
            && !integrations.iter().any(|integration| {
                let accepted_for_outcome = integration.outcome_id == outcome.outcome_id
                    || reuse.iter().any(|witness| {
                        witness.to_outcome_id == outcome.outcome_id
                            && witness.from_outcome_id == integration.outcome_id
                            && witness
                                .integration_acceptance_ids
                                .binary_search(&integration.integration_id)
                                .is_ok()
                    });
                integration.work_id == row.work_id
                    && accepted_for_outcome
                    && integration.generation == row.validation_generation
                    && integration
                        .child_work_ids
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        == children
                    && !integration.evidence_ids.is_empty()
                    && integration
                        .evidence_ids
                        .iter()
                        .all(|id| current_evidence.contains(id))
                    && children
                        .iter()
                        .all(|id| current_acceptances.contains_key(id))
            })
        {
            blockers.push(CompletionBlocker::MissingIntegration(row.work_id.clone()));
        }
    }
    blockers.extend(
        deferrals
            .into_iter()
            .filter(|row| {
                row.outcome_id == outcome.outcome_id
                    && row.status == DeferralStatus::Open
                    && row
                        .obligation_ids
                        .iter()
                        .any(|id| active_obligation_ids.contains(id))
            })
            .map(|row| CompletionBlocker::ApplicableDeferral(row.deferral_id)),
    );

    let promoted: BTreeSet<_> = promotions
        .into_iter()
        .filter(|row| {
            facts.get(&row.fact_id).is_some_and(|fact| {
                fact.epistemic_status == EpistemicStatus::Observed
                    && fact.acceptance_status == FactAcceptanceStatus::Accepted
                    && fact.source_applicability == SourceApplicabilityStatus::Applicable
                    && fact.subject_refs.binary_search(&row.target).is_ok()
            })
        })
        .map(|row| row.target)
        .collect();
    blockers.extend(
        outcome
            .required_promotions
            .iter()
            .filter(|subject| !promoted.contains(*subject))
            .cloned()
            .map(CompletionBlocker::MissingPromotion),
    );
    blockers.extend(
        outcome
            .required_final_gate_evidence_ids
            .iter()
            .filter(|id| !current_evidence.contains(*id))
            .cloned()
            .map(CompletionBlocker::MissingFinalGate),
    );
    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn active_outcome(state: &dyn StateReader) -> Result<Option<zap_wire::OutcomeId>, ZapError> {
    let mut outcomes = scan_all::<OutcomeRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .map(|row| row.outcome_id);
    let first = outcomes.next();
    if outcomes.next().is_some() {
        return Err(active_outcome_invariant());
    }
    Ok(first)
}

fn active_outcome_invariant() -> ZapError {
    zap_wire::ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS",
        "more than one outcome revision is active or the active outcome disappeared",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}

fn acceptance_is_current(
    row: &WorkAcceptanceRecord,
    accepted_for_outcome: bool,
    work: &BTreeMap<&zap_wire::WorkId, &WorkRecord>,
    contracts: &BTreeMap<zap_wire::WorkId, TaskContractRecord>,
    provenances: &BTreeMap<zap_wire::CandidateId, CandidateProvenanceRecord>,
    current_evidence: &BTreeSet<zap_wire::EvidenceId>,
    obligations: &[ObligationRecord],
) -> bool {
    let Some(current_work) = work.get(&row.work_id) else {
        return false;
    };
    let Some(contract) = contracts.get(&row.work_id) else {
        return false;
    };
    let current_owned: BTreeSet<_> = obligations
        .iter()
        .filter(|obligation| {
            obligation
                .owners
                .iter()
                .any(|owner| owner.work_id == row.work_id)
        })
        .map(|obligation| obligation.obligation_id.clone())
        .collect();
    let claimed: BTreeSet<_> = row.obligation_ids.iter().cloned().collect();
    let provenance_matches = provenances
        .get(&row.candidate_id)
        .is_some_and(|provenance| {
            provenance.contract_id() == &contract.contract_id
                && provenance.contract_digest() == contract.contract_digest
                && provenance
                    .subjects()
                    .binary_search(&zap_wire::SubjectRef::Work(row.work_id.clone()))
                    .is_ok()
        });
    accepted_for_outcome
        && current_work.state == WorkState::Accepted
        && row.generation == current_work.validation_generation
        && row.contract_version == contract.version
        && !row.evidence_ids.is_empty()
        && row
            .evidence_ids
            .iter()
            .all(|id| current_evidence.contains(id))
        && claimed == current_owned
        && provenance_matches
}
