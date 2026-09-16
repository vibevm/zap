use specmark::spec;

use std::collections::BTreeSet;

use serde::Serialize;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, CandidateProvenanceRecord,
    ClosureRequirement, ContextRequirement, EffectState, StateReader, StateReaderExt,
    WorkExecutionObservationRecord,
};
use zap_wire::{
    CanonicalOutput, CodecEpoch, PayloadDigest, Revision, SubjectRef, WorkId, ZapError,
};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{DeferralRecord, ObligationRecord, TaskContractRecord, WorkRecord};
use crate::dreamer::*;
use crate::knowledge::{ClosureStatus, DomainBasisProvider, KnowledgeClosureRecord};
use crate::lowering::{
    LoweringRecord, PlanningRevisionState, StrategicNode, StrategicPlanRecord, strategy_digest,
    validate_strategy,
};
use crate::seams::{ObligationStatus, scan_all};

mod scope;

use scope::{
    DreamScope, attachment_and_grill, current_strategy, dream_scope, projection_measures,
    removal_plans_complete, validate_draft,
};

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
pub fn delta_digest(delta: &DreamDelta) -> Result<PayloadDigest, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, delta)?.digest())
}

pub(crate) fn create_branch(
    state: &dyn StateReader,
    draft: &DreamDraft,
    intent: DreamIntent,
) -> Result<DreamBranchRecord, ZapError> {
    validate_draft(draft, &intent)?;
    validate_charter_requirement(state, draft, &intent)?;
    if state
        .get_typed::<DreamBranchRecord>(&draft.dream_id)?
        .is_some()
    {
        return Err(dream_error("dream identity already exists"));
    }
    let strategy = current_strategy(state, &draft.base_strategic_revision)?;
    let (attachment, grill) = attachment_and_grill(&draft.attachment)?;
    let scope = dream_scope(state, &strategy, &attachment, &draft.delta)?;
    Ok(DreamBranchRecord {
        dream_id: draft.dream_id.clone(),
        base_revision: state.revision(),
        base_strategic_revision: strategy.strategic_revision_id,
        base_strategy_record_revision: strategy.revision,
        base_strategy_semantic_digest: strategy.semantic_digest,
        base_scope_digest: scope.digest,
        summary: draft.summary.clone(),
        attachment,
        intent,
        grill,
        delta: draft.delta.clone(),
        delta_digest: delta_digest(&draft.delta)?,
        assumptions: draft.assumptions.clone(),
        unknowns: draft.unknowns.clone(),
        alternatives: draft.alternatives.clone(),
        estimate: draft.estimate.clone(),
        required_charter_change: draft.required_charter_change.clone(),
        status: DreamStatus::Exploring,
        revision: Revision::new(1),
    })
}

fn validate_charter_requirement(
    state: &dyn StateReader,
    draft: &DreamDraft,
    intent: &DreamIntent,
) -> Result<(), ZapError> {
    let Some(required) = &draft.required_charter_change else {
        return Ok(());
    };
    let active = scan_all::<crate::intent::CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == crate::seams::LifecycleStatus::Active)
        .collect::<Vec<_>>();
    if !matches!(intent, DreamIntent::ExplicitScopeChange { .. })
        || active.len() != 1
        || active[0].charter_id != required.original_charter_id
        || active[0].revision != required.original_revision
        || active[0].digest != required.original_digest
        || required.required_actions.is_empty()
        || !required
            .required_actions
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || required.required_mutable_obligations.is_empty()
        || !required
            .required_mutable_obligations
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    {
        return Err(dream_error(
            "combined Dream charter requirement is not the exact active scoped expansion",
        ));
    }
    Ok(())
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
pub fn project_dream(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
) -> Result<DreamProjection, ZapError> {
    project_dream_with_policy(state, branch, ContextRequirement::Required)
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
pub fn project_dream_for_combined(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
) -> Result<DreamProjection, ZapError> {
    if branch.required_charter_change.is_none() {
        return Err(dream_error(
            "combined projection requires an exact charter-change requirement",
        ));
    }
    project_dream_with_policy(state, branch, ContextRequirement::NotApplicable)
}

fn project_dream_with_policy(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
    policy: ContextRequirement,
) -> Result<DreamProjection, ZapError> {
    let strategy = current_strategy(state, &branch.base_strategic_revision)?;
    let scope = dream_scope(state, &strategy, &branch.attachment, &branch.delta)?;
    let stale = scope.digest != branch.base_scope_digest;
    let mut unresolved = branch.unknowns.clone();
    unresolved.extend(scope.derived_unknowns.clone());
    let removal_complete = removal_plans_complete(state, &strategy, &branch.delta, &scope)?;
    if !removal_complete {
        unresolved.push(DreamUnknown {
            subject: SubjectRef::Dream(branch.dream_id.clone()),
            question: zap_wire::BoundedText::parse(
                "Removal dispositions do not cover the exact current dependent scope",
            )?,
            resolution_action: zap_wire::BoundedText::parse(
                "Recalculate and record every obligation, dependent, proof, artifact, debt, deferral and live effect",
            )?,
            disposition: DreamUnknownDisposition::AdmissionCritical,
        });
    }
    unresolved.sort_by(|left, right| left.subject.cmp(&right.subject));
    unresolved
        .dedup_by(|left, right| left.subject == right.subject && left.question == right.question);
    let mut affected_subjects = scope.subjects.clone();
    affected_subjects.extend(unresolved.iter().map(|unknown| unknown.subject.clone()));
    affected_subjects.sort();
    affected_subjects.dedup();
    let basis_request = dream_basis_request(branch, &strategy, &scope, policy)?;
    let relevant_basis = DomainBasisProvider
        .relevant_basis(state, &basis_request)?
        .digest;
    let ready = branch.status == DreamStatus::Ready
        && matches!(
            branch.grill,
            GrillState::Declined { .. } | GrillState::Complete { .. }
        )
        && matches!(branch.attachment, DreamAttachmentState::Exact { .. });
    let has_admission_critical_unknown = unresolved
        .iter()
        .any(|unknown| unknown.disposition == DreamUnknownDisposition::AdmissionCritical);
    let promotable = ready
        && matches!(branch.intent, DreamIntent::ExplicitScopeChange { .. })
        && branch.estimate.is_some()
        && !stale
        && removal_complete
        && !has_admission_critical_unknown;
    let projected_strategy_digest = if !stale && removal_complete {
        let projected = projected_strategy(&strategy, branch, relevant_basis)?;
        Some(projected.semantic_digest)
    } else {
        None
    };
    let (value, cost, burden) = projection_measures(branch, &scope, &unresolved);
    let mut projection = DreamProjection {
        dream_id: branch.dream_id.clone(),
        base_revision: branch.base_revision,
        observed_revision: state.revision(),
        strategic_revision: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        scope_digest: scope.digest,
        delta_digest: branch.delta_digest,
        affected_work_ids: scope.affected_work_ids,
        dependent_work_ids: scope.dependent_work_ids,
        affected_subjects,
        preserved_evidence_ids: scope.evidence_ids,
        preserved_artifacts: scope.artifacts,
        affected_lowering_ids: scope.lowering_ids,
        unresolved,
        value,
        cost,
        burden,
        relevant_basis,
        projected_strategy_digest,
        rebased: !stale
            && (state.revision() != branch.base_revision
                || strategy.revision != branch.base_strategy_record_revision),
        stale,
        promotable,
        digest: PayloadDigest::hash(b"pending-dream-projection"),
    };
    projection.digest = projection_digest(&projection)?;
    Ok(projection)
}

pub(crate) fn projection_digest(projection: &DreamProjection) -> Result<PayloadDigest, ZapError> {
    #[derive(Serialize)]
    struct ProjectionDigestBody<'a> {
        dream_id: &'a zap_wire::DreamId,
        base_revision: Revision,
        strategic_revision: &'a zap_wire::StrategicRevisionId,
        strategic_record_revision: Revision,
        strategic_semantic_digest: PayloadDigest,
        scope_digest: PayloadDigest,
        delta_digest: PayloadDigest,
        affected_work_ids: &'a [WorkId],
        dependent_work_ids: &'a [WorkId],
        affected_subjects: &'a [SubjectRef],
        preserved_evidence_ids: &'a [zap_wire::EvidenceId],
        preserved_artifacts: &'a [zap_wire::ArtifactDigest],
        affected_lowering_ids: &'a [zap_wire::LoweringId],
        unresolved: &'a [DreamUnknown],
        value: &'a DreamValueProjection,
        cost: &'a DreamCostProjection,
        burden: &'a DreamBurdenProjection,
        relevant_basis: zap_wire::RelevantBasisDigest,
        projected_strategy_digest: Option<PayloadDigest>,
        rebased: bool,
        stale: bool,
        promotable: bool,
    }
    Ok(CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &ProjectionDigestBody {
            dream_id: &projection.dream_id,
            base_revision: projection.base_revision,
            strategic_revision: &projection.strategic_revision,
            strategic_record_revision: projection.strategic_record_revision,
            strategic_semantic_digest: projection.strategic_semantic_digest,
            scope_digest: projection.scope_digest,
            delta_digest: projection.delta_digest,
            affected_work_ids: &projection.affected_work_ids,
            dependent_work_ids: &projection.dependent_work_ids,
            affected_subjects: &projection.affected_subjects,
            preserved_evidence_ids: &projection.preserved_evidence_ids,
            preserved_artifacts: &projection.preserved_artifacts,
            affected_lowering_ids: &projection.affected_lowering_ids,
            unresolved: &projection.unresolved,
            value: &projection.value,
            cost: &projection.cost,
            burden: &projection.burden,
            relevant_basis: projection.relevant_basis,
            projected_strategy_digest: projection.projected_strategy_digest,
            rebased: projection.rebased,
            stale: projection.stale,
            promotable: projection.promotable,
        },
    )?
    .digest())
}

fn dream_basis_request(
    branch: &DreamBranchRecord,
    strategy: &StrategicPlanRecord,
    scope: &DreamScope,
    policy: ContextRequirement,
) -> Result<BasisRequest, ZapError> {
    let mut roots = scope.read_roots.clone();
    if roots.is_empty() {
        roots.push(SubjectRef::Outcome(strategy.outcome_id.clone()));
    }
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::DreamPromotion(branch.dream_id.clone()),
        roots,
        policy,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub(crate) fn application_basis_request(
    state: &dyn StateReader,
    payload: &DreamApplied,
) -> Result<BasisRequest, ZapError> {
    let branch = state
        .get_typed::<DreamBranchRecord>(&payload.dream_id)?
        .ok_or_else(|| dream_error("dream branch is missing"))?;
    let strategy = current_strategy(state, &branch.base_strategic_revision)?;
    let scope = dream_scope(state, &strategy, &branch.attachment, &branch.delta)?;
    let policy = match (&branch.required_charter_change, &payload.combined_charter) {
        (None, None) => ContextRequirement::Required,
        (Some(required), Some(binding))
            if required.original_charter_id == binding.original_charter_id
                && required.original_revision == binding.original_revision
                && required.original_digest == binding.original_digest
                && required.replacement_charter_id == binding.replacement_charter_id =>
        {
            ContextRequirement::NotApplicable
        }
        _ => {
            return Err(dream_error(
                "Dream charter requirement and combined binding do not match",
            ));
        }
    };
    dream_basis_request(&branch, &strategy, &scope, policy)
}

pub(crate) fn application_projection(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
    payload: &DreamApplied,
) -> Result<DreamProjection, ZapError> {
    if payload.combined_charter.is_some() {
        project_dream_for_combined(state, branch)
    } else {
        project_dream(state, branch)
    }
}

pub(crate) fn projected_strategy(
    current: &StrategicPlanRecord,
    branch: &DreamBranchRecord,
    relevant_basis: zap_wire::RelevantBasisDigest,
) -> Result<StrategicPlanRecord, ZapError> {
    let mut strategy = current.clone();
    for operation in &branch.delta.operations {
        apply_strategy_operation(&mut strategy, &branch.attachment, operation)?;
    }
    strategy
        .nodes
        .sort_by(|left, right| left.work_id.cmp(&right.work_id));
    strategy.previous = Some(current.strategic_revision_id.clone());
    strategy.revision = current.revision.checked_next()?;
    strategy.relevant_basis = relevant_basis;
    strategy.state = PlanningRevisionState::Candidate;
    strategy.semantic_digest = PayloadDigest::hash(b"pending-strategy");
    strategy.semantic_digest = strategy_digest(&strategy)?;
    validate_strategy(&strategy, Some(current))?;
    strategy.state = PlanningRevisionState::Current;
    Ok(strategy)
}

fn apply_strategy_operation(
    strategy: &mut StrategicPlanRecord,
    attachment: &DreamAttachmentState,
    operation: &DreamDeltaOperation,
) -> Result<(), ZapError> {
    match operation {
        DreamDeltaOperation::Add(add) => {
            validate_new_node(strategy, attachment, &add.node)?;
            strategy.nodes.push(add.node.clone());
        }
        DreamDeltaOperation::Remove(removal) => remove_node(strategy, removal)?,
        DreamDeltaOperation::Move(moved) => {
            let node = strategy
                .nodes
                .iter_mut()
                .find(|row| row.work_id == moved.work_id)
                .ok_or_else(|| dream_error("moved strategic node is missing"))?;
            if let DreamAttachment::Subgoal { parent_work_id } = &moved.attachment
                && node.depends_on.binary_search(parent_work_id).is_err()
            {
                node.depends_on.push(parent_work_id.clone());
                node.depends_on.sort();
            }
        }
        DreamDeltaOperation::Replace(replaced) => {
            remove_node(strategy, &replaced.removal)?;
            validate_new_node(strategy, attachment, &replaced.replacement)?;
            strategy.nodes.push(replaced.replacement.clone());
        }
    }
    Ok(())
}

fn remove_node(
    strategy: &mut StrategicPlanRecord,
    removal: &DreamRemovalPlan,
) -> Result<(), ZapError> {
    let index = strategy
        .nodes
        .iter()
        .position(|row| row.work_id == removal.removed_work_id)
        .ok_or_else(|| dream_error("removed strategic node is missing"))?;
    strategy.nodes.remove(index);
    for disposition in &removal.obligations {
        let successor = strategy
            .nodes
            .iter_mut()
            .find(|row| row.work_id == disposition.successor_work_id)
            .ok_or_else(|| dream_error("obligation successor is missing"))?;
        if successor
            .obligation_ids
            .binary_search(&disposition.obligation_id)
            .is_err()
        {
            successor
                .obligation_ids
                .push(disposition.obligation_id.clone());
            successor.obligation_ids.sort();
        }
    }
    for disposition in &removal.dependents {
        let dependent = strategy
            .nodes
            .iter_mut()
            .find(|row| row.work_id == disposition.dependent_work_id)
            .ok_or_else(|| dream_error("dependent strategic node is missing"))?;
        for dependency in &mut dependent.depends_on {
            if dependency == &removal.removed_work_id {
                *dependency = disposition.replacement_prerequisite_id.clone();
            }
        }
        dependent.depends_on.sort();
        dependent.depends_on.dedup();
    }
    Ok(())
}

fn validate_new_node(
    strategy: &StrategicPlanRecord,
    attachment: &DreamAttachmentState,
    node: &StrategicNode,
) -> Result<(), ZapError> {
    if strategy.nodes.iter().any(|row| row.work_id == node.work_id)
        || node.obligation_ids.is_empty()
        || !sorted_unique(&node.obligation_ids)
        || !sorted_unique(&node.depends_on)
        || node
            .depends_on
            .iter()
            .any(|dependency| !strategy.nodes.iter().any(|row| &row.work_id == dependency))
    {
        return Err(dream_error(
            "new dream node is duplicate or references missing scope",
        ));
    }
    if let DreamAttachmentState::Exact {
        attachment: DreamAttachment::Subgoal { parent_work_id },
    } = attachment
        && node.depends_on.binary_search(parent_work_id).is_err()
    {
        return Err(dream_error(
            "subgoal delta must name its exact parent dependency",
        ));
    }
    Ok(())
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
