use specmark::spec;
use zap_wire::ZapError;

use super::{
    AgentCapabilities, CandidateResult, DispatchIntent, DispatchReceipt, ExternalJobHandle,
    JobObservation, ReconciliationObservation, StopReceipt, StopRequest,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE");

/// Effect boundary for executable workers; it grants no product authority.
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE")]
///
/// ```
/// use zap_core::AgentHost;
/// fn observe(host: &dyn AgentHost, handle: &zap_core::ExternalJobHandle) -> Result<zap_core::JobObservation, zap_wire::ZapError> { host.observe(handle) }
/// ```
pub trait AgentHost: Send + Sync {
    fn capabilities(&self) -> AgentCapabilities;
    fn dispatch(&self, intent: DispatchIntent) -> Result<DispatchReceipt, ZapError>;
    fn observe(&self, handle: &ExternalJobHandle) -> Result<JobObservation, ZapError>;
    fn collect(&self, handle: &ExternalJobHandle) -> Result<CandidateResult, ZapError>;
    fn request_stop(
        &self,
        handle: &ExternalJobHandle,
        stop: StopRequest,
    ) -> Result<StopReceipt, ZapError>;
    fn reconcile(
        &self,
        intent: &DispatchIntent,
        known: Option<&DispatchReceipt>,
    ) -> Result<ReconciliationObservation, ZapError>;
}
