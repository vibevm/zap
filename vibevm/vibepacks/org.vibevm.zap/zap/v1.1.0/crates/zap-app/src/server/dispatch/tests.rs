use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

#[test]
fn timed_out_command_returns_unknown_while_submission_finishes() -> Result<(), ZapError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| server_error(ErrorCode::InternalInvariant, "test runtime failed"))?;
    let completed = Arc::new(AtomicBool::new(false));
    let task_completed = completed.clone();
    let task = runtime.spawn_blocking(move || -> Result<MachineResponse, ZapError> {
        std::thread::sleep(Duration::from_millis(40));
        task_completed.store(true, Ordering::Release);
        Err(ZapError::unsupported_operation())
    });
    let command_id = CommandId::parse("command.timeout.r13c")?;
    let command_digest = CommandDigest::hash(b"canonical-timeout-frame");
    let response = runtime
        .block_on(await_service_task(
            task,
            Duration::from_millis(5),
            Some((command_id.clone(), command_digest)),
        ))
        .map_err(|failure| failure.error)?;
    assert!(matches!(
        response,
        MachineResponse::Command(zap_api::SubmissionStatusView::Unknown {
            command_id: returned_id,
            command_digest: returned_digest,
        }) if returned_id == command_id && returned_digest == command_digest
    ));
    std::thread::sleep(Duration::from_millis(50));
    assert!(completed.load(Ordering::Acquire));
    runtime.shutdown_timeout(Duration::from_millis(10));
    Ok(())
}
