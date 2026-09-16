use specmark::spec;
use zap_core::{
    BasisPurpose, BasisRequest, BasisRequestInput, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ActionClass, ControlClass, ErrorCode, ErrorDetail, FixSurface, Revision, RouteClass, ZapError,
};

use crate::control::ObligationRecord;
use crate::intent::{
    CharterAmended, CharterDrafted, CharterRecord, IntentProposed, IntentRecord, OutcomeAdopted,
    OutcomeProposed, OutcomeRecord,
};
use crate::seams::{
    DomainMutation, LifecycleStatus, cell_descriptor, impl_command_payload, refuse, scan_all,
};

use super::{
    activate_charter, adopt_intent, adopt_outcome, amend_charter, draft_charter, propose_intent,
    propose_outcome,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION");

const CHARTER_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION";
const INTENT_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AUTHORITY";
const OUTCOME_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS";

impl_command_payload!(CharterDrafted, "control.charter-drafted");
impl_command_payload!(crate::intent::CharterActivated, "control.charter-activated");
impl_command_payload!(CharterAmended, "control.charter-amended");
impl_command_payload!(IntentProposed, "domain.intent-proposed");
impl_command_payload!(crate::intent::IntentAdopted, "domain.intent-adopted");
impl_command_payload!(OutcomeProposed, "domain.outcome-proposed");
impl_command_payload!(OutcomeAdopted, "domain.outcome-adopted");

fn result(command_revision: Revision) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command_revision.checked_next()?,
    })
}

fn active_one<R, F>(state: &dyn StateReader, is_active: F) -> Result<Option<R>, ZapError>
where
    R: zap_core::StoredRecord,
    F: Fn(&R) -> bool,
{
    let mut rows = scan_all::<R>(state)?.into_iter().filter(is_active);
    let first = rows.next();
    if rows.next().is_some() {
        return refuse(
            ErrorCode::InternalInvariant,
            OUTCOME_REQ,
            "more than one record in a singleton lifecycle family is active",
            FixSurface::Store,
            ErrorDetail::None,
        );
    }
    Ok(first)
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct CharterDraftedCell;

impl TransitionCell for CharterDraftedCell {
    type Payload = CharterDrafted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            CharterDrafted::KIND,
            RouteClass::DataProposal,
            &[CharterRecord::FAMILY],
            CHARTER_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let charter = draft_charter(command.payload())?;
        if state
            .get_typed::<CharterRecord>(&charter.charter_id)?
            .is_some()
        {
            return refuse(
                ErrorCode::DuplicateIdentity,
                CHARTER_REQ,
                "charter identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        changes.insert(charter)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct CharterActivatedCell;

impl TransitionCell for CharterActivatedCell {
    type Payload = crate::intent::CharterActivated;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::OwnerControl(ControlClass::CharterActivate),
            &[CharterRecord::FAMILY],
            CHARTER_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        if active_one::<CharterRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .is_some()
        {
            return refuse(
                ErrorCode::Conflict,
                CHARTER_REQ,
                "an active charter must be amended rather than activated again",
                FixSurface::Command,
                ErrorDetail::None,
            );
        }
        let payload = command.payload();
        let draft = state
            .get_typed::<CharterRecord>(&payload.charter_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    CHARTER_REQ,
                    "charter proposal is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let active = activate_charter(&draft, payload.charter_digest)?;
        changes.replace(draft.revision, active)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct CharterAmendedCell;

impl TransitionCell for CharterAmendedCell {
    type Payload = CharterAmended;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::OwnerControl(ControlClass::CharterAmend),
            &[CharterRecord::FAMILY],
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AMENDMENT",
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let current =
            active_one::<CharterRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
                .ok_or_else(|| {
                    ZapError::from_static(
                        ErrorCode::MissingReference,
                        CHARTER_REQ,
                        "active charter is missing",
                        FixSurface::Authority,
                        ErrorDetail::None,
                    )
                })?;
        if state
            .get_typed::<CharterRecord>(&command.payload().charter.charter_id)?
            .is_some()
        {
            return refuse(
                ErrorCode::DuplicateIdentity,
                CHARTER_REQ,
                "amended charter identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        let (old, active) = amend_charter(&current, command.payload())?;
        changes.replace(current.revision, old)?;
        changes.insert(active)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct IntentProposedCell;

impl TransitionCell for IntentProposedCell {
    type Payload = IntentProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[IntentRecord::FAMILY],
            INTENT_REQ,
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
        if state
            .get_typed::<IntentRecord>(&payload.intent_id)?
            .is_some()
        {
            return refuse(
                ErrorCode::DuplicateIdentity,
                INTENT_REQ,
                "intent identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        let previous = payload
            .previous_intent_id
            .as_ref()
            .map(|id| state.get_typed::<IntentRecord>(id))
            .transpose()?
            .flatten();
        changes.insert(propose_intent(payload, previous.as_ref())?)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct IntentAdoptedCell;

pub(crate) struct IntentAdoptionBasisScope;

impl PayloadBasisScope<crate::intent::IntentAdopted> for IntentAdoptionBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &crate::intent::IntentAdopted,
    ) -> Result<BasisRequest, ZapError> {
        semantic_adoption_basis(
            crate::intent::IntentAdopted::KIND,
            vec![zap_wire::SubjectRef::Intent(payload.intent_id.clone())],
        )
    }
}

pub(crate) struct IntentAdoptionEffectContract;

impl EffectContract<crate::intent::IntentAdopted> for IntentAdoptionEffectContract {
    fn scope(
        &self,
        _state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &crate::intent::IntentAdopted,
    ) -> Result<EffectScope, ZapError> {
        let subject = zap_wire::SubjectRef::Intent(payload.intent_id.clone());
        EffectScope::new(
            semantic_adoption_basis(crate::intent::IntentAdopted::KIND, vec![subject.clone()])?,
            Vec::new(),
            vec![subject],
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        _context: &EffectSimulationContext,
        payload: &crate::intent::IntentAdopted,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_intent_adoption(state, payload, changes)
    }
}

fn semantic_adoption_basis(
    kind: &'static str,
    subjects: Vec<zap_wire::SubjectRef>,
) -> Result<BasisRequest, ZapError> {
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse(kind)?),
        roots: subjects,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn apply_intent_adoption(
    state: &dyn StateReader,
    payload: &crate::intent::IntentAdopted,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let proposal = state
        .get_typed::<IntentRecord>(&payload.intent_id)?
        .ok_or_else(|| {
            ZapError::from_static(
                ErrorCode::MissingReference,
                INTENT_REQ,
                "intent proposal is missing",
                FixSurface::Payload,
                ErrorDetail::None,
            )
        })?;
    let charter =
        active_one::<CharterRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::Unauthorized,
                    INTENT_REQ,
                    "active charter is missing",
                    FixSurface::Authority,
                    ErrorDetail::None,
                )
            })?;
    let prior = active_one::<IntentRecord, _>(state, |row| row.status == LifecycleStatus::Active)?;
    let active_outcome =
        active_one::<OutcomeRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .is_some();
    let (old, active) = adopt_intent(&proposal, &charter, prior.as_ref(), active_outcome)?;
    if let Some(old) = old {
        changes.replace(old.revision, old)?;
    }
    changes.replace(proposal.revision, active)
}

impl TransitionCell for IntentAdoptedCell {
    type Payload = crate::intent::IntentAdopted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("outcome.adopt")?),
            &[CharterRecord::FAMILY, IntentRecord::FAMILY],
            INTENT_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        apply_intent_adoption(state, command.payload(), changes)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct OutcomeProposedCell;

impl TransitionCell for OutcomeProposedCell {
    type Payload = OutcomeProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[OutcomeRecord::FAMILY],
            OUTCOME_REQ,
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
        if state
            .get_typed::<OutcomeRecord>(&payload.outcome_id)?
            .is_some()
        {
            return refuse(
                ErrorCode::DuplicateIdentity,
                OUTCOME_REQ,
                "outcome identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        let previous = payload
            .previous_outcome_id
            .as_ref()
            .map(|id| state.get_typed::<OutcomeRecord>(id))
            .transpose()?
            .flatten();
        changes.insert(propose_outcome(payload, previous.as_ref())?)?;
        result(command.header().expected_revision())
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct OutcomeAdoptedCell;

pub(crate) struct OutcomeAdoptionBasisScope;

impl PayloadBasisScope<OutcomeAdopted> for OutcomeAdoptionBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &OutcomeAdopted,
    ) -> Result<BasisRequest, ZapError> {
        semantic_adoption_basis(OutcomeAdopted::KIND, outcome_adoption_subjects(payload))
    }
}

pub(crate) struct OutcomeAdoptionEffectContract;

impl EffectContract<OutcomeAdopted> for OutcomeAdoptionEffectContract {
    fn scope(
        &self,
        _state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &OutcomeAdopted,
    ) -> Result<EffectScope, ZapError> {
        let subjects = outcome_adoption_subjects(payload);
        EffectScope::new(
            semantic_adoption_basis(OutcomeAdopted::KIND, subjects.clone())?,
            Vec::new(),
            subjects,
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        _context: &EffectSimulationContext,
        payload: &OutcomeAdopted,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_outcome_adoption(state, payload, changes)
    }
}

fn outcome_adoption_subjects(payload: &OutcomeAdopted) -> Vec<zap_wire::SubjectRef> {
    let mut subjects = vec![zap_wire::SubjectRef::Outcome(payload.outcome_id.clone())];
    subjects.extend(
        payload
            .obligation_dispositions
            .iter()
            .map(|row| zap_wire::SubjectRef::Obligation(row.obligation_id.clone())),
    );
    subjects
}

fn apply_outcome_adoption(
    state: &dyn StateReader,
    payload: &OutcomeAdopted,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let proposal = state
        .get_typed::<OutcomeRecord>(&payload.outcome_id)?
        .ok_or_else(|| {
            ZapError::from_static(
                ErrorCode::MissingReference,
                OUTCOME_REQ,
                "outcome proposal is missing",
                FixSurface::Payload,
                ErrorDetail::None,
            )
        })?;
    let intent = active_one::<IntentRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
        .ok_or_else(|| {
            ZapError::from_static(
                ErrorCode::MissingReference,
                OUTCOME_REQ,
                "active intent is missing",
                FixSurface::Payload,
                ErrorDetail::None,
            )
        })?;
    let current =
        active_one::<OutcomeRecord, _>(state, |row| row.status == LifecycleStatus::Active)?;
    let charter =
        active_one::<CharterRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::Unauthorized,
                    OUTCOME_REQ,
                    "active charter is missing",
                    FixSurface::Authority,
                    ErrorDetail::None,
                )
            })?;
    let obligations = scan_all::<ObligationRecord>(state)?;
    let adoption = adopt_outcome(
        &proposal,
        &intent,
        current.as_ref(),
        &obligations,
        payload,
        &charter,
    )?;
    if let Some(previous) = adoption.previous {
        changes.replace(previous.revision, previous)?;
    }
    changes.replace(proposal.revision, adoption.active)?;
    for obligation in adoption.prior_obligations {
        let expected = obligations
            .iter()
            .find(|row| row.obligation_id == obligation.obligation_id)
            .map(|row| row.revision)
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    OUTCOME_REQ,
                    "prior obligation is missing",
                    FixSurface::Store,
                    ErrorDetail::None,
                )
            })?;
        changes.replace(expected, obligation)?;
    }
    for obligation in adoption.created_obligations {
        changes.insert(obligation)?;
    }
    Ok(())
}

impl TransitionCell for OutcomeAdoptedCell {
    type Payload = OutcomeAdopted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("outcome.adopt")?),
            &[ObligationRecord::FAMILY, OutcomeRecord::FAMILY],
            OUTCOME_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        apply_outcome_adoption(state, command.payload(), changes)?;
        result(command.header().expected_revision())
    }
}
