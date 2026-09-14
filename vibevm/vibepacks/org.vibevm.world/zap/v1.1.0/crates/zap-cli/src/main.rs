use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use zap_api::{
    AffectedTraversalBeginRequest, AffectedTraversalCancelRequest,
    AffectedTraversalContinueRequest, BundleArchiveRequest, BundleEntryReadRequest,
    IndexRebuildRequest, MachineRequest, MachineResponse, execute_read,
};
use zap_app::{
    ApplicationServerConfig, ApplicationService, LegacyImportConfig, ReadApplication, ReadServer,
    ReadServerConfig, import_legacy,
};

fn main() {
    if let Err(error) = run() {
        let body = serde_json::to_string(&error)
            .unwrap_or_else(|_| "{\"code\":\"internal_invariant\"}".to_owned());
        eprintln!("{body}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), zap_wire::ZapError> {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        println!(
            "zap <store.redb> <request.json>\nzap capabilities\nzap serve <read-config.json>\nzap serve-runtime <application-config.json>\nzap recover-service-lease <application-config.json>\nzap rebuild-indexes <application-config.json> <request.json>\nzap traversal-begin <application-config.json> <request.json>\nzap traversal-continue <application-config.json> <request.json>\nzap traversal-cancel <application-config.json> <request.json>\nzap archive-publish <application-config.json> <request.json>\nzap archive-verify <application-config.json> <request.json>\nzap archive-entry <application-config.json> <request.json>\nzap import-legacy <config.json>"
        );
        return Ok(());
    };
    if first == "capabilities" {
        let body = serde_json::to_string(&zap_api::SurfaceCapabilities::default())
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        println!("{body}");
        return Ok(());
    }
    if first == "serve" {
        let config_path = args
            .next()
            .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: ReadServerConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let server = ReadServer::from_config(config)?;
        server.serve_until(&AtomicBool::new(false))?;
        return Ok(());
    }
    if first == "serve-runtime" || first == "recover-service-lease" {
        let config_path = PathBuf::from(
            args.next()
                .ok_or_else(zap_wire::ZapError::unsupported_operation)?,
        );
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(&config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: ApplicationServerConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        if first == "recover-service-lease" {
            let recovered = ApplicationService::recover_service_lease(&config.service)?;
            println!("{{\"recovered\":{recovered}}}");
            return Ok(());
        }
        let config_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let server = ReadServer::from_application_config(config_dir, config)?;
        server.serve_until(&AtomicBool::new(false))?;
        return Ok(());
    }
    if first == "archive-publish" || first == "archive-verify" || first == "archive-entry" {
        let config_path = PathBuf::from(
            args.next()
                .ok_or_else(zap_wire::ZapError::unsupported_operation)?,
        );
        let request_path = args
            .next()
            .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(&config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: ApplicationServerConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let request_bytes =
            std::fs::read(request_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let response = if first == "archive-publish" {
            let request: BundleArchiveRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            let credential_id = config.service.trust.trusted.channel.credential_id.clone();
            let secret = std::fs::read(&config.service.trust.trusted.channel.credential_file)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            let service = ApplicationService::open_filesystem(config_dir, config.service)?;
            MachineResponse::BundleArchive(service.publish_bundle_archive(
                &credential_id,
                &secret,
                &request,
            )?)
        } else if first == "archive-verify" {
            let request: BundleArchiveRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            let service = ApplicationService::open_filesystem(config_dir, config.service)?;
            MachineResponse::BundleArchive(service.verify_bundle_archive(&request)?)
        } else {
            let request: BundleEntryReadRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            let service = ApplicationService::open_filesystem(config_dir, config.service)?;
            MachineResponse::BundleEntry(service.read_bundle_entry(&request)?)
        };
        println!(
            "{}",
            serde_json::to_string(&response)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?
        );
        return Ok(());
    }
    if first == "rebuild-indexes" {
        let config_path = PathBuf::from(
            args.next()
                .ok_or_else(zap_wire::ZapError::unsupported_operation)?,
        );
        let request_path = args
            .next()
            .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(&config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: ApplicationServerConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let request_bytes =
            std::fs::read(request_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let request: IndexRebuildRequest = serde_json::from_slice(&request_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let credential_id = config.service.trust.owner.credential.credential_id.clone();
        let secret = std::fs::read(&config.service.trust.owner.credential.credential_file)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let service = ApplicationService::open_filesystem(config_dir, config.service)?;
        let response = MachineResponse::IndexRebuild(service.rebuild_viewer_indexes(
            &credential_id,
            &secret,
            &request,
        )?);
        println!(
            "{}",
            serde_json::to_string(&response)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?
        );
        return Ok(());
    }
    if first == "traversal-begin" || first == "traversal-continue" || first == "traversal-cancel" {
        let config_path = PathBuf::from(
            args.next()
                .ok_or_else(zap_wire::ZapError::unsupported_operation)?,
        );
        let request_path = args
            .next()
            .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(&config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: ApplicationServerConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let request_bytes =
            std::fs::read(request_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let credential_id = config.service.trust.owner.credential.credential_id.clone();
        let secret = std::fs::read(&config.service.trust.owner.credential.credential_file)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let service = ApplicationService::open_filesystem(config_dir, config.service)?;
        let response = if first == "traversal-begin" {
            let request: AffectedTraversalBeginRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            MachineResponse::AffectedTraversal(service.begin_affected_traversal(
                &credential_id,
                &secret,
                &request,
            )?)
        } else if first == "traversal-continue" {
            let request: AffectedTraversalContinueRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            MachineResponse::AffectedTraversal(service.continue_affected_traversal(
                &credential_id,
                &secret,
                &request,
            )?)
        } else {
            let request: AffectedTraversalCancelRequest = serde_json::from_slice(&request_bytes)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
            MachineResponse::AffectedTraversal(service.cancel_affected_traversal(
                &credential_id,
                &secret,
                &request,
            )?)
        };
        println!(
            "{}",
            serde_json::to_string(&response)
                .map_err(|_| zap_wire::ZapError::unsupported_operation())?
        );
        return Ok(());
    }
    if first == "import-legacy" {
        let config_path = args
            .next()
            .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
        if args.next().is_some() {
            return Err(zap_wire::ZapError::unsupported_operation());
        }
        let config_bytes =
            std::fs::read(config_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let config: LegacyImportConfig = serde_json::from_slice(&config_bytes)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        let receipt = import_legacy(&config)?;
        let body = serde_json::to_string(&receipt)
            .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
        println!("{body}");
        return Ok(());
    }
    let store = PathBuf::from(first);
    let request_path = args
        .next()
        .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
    if args.next().is_some() {
        return Err(zap_wire::ZapError::unsupported_operation());
    }
    let bytes =
        std::fs::read(request_path).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
    let request: MachineRequest =
        serde_json::from_slice(&bytes).map_err(|_| zap_wire::ZapError::unsupported_operation())?;
    let app = ReadApplication::open(store)?;
    let response = execute_read(&app, &request)?;
    let body = serde_json::to_string(&response)
        .map_err(|_| zap_wire::ZapError::unsupported_operation())?;
    println!("{body}");
    Ok(())
}
