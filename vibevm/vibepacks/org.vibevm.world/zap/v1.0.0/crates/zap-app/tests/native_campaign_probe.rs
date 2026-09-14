#[path = "packet_resolution_service.rs"]
pub mod journey;

#[test]
#[ignore = "requires root-owned native collaboration receipts"]
fn one_campaign_two_native_jobs_complete_through_product_routes()
-> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR").is_none() {
        return Err("ZAP_R16_NATIVE_PROBE_DIR must name a fresh durable probe directory".into());
    }
    journey::real_lowered_packet_seals_runtime_claim_and_replays_captured_material()
}

#[test]
#[ignore = "requires a preserved root-owned native probe directory"]
fn inspect_preserved_native_campaign_without_replaying_effects()
-> Result<(), Box<dyn std::error::Error>> {
    journey::inspect_native_campaign_probe_without_replay()
}
