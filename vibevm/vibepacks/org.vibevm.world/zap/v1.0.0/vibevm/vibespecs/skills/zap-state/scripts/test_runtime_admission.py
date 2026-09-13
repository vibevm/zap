"""Fail-closed runtime admission tests."""
from __future__ import annotations

from dataclasses import replace
from pathlib import Path
import tempfile
import unittest

from zaplib.runtime import AutomaticCoordinator, runtime_state
from zaplib.domain import domain_state
from test_runtime_support import RuntimeFixture, tick_until


class RuntimeAdmissionTests(unittest.TestCase):
    def test_successful_process_with_blocked_worker_report_never_becomes_candidate(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-worker-blocked-") as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.1)
            fixture.worker_script.write_text('import json; print(json.dumps({"schema":"zap-worker-candidate/1","status":"blocked"}))\n', encoding="utf-8")
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: any(row["scope"] == "worker_failure" for row in runtime_state(fixture.state())["review_requests"].values()))
            runtime = runtime_state(fixture.state())
            self.assertTrue(any(job["state"] == "result_ready" and job["result"]["diagnostic"]["classification"] == "worker_blocked"
                                for job in runtime["jobs"].values()))
            self.assertFalse(any(job["state"] in {"candidate", "verification", "review_pending", "accepted"} for job in runtime["jobs"].values()))
            self.assertEqual(runtime["verification_jobs"], {})
            self.assertEqual(domain_state(fixture.state())["acceptances"], {})

    def test_draft_campaign_never_calls_worker_or_semantic_transport(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-draft-") as temporary:
            fixture = RuntimeFixture(Path(temporary), activated=False)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            result = coordinator.tick()
            self.assertFalse(result["campaign_complete"])
            self.assertEqual(runtime_state(fixture.state())["jobs"], {})
            self.assertEqual(fixture.semantic.requests, {})
            self.assertEqual(list((Path(temporary) / "transport" / "jobs").iterdir()), [])

    def test_unknown_stop_assessment_records_rejection_and_never_spawns(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-unknown-") as temporary:
            fixture = RuntimeFixture(Path(temporary), unknown_dispatch=True)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            history = [coordinator.tick() for _ in range(5)]
            self.assertEqual(runtime_state(fixture.state())["jobs"], {})
            self.assertEqual(list((Path(temporary) / "transport" / "jobs").iterdir()), [])
            self.assertTrue(any(any(action.get("code") == "NEEDS_EVIDENCE" for action in row["actions"]) for row in history))
            self.assertFalse(history[-1]["campaign_complete"])

    def test_successful_worker_without_configured_independent_proof_is_never_accepted(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-no-proof-") as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.2)
            config = replace(fixture.config, verifications={})
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, config)
            history = [coordinator.tick() for _ in range(14)]
            runtime = runtime_state(fixture.state())
            self.assertTrue(runtime["jobs"])
            self.assertFalse(any(job["state"] == "accepted" for job in runtime["jobs"].values()))
            self.assertEqual(domain_state(fixture.state())["acceptances"], {})
            self.assertTrue(any(any(action.get("kind") == "semantic_rejected" for action in row["actions"]) for row in history))


if __name__ == "__main__":
    unittest.main()
