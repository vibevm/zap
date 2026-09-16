use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    Completeness, Page, QuerySet, QuerySnapshot, QuerySpec, StateReader, StateReaderExt,
};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, MilestoneId, PayloadDigest, ZapError};

use super::*;
use crate::seams::impl_canonical;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneTransformPreviewInput {
    pub plan: MilestoneTransformPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneTransformInput {
    pub operation_id: zap_wire::OperationId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneTransformView {
    pub transform: MilestoneTransformRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneTransformPreview {
    pub operation_id: zap_wire::OperationId,
    pub before_revision_ids: Vec<zap_wire::MilestoneRevisionId>,
    pub after_revision_ids: Vec<zap_wire::MilestoneRevisionId>,
    pub proof_retained_milestone_ids: Vec<MilestoneId>,
    pub obligations_conserved: bool,
    pub consumers_conserved: bool,
    pub contributions_conserved: bool,
    pub removed_contributions: Vec<MilestoneOwnedContribution>,
    pub added_contributions: Vec<MilestoneOwnedContribution>,
    pub removed_dependencies: Vec<MilestoneDependencyEdge>,
    pub added_dependencies: Vec<MilestoneDependencyEdge>,
    pub dependent_scan_is_store_wide: bool,
    pub preview_digest: PayloadDigest,
}

impl_canonical!(MilestoneTransformPreviewInput);
impl_canonical!(MilestoneTransformPreview);
impl_canonical!(MilestoneTransformInput);
impl_canonical!(MilestoneTransformView);

pub(crate) struct PreparedTransform {
    pub preview: MilestoneTransformPreview,
    pub before: BTreeMap<MilestoneId, MilestoneRevisionRecord>,
}

pub(crate) fn prepare_transform(
    state: &dyn StateReader,
    plan: &MilestoneTransformPlan,
) -> Result<PreparedTransform, ZapError> {
    if plan.changes.is_empty()
        || plan.changes.len() > 512
        || plan.reason.as_str().trim().is_empty()
        || !strict_sorted_by(&plan.changes, |row| &row.milestone_id)
        || !strict_sorted(&plan.affected_work_ids)
        || !strict_sorted(&plan.affected_subjects)
        || state
            .get_typed::<MilestoneTransformRecord>(&plan.operation_id)?
            .is_some()
    {
        return Err(transform_error(
            ErrorCode::InvalidValue,
            "milestone transform identity, order, bounds, or reason is invalid",
        ));
    }
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    let mut revision_ids = BTreeSet::new();
    for change in &plan.changes {
        if !revision_ids.insert(change.new_revision_id.clone())
            || state
                .get_typed::<MilestoneRevisionRecord>(&change.new_revision_id)?
                .is_some()
        {
            return Err(transform_error(
                ErrorCode::DuplicateIdentity,
                "milestone transform revision identity already exists",
            ));
        }
        let head = state.get_typed::<MilestoneRecord>(&change.milestone_id)?;
        match (
            &head,
            change.expected_head_revision,
            &change.expected_current_revision_id,
            change.expected_current_fingerprint,
        ) {
            (None, None, None, None) => {}
            (
                Some(head),
                Some(expected_head),
                Some(expected_revision),
                Some(expected_fingerprint),
            ) => {
                let revision = state
                    .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
                    .ok_or_else(|| {
                        transform_error(
                            ErrorCode::MissingReference,
                            "milestone transform source revision is missing",
                        )
                    })?;
                if head.revision != expected_head
                    || &head.current_revision_id != expected_revision
                    || revision.semantic_fingerprint != expected_fingerprint
                    || !milestone_revision_is_self_consistent(&revision)?
                {
                    return Err(transform_error(
                        ErrorCode::StaleRevision,
                        "milestone transform source CAS is stale",
                    ));
                }
                before.insert(change.milestone_id.clone(), revision);
            }
            _ => {
                return Err(transform_error(
                    ErrorCode::StaleRevision,
                    "milestone transform change has incomplete CAS",
                ));
            }
        }
        after.insert(change.milestone_id.clone(), &change.definition);
    }
    let overlay = plan
        .changes
        .iter()
        .map(|change| {
            Ok((
                change.milestone_id.clone(),
                (
                    change.new_revision_id.clone(),
                    milestone_semantic_fingerprint(&change.milestone_id, &change.definition)?,
                    change.definition.clone(),
                ),
            ))
        })
        .collect::<Result<super::validation::MilestoneOverlay, ZapError>>()?;
    for change in &plan.changes {
        super::validation::validate_definition_with_overlay(
            state,
            &change.milestone_id,
            &change.definition,
            &overlay,
        )?;
    }
    super::transform_validation::validate_complete_dependent_rewrites(state, &overlay)?;
    let diff = super::transform_validation::validate_transform(state, plan, &before, &after)?;
    let proof_retained_milestone_ids = before
        .iter()
        .filter_map(|(id, old)| {
            after.get(id).and_then(|next| {
                (old.proof_fingerprint == milestone_proof_fingerprint(id, next).ok()?)
                    .then(|| id.clone())
            })
        })
        .collect::<Vec<_>>();
    let before_revision_ids = before.values().map(|row| row.revision_id.clone()).collect();
    let after_revision_ids = plan
        .changes
        .iter()
        .map(|row| row.new_revision_id.clone())
        .collect::<Vec<_>>();
    #[derive(Serialize)]
    struct DigestBody<'a> {
        algorithm: &'static str,
        plan: &'a MilestoneTransformPlan,
        before: &'a BTreeMap<MilestoneId, MilestoneRevisionRecord>,
        after_revision_ids: &'a [zap_wire::MilestoneRevisionId],
    }
    let preview_digest = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &DigestBody {
            algorithm: "zap.milestone.transform-preview/v1",
            plan,
            before: &before,
            after_revision_ids: &after_revision_ids,
        },
    )?
    .digest();
    Ok(PreparedTransform {
        preview: MilestoneTransformPreview {
            operation_id: plan.operation_id.clone(),
            before_revision_ids,
            after_revision_ids,
            proof_retained_milestone_ids,
            obligations_conserved: true,
            consumers_conserved: true,
            contributions_conserved: true,
            removed_contributions: diff.removed_contributions,
            added_contributions: diff.added_contributions,
            removed_dependencies: diff.removed_dependencies,
            added_dependencies: diff.added_dependencies,
            dependent_scan_is_store_wide: true,
            preview_digest,
        },
        before,
    })
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneTransformPreviewQuery;
impl QuerySpec for MilestoneTransformPreviewQuery {
    type Input = MilestoneTransformPreviewInput;
    type Item = MilestoneTransformPreview;
    const ID: &'static str = "zap.milestone.transform-preview";
    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let preview = prepare_transform(snapshot, &input.plan)?.preview;
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![preview],
            completeness: Completeness::Complete,
        })
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#queries")]
pub struct MilestoneTransformQuery;
impl QuerySpec for MilestoneTransformQuery {
    type Input = MilestoneTransformInput;
    type Item = MilestoneTransformView;
    const ID: &'static str = "zap.milestone.transform";
    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let transform = snapshot
            .get_typed::<MilestoneTransformRecord>(&input.operation_id)?
            .ok_or_else(|| {
                transform_error(
                    ErrorCode::MissingReference,
                    "milestone transform history is missing",
                )
            })?;
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![MilestoneTransformView { transform }],
            completeness: Completeness::Complete,
        })
    }
}

fn strict_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
fn strict_sorted_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}
fn transform_error(code: ErrorCode, message: &'static str) -> ZapError {
    super::validation::error(code, message)
}
pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::compose([
        QuerySet::single(MilestoneTransformPreviewQuery)?,
        QuerySet::single(MilestoneTransformQuery)?,
    ])
}
