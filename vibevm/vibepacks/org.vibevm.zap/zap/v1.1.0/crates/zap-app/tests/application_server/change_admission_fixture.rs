use super::change_admission_support::seed_with_profile;
use super::*;

#[test]
#[ignore = "explicit opt-in writes a reusable synthetic local HTTP fixture"]
fn write_isolated_change_admission_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("ZAP_CHANGE_ADMISSION_FIXTURE_DIR")
        .map(std::path::PathBuf::from)
        .ok_or("set ZAP_CHANGE_ADMISSION_FIXTURE_DIR to a new empty directory")?;
    if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
        return Err("fixture directory must be empty".into());
    }
    std::fs::create_dir_all(root.join("materials"))?;
    for (name, secret) in [
        ("reader.secret", b"reader-secret".as_slice()),
        ("owner.secret", b"owner-secret".as_slice()),
        ("coordinator.secret", b"coordinator-secret".as_slice()),
        ("data.secret", b"data-secret".as_slice()),
        ("trusted.secret", b"trusted-secret".as_slice()),
    ] {
        std::fs::write(root.join(name), secret)?;
    }
    let mut service = service_config(&root)?;
    let identity = match &service.store_mode {
        ApplicationStoreMode::Create { identity } => identity.clone(),
        ApplicationStoreMode::Open => unreachable!(),
    };
    let current_strategy =
        std::env::var("ZAP_CHANGE_ADMISSION_FIXTURE_PROFILE").is_ok_and(|value| value == "current");
    seed_with_profile(&service.store, &identity, current_strategy)?;
    service.store_mode = ApplicationStoreMode::Open;
    let server = ReadServerConfig {
        store: service.store.clone(),
        bind: "127.0.0.1:0".parse()?,
        credential_id: "reader.application-server".into(),
        credential_file: root.join("reader.secret"),
        max_request_bytes: 512 * 1024,
        max_connections: 16,
        event_page_limit: 128,
        max_response_bytes: 1024 * 1024,
        io_timeout_millis: 3_000,
    };
    let config_path = root.join("application-server.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&ApplicationServerConfig { service, server })?,
    )?;
    std::fs::write(
        root.join("fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "zap.change-admission-fixture/1",
            "application_config": config_path,
            "endpoint_file": root.join("application.endpoint.json"),
            "store": identity,
            "credentials": {
                "reader": {"id": "reader.application-server", "file": root.join("reader.secret")},
                "data": {"id": "data.application-server", "file": root.join("data.secret")},
                "coordinator": {"id": "coordinator.application-server", "file": root.join("coordinator.secret")},
                "owner": {"id": "owner.application-server", "file": root.join("owner.secret")}
            },
            "seed": {
                "profile": if current_strategy { "current" } else { "candidate" },
                "outcome_id": "outcome.http-ready",
                "strategy_id": "strategy.http-ready",
                "work_ids": ["work.change-admission", "work.change-admission-successor"],
                "milestone_id": "milestone.http-ready",
                "adopted_plan": {"outcome_id": "outcome.http-ready", "generation": 1},
                "change_baseline_id": "baseline.http-ready"
                ,"resource_id": "resource.quicklens-fixture"
            },
            "start": "zap serve-runtime application-server.json"
        }))?,
    )?;
    println!("{}", root.display());
    Ok(())
}
