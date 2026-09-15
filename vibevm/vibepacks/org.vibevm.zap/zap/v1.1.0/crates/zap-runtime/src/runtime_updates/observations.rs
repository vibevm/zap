use super::*;
use specmark::spec;

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-LIVENESS-AND-CHECKPOINT"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub struct LivenessBoundaryCell;

pub(crate) struct LivenessBoundaryArtifacts;

impl PayloadArtifacts<LivenessBoundaryPayload> for LivenessBoundaryArtifacts {
    fn artifacts(
        &self,
        payload: &LivenessBoundaryPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError> {
        Ok(payload.record.useful_checkpoint.into_iter().collect())
    }
}

impl TransitionCell for LivenessBoundaryCell {
    type Payload = LivenessBoundaryPayload;
    type Output = RuntimeLivenessRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[RuntimeLivenessRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        validate_driver(state, command, &payload.provenance)?;
        let mut record = payload.record.clone();
        if record.observation != payload.provenance.observation
            || state
                .get_typed::<RuntimeJobRecord>(&record.job_id)?
                .is_none()
        {
            return Err(update_error(
                "liveness boundary does not bind a current job",
            ));
        }
        record.revision = command.header().expected_revision().checked_next()?;
        match state.get_typed::<RuntimeLivenessRecord>(&record.job_id)? {
            None if record.observation_count == 1 => changes.insert(record.clone())?,
            Some(previous)
                if record.observation_count > previous.observation_count
                    && record.last_observed_ns >= previous.last_observed_ns
                    && record.useful_checkpoint.is_some()
                    && record.useful_checkpoint != previous.useful_checkpoint =>
            {
                changes.replace(previous.revision, record.clone())?
            }
            _ => {
                return Err(update_error(
                    "unchanged heartbeat must remain coalesced outside semantic history",
                ));
            }
        }
        Ok(record)
    }
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct MalformedCandidateRecordedCell;

pub(crate) struct MalformedCandidateArtifacts;

impl PayloadArtifacts<MalformedCandidateRecordedPayload> for MalformedCandidateArtifacts {
    fn artifacts(
        &self,
        payload: &MalformedCandidateRecordedPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError> {
        Ok(payload
            .record
            .preserved_artifacts
            .iter()
            .map(|artifact| artifact.digest)
            .collect())
    }
}

impl TransitionCell for MalformedCandidateRecordedCell {
    type Payload = MalformedCandidateRecordedPayload;
    type Output = MalformedCandidateRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[MalformedCandidateRecord::FAMILY, RuntimeJobRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        validate_driver(state, command, &payload.provenance)?;
        let mut record = MalformedCandidateRecord::new(payload.record.clone())?;
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&record.job_id)?
            .ok_or_else(|| update_error("malformed candidate references unknown job"))?;
        if record.attempt_id != job.attempt_id
            || record.packet_id != job.packet_id
            || record.relevant_basis != job.relevant_basis
            || record.transport_observation != payload.provenance.observation
        {
            return Err(update_error(
                "malformed candidate does not bind the current job attempt and packet",
            ));
        }
        let revision = command.header().expected_revision().checked_next()?;
        record.revision = revision;
        match state.get_typed::<MalformedCandidateRecord>(&record.attempt_id)? {
            Some(previous) if record.repair_count > previous.repair_count => {
                changes.replace(previous.revision, record.clone())?
            }
            None => changes.insert(record.clone())?,
            Some(_) => return Err(update_error("malformed repair count did not advance")),
        }
        let prior_job_revision = job.revision;
        job.collection = crate::CollectionState::Malformed;
        job.revision = revision;
        changes.replace(prior_job_revision, job)?;
        Ok(record)
    }
}
