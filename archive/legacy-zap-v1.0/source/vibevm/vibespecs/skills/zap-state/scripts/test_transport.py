"""Targeted real-process tests for the durable subprocess adapter."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from zaplib.common import Refusal
import zaplib.transport as transport_module
from zaplib.transport import ProcessTransport
from zaplib.transport_io import atomic_json, process_token
import zaplib.worker_host as worker_host


FIXTURE = r'''from __future__ import annotations
import json
import os
from pathlib import Path
import sys
import time

mode = sys.argv[1]
if mode == "echo":
    print(json.dumps({"packet": Path(sys.argv[2]).read_text(encoding="utf-8"), "argument": sys.argv[3]}), flush=True)
elif mode == "nonzero":
    print("provider rate limit; retry later", file=sys.stderr, flush=True)
    raise SystemExit(7)
elif mode == "count":
    with Path(sys.argv[3]).open("ab") as stream:
        stream.write(b"S")
        stream.flush()
        os.fsync(stream.fileno())
    time.sleep(float(sys.argv[4]))
    print(Path(sys.argv[2]).read_text(encoding="utf-8"), flush=True)
elif mode == "stop":
    stop = Path(os.environ["ZAP_STOP_FILE"])
    print("started", flush=True)
    while not stop.exists():
        time.sleep(0.02)
    print("cooperative-stop-observed", flush=True)
    time.sleep(1.5)
elif mode == "ignore-stop":
    print("ignoring-stop", flush=True)
    time.sleep(4)
elif mode == "self-signal":
    Path(os.environ["ZAP_STOP_FILE"]).write_text("self", encoding="utf-8")
    print("self-signal-written", flush=True)
    time.sleep(1)
elif mode == "env":
    print(json.dumps({
        "owner": os.environ.get("ZAP_OWNER_TOKEN"),
        "safe": os.environ.get("SAFE_VALUE"),
        "stop": os.environ.get("ZAP_STOP_FILE"),
        "packet": Path(sys.argv[2]).read_text(encoding="utf-8"),
    }), flush=True)
else:
    raise SystemExit(91)
'''


class ProcessTransportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.workspace = self.base / "workspace"
        self.workspace.mkdir()
        self.fixture = self.workspace / "child fixture.py"
        self.fixture.write_text(FIXTURE, encoding="utf-8")
        self.transport_root = self.base / "transport"
        self.transport = ProcessTransport(self.transport_root, [self.workspace])

    def command(self, mode: str, *extra: str) -> list[str]:
        return [sys.executable, "-B", str(self.fixture), mode, "{packet_file}", *extra]

    def wait_result(self, transport: ProcessTransport, job_id: str, timeout: float = 12.0) -> dict:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = transport.collect(job_id)
            if result["ready"]:
                return result
            time.sleep(0.03)
        self.fail(f"job did not complete: {transport.reconcile(job_id)}")

    @staticmethod
    def output(result: dict, name: str = "stdout") -> bytes:
        return Path(result[name]["path"]).read_bytes()

    def test_successful_packet_echo_and_hostile_argument_is_literal(self) -> None:
        hostile = "prefix{packet_file}suffix; touch SHOULD_NOT_EXIST & $(echo evaluated) | powershell"
        receipt = self.transport.submit(
            "echo-job",
            argv=self.command("echo", hostile),
            cwd=self.workspace,
            packet="packet text; $(still-data)",
        )
        self.assertTrue(receipt["accepted"])
        result = self.wait_result(self.transport, "echo-job")
        self.assertEqual(("succeeded", 0), (result["state"], result["exit_code"]))
        value = json.loads(self.output(result))
        self.assertEqual("packet text; $(still-data)", value["packet"])
        self.assertEqual(hostile, value["argument"])
        self.assertFalse((self.workspace / "SHOULD_NOT_EXIST").exists())
        self.assertNotIn("packet text", json.dumps(result))

    def test_nonzero_exit_and_provider_classification(self) -> None:
        self.transport.submit("failed-job", argv=self.command("nonzero"), cwd=self.workspace, packet="input")
        result = self.wait_result(self.transport, "failed-job")
        self.assertEqual("failed", result["state"])
        self.assertEqual(7, result["exit_code"])
        self.assertEqual("provider_rate_limit", result["diagnostic"]["classification"])
        self.assertNotIn("retry later", json.dumps(result))

    def test_spawn_failure_has_atomic_empty_artifacts(self) -> None:
        missing = self.workspace / "executable-that-does-not-exist"
        self.transport.submit("spawn-failure", argv=[str(missing), "{packet_file}"], cwd=self.workspace, packet="input")
        result = self.wait_result(self.transport, "spawn-failure")
        self.assertEqual("failed", result["state"])
        self.assertIsNone(result["exit_code"])
        self.assertEqual("transport_spawn_error", result["diagnostic"]["classification"])
        self.assertEqual(0, result["stdout"]["bytes"])
        self.assertEqual(0, result["stderr"]["bytes"])
        self.assertFalse((Path(result["stdout"]["path"]).parent / "stdout.partial").exists())

    def test_reopened_adapter_collects_completed_receipt(self) -> None:
        self.transport.submit("reopen-job", argv=self.command("echo", "literal"), cwd=self.workspace, packet="restart-safe")
        original = self.wait_result(self.transport, "reopen-job")
        reopened = ProcessTransport(self.transport_root, [self.workspace])
        collected = reopened.collect("reopen-job")
        self.assertTrue(collected["ready"])
        self.assertEqual(original["descriptor_sha256"], collected["descriptor_sha256"])
        self.assertEqual(original["stdout"]["sha256"], collected["stdout"]["sha256"])

    def test_worker_host_survives_submitting_coordinator_exit(self) -> None:
        counter = self.workspace / "detached-starts.bin"
        submitter = self.workspace / "submitter.py"
        scripts = Path(__file__).resolve().parent
        submitter.write_text(
            "import sys\n"
            f"sys.path.insert(0, {str(scripts)!r})\n"
            "from zaplib.transport import ProcessTransport\n"
            "root, workspace, fixture, python, counter = sys.argv[1:]\n"
            "transport = ProcessTransport(root, [workspace])\n"
            "transport.submit('detached-job', argv=[python, '-B', fixture, 'count', '{packet_file}', counter, '0.8'], cwd=workspace, packet='detached')\n",
            encoding="utf-8",
        )
        submitted = subprocess.run(
            [sys.executable, "-B", str(submitter), str(self.transport_root), str(self.workspace), str(self.fixture), sys.executable, str(counter)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        self.assertEqual(0, submitted.returncode, submitted.stderr.decode("utf-8", "replace"))
        reopened = ProcessTransport(self.transport_root, [self.workspace])
        result = self.wait_result(reopened, "detached-job")
        self.assertEqual("succeeded", result["state"])
        self.assertEqual(b"S", counter.read_bytes())

    def test_concurrent_and_repeated_submit_starts_once(self) -> None:
        counter = self.workspace / "starts.bin"
        argv = self.command("count", str(counter), "0.6")

        def submit() -> dict:
            return self.transport.submit("once-job", argv=argv, cwd=self.workspace, packet="once")

        with ThreadPoolExecutor(max_workers=8) as pool:
            receipts = list(pool.map(lambda _: submit(), range(8)))
        self.wait_result(self.transport, "once-job")
        retry = submit()
        self.assertEqual(b"S", counter.read_bytes())
        self.assertEqual(1, sum(not row["idempotent"] for row in receipts))
        self.assertTrue(retry["idempotent"])

    def test_changed_descriptor_under_same_job_id_refuses(self) -> None:
        self.transport.submit("fixed-job", argv=self.command("echo", "same"), cwd=self.workspace, packet="one")
        with self.assertRaises(Refusal) as caught:
            self.transport.submit("fixed-job", argv=self.command("echo", "same"), cwd=self.workspace, packet="two")
        self.assertEqual("IDEMPOTENCY", caught.exception.code)
        self.wait_result(self.transport, "fixed-job")

    def test_stop_delivery_is_separate_from_actual_exit(self) -> None:
        self.transport.submit("stop-job", argv=self.command("stop"), cwd=self.workspace, packet="work")
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and self.transport.status("stop-job")["state"] != "running":
            time.sleep(0.02)
        requested = self.transport.request_stop("stop-job", "stop-request-1")
        self.assertTrue(requested["requested"])
        observed = None
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            candidate = self.transport.status("stop-job")
            if candidate["stop"]["delivered"] and candidate["process"]["active"] is True:
                observed = candidate
                break
            time.sleep(0.02)
        self.assertIsNotNone(observed, "delivery was not observed before process exit")
        self.assertEqual("stop_requested", observed["state"])
        result = self.wait_result(self.transport, "stop-job")
        self.assertEqual("stopped", result["state"])
        self.assertFalse(result["termination_sent"])
        self.assertTrue(result["delivery"]["delivered"])
        self.assertFalse(result["safe_state"]["verified"])
        self.assertTrue(result["safe_state"]["needs_reconcile"])
        self.assertEqual({"requested": False, "sent": False}, result["termination"])
        repeated = self.transport.request_stop("stop-job", "stop-request-1")
        self.assertTrue(repeated["idempotent"])
        self.assertTrue(repeated["requested"])
        self.assertTrue(repeated["delivered"])
        self.assertTrue(repeated["already_terminal"])
        self.assertTrue(repeated["actual_exit"])

    def test_child_cannot_self_authorize_stop_by_writing_signal(self) -> None:
        self.transport.submit("self-signal-job", argv=self.command("self-signal"), cwd=self.workspace, packet="work")
        result = self.wait_result(self.transport, "self-signal-job")
        status = self.transport.status("self-signal-job")
        self.assertEqual("succeeded", result["state"])
        self.assertFalse(result["termination_sent"])
        self.assertFalse(status["stop"]["requested"])
        self.assertFalse(status["delivery"]["delivered"])

    def test_process_termination_requires_explicit_capability_and_is_interrupted(self) -> None:
        self.transport.submit("no-force", argv=self.command("ignore-stop"), cwd=self.workspace, packet="work")
        with self.assertRaises(Refusal) as disabled:
            self.transport.request_stop("no-force", "force-disabled", mode="terminate", terminate_after_seconds=0.1)
        self.assertEqual("STOP", disabled.exception.code)
        self.transport.request_stop("no-force", "cooperative", mode="cooperative")
        self.wait_result(self.transport, "no-force")

        enabled = ProcessTransport(self.base / "force-transport", [self.workspace], allow_process_termination=True)
        enabled.submit("force-job", argv=self.command("ignore-stop"), cwd=self.workspace, packet="work")
        request = enabled.request_stop("force-job", "force-enabled", mode="terminate", terminate_after_seconds=0.1)
        self.assertEqual("terminate", request["mode"])
        with self.assertRaises(Refusal) as changed:
            enabled.request_stop("force-job", "force-enabled", mode="terminate", terminate_after_seconds=0.2)
        self.assertEqual("IDEMPOTENCY", changed.exception.code)
        result = self.wait_result(enabled, "force-job")
        self.assertEqual("interrupted", result["state"])
        self.assertTrue(result["termination_sent"])
        self.assertEqual({"requested": True, "sent": True}, result["termination"])
        self.assertFalse(result["safe_state"]["verified"])
        self.assertTrue(result["safe_state"]["needs_reconcile"])

    def test_late_stop_after_child_exit_does_not_reclassify_or_deliver(self) -> None:
        with patch.object(ProcessTransport, "_launch_host", lambda *_: None):
            self.transport.submit("late-stop", argv=self.command("echo", "done"), cwd=self.workspace, packet="work")
        job_dir, descriptor = self.transport._load_descriptor("late-stop")
        entered = threading.Event()
        release = threading.Event()
        real_publish = worker_host.publish_file
        calls = 0

        def blocked_publish(source: Path, destination: Path) -> None:
            nonlocal calls
            calls += 1
            if calls == 1:
                entered.set()
                self.assertTrue(release.wait(5))
            real_publish(source, destination)

        outcome = {}
        with patch.object(worker_host, "publish_file", side_effect=blocked_publish):
            thread = threading.Thread(target=lambda: outcome.setdefault("code", worker_host.run(job_dir, descriptor)))
            thread.start()
            self.assertTrue(entered.wait(5), "child did not reach post-exit publication boundary")
            requested = self.transport.request_stop("late-stop", "late-request")
            self.assertFalse(requested["delivered"])
            release.set()
            thread.join(5)
        self.assertEqual(0, outcome.get("code"))
        result = self.transport.collect("late-stop")
        self.assertEqual("succeeded", result["state"])
        self.assertFalse(result["delivery"]["delivered"])
        self.assertTrue(result["safe_state"]["needs_reconcile"], "late request remains visible for reconciliation")

    def test_known_pre_effect_prepared_state_resumes_once(self) -> None:
        real_atomic_json = transport_module.atomic_json

        def fail_prepared(path: Path, value: object) -> None:
            if path.name == "prepared.json":
                raise OSError("injected pre-effect crash")
            real_atomic_json(path, value)

        with patch("zaplib.transport.atomic_json", side_effect=fail_prepared):
            with self.assertRaises(OSError):
                self.transport.submit("prepared-job", argv=self.command("echo", "resumed"), cwd=self.workspace, packet="work")
        self.assertEqual("prepared", self.transport.status("prepared-job")["state"])
        retry = self.transport.submit("prepared-job", argv=self.command("echo", "resumed"), cwd=self.workspace, packet="work")
        self.assertTrue(retry["idempotent"])
        self.assertTrue(retry["resumed_pre_effect"])
        self.assertEqual("succeeded", self.wait_result(self.transport, "prepared-job")["state"])

    def test_completion_cannot_claim_another_jobs_artifacts(self) -> None:
        for job_id, value in (("artifact-a", "alpha"), ("artifact-b", "bravo")):
            self.transport.submit(job_id, argv=self.command("echo", value), cwd=self.workspace, packet="work")
            self.wait_result(self.transport, job_id)
        a_dir = self.transport._job_dir("artifact-a")
        b_result = self.transport.collect("artifact-b")
        completion = json.loads((a_dir / "completed.json").read_text(encoding="utf-8"))
        completion["stdout"] = b_result["stdout"]
        completion["stderr"] = b_result["stderr"]
        atomic_json(a_dir / "completed.json", completion)
        result = self.transport.collect("artifact-a")
        self.assertFalse(result["ready"])
        self.assertEqual("unknown_effect", result["state"])

    def test_started_receipt_without_host_chain_does_not_prove_ownership(self) -> None:
        real_atomic_json = transport_module.atomic_json

        def fail_prepared(path: Path, value: object) -> None:
            if path.name == "prepared.json":
                raise OSError("injected pre-effect crash")
            real_atomic_json(path, value)

        with patch("zaplib.transport.atomic_json", side_effect=fail_prepared):
            with self.assertRaises(OSError):
                self.transport.submit("forged-start", argv=self.command("echo", "never"), cwd=self.workspace, packet="work")
        job_dir, descriptor = self.transport._load_descriptor("forged-start")
        atomic_json(job_dir / "started.json", self.transport._base_record(
            descriptor, "zap-process-started/1", pid=os.getpid(), process_token=process_token(os.getpid()),
            started_at_ns=time.time_ns(),
        ))
        status = self.transport.status("forged-start")
        self.assertEqual("unknown_effect", status["state"])
        self.assertFalse(status["process"]["ownership_verified"])

    def test_missing_host_receipt_reconciles_without_relaunch(self) -> None:
        class Phantom:
            pid = 2_000_000_000

        argv = self.command("echo", "never")
        with patch("zaplib.transport.subprocess.Popen", return_value=Phantom()) as spawn:
            first = self.transport.submit("ambiguous-job", argv=argv, cwd=self.workspace, packet="ambiguous")
        self.assertEqual("starting", first["state"])
        self.assertEqual(1, spawn.call_count)
        time.sleep(0.55)
        recovery = self.transport.reconcile("ambiguous-job")
        self.assertEqual("unknown_effect", recovery["state"])
        self.assertFalse(recovery["relaunch_attempted"])
        with patch("zaplib.transport.subprocess.Popen", side_effect=AssertionError("must not relaunch")) as retry_spawn:
            retry = self.transport.submit("ambiguous-job", argv=argv, cwd=self.workspace, packet="ambiguous")
        self.assertTrue(retry["idempotent"])
        self.assertEqual(0, retry_spawn.call_count)

    def test_workspace_and_symlink_escape_refuse(self) -> None:
        outside = self.base / "outside"
        outside.mkdir()
        with self.assertRaises(Refusal) as caught:
            self.transport.submit("outside-job", argv=self.command("echo", "x"), cwd=outside, packet="x")
        self.assertEqual("PATH", caught.exception.code)
        link = self.workspace / "escape-link"
        junction = False
        try:
            link.symlink_to(outside, target_is_directory=True)
        except OSError as exc:
            if os.name != "nt":
                self.skipTest(f"directory symlinks unavailable: {exc}")
            created = subprocess.run(
                ["cmd.exe", "/d", "/c", "mklink", "/J", str(link), str(outside)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            if created.returncode != 0:
                self.skipTest(f"directory links unavailable: {created.stderr!r}")
            junction = True
        try:
            with self.assertRaises(Refusal) as linked:
                self.transport.submit("link-job", argv=self.command("echo", "x"), cwd=link, packet="x")
            self.assertEqual("PATH", linked.exception.code)
        finally:
            if junction:
                os.rmdir(link)

    def test_credentials_are_not_in_child_or_public_records(self) -> None:
        secret = "owner-secret-value-43872"
        allowed_value = "allowed-but-private-9981"
        old = os.environ.get("ZAP_OWNER_TOKEN")
        os.environ["ZAP_OWNER_TOKEN"] = secret
        self.addCleanup(lambda: os.environ.pop("ZAP_OWNER_TOKEN", None) if old is None else os.environ.__setitem__("ZAP_OWNER_TOKEN", old))
        transport = ProcessTransport(
            self.base / "credential-transport",
            [self.workspace],
            inherited_environment=(),
            environment_allowlist=("SAFE_VALUE",),
        )
        receipt = transport.submit(
            "environment-job",
            argv=self.command("env"),
            cwd=self.workspace,
            packet="private packet",
            environment={"SAFE_VALUE": allowed_value},
        )
        result = self.wait_result(transport, "environment-job")
        child = json.loads(self.output(result))
        self.assertIsNone(child["owner"])
        self.assertEqual(allowed_value, child["safe"])
        public = json.dumps([receipt, transport.status("environment-job"), result])
        self.assertNotIn(secret, public)
        self.assertNotIn(allowed_value, public)
        with self.assertRaises(Refusal) as caught:
            ProcessTransport(self.base / "bad-env", [self.workspace], environment_allowlist=("ZAP_OWNER_TOKEN",))
        self.assertEqual("CREDENTIAL", caught.exception.code)
        for index, name in enumerate(("GITHUB_PAT", "SESSION_COOKIE", "BEARER", "PRIVATE_KEY")):
            with self.subTest(name=name), self.assertRaises(Refusal) as shaped:
                ProcessTransport(self.base / f"bad-shaped-{index}", [self.workspace], environment_allowlist=(name,))
            self.assertEqual("CREDENTIAL", shaped.exception.code)

    @unittest.skipUnless(os.name == "nt", "Windows environment names are case-insensitive")
    def test_windows_environment_case_is_canonical_before_hash_and_spawn(self) -> None:
        transport = ProcessTransport(
            self.base / "case-transport", [self.workspace],
            inherited_environment=("PATH",), environment_allowlist=("PATH",),
        )
        argv = self.command("echo", "case")
        transport.submit("case-job", argv=argv, cwd=self.workspace, packet="work", environment={"Path": "X"})
        self.wait_result(transport, "case-job")
        retry = transport.submit("case-job", argv=argv, cwd=self.workspace, packet="work", environment={"PATH": "X"})
        descriptor = json.loads((transport._job_dir("case-job") / "descriptor.json").read_text(encoding="utf-8"))
        self.assertTrue(retry["idempotent"])
        self.assertEqual(["PATH"], list(descriptor["logical"]["environment"]))
        with self.assertRaises(Refusal):
            transport.submit("case-duplicate", argv=argv, cwd=self.workspace, packet="work",
                             environment={"Path": "X", "PATH": "Y"})


if __name__ == "__main__":
    unittest.main()
