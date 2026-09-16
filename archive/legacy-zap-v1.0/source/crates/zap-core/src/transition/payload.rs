use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

/// A concrete strictly decodable command payload.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
///
/// ```compile_fail
/// use zap_core::CommandPayload;
/// struct Unencoded;
/// impl CommandPayload for Unencoded { const KIND: &'static str = "invalid.unencoded"; }
/// ```
pub trait CommandPayload: CanonicalEncode + CanonicalDecode + Send + Sync + 'static {
    const KIND: &'static str;
}

/// ```
/// use zap_core::{CommandPayload, PayloadDispatchEligibility};
/// fn request<P: CommandPayload>(scope: &dyn PayloadDispatchEligibility<P>, payload: &P) -> Result<zap_core::DispatchEligibilityRequest, zap_wire::ZapError> { scope.request(payload) }
/// ```
pub trait PayloadDispatchEligibility<P: CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P) -> Result<DispatchEligibilityRequest, ZapError>;
}

/// ```
/// use zap_core::{CommandPayload, PayloadAffectedJobs};
/// fn request<P: CommandPayload>(scope: &dyn PayloadAffectedJobs<P>, payload: &P) -> Result<zap_core::AffectedJobRequest, zap_wire::ZapError> { scope.request(payload) }
/// ```
pub trait PayloadAffectedJobs<P: CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P) -> Result<AffectedJobRequest, ZapError>;
}

/// ```
/// use zap_core::{CommandPayload, PayloadAffectedScope, StateReader};
/// fn request<P: CommandPayload>(scope: &dyn PayloadAffectedScope<P>, state: &dyn StateReader, payload: &P) -> Result<zap_core::AffectedScopeRequest, zap_wire::ZapError> { scope.request(state, payload) }
/// ```
pub trait PayloadAffectedScope<P: CommandPayload>: Send + Sync + 'static {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<AffectedScopeRequest, ZapError>;
}

/// ```
/// use zap_core::{CommandPayload, PayloadSafeJobs, StateReader};
/// fn requests<P: CommandPayload>(scope: &dyn PayloadSafeJobs<P>, state: &dyn StateReader, payload: &P) -> Result<Vec<zap_core::SafeJobRequest>, zap_wire::ZapError> { scope.requests(state, payload) }
/// ```
pub trait PayloadSafeJobs<P: CommandPayload>: Send + Sync + 'static {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<Vec<SafeJobRequest>, ZapError>;
}
