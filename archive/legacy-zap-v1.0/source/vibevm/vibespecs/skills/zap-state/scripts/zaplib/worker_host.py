"""Independent durable host for one descriptor-bound subprocess."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from zaplib.common import packed, sha
    from zaplib.transport_receipts import complete_spawn_failure, owned, request_policy, stop_requests
    from zaplib.transport_io import (
        artifact,
        atomic_json,
        classify_failure,
        creation_options,
        now_ns,
        process_matches,
        process_token,
        publish_file,
        read_json,
        validate_environment_name,
    )
else:
    from .common import packed, sha
    from .transport_receipts import complete_spawn_failure, owned, request_policy, stop_requests
    from .transport_io import (
        artifact,
        atomic_json,
        classify_failure,
        creation_options,
        now_ns,
        process_matches,
        process_token,
        publish_file,
        read_json,
        validate_environment_name,
    )


def base_record(descriptor: dict[str, Any], schema: str, **extra: Any) -> dict[str, Any]:
    return {
        "schema": schema,
        "job_id": descriptor["job_id"],
        "nonce": descriptor["nonce"],
        "descriptor_sha256": descriptor["descriptor_sha256"],
        **extra,
    }


def load_bound_descriptor(args: argparse.Namespace) -> tuple[Path, dict[str, Any]]:
    root = Path(args.root).absolute().resolve(strict=True)
    job_dir = (root / "jobs" / args.job_key).absolute()
    if not job_dir.resolve(strict=True).is_relative_to(root) or args.job_key != sha(args.job_id.encode("utf-8")):
        raise ValueError("job path binding differs")
    descriptor = read_json(job_dir / "descriptor.json")
    if not (
        descriptor.get("schema") == "zap-process-descriptor/1"
        and descriptor.get("job_id") == args.job_id
        and descriptor.get("nonce") == args.nonce
        and descriptor.get("descriptor_sha256") == args.descriptor_sha256
        and descriptor["descriptor_sha256"] == sha(packed(descriptor.get("logical")))
    ):
        raise ValueError("descriptor binding differs")
    packet = job_dir / "packet.txt"
    if sha(packet.read_bytes()) != descriptor["logical"].get("packet_sha256"):
        raise ValueError("packet binding differs")
    return job_dir, descriptor


def wait_for_process_token(pid: int) -> str | None:
    deadline = time.monotonic() + 1.0
    token = process_token(pid)
    while token is None and time.monotonic() < deadline:
        time.sleep(0.01)
        token = process_token(pid)
    return token


def deliver_stops(job_dir: Path, descriptor: dict[str, Any], requests: list[dict[str, Any]]) -> list[str]:
    deliveries = job_dir / "stop_deliveries"
    deliveries.mkdir(exist_ok=True)
    delivered = []
    for request in requests:
        key = sha(request["request_id"].encode("utf-8"))
        path = deliveries / f"{key}.json"
        if not path.exists():
            atomic_json(
                path,
                base_record(
                    descriptor,
                    "zap-process-stop-delivery/1",
                    request_id=request["request_id"],
                    delivered=True,
                    mode=request["mode"],
                    method="cooperative_stop_file",
                    delivered_at_ns=now_ns(),
                ),
            )
        else:
            existing = read_json(path)
            if not (owned(existing, descriptor, "zap-process-stop-delivery/1")
                    and existing.get("request_id") == request["request_id"]
                    and existing.get("mode") == request["mode"]
                    and existing.get("delivered") is True):
                continue
        delivered.append(request["request_id"])
    return delivered


def safe_terminate(
    process: subprocess.Popen[bytes], job_dir: Path, descriptor: dict[str, Any],
    child_token: str | None, request: dict[str, Any],
) -> bool:
    try:
        started = read_json(job_dir / "started.json")
    except (OSError, ValueError):
        return False
    if not (
        owned(started, descriptor, "zap-process-started/1")
        and started.get("pid") == process.pid
        and started.get("process_token") == child_token
        and process_matches(process.pid, child_token) is True
        and process.poll() is None
    ):
        return False
    try:
        process.terminate()
    except (OSError, ProcessLookupError):
        return False
    atomic_json(
        job_dir / "termination.json",
        base_record(
            descriptor,
            "zap-process-termination/1",
            pid=process.pid,
            process_token=child_token,
            request_id=request["request_id"],
            mode="terminate",
            ownership_verified=True,
            signal_sent=True,
            sent_at_ns=now_ns(),
        ),
    )
    return True


def run(job_dir: Path, descriptor: dict[str, Any]) -> int:
    atomic_json(
        job_dir / "host_started.json",
        base_record(
            descriptor,
            "zap-process-host/1",
            host_pid=os.getpid(),
            host_process_token=wait_for_process_token(os.getpid()),
            started_at_ns=now_ns(),
        ),
    )
    logical = descriptor["logical"]
    argv = logical.get("argv")
    if not isinstance(argv, list) or sum(item == "{packet_file}" for item in argv) != 1:
        complete_spawn_failure(job_dir, descriptor)
        return 2
    command = [str(job_dir / "packet.txt") if item == "{packet_file}" else item for item in argv]
    environment = logical.get("environment")
    if not isinstance(environment, dict) or not all(isinstance(value, str) for value in environment.values()):
        complete_spawn_failure(job_dir, descriptor)
        return 2
    try:
        for name in environment:
            validate_environment_name(name)
    except ValueError:
        complete_spawn_failure(job_dir, descriptor)
        return 2
    child_environment = dict(environment)
    child_environment["ZAP_STOP_FILE"] = str(job_dir / "stop.signal")
    stdout_path = job_dir / "stdout.bin"
    stderr_path = job_dir / "stderr.bin"
    stdout_partial = job_dir / "stdout.partial"
    stderr_partial = job_dir / "stderr.partial"
    spawn_failed = False
    with stdout_partial.open("xb", buffering=0) as stdout, stderr_partial.open("xb", buffering=0) as stderr:
        try:
            process = subprocess.Popen(
                command,
                cwd=logical["cwd"],
                env=child_environment,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                close_fds=True,
                **creation_options(),
            )
        except (OSError, ValueError):
            spawn_failed = True
        if not spawn_failed:
            child_token = wait_for_process_token(process.pid)
            atomic_json(
                job_dir / "started.json",
                base_record(
                    descriptor,
                    "zap-process-started/1",
                    pid=process.pid,
                    process_token=child_token,
                    started_at_ns=now_ns(),
                ),
            )
            termination_seen_at: dict[str, float] = {}
            delivered_before_exit: set[str] = set()
            termination_sent = False
            while process.poll() is None:
                requests = stop_requests(job_dir, descriptor)
                if requests and process.poll() is None:
                    delivered_before_exit.update(deliver_stops(job_dir, descriptor, requests))
                    current = time.monotonic()
                    for request in requests:
                        mode, delay = request_policy(request) or (None, None)
                        if (mode == "terminate" and logical.get("allow_process_termination") is True
                                and request["request_id"] in delivered_before_exit and not termination_sent):
                            observed = termination_seen_at.setdefault(request["request_id"], current)
                            if current - observed >= delay:
                                termination_sent = safe_terminate(
                                    process, job_dir, descriptor, child_token, request,
                                )
                time.sleep(0.03)
            exit_code = process.wait()
            exit_observed_at_ns = now_ns()
        stdout.flush()
        stderr.flush()
        os.fsync(stdout.fileno())
        os.fsync(stderr.fileno())
    publish_file(stdout_partial, stdout_path)
    publish_file(stderr_partial, stderr_path)
    if spawn_failed:
        complete_spawn_failure(job_dir, descriptor)
        return 2
    stderr_raw = stderr_path.read_bytes()
    stopped = bool(delivered_before_exit)
    state = "interrupted" if termination_sent else ("stopped" if stopped else ("succeeded" if exit_code == 0 else "failed"))
    classification = (
        "process_terminated" if termination_sent else
        "stop_completed" if stopped else
        "success" if exit_code == 0 else classify_failure(stderr_raw, exit_code)
    )
    atomic_json(
        job_dir / "completed.json",
        base_record(
            descriptor,
            "zap-process-completed/1",
            state=state,
            exit_code=exit_code,
            exit_observed_at_ns=exit_observed_at_ns,
            completed_at_ns=now_ns(),
            stdout=artifact(stdout_path),
            stderr=artifact(stderr_path),
            diagnostic={"classification": classification},
            termination_sent=termination_sent,
            delivered_stop_request_ids=sorted(delivered_before_exit),
            safe_state_verified=False,
            effects_reconciled=False,
        ),
    )
    return 0


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(add_help=False)
    result.add_argument("--root", required=True)
    result.add_argument("--job-key", required=True)
    result.add_argument("--job-id", required=True)
    result.add_argument("--nonce", required=True)
    result.add_argument("--descriptor-sha256", required=True)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        job_dir, descriptor = load_bound_descriptor(args)
        return run(job_dir, descriptor)
    except (OSError, ValueError, KeyError, TypeError):
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
