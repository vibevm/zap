"""Real subprocess automatic-coordinator integration tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from zaplib.domain import domain_state
from zaplib.runtime import AutomaticCoordinator, runtime_state
from test_runtime_support import RuntimeFixture, tick_until


class RuntimeLoopTests(unittest.TestCase):
    def test_activated_campaign_claims_runs_verifies_and_centrally_accepts_parallel_work(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-loop-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            result, history = tick_until(coordinator, lambda row: len(runtime_state(fixture.state())["jobs"]) == 2 and
                                          all(job["state"] == "accepted" for job in runtime_state(fixture.state())["jobs"].values()))
            self.assertTrue(result["ok"])
            self.assertFalse(result["campaign_complete"])
            self.assertTrue(any(len(row["active_jobs"]) == 2 for row in history), "nonconflicting workers should overlap")
            state = fixture.state(); runtime = runtime_state(state); domain = domain_state(state)
            self.assertEqual({job["work_id"] for job in runtime["jobs"].values() if job["state"] == "accepted"}, {"T", "U"})
            self.assertEqual({row["work_id"] for row in domain["acceptances"].values()}, {"T", "U"})
            self.assertEqual(len(state["evidence"]), 3)
            self.assertTrue(all(row["disposition"] == "accepted" for row in domain["evidence_adjudications"].values()))
            self.assertEqual(domain["closure"], None, "empty frontier is not campaign closure")

    def test_conflicting_write_subjects_never_overlap_and_eventually_serialize(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-conflict-") as temporary:
            fixture = RuntimeFixture(Path(temporary), conflict=True)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            _result, history = tick_until(coordinator, lambda row: len(runtime_state(fixture.state())["jobs"]) == 2 and
                                           all(job["state"] == "accepted" for job in runtime_state(fixture.state())["jobs"].values()), limit=80)
            self.assertTrue(all(len(row["active_jobs"]) <= 1 for row in history))
            self.assertTrue(any(any(action.get("code") == "RUNTIME_CONFLICT" for action in row["actions"]) for row in history))


if __name__ == "__main__":
    unittest.main()
