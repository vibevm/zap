specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    CommandPayload, CurrentPacketSelection, DispatchEligibilityRequest, DispatchReceipt,
    DriverProvenance, StoreIdentity,
};
use zap_runtime::*;
use zap_store::RedbStore;
use zap_wire::{
    BasisBinding, BoundedText, CanonicalCommandFrame, CanonicalPayload, CodecEpoch, CommandHeader,
    CommandHeaderInput, CommandId, CommandReason, CommandReasonInput, EventId, EventKind, JobId,
    ProtocolEpoch, ZapError,
};

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#runtime-composition")]
pub struct ApplicationRuntimeCommandFactory {
    store: RedbStore,
    identity: StoreIdentity,
}

impl ApplicationRuntimeCommandFactory {
    pub fn new(store: RedbStore, identity: StoreIdentity) -> Self {
        Self { store, identity }
    }

    pub(super) fn frame<P: CommandPayload + serde::Serialize>(
        &self,
        payload: &P,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        let canonical = CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?;
        let suffix = format!("{}:{}", P::KIND.replace('.', "-"), canonical.digest());
        CanonicalCommandFrame::new(
            CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                command_id: CommandId::parse(&format!("runtime:{suffix}"))?,
                event_id: EventId::parse(&format!("event:runtime:{suffix}"))?,
                expected_revision: self.store.head()?,
                kind: EventKind::parse(P::KIND)?,
                causes: Vec::new(),
                basis: BasisBinding::NotApplicable,
            })?,
            CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("application runtime transition")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            canonical,
        )
    }
}

impl RuntimeCommandFactory for ApplicationRuntimeCommandFactory {
    fn prepare_claim(
        &self,
        packet: &CurrentPacketSelection,
    ) -> Result<zap_core::PacketResolutionRequest, ZapError> {
        let suffix = packet.packet_id.as_str();
        zap_core::PacketResolutionRequest::new(
            packet.packet_id.clone(),
            JobId::parse(&format!("job.runtime.{suffix}"))?,
            zap_wire::AttemptId::parse(&format!("attempt.runtime.{suffix}"))?,
            zap_wire::DispatchId::parse(&format!("dispatch.runtime.{suffix}"))?,
            zap_wire::EffectId::parse(&format!("effect.runtime.{suffix}"))?,
        )
    }

    fn claim(
        &self,
        request: &zap_core::PacketResolutionRequest,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&JobClaimPayload {
            packet_id: request.packet_id().clone(),
            job_id: request.job_id().clone(),
            attempt_id: request.attempt_id().clone(),
            dispatch_id: request.dispatch_id().clone(),
            effect_id: request.effect_id().clone(),
        })
    }

    fn dispatch_receipt(
        &self,
        job: &RuntimeJobRecord,
        receipt: &DispatchReceipt,
        provenance: &DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchReceiptPayload {
            job_id: job.job_id.clone(),
            expected_record_revision: job.revision,
            receipt: receipt.clone(),
            provenance: provenance.clone(),
        })
    }

    fn authorize_dispatch(
        &self,
        job: &RuntimeJobRecord,
        eligibility: &DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchAuthorizePayload {
            job_id: job.job_id.clone(),
            eligibility: eligibility.clone(),
        })
    }

    fn consume_dispatch(
        &self,
        job: &RuntimeJobRecord,
        authorization: &PreEffectAuthorizationRecord,
        eligibility: &DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchConsumePayload {
            job_id: job.job_id.clone(),
            expected_authorization_revision: authorization.revision,
            eligibility: eligibility.clone(),
        })
    }

    fn reconciliation(
        &self,
        job: &RuntimeJobRecord,
        observation: &zap_core::ReconciliationObservation,
        provenance: &DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&ReconciliationRecordedPayload {
            job_id: job.job_id.clone(),
            expected_job_revision: job.revision,
            observation: observation.clone(),
            provenance: provenance.clone(),
        })
    }

    fn native_spawn_observation(
        &self,
        payload: &NativeSpawnObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn native_spawn_retry_release(
        &self,
        payload: &NativeSpawnRetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn job_observation(
        &self,
        payload: &JobObservationRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn candidate(
        &self,
        payload: &CandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn stop_request(
        &self,
        payload: &StopRequestedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn stop_delivery(
        &self,
        payload: &StopDeliveryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn safe_state(
        &self,
        payload: &SafeStateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn verification_claim(
        &self,
        payload: &VerificationClaimedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn verification_result(
        &self,
        payload: &VerificationResultPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn retry_record(
        &self,
        payload: &RetryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn retry_release(
        &self,
        payload: &RetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn liveness_boundary(
        &self,
        payload: &LivenessBoundaryPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn malformed_candidate(
        &self,
        payload: &MalformedCandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn capability_observation(
        &self,
        payload: &CapabilityObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn goal_projection(
        &self,
        payload: &GoalProjectionRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn goal_fallback(
        &self,
        payload: &GoalFallbackRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn goal_acknowledgment(
        &self,
        payload: &GoalAcknowledgedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
}
