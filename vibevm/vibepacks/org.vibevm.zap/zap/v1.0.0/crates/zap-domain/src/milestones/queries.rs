use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    Completeness, Page, QuerySet, QuerySnapshot, QuerySpec, StateReader, StateReaderExt,
};
use zap_wire::{MilestoneAchievementId, MilestoneId, MilestoneRevisionId, ZapError};

use super::{
    MilestoneAchievementRecord, MilestoneAchievementValidity, MilestoneRecord,
    MilestoneRevisionRecord,
};
use crate::seams::impl_canonical;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-QUERIES");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneReadInput {
    pub milestone_id: MilestoneId,
    pub evaluate_current_achievement: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub enum MilestoneProofEvaluationCost {
    NotRequested,
    StoreWideCurrentProofContextScan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneView {
    pub head: MilestoneRecord,
    pub current_revision: MilestoneRevisionRecord,
    pub latest_achievement: Option<MilestoneAchievementRecord>,
    pub current_validity: Option<MilestoneAchievementValidity>,
    pub proof_evaluation_cost: MilestoneProofEvaluationCost,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneAchievementInput {
    pub achievement_id: MilestoneAchievementId,
    pub evaluate_current_validity: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneAchievementView {
    pub receipt: MilestoneAchievementRecord,
    pub current_validity: Option<MilestoneAchievementValidity>,
    pub proof_evaluation_cost: MilestoneProofEvaluationCost,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneRevisionInput {
    pub revision_id: MilestoneRevisionId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneRevisionView {
    pub revision: MilestoneRevisionRecord,
    pub is_current: bool,
}

impl_canonical!(MilestoneReadInput);
impl_canonical!(MilestoneView);
impl_canonical!(MilestoneAchievementInput);
impl_canonical!(MilestoneAchievementView);
impl_canonical!(MilestoneRevisionInput);
impl_canonical!(MilestoneRevisionView);

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
pub fn milestone_achievement_validity(
    state: &dyn StateReader,
    receipt: &MilestoneAchievementRecord,
) -> Result<MilestoneAchievementValidity, ZapError> {
    super::achievement_core::milestone_achievement_validity_inner(
        state,
        receipt,
        &mut HashSet::new(),
    )
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneReadQuery;

impl QuerySpec for MilestoneReadQuery {
    type Input = MilestoneReadInput;
    type Item = MilestoneView;
    const ID: &'static str = "zap.milestone.read";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let head = snapshot
            .get_typed::<MilestoneRecord>(&input.milestone_id)?
            .ok_or_else(|| {
                super::validation::error(
                    zap_wire::ErrorCode::MissingReference,
                    "milestone query target is missing",
                )
            })?;
        let current_revision = snapshot
            .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
            .ok_or_else(|| {
                super::validation::error(
                    zap_wire::ErrorCode::MissingReference,
                    "milestone current revision is missing",
                )
            })?;
        if current_revision.milestone_id != head.milestone_id
            || !super::milestone_revision_is_self_consistent(&current_revision)?
        {
            return Err(super::validation::error(
                zap_wire::ErrorCode::Conflict,
                "milestone head and current revision disagree",
            ));
        }
        let latest_achievement = head
            .latest_achievement_id
            .as_ref()
            .map(|id| {
                snapshot
                    .get_typed::<MilestoneAchievementRecord>(id)?
                    .ok_or_else(|| {
                        super::validation::error(
                            zap_wire::ErrorCode::MissingReference,
                            "milestone latest achievement receipt is missing",
                        )
                    })
            })
            .transpose()?;
        if latest_achievement
            .as_ref()
            .is_some_and(|receipt| receipt.milestone_id != head.milestone_id)
        {
            return Err(super::validation::error(
                zap_wire::ErrorCode::Conflict,
                "milestone latest achievement belongs to another milestone",
            ));
        }
        let current_validity = if input.evaluate_current_achievement {
            latest_achievement
                .as_ref()
                .map(|receipt| milestone_achievement_validity(snapshot, receipt))
                .transpose()?
        } else {
            None
        };
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![MilestoneView {
                head,
                current_revision,
                latest_achievement,
                current_validity,
                proof_evaluation_cost: if input.evaluate_current_achievement {
                    MilestoneProofEvaluationCost::StoreWideCurrentProofContextScan
                } else {
                    MilestoneProofEvaluationCost::NotRequested
                },
            }],
            completeness: Completeness::Complete,
        })
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneAchievementQuery;

impl QuerySpec for MilestoneAchievementQuery {
    type Input = MilestoneAchievementInput;
    type Item = MilestoneAchievementView;
    const ID: &'static str = "zap.milestone.achievement";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let receipt = snapshot
            .get_typed::<MilestoneAchievementRecord>(&input.achievement_id)?
            .ok_or_else(|| {
                super::validation::error(
                    zap_wire::ErrorCode::MissingReference,
                    "milestone achievement receipt is missing",
                )
            })?;
        let current_validity = input
            .evaluate_current_validity
            .then(|| milestone_achievement_validity(snapshot, &receipt))
            .transpose()?;
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![MilestoneAchievementView {
                receipt,
                current_validity,
                proof_evaluation_cost: if input.evaluate_current_validity {
                    MilestoneProofEvaluationCost::StoreWideCurrentProofContextScan
                } else {
                    MilestoneProofEvaluationCost::NotRequested
                },
            }],
            completeness: Completeness::Complete,
        })
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneRevisionQuery;

impl QuerySpec for MilestoneRevisionQuery {
    type Input = MilestoneRevisionInput;
    type Item = MilestoneRevisionView;
    const ID: &'static str = "zap.milestone.revision";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let revision = snapshot
            .get_typed::<MilestoneRevisionRecord>(&input.revision_id)?
            .ok_or_else(|| {
                super::validation::error(
                    zap_wire::ErrorCode::MissingReference,
                    "milestone historical revision is missing",
                )
            })?;
        let is_current = snapshot
            .get_typed::<MilestoneRecord>(&revision.milestone_id)?
            .is_some_and(|head| head.current_revision_id == revision.revision_id);
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![MilestoneRevisionView {
                revision,
                is_current,
            }],
            completeness: Completeness::Complete,
        })
    }
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::compose([
        QuerySet::single(MilestoneReadQuery)?,
        QuerySet::single(MilestoneAchievementQuery)?,
        QuerySet::single(MilestoneRevisionQuery)?,
        super::transform_query::query_set()?,
    ])
}
