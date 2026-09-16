use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

/// Object-safe adapter for heterogeneous typed transition cells.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
///
/// ```
/// use zap_core::ErasedTransitionCell;
/// fn decode(cell: &dyn ErasedTransitionCell, payload: &zap_wire::CanonicalPayload) -> Result<Box<dyn zap_core::ErasedCommandPayload>, zap_wire::ZapError> {
///     assert_eq!(cell.descriptor().payload_codec(), payload.codec());
///     cell.decode_payload(payload)
/// }
/// ```
pub trait ErasedTransitionCell: Send + Sync {
    fn descriptor(&self) -> &CellDescriptor;
    fn has_effect_contract(&self) -> bool;
    fn has_effect_bundles(&self) -> bool;
    fn has_affected_scope(&self) -> bool;
    fn has_safe_jobs(&self) -> bool;
    fn decode_payload(
        &self,
        payload: &CanonicalPayload,
    ) -> Result<Box<dyn ErasedCommandPayload>, ZapError>;
    fn basis_request(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<BasisRequest>, ZapError>;
    fn dispatch_eligibility_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<DispatchEligibilityRequest>, ZapError>;
    fn affected_job_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<AffectedJobRequest>, ZapError>;
    fn action_impact_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<ActionImpactRequest>, ZapError>;
    fn effect_scope(
        &self,
        state: &dyn StateReader,
        context: &EffectScopeContext,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<EffectScope>, ZapError>;
    fn simulate_effect(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &dyn ErasedCommandPayload,
        changes: &mut ChangeSet,
    ) -> Result<bool, ZapError>;
    fn effect_bundle_requests(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<EffectBundleRequest>, ZapError>;
    fn affected_scope_request(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<AffectedScopeRequest>, ZapError>;
    fn safe_job_requests(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<SafeJobRequest>, ZapError>;
    fn packet_resolution_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<PacketResolutionRequest>, ZapError>;
    fn artifact_digests(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError>;
    fn validate_apply_decoded(
        &self,
        state: &dyn StateReader,
        header: &ValidatedHeader,
        reason: &CommandReason,
        payload: Box<dyn ErasedCommandPayload>,
        changes: &mut ChangeSet,
    ) -> Result<CanonicalOutput, ZapError>;
}

pub(super) struct CellAdapter<C: TransitionCell> {
    pub(super) cell: C,
    pub(super) descriptor: CellDescriptor,
    pub(super) basis_scope: Option<Arc<dyn PayloadBasisScope<C::Payload>>>,
    pub(super) dispatch_scope: Option<Arc<dyn PayloadDispatchEligibility<C::Payload>>>,
    pub(super) affected_job_scope: Option<Arc<dyn PayloadAffectedJobs<C::Payload>>>,
    pub(super) artifact_scope: Option<Arc<dyn PayloadArtifacts<C::Payload>>>,
    pub(super) action_impact_scope: Option<Arc<dyn PayloadActionImpact<C::Payload>>>,
    pub(super) effect_contract: Option<Arc<dyn EffectContract<C::Payload>>>,
    pub(super) effect_bundles: Option<Arc<dyn crate::PayloadEffectBundles<C::Payload>>>,
    pub(super) affected_scope: Option<Arc<dyn PayloadAffectedScope<C::Payload>>>,
    pub(super) safe_jobs: Option<Arc<dyn PayloadSafeJobs<C::Payload>>>,
    pub(super) packet_resolution: Option<Arc<dyn PayloadPacketResolution<C::Payload>>>,
}

impl<C: TransitionCell> ErasedTransitionCell for CellAdapter<C> {
    fn descriptor(&self) -> &CellDescriptor {
        &self.descriptor
    }

    fn has_effect_contract(&self) -> bool {
        self.effect_contract.is_some()
    }

    fn has_effect_bundles(&self) -> bool {
        self.effect_bundles.is_some()
    }

    fn has_affected_scope(&self) -> bool {
        self.affected_scope.is_some()
    }

    fn has_safe_jobs(&self) -> bool {
        self.safe_jobs.is_some()
    }

    fn decode_payload(
        &self,
        payload: &CanonicalPayload,
    ) -> Result<Box<dyn ErasedCommandPayload>, ZapError> {
        if payload.codec() != self.descriptor.payload_codec() {
            return Err(registry_invariant());
        }
        Ok(Box::new(DecodedPayload(C::Payload::decode_canonical(
            payload,
        )?)))
    }

    fn basis_request(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<BasisRequest>, ZapError> {
        let Some(scope) = &self.basis_scope else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        scope.request(state, payload).map(Some)
    }

    fn dispatch_eligibility_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<DispatchEligibilityRequest>, ZapError> {
        let Some(scope) = &self.dispatch_scope else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        scope.request(payload).map(Some)
    }

    fn affected_job_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<AffectedJobRequest>, ZapError> {
        let Some(scope) = &self.affected_job_scope else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        scope.request(payload).map(Some)
    }

    fn action_impact_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<ActionImpactRequest>, ZapError> {
        let Some(scope) = &self.action_impact_scope else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        scope.request(payload).map(Some)
    }

    fn effect_scope(
        &self,
        state: &dyn StateReader,
        context: &EffectScopeContext,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<EffectScope>, ZapError> {
        let Some(contract) = &self.effect_contract else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        contract.scope(state, context, payload).map(Some)
    }

    fn simulate_effect(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &dyn ErasedCommandPayload,
        changes: &mut ChangeSet,
    ) -> Result<bool, ZapError> {
        let Some(contract) = &self.effect_contract else {
            return Ok(false);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        contract.simulate(state, context, payload, changes)?;
        Ok(true)
    }

    fn effect_bundle_requests(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<EffectBundleRequest>, ZapError> {
        let Some(adapter) = &self.effect_bundles else {
            return Ok(Vec::new());
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        adapter.requests(state, payload)
    }

    fn affected_scope_request(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<AffectedScopeRequest>, ZapError> {
        let Some(adapter) = &self.affected_scope else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        adapter.request(state, payload).map(Some)
    }

    fn safe_job_requests(
        &self,
        state: &dyn StateReader,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<SafeJobRequest>, ZapError> {
        let Some(adapter) = &self.safe_jobs else {
            return Ok(Vec::new());
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        adapter.requests(state, payload)
    }

    fn packet_resolution_request(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Option<PacketResolutionRequest>, ZapError> {
        let Some(adapter) = &self.packet_resolution else {
            return Ok(None);
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        adapter.request(payload).map(Some)
    }

    fn artifact_digests(
        &self,
        payload: &dyn ErasedCommandPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError> {
        let Some(scope) = &self.artifact_scope else {
            return Ok(Vec::new());
        };
        let payload = payload
            .as_any()
            .downcast_ref::<C::Payload>()
            .ok_or_else(registry_invariant)?;
        crate::artifact::validate_artifact_digests(scope.artifacts(payload)?)
    }

    fn validate_apply_decoded(
        &self,
        state: &dyn StateReader,
        header: &ValidatedHeader,
        reason: &CommandReason,
        payload: Box<dyn ErasedCommandPayload>,
        changes: &mut ChangeSet,
    ) -> Result<CanonicalOutput, ZapError> {
        if header.header().kind() != self.descriptor.kind() {
            return Err(registry_invariant());
        }
        let payload = payload
            .into_any()
            .downcast::<C::Payload>()
            .map_err(|_| registry_invariant())?;
        let envelope = CommandEnvelope::new(header.header.clone(), reason.clone(), *payload);
        let command = ValidatedCommand::new(
            envelope,
            header.command_digest,
            header.authority.clone(),
            header.completion.clone(),
            header.dispatch_eligibility.clone(),
            header.affected_jobs.clone(),
            header.preflight.clone(),
        );
        self.cell
            .apply(state, &command, changes)?
            .encode_canonical(self.descriptor.payload_codec())
    }
}
