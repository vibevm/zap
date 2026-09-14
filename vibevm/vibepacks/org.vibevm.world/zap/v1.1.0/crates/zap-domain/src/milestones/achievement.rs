use zap_core::{
    ChangeSet, CommandPayload, PayloadBasisScope, StateReader, StoredRecord, TransitionCell,
    ValidatedCommand,
};
use zap_wire::{ActionClass, BasisBinding, ErrorCode, RouteClass, ZapError};

use super::*;
use crate::seams::{DomainMutation, cell_descriptor};

const ACCEPTANCE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ACCEPTANCE";

pub(super) struct AcceptAchievementCell;

impl TransitionCell for AcceptAchievementCell {
    type Payload = MilestoneAchievementAccepted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("work.accept")?),
            &[MilestoneAchievementRecord::FAMILY, MilestoneRecord::FAMILY],
            ACCEPTANCE_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let acceptor = command.authority().actor().ok_or_else(|| {
            super::validation::error(
                ErrorCode::Unauthorized,
                "milestone achievement requires admitted acceptance authority",
            )
        })?;
        let basis = match command.header().basis() {
            BasisBinding::Exact(digest) => *digest,
            BasisBinding::NotApplicable => {
                return Err(super::validation::error(
                    ErrorCode::StaleBasis,
                    "milestone achievement requires an exact current proof basis",
                ));
            }
        };
        super::achievement_core::apply_achievement(
            state,
            command.payload(),
            acceptor,
            basis,
            changes,
        )?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

pub(super) struct AchievementBasis;

impl PayloadBasisScope<MilestoneAchievementAccepted> for AchievementBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &MilestoneAchievementAccepted,
    ) -> Result<zap_core::BasisRequest, ZapError> {
        let revision = load_current_milestone_revision(state, &payload.milestone_revision_id)?;
        super::achievement_core::milestone_achievement_basis_request(
            &revision,
            &payload.evidence_ids,
        )
    }
}

pub(super) fn cell_set() -> Result<zap_core::CellSet, ZapError> {
    use zap_core::{ActionImpactRequest, ActionImpactRule, CellRegistrationBuilder};
    use zap_wire::SubjectRef;

    CellRegistrationBuilder::new(AcceptAchievementCell)
        .basis(AchievementBasis)?
        .action_impact(crate::seams::TypedActionImpact::new(
            |payload: &MilestoneAchievementAccepted| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    Vec::new(),
                    payload
                        .evidence_ids
                        .iter()
                        .cloned()
                        .map(SubjectRef::Evidence)
                        .collect(),
                )
            },
        ))?
        .build()
}
