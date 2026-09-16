use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    BasisProvider, CellSet, ChangeSet, CommandPayload, StateReader, StateReaderExt, StoredRecord,
    TransitionCell, ValidatedCommand,
};
use zap_wire::{
    EffectId, ErrorCode, EventId, EventKind, OperationId, PayloadDigest, RelevantBasisDigest,
    Revision, RouteClass, ZapError,
};

use super::{MilestonePlanContent, MilestonePlanKey, MilestonePlanProposalRecord};
use crate::knowledge::DomainBasisProvider;
use crate::seams::{DomainMutation, cell_descriptor, impl_canonical, impl_stored_record};

const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE";
pub(crate) const COMPOSITE_PLAN_RECORDED_KIND: &str = "milestone.composite-plan-recorded";

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct CompositeSuccessorPlanIntent {
    pub key: MilestonePlanKey,
    pub previous: Option<MilestonePlanKey>,
    pub strategic_revision_id: zap_wire::StrategicRevisionId,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub outcome_revision: Revision,
    pub expected_plan_state_revision: Option<Revision>,
    pub content: MilestonePlanContent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct CompositePlanCandidateRecorded {
    pub operation_id: OperationId,
    pub request_digest: PayloadDigest,
    pub expected_plan_state_revision: Option<Revision>,
    pub plan: MilestonePlanProposalRecord,
    pub binding: CompositePlanCandidateBindingRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct CompositePrecursorEffectBinding {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload_digest: PayloadDigest,
    pub predecessors: Vec<EffectId>,
    pub product_event_id: EventId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct CompositePlanCandidateBindingRecord {
    pub plan_key: MilestonePlanKey,
    pub operation_id: OperationId,
    pub request_digest: PayloadDigest,
    pub precursors: Vec<CompositePrecursorEffectBinding>,
    pub revision: Revision,
}

impl_canonical!(CompositeSuccessorPlanIntent);
impl_canonical!(CompositePlanCandidateRecorded);
impl_canonical!(CompositePrecursorEffectBinding);
impl_canonical!(CompositePlanCandidateBindingRecord);
impl_stored_record!(
    CompositePlanCandidateBindingRecord,
    MilestonePlanKey,
    plan_key,
    revision,
    "zap.milestone.composite-plan-binding"
);

impl CommandPayload for CompositePlanCandidateRecorded {
    const KIND: &'static str = COMPOSITE_PLAN_RECORDED_KIND;
}

pub fn prepare_composite_successor_plan(
    state: &dyn StateReader,
    intent: &CompositeSuccessorPlanIntent,
    candidate_revision: Revision,
) -> Result<MilestonePlanProposalRecord, ZapError> {
    validate_plan_state(state, intent)?;
    let mut plan = MilestonePlanProposalRecord {
        key: intent.key.clone(),
        previous: intent.previous.clone(),
        strategic_revision_id: intent.strategic_revision_id.clone(),
        strategic_record_revision: intent.strategic_record_revision,
        strategic_semantic_digest: intent.strategic_semantic_digest,
        outcome_revision: intent.outcome_revision,
        relevant_basis: RelevantBasisDigest::hash(b"composite-successor-basis-placeholder"),
        content: intent.content.clone(),
        semantic_fingerprint: PayloadDigest::hash(b"composite-successor-fingerprint-placeholder"),
        revision: candidate_revision,
    };
    let basis_request = super::cells::plan_basis(state, &plan)?;
    plan.relevant_basis = DomainBasisProvider
        .relevant_basis(state, &basis_request)?
        .digest;
    plan.semantic_fingerprint = super::milestone_plan_fingerprint(&plan)?;
    super::validation::validate_plan_proposal(state, &plan)?;
    Ok(plan)
}

fn validate_plan_state(
    state: &dyn StateReader,
    intent: &CompositeSuccessorPlanIntent,
) -> Result<(), ZapError> {
    let current = state.get_typed::<super::MilestonePlanStateRecord>(&intent.key.outcome_id)?;
    match (
        &current,
        intent.expected_plan_state_revision,
        &intent.previous,
    ) {
        (None, None, None) => Ok(()),
        (Some(row), Some(expected), Some(previous))
            if row.revision == expected && &row.adopted_plan == previous =>
        {
            Ok(())
        }
        _ => Err(error(
            ErrorCode::StaleRevision,
            "successor plan state or lineage is stale",
        )),
    }
}

struct CompositePlanCandidateRecordedCell;

impl TransitionCell for CompositePlanCandidateRecordedCell {
    type Payload = CompositePlanCandidateRecorded;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[
                MilestonePlanProposalRecord::FAMILY,
                CompositePlanCandidateBindingRecord::FAMILY,
            ],
            REQUIREMENT,
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
        if payload.plan.revision != command.header().expected_revision().checked_next()?
            || state
                .get_typed::<MilestonePlanProposalRecord>(&payload.plan.key)?
                .is_some()
        {
            return Err(error(
                ErrorCode::StaleRevision,
                "composite successor candidate identity or revision is stale",
            ));
        }
        changes.insert(payload.plan.clone())?;
        changes.insert(payload.binding.clone())?;
        Ok(DomainMutation {
            revision: payload.plan.revision,
        })
    }
}

pub(crate) fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::single(CompositePlanCandidateRecordedCell)
}

fn error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        REQUIREMENT,
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
