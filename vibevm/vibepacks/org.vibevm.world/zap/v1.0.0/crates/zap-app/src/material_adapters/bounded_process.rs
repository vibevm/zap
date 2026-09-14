use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::time::{Duration, Instant};

use zap_wire::{ErrorCode, FixSurface, ZapError};

use super::adapter_error;

pub(super) struct BoundedOutput {
    pub(super) success: bool,
    pub(super) stdout: Vec<u8>,
}

enum StreamMessage {
    Chunk { stdout: bool, bytes: Vec<u8> },
    Finished,
    Failed,
}

pub(super) fn run_bounded(
    executable: &Path,
    arguments: &[&str],
    maximum_output_bytes: u64,
    timeout_ms: u64,
) -> Result<BoundedOutput, ZapError> {
    if maximum_output_bytes == 0 || timeout_ms == 0 {
        return Err(process_limit());
    }
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| process_unavailable())?;
    let stdout = child.stdout.take().ok_or_else(process_unavailable)?;
    let stderr = child.stderr.take().ok_or_else(process_unavailable)?;
    let (sender, receiver) = sync_channel(4);
    let stdout_reader = spawn_reader(stdout, true, sender.clone());
    let stderr_reader = spawn_reader(stderr, false, sender);
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .ok_or_else(process_limit)?;
    let mut stdout_bytes = Vec::new();
    let mut observed = 0_u64;
    let mut finished = 0_u8;
    let mut cancelled = false;
    while finished < 2 {
        let now = Instant::now();
        if now >= deadline {
            cancelled = true;
            break;
        }
        match receive_until(&receiver, deadline.saturating_duration_since(now)) {
            Ok(StreamMessage::Chunk { stdout, bytes }) => {
                observed = observed
                    .checked_add(u64::try_from(bytes.len()).map_err(|_| process_limit())?)
                    .ok_or_else(process_limit)?;
                if observed > maximum_output_bytes {
                    cancelled = true;
                    break;
                }
                if stdout {
                    stdout_bytes.extend_from_slice(&bytes);
                }
            }
            Ok(StreamMessage::Finished) => finished += 1,
            Ok(StreamMessage::Failed) => {
                cancelled = true;
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    cancelled = true;
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                cancelled = true;
                break;
            }
        }
    }
    if cancelled {
        drop(receiver);
        kill_and_reap(child)?;
        drop(stdout_reader);
        drop(stderr_reader);
        return Err(process_limit());
    }
    let Some(status) = wait_until(&mut child, deadline)? else {
        drop(receiver);
        kill_and_reap(child)?;
        drop(stdout_reader);
        drop(stderr_reader);
        return Err(process_limit());
    };
    drop(receiver);
    let stdout_ok = stdout_reader.join().is_ok();
    let stderr_ok = stderr_reader.join().is_ok();
    if !stdout_ok || !stderr_ok || finished < 2 {
        return Err(process_limit());
    }
    Ok(BoundedOutput {
        success: status.success(),
        stdout: stdout_bytes,
    })
}

fn wait_until(
    child: &mut std::process::Child,
    deadline: Instant,
) -> Result<Option<std::process::ExitStatus>, ZapError> {
    loop {
        if let Some(status) = child.try_wait().map_err(|_| process_unavailable())? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(10)),
        );
    }
}

fn kill_and_reap(mut child: std::process::Child) -> Result<(), ZapError> {
    match child.kill() {
        Ok(()) => {
            child.wait().map_err(|_| process_unavailable())?;
            Ok(())
        }
        Err(_) => match child.try_wait().map_err(|_| process_unavailable())? {
            Some(_) => Ok(()),
            None => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                Err(process_unavailable())
            }
        },
    }
}

fn receive_until(
    receiver: &Receiver<StreamMessage>,
    remaining: Duration,
) -> Result<StreamMessage, RecvTimeoutError> {
    receiver.recv_timeout(remaining.min(Duration::from_millis(25)))
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    stdout: bool,
    sender: SyncSender<StreamMessage>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || read_stream(reader, stdout, sender))
}

fn read_stream<R: Read>(mut reader: R, stdout: bool, sender: SyncSender<StreamMessage>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(StreamMessage::Finished);
                return;
            }
            Ok(read) => {
                if sender
                    .send(StreamMessage::Chunk {
                        stdout,
                        bytes: buffer[..read].to_vec(),
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(_) => {
                let _ = sender.send(StreamMessage::Failed);
                return;
            }
        }
    }
}

fn process_limit() -> ZapError {
    adapter_error(
        ErrorCode::LimitExceeded,
        "configured adapter process exceeded its output or execution bound",
        FixSurface::Adapter,
    )
}

fn process_unavailable() -> ZapError {
    adapter_error(
        ErrorCode::Unavailable,
        "configured adapter process is unavailable",
        FixSurface::Adapter,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excessive_output_and_hanging_children_are_killed_and_reaped() {
        let (program, output_args, hang_args, closed_stream_args) = commands();
        assert!(run_bounded(Path::new(program), &output_args, 32, 5_000).is_err());
        let started = Instant::now();
        assert!(run_bounded(Path::new(program), &hang_args, 1024, 50).is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
        let started = Instant::now();
        assert!(run_bounded(Path::new(program), &closed_stream_args, 1024, 1_500).is_err());
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[cfg(windows)]
    fn commands() -> (
        &'static str,
        [&'static str; 4],
        [&'static str; 4],
        [&'static str; 4],
    ) {
        (
            "powershell.exe",
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Out.Write(('x' * 65536))",
            ],
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 10",
            ],
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Out.Close(); [Console]::Error.Close(); Start-Sleep -Seconds 10",
            ],
        )
    }

    #[cfg(unix)]
    fn commands() -> (
        &'static str,
        [&'static str; 2],
        [&'static str; 2],
        [&'static str; 2],
    ) {
        (
            "sh",
            ["-c", "head -c 65536 /dev/zero"],
            ["-c", "sleep 10"],
            ["-c", "exec 1>&- 2>&-; sleep 10"],
        )
    }
}
