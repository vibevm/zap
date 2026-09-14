use std::collections::{BTreeSet, HashSet};

use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ChangeSet, ClosureRequirement,
    ContextRequirement, StateReader, StateReaderExt,
};
use zap_wire::{ErrorCode, EventKind, SubjectRef, ZapError};

use super::*;
use crate::knowledge::{DomainBasisProvider, current_proof_set};

pub(super) fn apply_achievement(
    state: &dyn StateReader,
    payload: &MilestoneAchievementAccepted,
    acceptor: &zap_core::ActorRef,
    basis: zap_wire::RelevantBasisDigest,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let mut head = state
        .get_typed::<MilestoneRecord>(&payload.milestone_id)?
        .ok_or_else(|| error(ErrorCode::MissingReference, "milestone is missing"))?;
    if head.revision != payload.expected_head_revision
        || head.current_revision_id != payload.milestone_revision_id
        || state
            .get_typed::<MilestoneAchievementRecord>(&payload.achievement_id)?
            .is_some()
    {
        return Err(error(
            ErrorCode::StaleRevision,
            "milestone achievement CAS or receipt identity is stale",
        ));
    }
    let revision = load_current_milestone_revision(state, &payload.milestone_revision_id)?;
    if revision.lifecycle() != MilestoneLifecycle::Active
        || revision.semantic_fingerprint != payload.expected_milestone_fingerprint
        || revision.proof_fingerprint != payload.expected_proof_fingerprint
    {
        return Err(error(
            ErrorCode::StaleRevision,
            "milestone achievement does not bind the current active proof contract",
        ));
    }
    validate_current_proof(state, &revision, &payload.evidence_ids)?;
    validate_achievement_dependencies(state, &revision, &mut HashSet::new())?;
    let expected_basis = DomainBasisProvider
        .relevant_basis(
            state,
            &milestone_achievement_basis_request(&revision, &payload.evidence_ids)?,
        )?
        .digest;
    if basis != expected_basis {
        return Err(error(
            ErrorCode::StaleBasis,
            "milestone achievement basis is stale",
        ));
    }
    let receipt = MilestoneAchievementRecord {
        achievement_id: payload.achievement_id.clone(),
        milestone_id: payload.milestone_id.clone(),
        milestone_revision_id: revision.revision_id.clone(),
        milestone_semantic_fingerprint: revision.semantic_fingerprint,
        milestone_proof_fingerprint: revision.proof_fingerprint,
        strategic_revision_id: revision.definition.strategic_revision_id.clone(),
        outcome_id: revision.definition.outcome_id.clone(),
        obligation_ids: revision.definition.required_obligation_ids.clone(),
        evidence_ids: payload.evidence_ids.clone(),
        relevant_basis: basis,
        acceptor: acceptor.clone(),
        summary: payload.summary.clone(),
        revision: zap_wire::Revision::new(1),
    };
    let expected = head.revision;
    head.latest_achievement_id = Some(receipt.achievement_id.clone());
    head.revision = head.revision.checked_next()?;
    changes.insert(receipt)?;
    changes.replace(expected, head)
}

pub(crate) fn validate_current_proof(
    state: &dyn StateReader,
    revision: &MilestoneRevisionRecord,
    evidence_ids: &[zap_wire::EvidenceId],
) -> Result<(), ZapError> {
    if evidence_ids.is_empty() || !strict_sorted(evidence_ids) {
        return Err(error(
            ErrorCode::NeedsEvidence,
            "milestone achievement requires sorted unique accepted evidence",
        ));
    }
    let outcome = state
        .get_typed::<crate::intent::OutcomeRecord>(&revision.definition.outcome_id)?
        .ok_or_else(|| error(ErrorCode::NeedsEvidence, "milestone outcome is unavailable"))?;
    if outcome.status != crate::seams::LifecycleStatus::Active
        || outcome.revision != revision.definition.outcome_revision
    {
        return Err(error(
            ErrorCode::NeedsEvidence,
            "milestone outcome scope is no longer current",
        ));
    }
    for obligation_id in &revision.definition.required_obligation_ids {
        let obligation = state
            .get_typed::<crate::control::ObligationRecord>(obligation_id)?
            .ok_or_else(|| {
                error(
                    ErrorCode::NeedsEvidence,
                    "milestone obligation is unavailable",
                )
            })?;
        if obligation.status != crate::seams::ObligationStatus::Active
            || obligation
                .current_outcomes
                .binary_search(&revision.definition.outcome_id)
                .is_err()
        {
            return Err(error(
                ErrorCode::NeedsEvidence,
                "milestone obligation scope is no longer current",
            ));
        }
    }
    let proof = current_proof_set(state, evidence_ids)?;
    let mut covered = BTreeSet::new();
    for evidence_id in evidence_ids {
        let row = proof.get(evidence_id).ok_or_else(|| {
            error(
                ErrorCode::NeedsEvidence,
                "milestone evidence is not current",
            )
        })?;
        if row.applies_to.outcome_id != revision.definition.outcome_id {
            return Err(error(
                ErrorCode::NeedsEvidence,
                "milestone evidence belongs to another outcome",
            ));
        }
        covered.extend(row.applies_to.obligation_ids.iter().cloned());
    }
    if !revision
        .definition
        .required_obligation_ids
        .iter()
        .all(|id| covered.contains(id))
    {
        return Err(error(
            ErrorCode::NeedsEvidence,
            "milestone evidence does not cover every required obligation",
        ));
    }
    Ok(())
}

fn validate_achievement_dependencies(
    state: &dyn StateReader,
    revision: &MilestoneRevisionRecord,
    visited: &mut HashSet<zap_wire::MilestoneId>,
) -> Result<(), ZapError> {
    if !visited.insert(revision.milestone_id.clone()) || visited.len() > 512 {
        return Err(error(
            ErrorCode::Conflict,
            "milestone achievement dependency closure is cyclic or too deep",
        ));
    }
    for dependency in revision
        .definition
        .dependencies
        .iter()
        .filter(|row| row.kind == MilestoneDependencyKind::AchievementPrerequisite)
    {
        let head = state
            .get_typed::<MilestoneRecord>(&dependency.milestone_id)?
            .ok_or_else(|| {
                error(
                    ErrorCode::NeedsEvidence,
                    "milestone prerequisite is missing",
                )
            })?;
        if head.current_revision_id != dependency.revision_id {
            return Err(error(
                ErrorCode::NeedsEvidence,
                "milestone prerequisite revision binding is stale",
            ));
        }
        let dependency_revision = state
            .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
            .ok_or_else(|| {
                error(
                    ErrorCode::NeedsEvidence,
                    "milestone prerequisite revision is missing",
                )
            })?;
        if dependency_revision.semantic_fingerprint != dependency.semantic_fingerprint {
            return Err(error(
                ErrorCode::NeedsEvidence,
                "milestone prerequisite fingerprint binding is stale",
            ));
        }
        let receipt_id = head.latest_achievement_id.ok_or_else(|| {
            error(
                ErrorCode::NeedsEvidence,
                "milestone prerequisite is not achieved",
            )
        })?;
        let receipt = state
            .get_typed::<MilestoneAchievementRecord>(&receipt_id)?
            .ok_or_else(|| {
                error(
                    ErrorCode::NeedsEvidence,
                    "milestone prerequisite receipt is missing",
                )
            })?;
        if milestone_achievement_validity_inner(state, &receipt, visited)?
            != MilestoneAchievementValidity::Current
        {
            return Err(error(
                ErrorCode::NeedsEvidence,
                "milestone prerequisite achievement is not current",
            ));
        }
    }
    visited.remove(&revision.milestone_id);
    Ok(())
}

pub(crate) fn milestone_achievement_validity_inner(
    state: &dyn StateReader,
    receipt: &MilestoneAchievementRecord,
    visited: &mut HashSet<zap_wire::MilestoneId>,
) -> Result<MilestoneAchievementValidity, ZapError> {
    let Some(head) = state.get_typed::<MilestoneRecord>(&receipt.milestone_id)? else {
        return Ok(MilestoneAchievementValidity::Unavailable);
    };
    let Some(revision) = state.get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
    else {
        return Ok(MilestoneAchievementValidity::Unavailable);
    };
    if revision.lifecycle() == MilestoneLifecycle::Retired {
        return Ok(MilestoneAchievementValidity::Retired);
    }
    if revision.proof_fingerprint != receipt.milestone_proof_fingerprint
        || revision.definition.required_obligation_ids != receipt.obligation_ids
        || revision.definition.outcome_id != receipt.outcome_id
        || validate_current_proof(state, &revision, &receipt.evidence_ids).is_err()
        || validate_achievement_dependencies(state, &revision, visited).is_err()
    {
        return Ok(MilestoneAchievementValidity::NeedsRevalidation);
    }
    let current_basis = DomainBasisProvider
        .relevant_basis(
            state,
            &milestone_achievement_basis_request(&revision, &receipt.evidence_ids)?,
        )?
        .digest;
    Ok(if current_basis == receipt.relevant_basis {
        MilestoneAchievementValidity::Current
    } else {
        MilestoneAchievementValidity::NeedsRevalidation
    })
}

pub(crate) fn milestone_achievement_basis_request(
    revision: &MilestoneRevisionRecord,
    evidence_ids: &[zap_wire::EvidenceId],
) -> Result<BasisRequest, ZapError> {
    let mut roots = BTreeSet::from([SubjectRef::Outcome(revision.definition.outcome_id.clone())]);
    roots.extend(
        revision
            .definition
            .required_obligation_ids
            .iter()
            .cloned()
            .map(SubjectRef::Obligation),
    );
    roots.extend(evidence_ids.iter().cloned().map(SubjectRef::Evidence));
    roots.extend(revision.definition.consumers.iter().cloned());
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(MILESTONE_ACHIEVEMENT_ACCEPTED_KIND)?),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn strict_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn error(code: ErrorCode, message: &'static str) -> ZapError {
    super::validation::error(code, message)
}
