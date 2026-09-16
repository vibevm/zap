#[path = "packet_resolution_service.rs"]
pub mod journey;

#[test]
fn two_algorithmic_jobs_accept_and_close_one_nonempty_campaign()
-> Result<(), Box<dyn std::error::Error>> {
    journey::deterministic_two_job_campaign_completes_through_product_routes()
}
