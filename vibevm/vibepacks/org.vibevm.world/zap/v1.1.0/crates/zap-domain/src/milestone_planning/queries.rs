use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{Completeness, Page, QuerySet, QuerySnapshot, QuerySpec, StateReaderExt};
use zap_wire::{MilestoneAchievementId, MilestoneRevisionId, ObligationId, OutcomeId, ZapError};

use super::{MilestonePlanKey, MilestonePlanProposalRecord, MilestonePlanStateRecord};
use crate::control::ObligationRecord;
use crate::lowering::StrategicPlanRecord;
use crate::milestones::{
    MilestoneAchievementRecord, MilestoneAchievementValidity, MilestoneRecord,
    MilestoneRevisionRecord, milestone_achievement_validity,
};
use crate::seams::{
    LifecycleStatus, ObligationDisposition, ObligationStatus, impl_canonical, scan_all,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-QUERIES");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub struct MilestonePlanViewInput {
    pub outcome_id: OutcomeId,
    pub plan_key: Option<MilestonePlanKey>,
    pub maximum_milestones: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub enum MilestonePlanViewStatus {
    NoAdoptedPlan,
    Proposal,
    Adopted,
    AdoptedNeedsReassessment,
    AllSatisfied,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub enum MilestonePlanGap {
    ProposalNotAdopted,
    PlanRecordMissing {
        plan_key: MilestonePlanKey,
    },
    PlanBindingStale,
    MilestoneRevisionMissing {
        revision_id: MilestoneRevisionId,
    },
    MilestoneRevisionStale {
        revision_id: MilestoneRevisionId,
    },
    MilestoneAchievementMissing {
        revision_id: MilestoneRevisionId,
        achievement_id: MilestoneAchievementId,
    },
    FocusMissing,
    FocusSatisfied {
        revision_id: MilestoneRevisionId,
    },
    CurrentOutcomeObligationUncovered {
        obligation_id: ObligationId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub struct MilestonePlanQueryCost {
    pub whole_plan_record_decoded: bool,
    pub milestone_records_decoded: u32,
    pub proof_validity_evaluations: u32,
    pub store_wide_proof_cost: StoreWideProofCost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub enum StoreWideProofCost {
    None,
    PerEvaluationWithRecursiveMilestoneDependencies,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub struct MilestonePlanView {
    pub outcome_id: OutcomeId,
    pub adopted_state: Option<MilestonePlanStateRecord>,
    pub plan: Option<MilestonePlanProposalRecord>,
    pub status: MilestonePlanViewStatus,
    pub focus: Option<MilestoneRevisionRecord>,
    pub focus_achievement_validity: Option<MilestoneAchievementValidity>,
    pub unsatisfied_milestone_revision_ids: Vec<MilestoneRevisionId>,
    pub distant_horizons: Vec<super::MilestonePlanHorizon>,
    pub gaps: Vec<MilestonePlanGap>,
    pub query_cost: MilestonePlanQueryCost,
}

impl_canonical!(MilestonePlanViewInput);
impl_canonical!(MilestonePlanView);

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#queries")]
pub struct MilestonePlanViewQuery;

impl QuerySpec for MilestonePlanViewQuery {
    type Input = MilestonePlanViewInput;
    type Item = MilestonePlanView;
    const ID: &'static str = "zap.milestone.plan";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        if input.maximum_milestones == 0
            || input.maximum_milestones > snapshot.limits().maximum_page_size
        {
            return Err(super::validation::error(
                zap_wire::ErrorCode::LimitExceeded,
                "milestone plan query limit is zero or exceeds the query profile",
            ));
        }
        let adopted_state = snapshot.get_typed::<MilestonePlanStateRecord>(&input.outcome_id)?;
        let selected_key = input
            .plan_key
            .clone()
            .or_else(|| adopted_state.as_ref().map(|row| row.adopted_plan.clone()));
        let plan = selected_key
            .as_ref()
            .map(|key| snapshot.get_typed::<MilestonePlanProposalRecord>(key))
            .transpose()?
            .flatten();
        if let Some(plan) = &plan
            && plan.content.milestone_revision_ids.len() > input.maximum_milestones as usize
        {
            return Err(super::validation::error(
                zap_wire::ErrorCode::LimitExceeded,
                "milestone plan exceeds the explicit query milestone budget",
            ));
        }
        if plan.is_none()
            && let Some(key) = selected_key
        {
            let is_adopted = adopted_state
                .as_ref()
                .is_some_and(|state| state.adopted_plan == key);
            let view = missing_plan_view(input, adopted_state, key, is_adopted);
            return Ok(page(snapshot, view));
        }
        let view = build_view(snapshot, input, adopted_state, plan)?;
        Ok(page(snapshot, view))
    }
}

fn build_view(
    snapshot: &dyn QuerySnapshot,
    input: &MilestonePlanViewInput,
    adopted_state: Option<MilestonePlanStateRecord>,
    plan: Option<MilestonePlanProposalRecord>,
) -> Result<MilestonePlanView, ZapError> {
    let Some(plan) = plan else {
        return Ok(MilestonePlanView {
            outcome_id: input.outcome_id.clone(),
            adopted_state,
            plan: None,
            status: MilestonePlanViewStatus::NoAdoptedPlan,
            focus: None,
            focus_achievement_validity: None,
            unsatisfied_milestone_revision_ids: Vec::new(),
            distant_horizons: Vec::new(),
            gaps: Vec::new(),
            query_cost: MilestonePlanQueryCost {
                whole_plan_record_decoded: false,
                milestone_records_decoded: 0,
                proof_validity_evaluations: 0,
                store_wide_proof_cost: StoreWideProofCost::None,
            },
        });
    };
    let is_adopted = adopted_state.as_ref().is_some_and(|state| {
        state.adopted_plan == plan.key && state.adopted_fingerprint == plan.semantic_fingerprint
    });
    let mut gaps = Vec::new();
    if !is_adopted {
        gaps.push(MilestonePlanGap::ProposalNotAdopted);
    }
    if plan.key.outcome_id != input.outcome_id || plan_binding_stale(snapshot, &plan)? {
        gaps.push(MilestonePlanGap::PlanBindingStale);
    }
    let mut unsatisfied = Vec::new();
    let mut focus = None;
    let mut focus_validity = None;
    let mut milestone_records_decoded = 0_u32;
    let mut proof_validity_evaluations = 0_u32;
    for revision_id in &plan.content.milestone_revision_ids {
        let Some(revision) = snapshot.get_typed::<MilestoneRevisionRecord>(revision_id)? else {
            gaps.push(MilestonePlanGap::MilestoneRevisionMissing {
                revision_id: revision_id.clone(),
            });
            continue;
        };
        milestone_records_decoded = milestone_records_decoded.saturating_add(1);
        let head = snapshot.get_typed::<MilestoneRecord>(&revision.milestone_id)?;
        if head
            .as_ref()
            .is_none_or(|row| row.current_revision_id != *revision_id)
        {
            gaps.push(MilestonePlanGap::MilestoneRevisionStale {
                revision_id: revision_id.clone(),
            });
        }
        let achievement_id = head
            .as_ref()
            .and_then(|row| row.latest_achievement_id.as_ref());
        let receipt = achievement_id
            .map(|id| snapshot.get_typed::<MilestoneAchievementRecord>(id))
            .transpose()?
            .flatten();
        if let Some(achievement_id) = achievement_id
            && receipt.is_none()
        {
            gaps.push(MilestonePlanGap::MilestoneAchievementMissing {
                revision_id: revision_id.clone(),
                achievement_id: achievement_id.clone(),
            });
        }
        if receipt.is_some() {
            proof_validity_evaluations = proof_validity_evaluations.saturating_add(1);
        }
        let validity = receipt
            .map(|receipt| milestone_achievement_validity(snapshot, &receipt))
            .transpose()?;
        if validity != Some(MilestoneAchievementValidity::Current) {
            unsatisfied.push(revision_id.clone());
        }
        if plan.content.focus_milestone_revision_id.as_ref() == Some(revision_id) {
            if validity == Some(MilestoneAchievementValidity::Current) {
                gaps.push(MilestonePlanGap::FocusSatisfied {
                    revision_id: revision_id.clone(),
                });
            }
            focus = Some(revision);
            focus_validity = validity;
        }
    }
    if plan.content.focus_milestone_revision_id.is_some() && focus.is_none() {
        gaps.push(MilestonePlanGap::FocusMissing);
    }
    let covered = plan
        .content
        .obligation_coverage
        .iter()
        .map(|row| row.obligation_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    gaps.extend(
        current_outcome_obligations(snapshot, &input.outcome_id)?
            .into_iter()
            .filter(|id| !covered.contains(id))
            .map(
                |obligation_id| MilestonePlanGap::CurrentOutcomeObligationUncovered {
                    obligation_id,
                },
            ),
    );
    let status = if is_adopted && unsatisfied.is_empty() && !has_binding_gap(&gaps) {
        MilestonePlanViewStatus::AllSatisfied
    } else if is_adopted && gaps.is_empty() {
        MilestonePlanViewStatus::Adopted
    } else if is_adopted {
        MilestonePlanViewStatus::AdoptedNeedsReassessment
    } else {
        MilestonePlanViewStatus::Proposal
    };
    let distant_horizons = plan.content.horizons.clone();
    Ok(MilestonePlanView {
        outcome_id: input.outcome_id.clone(),
        adopted_state,
        plan: Some(plan),
        status,
        focus,
        focus_achievement_validity: focus_validity,
        unsatisfied_milestone_revision_ids: unsatisfied,
        distant_horizons,
        gaps,
        query_cost: MilestonePlanQueryCost {
            whole_plan_record_decoded: true,
            milestone_records_decoded,
            proof_validity_evaluations,
            store_wide_proof_cost: if proof_validity_evaluations == 0 {
                StoreWideProofCost::None
            } else {
                StoreWideProofCost::PerEvaluationWithRecursiveMilestoneDependencies
            },
        },
    })
}

fn missing_plan_view(
    input: &MilestonePlanViewInput,
    adopted_state: Option<MilestonePlanStateRecord>,
    plan_key: MilestonePlanKey,
    is_adopted: bool,
) -> MilestonePlanView {
    MilestonePlanView {
        outcome_id: input.outcome_id.clone(),
        adopted_state,
        plan: None,
        status: if is_adopted {
            MilestonePlanViewStatus::AdoptedNeedsReassessment
        } else {
            MilestonePlanViewStatus::Proposal
        },
        focus: None,
        focus_achievement_validity: None,
        unsatisfied_milestone_revision_ids: Vec::new(),
        distant_horizons: Vec::new(),
        gaps: vec![MilestonePlanGap::PlanRecordMissing { plan_key }],
        query_cost: MilestonePlanQueryCost {
            whole_plan_record_decoded: false,
            milestone_records_decoded: 0,
            proof_validity_evaluations: 0,
            store_wide_proof_cost: StoreWideProofCost::None,
        },
    }
}

fn has_binding_gap(gaps: &[MilestonePlanGap]) -> bool {
    gaps.iter().any(|gap| {
        !matches!(
            gap,
            MilestonePlanGap::FocusSatisfied { .. } | MilestonePlanGap::ProposalNotAdopted
        )
    })
}

fn page(snapshot: &dyn QuerySnapshot, view: MilestonePlanView) -> Page<MilestonePlanView> {
    Page {
        store: snapshot.identity(),
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        items: vec![view],
        completeness: Completeness::Complete,
    }
}

fn plan_binding_stale(
    snapshot: &dyn QuerySnapshot,
    plan: &MilestonePlanProposalRecord,
) -> Result<bool, ZapError> {
    let strategy = snapshot.get_typed::<StrategicPlanRecord>(&plan.strategic_revision_id)?;
    let outcome = snapshot.get_typed::<crate::intent::OutcomeRecord>(&plan.key.outcome_id)?;
    Ok(strategy.as_ref().is_none_or(|row| {
        row.revision != plan.strategic_record_revision
            || row.semantic_digest != plan.strategic_semantic_digest
            || row.state == crate::lowering::PlanningRevisionState::Superseded
    }) || outcome.as_ref().is_none_or(|row| {
        row.revision != plan.outcome_revision || row.status != LifecycleStatus::Active
    }))
}

fn current_outcome_obligations(
    snapshot: &dyn QuerySnapshot,
    outcome_id: &OutcomeId,
) -> Result<Vec<ObligationId>, ZapError> {
    Ok(scan_all::<ObligationRecord>(snapshot)?
        .into_iter()
        .filter(|row| {
            row.status == ObligationStatus::Active
                && row.disposition == ObligationDisposition::Retained
                && row.current_outcomes.binary_search(outcome_id).is_ok()
        })
        .map(|row| row.obligation_id)
        .collect())
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(MilestonePlanViewQuery)
}
