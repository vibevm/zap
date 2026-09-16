use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    AttemptId, BaseId, CampaignId, CodecEpoch, DecisionId, EffectId, ErrorCode, ErrorDetail,
    FixSurface, GoalId, HoldId, JobId, PacketId, PauseId, QueryId, ResumeDigest, Revision, StoreId,
    WaitId, WorkId, ZapError,
};

use crate::StoreIdentity;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPACTION-RESUME");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub struct AcceptedBoundary {
    pub revision: Revision,
    pub accepted_work: Vec<WorkId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub struct AssignmentRef {
    pub work_id: WorkId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub struct QueryHandle {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub query_id: QueryId,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub enum AdmissibleAction {
    Reconcile { job_id: JobId },
    Dispatch { work_id: WorkId },
    Collect { job_id: JobId },
    Verify { work_id: WorkId },
    Review { work_id: WorkId },
    ApplyGoal { goal_id: GoalId },
    Wait { wait_id: WaitId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub struct ResumeViewInput {
    pub campaign_id: CampaignId,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub accepted_boundary: AcceptedBoundary,
    pub active_assignments: Vec<AssignmentRef>,
    pub pending_effects: Vec<EffectId>,
    pub holds: Vec<HoldId>,
    pub pauses: Vec<PauseId>,
    pub waits: Vec<WaitId>,
    pub relevant_decisions: Vec<DecisionId>,
    pub next_actions: Vec<AdmissibleAction>,
    pub navigation: Vec<QueryHandle>,
}

/// Deterministic bounded continuation state derived only from stored records.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPACTION-RESUME")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#resume-projection")]
pub struct ResumeView {
    pub campaign_id: CampaignId,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub accepted_boundary: AcceptedBoundary,
    pub active_assignments: Vec<AssignmentRef>,
    pub pending_effects: Vec<EffectId>,
    pub holds: Vec<HoldId>,
    pub pauses: Vec<PauseId>,
    pub waits: Vec<WaitId>,
    pub relevant_decisions: Vec<DecisionId>,
    pub next_actions: Vec<AdmissibleAction>,
    pub navigation: Vec<QueryHandle>,
    pub digest: ResumeDigest,
}

impl ResumeView {
    /// Canonicalizes set-like rows and computes the exact resume digest.
    pub fn build(mut input: ResumeViewInput) -> Result<Self, ZapError> {
        if input.campaign_id != input.store.campaign_id
            || input.accepted_boundary.revision > input.revision
        {
            return Err(conflicting_resume_identity());
        }
        input.accepted_boundary.accepted_work.sort();
        input.accepted_boundary.accepted_work.dedup();
        input
            .active_assignments
            .sort_by(|left, right| left.job_id.cmp(&right.job_id));
        if input
            .active_assignments
            .windows(2)
            .any(|pair| pair[0].job_id == pair[1].job_id && pair[0] != pair[1])
        {
            return Err(conflicting_resume_identity());
        }
        input
            .active_assignments
            .dedup_by(|left, right| left.job_id == right.job_id);
        sort_dedup(&mut input.pending_effects);
        sort_dedup(&mut input.holds);
        sort_dedup(&mut input.pauses);
        sort_dedup(&mut input.waits);
        sort_dedup(&mut input.relevant_decisions);
        sort_dedup(&mut input.next_actions);
        input
            .navigation
            .sort_by(|left, right| left.query_id.cmp(&right.query_id));
        if input
            .navigation
            .windows(2)
            .any(|pair| pair[0].query_id == pair[1].query_id && pair[0] != pair[1])
            || input.navigation.iter().any(|handle| {
                handle.store_id != input.store.store_id
                    || handle.base_id != input.store.base_id
                    || handle.revision != input.revision
            })
        {
            return Err(conflicting_resume_identity());
        }
        input
            .navigation
            .dedup_by(|left, right| left.query_id == right.query_id);

        let bytes = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &input)?;
        Ok(Self {
            campaign_id: input.campaign_id,
            store: input.store,
            revision: input.revision,
            accepted_boundary: input.accepted_boundary,
            active_assignments: input.active_assignments,
            pending_effects: input.pending_effects,
            holds: input.holds,
            pauses: input.pauses,
            waits: input.waits,
            relevant_decisions: input.relevant_decisions,
            next_actions: input.next_actions,
            navigation: input.navigation,
            digest: ResumeDigest::hash(bytes.as_bytes()),
        })
    }
}

fn sort_dedup<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

fn conflicting_resume_identity() -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPACTION-RESUME",
        "resume projection reused an assignment or query identity with conflicting fields",
        FixSurface::Store,
        ErrorDetail::None,
    )
}
