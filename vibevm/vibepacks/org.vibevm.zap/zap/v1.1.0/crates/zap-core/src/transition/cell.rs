use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

/// ```
/// use zap_core::ErasedCommandPayload;
/// fn assert_decoded<P: 'static>(payload: &dyn ErasedCommandPayload) { assert!(payload.as_any().is::<P>()); }
/// ```
pub trait ErasedCommandPayload: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
}

pub(super) struct DecodedPayload<P: CommandPayload>(pub(super) P);

impl<P: CommandPayload> ErasedCommandPayload for DecodedPayload<P> {
    fn as_any(&self) -> &dyn Any {
        &self.0
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync> {
        Box::new(self.0)
    }
}

/// A pure typed state-transition cell.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
///
/// ```
/// use zap_core::TransitionCell;
/// fn descriptor<C: TransitionCell>(cell: &C) -> Result<zap_core::CellDescriptor, zap_wire::ZapError> { cell.descriptor() }
/// ```
pub trait TransitionCell: Send + Sync + 'static {
    type Payload: CommandPayload;
    type Output: CanonicalEncode + Send + Sync + 'static;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError>;
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError>;
}

/// Header and admission facts already checked against a transaction pre-state.
#[derive(Clone, Debug)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#validated-command-use")]
pub struct ValidatedHeader {
    pub(super) header: CommandHeader,
    pub(super) command_digest: CommandDigest,
    pub(super) authority: AdmittedAuthority,
    pub(super) completion: Option<CompletionView>,
    pub(super) dispatch_eligibility: Option<DispatchEligibilityView>,
    pub(super) affected_jobs: Option<AffectedJobView>,
    pub(super) preflight: Arc<ValidatedCommandPreflight>,
}

impl ValidatedHeader {
    pub(crate) fn new(
        header: CommandHeader,
        command_digest: CommandDigest,
        authority: AdmittedAuthority,
        completion: Option<CompletionView>,
        dispatch_eligibility: Option<DispatchEligibilityView>,
        affected_jobs: Option<AffectedJobView>,
        preflight: Arc<ValidatedCommandPreflight>,
    ) -> Self {
        Self {
            header,
            command_digest,
            authority,
            completion,
            dispatch_eligibility,
            affected_jobs,
            preflight,
        }
    }

    pub fn header(&self) -> &CommandHeader {
        &self.header
    }

    pub const fn command_digest(&self) -> CommandDigest {
        self.command_digest
    }
}

/// Transaction-derived evidence attached by the commit service.
#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#validated-command-use")]
pub struct ValidatedCommandPreflight {
    record: crate::CommandPreflightRecord,
    safe_jobs: Vec<crate::SafeJobWitness>,
    transaction_seal: Arc<()>,
    service_seal: Arc<()>,
    action: Option<crate::ActionAdmissionPreflightRecord>,
    packet_resolution: Option<RuntimeJobClaim>,
}

impl std::fmt::Debug for ValidatedCommandPreflight {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedCommandPreflight")
            .field("record", &self.record)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

impl ValidatedCommandPreflight {
    pub(crate) fn new(
        record: crate::CommandPreflightRecord,
        safe_jobs: Vec<crate::SafeJobWitness>,
        transaction_seal: Arc<()>,
        service_seal: Arc<()>,
        action: Option<crate::ActionAdmissionPreflightRecord>,
        packet_resolution: Option<RuntimeJobClaim>,
    ) -> Self {
        Self {
            record,
            safe_jobs,
            transaction_seal,
            service_seal,
            action,
            packet_resolution,
        }
    }

    pub(crate) fn empty(service_seal: Arc<()>) -> Arc<Self> {
        let transaction_seal = Arc::new(());
        Arc::new(Self::new(
            crate::CommandPreflightRecord::default(),
            Vec::new(),
            transaction_seal,
            service_seal,
            None,
            None,
        ))
    }

    pub fn record(&self) -> &crate::CommandPreflightRecord {
        &self.record
    }

    pub fn effect_preflights(&self) -> &[crate::EffectBundlePreflightView] {
        &self.record.effect_bundles
    }

    pub fn affected_scope(
        &self,
        request_digest: zap_wire::PayloadDigest,
    ) -> Option<&crate::AffectedScopeView> {
        self.record
            .affected_scopes
            .iter()
            .find(|view| view.request_digest == request_digest)
    }

    pub fn safe_jobs_for(&self, hold_id: &zap_wire::HoldId) -> Option<&crate::SafeJobWitness> {
        self.safe_jobs.iter().find(|witness| {
            witness.view().hold_id == *hold_id
                && Arc::ptr_eq(&witness.transaction_seal, &self.transaction_seal)
                && Arc::ptr_eq(&witness.service_seal, &self.service_seal)
        })
    }

    pub fn action_preflight(&self) -> Option<&crate::ActionAdmissionPreflightRecord> {
        self.action.as_ref()
    }

    pub fn packet_resolution(&self) -> Option<&RuntimeJobClaim> {
        self.packet_resolution.as_ref().filter(|claim| {
            Arc::ptr_eq(&claim.transaction_seal, &self.transaction_seal)
                && Arc::ptr_eq(&claim.service_seal, &self.service_seal)
        })
    }
}

/// A typed command with service-created authority and completion context.
#[derive(Clone, Debug)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#validated-command-use")]
pub struct ValidatedCommand<P> {
    envelope: CommandEnvelope<P>,
    command_digest: CommandDigest,
    authority: AdmittedAuthority,
    completion: Option<CompletionView>,
    dispatch_eligibility: Option<DispatchEligibilityView>,
    affected_jobs: Option<AffectedJobView>,
    preflight: Arc<ValidatedCommandPreflight>,
}

impl<P: CommandPayload> ValidatedCommand<P> {
    pub(super) fn new(
        envelope: CommandEnvelope<P>,
        command_digest: CommandDigest,
        authority: AdmittedAuthority,
        completion: Option<CompletionView>,
        dispatch_eligibility: Option<DispatchEligibilityView>,
        affected_jobs: Option<AffectedJobView>,
        preflight: Arc<ValidatedCommandPreflight>,
    ) -> Self {
        Self {
            envelope,
            command_digest,
            authority,
            completion,
            dispatch_eligibility,
            affected_jobs,
            preflight,
        }
    }

    pub fn header(&self) -> &CommandHeader {
        self.envelope.header()
    }

    pub fn reason(&self) -> &CommandReason {
        self.envelope.reason()
    }

    pub fn payload(&self) -> &P {
        self.envelope.payload()
    }

    pub fn authority(&self) -> &AdmittedAuthority {
        &self.authority
    }

    pub fn completion(&self) -> Option<&CompletionView> {
        self.completion.as_ref()
    }

    pub fn dispatch_eligibility(&self) -> Option<&DispatchEligibilityView> {
        self.dispatch_eligibility.as_ref()
    }

    pub fn affected_jobs(&self) -> Option<&AffectedJobView> {
        self.affected_jobs.as_ref()
    }

    pub const fn command_digest(&self) -> CommandDigest {
        self.command_digest
    }

    pub fn command_preflight(&self) -> &crate::CommandPreflightRecord {
        self.preflight.record()
    }

    pub fn effect_preflights(&self) -> &[crate::EffectBundlePreflightView] {
        self.preflight.effect_preflights()
    }

    pub fn affected_scope(
        &self,
        request_digest: zap_wire::PayloadDigest,
    ) -> Option<&crate::AffectedScopeView> {
        self.preflight.affected_scope(request_digest)
    }

    pub fn safe_jobs_for(&self, hold_id: &zap_wire::HoldId) -> Option<&crate::SafeJobWitness> {
        self.preflight.safe_jobs_for(hold_id)
    }

    pub fn action_preflight(&self) -> Option<&crate::ActionAdmissionPreflightRecord> {
        self.preflight.action_preflight()
    }

    pub fn packet_resolution(&self) -> Option<&RuntimeJobClaim> {
        self.preflight.packet_resolution()
    }
}
