"""Whole-campaign, scoped, and natural-exit stop reconciliation tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import time
import unittest

from zaplib.control import active_policy
from zaplib.runtime import ACTIVE_JOB_STATES, AutomaticCoordinator, runtime_state
from test_runtime_support import RuntimeFixture, tick_until


def pause(state, pause_id):
    return next(row for row in active_policy(state)["pauses"] if row["pause_id"] == pause_id)


class RuntimeStopTests(unittest.TestCase):
    def test_owner_global_stop_separates_delivery_process_exit_and_verified_safe_boundary(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-stop-") as temporary:
            fixture = RuntimeFixture(Path(temporary), conflict=True, safe_verifier=True, worker_seconds=8.0)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: bool(runtime_state(fixture.state())["jobs"]) and
                       any(job["state"] == "running" for job in runtime_state(fixture.state())["jobs"].values()), limit=20)
            attempts_before = len(runtime_state(fixture.state())["attempts"])
            fixture.owner_stop()
            tick_until(coordinator, lambda _: pause(fixture.state(), "OWNER-STOP")["delivery"]["state"] == "complete" and
                       pause(fixture.state(), "OWNER-STOP")["actual_safe_state"]["state"] == "reached", limit=40)
            stopped_job = next(iter(runtime_state(fixture.state())["jobs"].values()))
            delivery = next(iter(pause(fixture.state(), "OWNER-STOP")["delivery"]["acknowledgements"].values()))
            self.assertIn(delivery["state"], {"delivered", "already_terminal"})
            if delivery["state"] == "already_terminal":
                self.assertFalse(delivery["signal_delivered"])
                self.assertIn(stopped_job.get("result", {}).get("state"), {"succeeded", "failed"})
            else:
                self.assertIn(stopped_job.get("result", {}).get("state"), {"stopped", "interrupted"})
                self.assertTrue(Path(stopped_job["reservation"]["write_subjects"][0] + ".safe").is_file())
            fixture.owner_resume()
            coordinator.tick()
            self.assertGreaterEqual(len(runtime_state(fixture.state())["attempts"]), attempts_before)
            self.assertIn(stopped_job["attempt_id"], runtime_state(fixture.state())["attempts"])
            resumed_state = runtime_state(fixture.state())["jobs"][stopped_job["job_id"]]["state"]
            if delivery["state"] == "already_terminal":
                self.assertIn(resumed_state, {"candidate", "verification", "review_pending"})
            else:
                self.assertEqual(resumed_state, "retry_released")

    def test_natural_exit_before_delivery_uses_already_terminal_without_claiming_signal(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-natural-stop-") as temporary:
            fixture = RuntimeFixture(Path(temporary), conflict=True, safe_verifier=True)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: bool(runtime_state(fixture.state())["jobs"]) and
                       any(job["state"] in {"starting", "running"} for job in runtime_state(fixture.state())["jobs"].values()), limit=15)
            fixture.owner_stop("NATURAL")
            time.sleep(3.3)
            tick_until(coordinator, lambda _: pause(fixture.state(), "NATURAL")["delivery"]["state"] == "complete" and
                       pause(fixture.state(), "NATURAL")["actual_safe_state"]["state"] == "reached", limit=15)
            row = pause(fixture.state(), "NATURAL")
            acknowledgement = next(iter(row["delivery"]["acknowledgements"].values()))
            self.assertEqual(acknowledgement["state"], "already_terminal")
            self.assertFalse(acknowledgement["signal_delivered"])
            job = next(iter(runtime_state(fixture.state())["jobs"].values()))
            self.assertIn(job["result"]["state"], {"succeeded", "failed"})
            fixture.owner_resume("NATURAL")

    def test_scoped_pause_stops_only_matching_run(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-scoped-stop-") as temporary:
            fixture = RuntimeFixture(Path(temporary), scoped_stop=True, safe_verifier=True)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda row: len(row["active_jobs"]) == 2, limit=20)
            jobs = runtime_state(fixture.state())["jobs"]; target_id = next(job_id for job_id, job in jobs.items() if job["work_id"] == "T")
            other_id = next(job_id for job_id, job in jobs.items() if job["work_id"] == "U")
            fixture.scoped_stop(target_id)
            tick_until(coordinator, lambda _: pause(fixture.state(), f"pause:scoped-assessment:{target_id}")["actual_safe_state"]["state"] == "reached", limit=35)
            jobs = runtime_state(fixture.state())["jobs"]
            self.assertIsNotNone(jobs[target_id]["stop"])
            self.assertIsNone(jobs[other_id]["stop"])
            tick_until(coordinator, lambda _: all(job["state"] not in ACTIVE_JOB_STATES for job in runtime_state(fixture.state())["jobs"].values()) and
                       all(row["state"] == "observed" for row in runtime_state(fixture.state())["verification_jobs"].values()), limit=25)


if __name__ == "__main__":
    unittest.main()
