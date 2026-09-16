"""Persistent retry classification and restart reconciliation tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import time
import unittest

from zaplib.common import sha
from zaplib.runtime import AutomaticCoordinator, runtime_state
from test_runtime_support import RuntimeFixture, tick_until


class QuotaTransport:
    def __init__(self, clock):
        self.clock = clock
        self.submissions = []

    @staticmethod
    def descriptor(job_id):
        return sha(job_id.encode("utf-8"))

    def submit(self, job_id, **kwargs):
        self.submissions.append(job_id)
        return {"schema": "zap-transport-submit/1", "job_id": job_id, "descriptor_sha256": self.descriptor(job_id),
                "nonce": "nonce", "accepted": True, "idempotent": self.submissions.count(job_id) > 1, "state": "starting"}

    def reconcile(self, job_id):
        return {"schema": "zap-transport-recovery/1", "job_id": job_id, "descriptor_sha256": self.descriptor(job_id), "nonce": "nonce",
                "state": "failed", "process": {"active": False, "ownership_verified": True, "pid": None}, "stop": {},
                "delivery": {"requested": False, "delivered": False}, "termination": {"requested": False, "sent": False},
                "safe_state": {"verified": False, "needs_reconcile": False, "basis": None}, "result_available": True,
                "diagnostic": {"classification": "rate_limit", "retry_after_ns": self.clock[0] + 100}, "relaunch_attempted": False}

    def collect(self, job_id):
        return {"schema": "zap-transport-result/1", "job_id": job_id, "descriptor_sha256": self.descriptor(job_id), "nonce": "nonce",
                "state": "failed", "ready": True, "exit_code": 75,
                "stdout": {"path": "private", "bytes": 0, "sha256": "0" * 64},
                "stderr": {"path": "private", "bytes": 0, "sha256": "0" * 64},
                "diagnostic": {"classification": "rate_limit", "retry_after_ns": self.clock[0] + 100},
                "delivery": {"requested": False, "delivered": False}, "termination": {"requested": False, "sent": False},
                "safe_state": {"verified": False, "needs_reconcile": False, "basis": None}}

    def request_stop(self, job_id, request_id, **kwargs):
        return {"job_id": job_id, "request_id": request_id, "state": "failed", "delivered": False, "actual_exit": True}


class FailingSemantic:
    def __init__(self): self.submissions = []
    def submit(self, request_id, request): self.submissions.append(request_id); return {"accepted": True}
    def poll(self, request_id, request): return {"ready": True, "ok": False, "diagnostic": {"classification": "rate_limit", "retry_after_ns": 2_000}}
    def request_stop(self, request_id, stop_id): return {"requested": True}


class RuntimeRecoveryTests(unittest.TestCase):
    def test_semantic_provider_wait_persists_and_releases_before_new_request(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-semantic-wait-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); clock = [1_000]; semantic = FailingSemantic()
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, semantic, fixture.config, clock_ns=lambda: clock[0])
            coordinator.tick(); coordinator.tick()
            runtime = runtime_state(fixture.state()); waits = [row for row in runtime["resource_waits"].values() if row.get("operation_kind") == "semantic"]
            self.assertEqual((len(waits), waits[0]["classification"], waits[0]["next_retry_ns"]), (1, "rate_limit", 2_000))
            request_count = len(runtime["semantic_requests"])
            coordinator.tick()
            self.assertEqual(len(runtime_state(fixture.state())["semantic_requests"]), request_count)
            clock[0] = 2_001; coordinator.tick()
            self.assertEqual(runtime_state(fixture.state())["resource_waits"][waits[0]["wait_id"]]["state"], "released")
            self.assertGreater(len(runtime_state(fixture.state())["semantic_requests"]), request_count)

    def test_quota_wait_and_backoff_survive_restart_without_architecture_failure(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-quota-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); clock = [1_000]; transport = QuotaTransport(clock)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, transport, fixture.semantic, fixture.config, clock_ns=lambda: clock[0])
            tick_until(coordinator, lambda _: bool(runtime_state(fixture.state())["resource_waits"]), limit=20)
            runtime = runtime_state(fixture.state()); wait = next(iter(runtime["resource_waits"].values()))
            self.assertEqual((wait["classification"], wait["next_retry_ns"], wait["state"]), ("rate_limit", 1_100, "waiting"))
            self.assertEqual(fixture.state()["approaches"], {})
            submissions = list(transport.submissions)

            restarted = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, transport, fixture.semantic, fixture.config, clock_ns=lambda: clock[0])
            restarted.tick()
            self.assertEqual(transport.submissions, submissions, "wait must not blindly resubmit before retry boundary")
            attempts_before_release = set(runtime_state(fixture.state())["attempts"])
            clock[0] = 1_101
            restarted.tick()
            self.assertEqual(runtime_state(fixture.state())["resource_waits"][wait["wait_id"]]["state"], "released")
            tick_until(restarted, lambda _: len(runtime_state(fixture.state())["attempts"]) > len(attempts_before_release), limit=10)
            self.assertTrue(attempts_before_release < set(runtime_state(fixture.state())["attempts"]))

    def test_restart_after_external_completion_does_not_duplicate_attempt(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-restart-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            first = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(first, lambda _: any(job.get("transport") for job in runtime_state(fixture.state())["jobs"].values()), limit=12)
            first_attempts = set(runtime_state(fixture.state())["attempts"])
            restarted = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            time.sleep(3.2)
            tick_until(restarted, lambda _: all(job["state"] == "accepted" for job in runtime_state(fixture.state())["jobs"].values()) and
                       len(runtime_state(fixture.state())["jobs"]) == 2, limit=80)
            attempts = runtime_state(fixture.state())["attempts"]
            self.assertTrue(first_attempts <= set(attempts))
            self.assertEqual(len([row for row in attempts.values() if row["work_id"] == "T"]), 1)
            self.assertEqual(len([row for row in attempts.values() if row["work_id"] == "U"]), 1)


if __name__ == "__main__":
    unittest.main()
