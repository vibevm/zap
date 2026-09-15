specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

pub(super) const HEAD_KEY: &str = "head";
pub(super) const HEAD_EVENT_DIGEST_KEY: &str = "head_event_digest";
pub(super) const PHYSICAL_SCHEMA_KEY: &str = "physical_schema";
pub(super) const RECORD_HISTORY_SCHEMA_KEY: &str = "record_history_schema";
pub(super) const RECORD_HISTORY_SCHEMA_V1: &[u8] = &1_u32.to_be_bytes();
pub(super) const RECORD_HISTORY_SCHEMA_V2: &[u8] = &2_u32.to_be_bytes();
