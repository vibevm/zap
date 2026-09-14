specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH"
);

use super::*;
use specmark::spec;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct JobClaimCell;

pub(crate) struct JobClaimPacketResolution;

impl PayloadPacketResolution<JobClaimPayload> for JobClaimPacketResolution {
    fn request(&self, payload: &JobClaimPayload) -> Result<PacketResolutionRequest, ZapError> {
        PacketResolutionRequest::new(
            payload.packet_id.clone(),
            payload.job_id.clone(),
            payload.attempt_id.clone(),
            payload.dispatch_id.clone(),
            payload.effect_id.clone(),
        )
    }
}

impl TransitionCell for JobClaimCell {
    type Payload = JobClaimPayload;
    type Output = RuntimeTransitionOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[
                RuntimeJobRecord::FAMILY,
                WorkExecutionObservationRecord::FAMILY,
            ],
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let claim = command
            .packet_resolution()
            .ok_or_else(missing_packet_resolution)?;
        let record = claim.record();
        let payload = command.payload();
        if payload.packet_id != record.identity.packet_id
            || payload.job_id != record.job_id
            || payload.attempt_id != record.attempt_id
            || payload.dispatch_id != record.dispatch_id
            || payload.effect_id != record.effect_id
        {
            return Err(invalid_transition(
                "job claim operational identities differ from sealed packet resolution",
            ));
        }
        if state
            .get_typed::<RuntimeJobRecord>(&record.job_id)?
            .is_some()
        {
            return Err(invalid_transition("runtime job identity already exists"));
        }
        let producer = ProducerRef {
            actor: ActorRef {
                principal_id: record.expected_producer.principal_id.clone(),
                operation: OperationRef::Attempt(record.attempt_id.clone()),
                role: PrincipalRole::Worker,
            },
            job_id: record.job_id.clone(),
            attempt_id: record.attempt_id.clone(),
            packet_id: record.identity.packet_id.clone(),
        };
        let intent = DispatchIntent {
            dispatch_id: record.dispatch_id.clone(),
            campaign_id: record.identity.campaign_id.clone(),
            job_id: record.job_id.clone(),
            attempt_id: record.attempt_id.clone(),
            packet_id: record.identity.packet_id.clone(),
            packet_digest: record.identity.packet_digest,
            contract_id: record.work.contract_id.clone(),
            contract_digest: record.work.contract_digest,
            host: record.expected_producer.harness_id.clone(),
            capability_digest: record.capability_digest,
            role: record.role,
            resolved_profile: record.resolved_profile.clone(),
            workspace: record.workspace.binding.clone(),
            workspace_manifest: record.workspace.manifest_artifact,
            relevant_basis: record.work.relevant_basis,
            safe_stop: record.candidate_result.template.safe_stop.clone(),
        };
        let job = RuntimeJobRecord {
            job_id: record.job_id.clone(),
            work_id: record.work.work_id.clone(),
            attempt_id: record.attempt_id.clone(),
            dispatch_id: record.dispatch_id.clone(),
            packet_id: record.identity.packet_id.clone(),
            packet_digest: record.identity.packet_digest,
            packet_resolution_digest: record.digest,
            contract_id: record.work.contract_id.clone(),
            contract_version: record.work.contract_version,
            contract_digest: record.work.contract_digest,
            validation_generation: record.work.validation_generation,
            relevant_basis: record.work.relevant_basis,
            effect_id: record.effect_id.clone(),
            producer,
            expected_producer: record.expected_producer.clone(),
            candidate_result_contract: record.candidate_result.clone(),
            workspace_manifest: record.workspace.manifest_artifact,
            read_subjects: record.work.read_subjects.clone(),
            write_subjects: record.work.write_subjects.clone(),
            resources: record.work.resources.clone(),
            integration_owner: record.work.integration_owner.clone(),
            delivery_route: record.work.delivery_route.clone(),
            intent,
            receipt: None,
            last_observation: None,
            stop_request: None,
            stop_receipt: None,
            safe_observation: None,
            safe_verifications: Vec::new(),
            execution: ExecutionState::DispatchPending,
            collection: crate::CollectionState::Uncollected,
            safe: SafeState::NotStarted,
            acceptance: crate::AcceptanceState::Unreviewed,
            effect: EffectState::IntentCommitted,
            candidate_id: None,
            verifications: Vec::new(),
            revision: command.header().expected_revision().checked_next()?,
        }
        .validate()?;
        changes.insert(job.clone())?;
        changes.insert(execution_observation(&job)?)?;
        Ok(RuntimeTransitionOutput {
            job_id: job.job_id,
            execution: ExecutionState::DispatchPending,
        })
    }
}
