use std::collections::BTreeSet;

use zap_core::{
    AffectedJobCompleteness, AffectedScopeRequest, AffectedScopeView, CellDescriptor,
    CellDescriptorInput, CellRegistrationBuilder, CellSet, ChangeSet, CommandPayload,
    EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext, PayloadActionImpact,
    PayloadAffectedScope, PayloadBasisScope, RecordFamily, SafeState, StateReader, StateReaderExt,
    StoredRecord, ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, CanonicalOutput, CanonicalPayload, CodecEpoch, ControlClass,
    GrillDigest, ReducerEpoch, Revision, RouteClass, ZapError,
};

use crate::control::{DeferralRecord, ObligationRecord, WorkRecord};
use crate::dreamer::*;
use crate::economics::ChangeAssessmentRecord;
use crate::knowledge::{SourceCaptureStatus, SourceRecord};
use crate::lowering::{LoweringRecord, PacketState, PlanningRevisionState, WorkerPacketRecord};
use crate::seams::{DomainMutation, ObligationStatus, WorkState, scan_all};

use super::projection::{
    application_basis_request, application_projection, create_branch, projected_strategy,
};

const DREAM_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE";
const APPLY_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-EXACT-APPLICATION";

mod application;
mod combined;
mod grill;

use application::{
    DreamAppliedBasis, DreamAppliedCell, DreamAppliedEffect, DreamAppliedImpact, DreamAppliedScope,
};
use combined::DreamCombinedOwnerDecisionCell;
use grill::{
    DreamFactAnsweredCell, DreamGrillCompletedCell, DreamGrillDeclinedCell,
    DreamGrillQuestionSavedCell, DreamOwnerAnsweredCell, DreamRecalculatedCell,
};

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellSet::single(DreamExplorationStartedCell)?,
        CellSet::single(DreamScopeChangeRequestedCell)?,
        CellSet::single(DreamGrillQuestionSavedCell)?,
        CellSet::single(DreamFactAnsweredCell)?,
        CellSet::single(DreamOwnerAnsweredCell)?,
        CellSet::single(DreamGrillDeclinedCell)?,
        CellSet::single(DreamGrillCompletedCell)?,
        CellSet::single(DreamRecalculatedCell)?,
        CellSet::single(DreamCombinedOwnerDecisionCell)?,
        CellRegistrationBuilder::new(DreamAppliedCell)
            .basis(DreamAppliedBasis)?
            .affected_scope(DreamAppliedScope)?
            .effect_contract(DreamAppliedEffect)?
            .action_impact(DreamAppliedImpact)?
            .build()?,
    ])
}

struct DreamExplorationStartedCell;

impl zap_core::TransitionCell for DreamExplorationStartedCell {
    type Payload = DreamExplorationStarted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        changes.insert(create_branch(
            state,
            &command.payload().draft,
            DreamIntent::Hypothetical,
        )?)?;
        result(command)
    }
}

struct DreamScopeChangeRequestedCell;

impl zap_core::TransitionCell for DreamScopeChangeRequestedCell {
    type Payload = DreamScopeChangeRequested;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        changes.insert(create_branch(
            state,
            &command.payload().draft,
            DreamIntent::ExplicitScopeChange {
                operation: command.payload().operation,
            },
        )?)?;
        result(command)
    }
}

pub(super) fn current_branch(
    state: &dyn StateReader,
    dream_id: &zap_wire::DreamId,
    expected: Revision,
) -> Result<DreamBranchRecord, ZapError> {
    let branch = state
        .get_typed::<DreamBranchRecord>(dream_id)?
        .ok_or_else(|| dream_error("dream branch is missing"))?;
    if branch.revision != expected
        || matches!(
            branch.status,
            DreamStatus::Applied | DreamStatus::Rejected | DreamStatus::Withdrawn
        )
    {
        return Err(dream_error("dream branch revision or lifecycle is stale"));
    }
    Ok(branch)
}

pub(super) fn result<P: zap_core::CommandPayload>(
    command: &ValidatedCommand<P>,
) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

pub(super) fn descriptor(
    kind: &str,
    route: RouteClass,
    families: &[&str],
) -> Result<CellDescriptor, ZapError> {
    descriptor_with_requirement(kind, route, families, DREAM_REQ)
}

pub(super) fn descriptor_with_requirement(
    kind: &str,
    route: RouteClass,
    families: &[&str],
    requirement: &str,
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    let affected_indexes = crate::viewer_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: zap_wire::EventKind::parse(kind)?,
        route,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![zap_wire::RequirementRef::parse(requirement)?],
        requires_completion: false,
    })
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
