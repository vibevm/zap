specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#OWNER-STOP-LAW");

use specmark::spec;
use zap_core::{CompletionBlocker, CompletionBlockerProvider, StateReader};
use zap_wire::{CompletionProviderId, ZapError};

use crate::owner_control::{PauseRecord, PauseStatus};
use crate::seams::scan_all;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#owner-economic-decisions"
)]
pub struct ControlCompletionProvider {
    id: CompletionProviderId,
}

impl ControlCompletionProvider {
    pub fn new() -> Result<Self, ZapError> {
        Ok(Self {
            id: CompletionProviderId::parse("zap.control")?,
        })
    }
}

impl CompletionBlockerProvider for ControlCompletionProvider {
    fn id(&self) -> CompletionProviderId {
        self.id.clone()
    }

    fn blockers(&self, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
        control_blockers(state)
    }
}

pub fn control_blockers(state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
    let mut blockers = scan_all::<PauseRecord>(state)?
        .into_iter()
        .filter(|pause| pause.status == PauseStatus::Active)
        .map(|pause| CompletionBlocker::ActivePause(pause.pause_id))
        .collect::<Vec<_>>();
    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}
