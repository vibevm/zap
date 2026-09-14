use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{BaseId, CampaignId, CodecEpoch, ReducerEpoch, Revision, StoreEpoch, StoreId};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

/// The exact immutable and epoch identity of one campaign store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
pub struct StoreIdentity {
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub store_epoch: StoreEpoch,
    pub codec_epoch: CodecEpoch,
    pub reducer_epoch: ReducerEpoch,
}

/// A bounded store read boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "revision", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
pub enum ReadAt {
    Current,
    Revision(Revision),
}
