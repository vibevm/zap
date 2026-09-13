"""Evidence-gated automatic closure request tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from zaplib.runtime import AutomaticCoordinator, runtime_state
from test_runtime_support import RuntimeFixture, tick_until


class RuntimeClosureTests(unittest.TestCase):
    def test_success_closure_phase_starts_only_after_current_obligations_have_acceptance(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-closure-") as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.2, root_acceptance=False)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: len(runtime_state(fixture.state())["jobs"]) == 2 and
                       all(job["state"] == "accepted" for job in runtime_state(fixture.state())["jobs"].values()), limit=100)
            runtime = runtime_state(fixture.state())
            closure_requests = [row for row in runtime["semantic_requests"].values() if row["request_kind"] == "closure"]
            self.assertEqual(len(closure_requests), 1)
            body = closure_requests[0]["request"]["request"]
            self.assertEqual(body["allowed_classifications"], ["original", "revised"])
            self.assertTrue(body["closure_view"]["obligations"])
            self.assertIsNone(fixture.state()["extensions"]["domain"]["closure"])
            coordinator.tick(); count = len([row for row in runtime_state(fixture.state())["semantic_requests"].values() if row["request_kind"] == "closure"])
            coordinator.tick()
            self.assertEqual(len([row for row in runtime_state(fixture.state())["semantic_requests"].values() if row["request_kind"] == "closure"]), count)

    def test_source_changed_after_closure_request_stales_closure_and_reopens_review(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-closure-stale-") as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.2, root_acceptance=False)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: any(row["request_kind"] == "closure" and row["state"] in {"requested", "submitted"}
                                                  for row in runtime_state(fixture.state())["semantic_requests"].values()), limit=150)
            pending = next(key for key, row in runtime_state(fixture.state())["semantic_requests"].items()
                           if row["request_kind"] == "closure" and row["state"] in {"requested", "submitted"})
            fixture.source_path.write_text("changed after closure request", encoding="utf-8")
            coordinator.tick()
            state = fixture.state(); runtime = runtime_state(state)
            self.assertEqual(runtime["semantic_requests"][pending]["state"], "stale")
            self.assertIsNone(state["extensions"]["domain"]["closure"])
            self.assertTrue(any(row["scope"] == "source_invalidation" for row in runtime["review_requests"].values()))


if __name__ == "__main__":
    unittest.main()
