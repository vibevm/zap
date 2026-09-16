"""Durable argv-based subprocess adapter."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import threading
import time
from typing import Any, Iterable, Mapping, Sequence
import uuid

from .common import Refusal, identity, need, packed, sha
from .transport_io import (
    atomic_json,
    atomic_write,
    confined_directory,
    creation_options,
    existing_directory,
    file_lock,
    now_ns,
    private_root,
    process_matches,
    process_token,
    read_json,
    reject_link_components,
    validate_environment_name,
)
from .transport_receipts import (
    completed_record,
    complete_spawn_failure,
    host_started_record,
    owned,
    prepare_known_pre_effect,
    request_policy,
    started_record,
    stop_policy,
    stop_summary,
    valid_stop_request,
)


DESCRIPTOR_SCHEMA = "zap-process-job/1"
TERMINAL = {"succeeded", "failed", "stopped", "interrupted"}
DEFAULT_ENVIRONMENT = (
    ("SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "PATH", "TEMP", "TMP")
    if os.name == "nt"
    else ("PATH", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR")
)


def _retain_until_exit(process: subprocess.Popen[Any]) -> None:
    """Keep the detached host object alive and reap its local handle quietly."""
    def wait() -> None:
        try:
            process.wait()
        except OSError:
            pass

    threading.Thread(target=wait, name=f"zap-host-{process.pid}", daemon=True).start()


class ProcessTransport:
    """Persistent worker-host transport with descriptor-bound idempotency."""

    def __init__(
        self,
        root: str | os.PathLike[str],
        allowed_workspace_roots: Iterable[str | os.PathLike[str]],
        *,
        inherited_environment: Iterable[str] | None = None,
        environment_allowlist: Iterable[str] = (),
        allow_process_termination: bool = False,
    ) -> None:
        self.root = private_root(root)
        roots = tuple(existing_directory(item) for item in allowed_workspace_roots)
        need(bool(roots), "PATH", "at least one allowed workspace root is required")
        self.allowed_workspace_roots = roots
        inherited = DEFAULT_ENVIRONMENT if inherited_environment is None else tuple(inherited_environment)
        self.inherited_environment = self._environment_names(inherited)
        self.environment_allowlist = self._environment_names(tuple(environment_allowlist))
        need(type(allow_process_termination) is bool, "STOP", "allow_process_termination must be boolean")
        self.allow_process_termination = allow_process_termination
        self.jobs = self.root / "jobs"
        self.jobs.mkdir(exist_ok=True)
        reject_link_components(self.jobs)
        need(self.jobs.is_dir(), "PATH", "transport jobs path is not a directory")
        try:
            self.jobs.chmod(0o700)
        except OSError:
            pass
        self.lock_path = self.root / "transport.lock"

    @staticmethod
    def _environment_names(values: Iterable[Any]) -> tuple[str, ...]:
        validated = tuple(validate_environment_name(value) for value in values)
        names = tuple(name.upper() for name in validated) if os.name == "nt" else validated
        need(len(names) == len(set(names)), "ENVIRONMENT", "duplicate environment name")
        return names

    def _job_dir(self, job_id: str) -> Path:
        result = self.jobs / sha(job_id.encode("utf-8"))
        reject_link_components(result)
        return result

    @staticmethod
    def _request_key(request_id: str) -> str:
        return sha(request_id.encode("utf-8"))

    def _effective_environment(self, submitted: Mapping[str, str] | None) -> dict[str, str]:
        result: dict[str, str] = {}
        for name in self.inherited_environment:
            if name in os.environ:
                result[name] = os.environ[name]
        if submitted is None:
            return result
        need(isinstance(submitted, Mapping), "ENVIRONMENT", "environment must be a string mapping")
        allowed = set(self.environment_allowlist)
        submitted_keys = set()
        for name, value in submitted.items():
            validate_environment_name(name)
            key = name.upper() if os.name == "nt" else name
            need(key not in submitted_keys, "ENVIRONMENT", "duplicate environment name")
            submitted_keys.add(key)
            need(key in allowed, "ENVIRONMENT", f"environment name is not allowlisted: {name}")
            need(isinstance(value, str) and "\0" not in value, "ENVIRONMENT", "environment values must be NUL-free strings")
            result[key] = value
        return result

    def _logical_descriptor(
        self,
        job_id: str,
        argv: Sequence[str],
        cwd: str | os.PathLike[str],
        packet: str,
        environment: Mapping[str, str] | None,
    ) -> tuple[dict[str, Any], bytes]:
        identity(job_id)
        need(isinstance(argv, (list, tuple)) and bool(argv), "ARGV", "argv must be a nonempty sequence")
        need(all(isinstance(item, str) and "\0" not in item for item in argv), "ARGV", "argv entries must be NUL-free strings")
        need(sum(item == "{packet_file}" for item in argv) == 1, "ARGV", "argv must contain exactly one whole {packet_file} placeholder")
        need(isinstance(packet, str), "PACKET", "packet must be UTF-8 text")
        packet_raw = packet.encode("utf-8")
        workdir = confined_directory(cwd, self.allowed_workspace_roots)
        logical = {
            "schema": DESCRIPTOR_SCHEMA,
            "job_id": job_id,
            "argv": list(argv),
            "cwd": str(workdir),
            "packet_sha256": sha(packet_raw),
            "environment": self._effective_environment(environment),
            "allow_process_termination": self.allow_process_termination,
        }
        return logical, packet_raw

    def _load_descriptor(self, job_id: str) -> tuple[Path, dict[str, Any]]:
        identity(job_id)
        job_dir = self._job_dir(job_id)
        path = job_dir / "descriptor.json"
        need(path.exists(), "UNKNOWN_JOB", f"unknown transport job: {job_id}")
        descriptor = read_json(path)
        need(
            isinstance(descriptor, dict)
            and set(descriptor) == {"schema", "job_id", "nonce", "descriptor_sha256", "logical"}
            and descriptor["schema"] == "zap-process-descriptor/1"
            and descriptor["job_id"] == job_id
            and isinstance(descriptor["nonce"], str)
            and descriptor["descriptor_sha256"] == sha(packed(descriptor["logical"])),
            "TRANSPORT_CORRUPT",
            "invalid immutable transport descriptor",
        )
        identity(descriptor["nonce"])
        packet_path = job_dir / "packet.txt"
        need(packet_path.is_file() and sha(packet_path.read_bytes()) == descriptor["logical"].get("packet_sha256"), "TRANSPORT_CORRUPT", "packet artifact differs from descriptor")
        return job_dir, descriptor

    _owned = staticmethod(owned)

    def _resume_pre_effect(self, job_dir: Path, descriptor: dict[str, Any]) -> bool:
        """Complete a launch only when durable ordering proves no intent existed."""
        prepared = prepare_known_pre_effect(job_dir, descriptor)
        if prepared:
            self._launch_host(job_dir, descriptor)
        return prepared

    def submit(
        self,
        job_id: str,
        *,
        argv: Sequence[str],
        cwd: str | os.PathLike[str],
        packet: str,
        environment: Mapping[str, str] | None = None,
    ) -> dict[str, Any]:
        logical, packet_raw = self._logical_descriptor(job_id, argv, cwd, packet, environment)
        fingerprint = sha(packed(logical))
        idempotent = False
        resumed_pre_effect = False
        with file_lock(self.lock_path):
            job_dir = self._job_dir(job_id)
            descriptor_path = job_dir / "descriptor.json"
            if descriptor_path.exists():
                _, existing = self._load_descriptor(job_id)
                need(existing["descriptor_sha256"] == fingerprint, "IDEMPOTENCY", "job id reused with a changed descriptor")
                idempotent = True
                if not (job_dir / "launch_intent.json").exists():
                    resumed_pre_effect = self._resume_pre_effect(job_dir, existing)
            else:
                job_dir.mkdir(parents=True, exist_ok=True)
                try:
                    job_dir.chmod(0o700)
                except OSError:
                    pass
                nonce = uuid.uuid4().hex
                descriptor = {
                    "schema": "zap-process-descriptor/1",
                    "job_id": job_id,
                    "nonce": nonce,
                    "descriptor_sha256": fingerprint,
                    "logical": logical,
                }
                atomic_write(job_dir / "packet.txt", packet_raw)
                atomic_json(descriptor_path, descriptor)
                atomic_json(job_dir / "prepared.json", self._base_record(descriptor, "zap-process-prepared/1", prepared_at_ns=now_ns()))
                atomic_json(job_dir / "launch_intent.json", self._base_record(descriptor, "zap-process-launch-intent/1", intended_at_ns=now_ns()))
                self._launch_host(job_dir, descriptor)
        current = self.status(job_id)
        return {
            "schema": "zap-transport-submit/1",
            "job_id": job_id,
            "nonce": current["nonce"],
            "descriptor_sha256": fingerprint,
            "accepted": True,
            "idempotent": idempotent,
            "resumed_pre_effect": resumed_pre_effect,
            "state": current["state"],
        }

    @staticmethod
    def _base_record(descriptor: dict[str, Any], schema: str, **extra: Any) -> dict[str, Any]:
        return {
            "schema": schema,
            "job_id": descriptor["job_id"],
            "nonce": descriptor["nonce"],
            "descriptor_sha256": descriptor["descriptor_sha256"],
            **extra,
        }

    def _launch_host(self, job_dir: Path, descriptor: dict[str, Any]) -> None:
        worker = Path(__file__).with_name("worker_host.py")
        command = [
            sys.executable,
            "-B",
            str(worker),
            "--root",
            str(self.root),
            "--job-key",
            job_dir.name,
            "--job-id",
            descriptor["job_id"],
            "--nonce",
            descriptor["nonce"],
            "--descriptor-sha256",
            descriptor["descriptor_sha256"],
        ]
        host_env = dict(descriptor["logical"]["environment"])
        try:
            process = subprocess.Popen(
                command,
                cwd=worker.parent,
                env=host_env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                close_fds=True,
                **creation_options(),
            )
        except OSError:
            self._record_host_spawn_failure(job_dir, descriptor)
            return
        token = process_token(process.pid)
        if hasattr(process, "wait"):
            _retain_until_exit(process)
        atomic_json(
            job_dir / "parent_launch.json",
            self._base_record(descriptor, "zap-process-parent-launch/1", host_pid=process.pid, host_process_token=token, launched_at_ns=now_ns()),
        )

    def _record_host_spawn_failure(self, job_dir: Path, descriptor: dict[str, Any]) -> None:
        complete_spawn_failure(job_dir, descriptor)

    _stop_policy = staticmethod(stop_policy)
    _request_policy = staticmethod(request_policy)

    def _stop_summary(self, job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any]:
        return stop_summary(job_dir, descriptor, self._request_key)

    def _started(self, job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any] | None:
        return started_record(job_dir, descriptor)

    def _host_started(self, job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any] | None:
        return host_started_record(job_dir, descriptor)

    def _completed(self, job_dir: Path, descriptor: dict[str, Any]) -> dict[str, Any] | None:
        return completed_record(job_dir, descriptor, self.root, TERMINAL, self._request_key)

    def _derive_status(self, job_id: str, *, recovery: bool) -> dict[str, Any]:
        job_dir, descriptor = self._load_descriptor(job_id)
        stop = self._stop_summary(job_dir, descriptor)
        completed = self._completed(job_dir, descriptor)
        process = {"pid": None, "active": False, "ownership_verified": False}
        diagnostic = None
        if completed is not None:
            state = completed["state"]
            process["ownership_verified"] = True
            diagnostic = completed["diagnostic"]
        else:
            started_path = job_dir / "started.json"
            host_path = job_dir / "host_started.json"
            if started_path.exists():
                started = self._started(job_dir, descriptor)
                host = self._host_started(job_dir, descriptor)
                host_match = (process_matches(host.get("host_pid"), host.get("host_process_token"))
                              if host is not None else None)
                if started is not None and host_match is True:
                    process["pid"] = started.get("pid")
                    match = process_matches(started.get("pid"), started.get("process_token"))
                    process["active"] = match
                    process["ownership_verified"] = match is True
                    state = ("stop_requested" if stop["requested"] else "running") if match is True else "unknown_effect"
                    if match is not True:
                        diagnostic = {"classification": "missing_completion_receipt"}
                else:
                    state = "unknown_effect"
                    process["active"] = None
                    diagnostic = {"classification": "invalid_started_chain"}
            elif host_path.exists():
                host = self._host_started(job_dir, descriptor)
                match = process_matches(host.get("host_pid"), host.get("host_process_token")) if host is not None else None
                process.update({"pid": None, "active": None, "ownership_verified": False})
                state = "stop_requested" if stop["requested"] and match is True else ("starting" if match is True else "unknown_effect")
                if match is not True:
                    diagnostic = {"classification": "host_receipt_not_live"}
            elif (job_dir / "launch_intent.json").exists():
                state = "starting"
                if recovery:
                    parent_path = job_dir / "parent_launch.json"
                    parent_live = None
                    if parent_path.exists():
                        parent = read_json(parent_path)
                        if self._owned(parent, descriptor, "zap-process-parent-launch/1"):
                            parent_live = process_matches(parent.get("host_pid"), parent.get("host_process_token"))
                    intent = read_json(job_dir / "launch_intent.json")
                    old = isinstance(intent.get("intended_at_ns"), int) and now_ns() - intent["intended_at_ns"] > 500_000_000
                    if parent_live is not True and old:
                        state = "unknown_effect"
                        diagnostic = {"classification": "missing_host_receipt"}
                process["active"] = None
            else:
                state = "prepared"
                process["active"] = None
        return {
            "schema": "zap-transport-status/1",
            "job_id": job_id,
            "nonce": descriptor["nonce"],
            "descriptor_sha256": descriptor["descriptor_sha256"],
            "state": state,
            "process": process,
            "stop": stop,
            "delivery": {
                "requested": stop["requested"], "request_ids": stop["request_ids"],
                "delivered": stop["delivered"], "delivered_request_ids": stop["delivered_request_ids"],
            },
            "termination": {"requested": stop["termination_requested"], "sent": stop["termination_sent"]},
            "safe_state": {
                "verified": False,
                "basis": None,
                "needs_reconcile": stop["requested"] or state in {"stop_requested", "stopped", "interrupted", "unknown_effect"},
            },
            "result_available": completed is not None,
            "diagnostic": diagnostic,
        }

    def status(self, job_id: str) -> dict[str, Any]:
        return self._derive_status(job_id, recovery=False)

    def request_stop(
        self, job_id: str, request_id: str, *, mode: str = "cooperative",
        terminate_after_seconds: float | None = None,
    ) -> dict[str, Any]:
        identity(request_id)
        mode, terminate_after_seconds = self._stop_policy(mode, terminate_after_seconds)
        idempotent = False
        with file_lock(self.lock_path):
            job_dir, descriptor = self._load_descriptor(job_id)
            if mode == "terminate":
                need(self.allow_process_termination and descriptor["logical"].get("allow_process_termination") is True,
                     "STOP", "process termination is not enabled for this transport job")
            terminal = self._completed(job_dir, descriptor)
            request_dir = job_dir / "stop_requests"
            path = request_dir / f"{self._request_key(request_id)}.json"
            if terminal is not None:
                idempotent = path.exists()
                if idempotent:
                    existing = read_json(path)
                    need(valid_stop_request(existing, descriptor)
                         and existing.get("request_id") == request_id
                         and self._request_policy(existing) == (mode, terminate_after_seconds),
                         "IDEMPOTENCY", "stop request identity differs")
                delivery = job_dir / "stop_deliveries" / f"{self._request_key(request_id)}.json"
                delivered = False
                if delivery.exists():
                    value = read_json(delivery)
                    delivered = (self._owned(value, descriptor, "zap-process-stop-delivery/1")
                                 and value.get("request_id") == request_id and value.get("delivered") is True
                                 and value.get("mode", "cooperative") == mode)
                return self._stop_result(descriptor, request_id, terminal["state"], idempotent, delivered,
                                         True, True, mode, terminate_after_seconds)
            request_dir.mkdir(exist_ok=True)
            if path.exists():
                existing = read_json(path)
                need(valid_stop_request(existing, descriptor)
                     and existing.get("request_id") == request_id
                     and self._request_policy(existing) == (mode, terminate_after_seconds),
                     "IDEMPOTENCY", "stop request identity differs")
                idempotent = True
            else:
                atomic_json(path, self._base_record(
                    descriptor, "zap-process-stop-request/1", request_id=request_id,
                    mode=mode, terminate_after_seconds=terminate_after_seconds, requested_at_ns=now_ns(),
                ))
            signal = job_dir / "stop.signal"
            if not signal.exists():
                atomic_write(signal, b"stop\n")
        delivery = job_dir / "stop_deliveries" / f"{self._request_key(request_id)}.json"
        delivered = False
        if delivery.exists():
            value = read_json(delivery)
            delivered = (self._owned(value, descriptor, "zap-process-stop-delivery/1")
                         and value.get("request_id") == request_id and value.get("delivered") is True
                         and value.get("mode", "cooperative") == mode)
        state = self.status(job_id)["state"]
        return self._stop_result(descriptor, request_id, state, idempotent, delivered, False,
                                 state in TERMINAL, mode, terminate_after_seconds)

    @staticmethod
    def _stop_result(
        descriptor: dict[str, Any], request_id: str, state: str, idempotent: bool,
        delivered: bool, already_terminal: bool, actual_exit: bool,
        mode: str, terminate_after_seconds: float | None,
    ) -> dict[str, Any]:
        return {
            "schema": "zap-transport-stop/1",
            "job_id": descriptor["job_id"],
            "nonce": descriptor["nonce"],
            "descriptor_sha256": descriptor["descriptor_sha256"],
            "request_id": request_id,
            "requested": idempotent or not already_terminal,
            "idempotent": idempotent,
            "delivered": delivered,
            "already_terminal": already_terminal,
            "actual_exit": actual_exit,
            "state": state,
            "mode": mode,
            "terminate_after_seconds": terminate_after_seconds,
        }

    def collect(self, job_id: str) -> dict[str, Any]:
        status = self.status(job_id)
        base = {
            "schema": "zap-transport-result/1",
            "job_id": job_id,
            "nonce": status["nonce"],
            "descriptor_sha256": status["descriptor_sha256"],
            "state": status["state"],
            "delivery": status["delivery"],
            "termination": status["termination"],
            "safe_state": status["safe_state"],
        }
        if not status["result_available"]:
            return {**base, "ready": False, "diagnostic": status["diagnostic"]}
        job_dir, descriptor = self._load_descriptor(job_id)
        completed = self._completed(job_dir, descriptor)
        need(completed is not None, "TRANSPORT_CORRUPT", "terminal receipt became unavailable")
        return {
            **base,
            "ready": True,
            "exit_code": completed["exit_code"],
            "stdout": completed["stdout"],
            "stderr": completed["stderr"],
            "diagnostic": completed["diagnostic"],
            "termination_sent": completed.get("termination_sent") is True,
        }

    def reconcile(self, job_id: str) -> dict[str, Any]:
        result = self._derive_status(job_id, recovery=True)
        result["schema"] = "zap-transport-recovery/1"
        result["relaunch_attempted"] = False
        return result
