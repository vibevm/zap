specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-FOCUSED-VERIFICATION"
);

use super::*;
use specmark::spec;

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub struct VerificationClaimedCell;

impl TransitionCell for VerificationClaimedCell {
    type Payload = VerificationClaimedPayload;
    type Output = VerificationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(zap_wire::ActionClass::parse("verification.run")?),
            &[VerificationRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let mut record = command.payload().record.clone().validate()?;
        let job = state.get_typed::<RuntimeJobRecord>(&record.job_id)?;
        if !matches!(record.state, VerificationState::Claimed)
            || record.observation.is_some()
            || job
                .as_ref()
                .is_none_or(|job| !verification_scope_matches(job, &record))
            || state
                .get_typed::<VerificationRecord>(&record.verification_id)?
                .is_some()
        {
            return Err(update_error(
                "verification claim is stale or already exists",
            ));
        }
        record.revision = command.header().expected_revision().checked_next()?;
        changes.insert(record.clone())?;
        Ok(record)
    }
}

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub struct VerificationResultCell;

pub(crate) struct VerificationResultArtifacts;

impl PayloadArtifacts<VerificationResultPayload> for VerificationResultArtifacts {
    fn artifacts(
        &self,
        payload: &VerificationResultPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError> {
        Ok(payload.artifacts.clone())
    }
}

impl TransitionCell for VerificationResultCell {
    type Payload = VerificationResultPayload;
    type Output = VerificationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[VerificationRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        validate_driver(state, command, &command.payload().provenance)?;
        let payload = command.payload();
        let mut record = state
            .get_typed::<VerificationRecord>(&payload.verification_id)?
            .ok_or_else(|| update_error("verification result has no claim"))?;
        let job = state
            .get_typed::<RuntimeJobRecord>(&record.job_id)?
            .ok_or_else(|| update_error("verification result job is missing"))?;
        if record.revision != payload.expected_record_revision
            || !matches!(record.state, VerificationState::Claimed)
            || matches!(payload.state, VerificationState::Claimed)
            || payload.observation != payload.provenance.observation
            || !verification_scope_matches(&job, &record)
            || matches!(record.scope, VerificationScope::SafeBoundary { .. })
                && !job.execution.is_terminal()
                && !matches!(job.execution, crate::ExecutionState::UnknownEffect)
        {
            return Err(update_error(
                "verification result does not bind its current claim",
            ));
        }
        let previous = record.revision;
        record.state = payload.state;
        record.observation = Some(payload.observation.clone());
        record.artifacts = payload.artifacts.clone();
        record.artifacts.sort();
        record.artifacts.dedup();
        record.revision = command.header().expected_revision().checked_next()?;
        record = record.validate()?;
        changes.replace(previous, record.clone())?;
        Ok(record)
    }
}
