"""Controlled semantic no-action, repair, and evidence-reopening tests."""
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.runtime import AutomaticCoordinator, runtime_state
from test_runtime_support import RuntimeFixture


class SemanticScript:
    def __init__(self, disposition="no_change", invalid_first=False):
        self.requests = {}; self.disposition = disposition; self.invalid_first = invalid_first

    def submit(self, request_id, request):
        self.requests[request_id] = request; return {"accepted": True}

    def poll(self, request_id, request):
        if self.invalid_first and request_id == next(iter(self.requests)):
            return {"ready": True, "ok": False, "state": "invalid_response", "diagnostic": {
                "classification": "model_response_invalid", "validator_feedback": {"code": "COORDINATOR", "message": "shape"},
                "provider_response_sha256": "a" * 64, "profile_sha256": "profile"}}
        response = {"schema": "zap-coordinator-response/1", "request_id": request_id, "request_kind": request["request_kind"],
                    "state_revision": request["state_revision"], "request_sha256": sha(packed(request)), "disposition": self.disposition,
                    "rationale": "bounded test response", "selection": None, "commands": []}
        return {"ready": True, "ok": True, "response": response}

    def request_stop(self, request_id, stop_id): return {"requested": True}
    def profile_sha256(self): return "profile"


class RuntimeLivenessTests(unittest.TestCase):
    def test_identical_selection_no_change_is_not_polled_repeatedly(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-no-change-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); semantic = SemanticScript()
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, semantic, fixture.config)
            for _ in range(4): coordinator.tick()
            self.assertEqual(len(semantic.requests), 1)

    def test_relevant_new_fog_reopens_needs_evidence_but_unrelated_fog_does_not(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-review-reopen-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); semantic = SemanticScript("needs_evidence")
            fixture._observe("runtime.review-requested", {"schema": "zap-runtime/review-requested/1", "review_request_id": "review:fog",
                "trigger_ids": ["fog"], "work_ids": ["T"], "scope": "fog", "summary": "Need relevant fog evidence"}, "Request fog review")
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, semantic, fixture.config)
            coordinator.tick(); coordinator.tick()
            self.assertEqual(runtime_state(fixture.state())["review_requests"]["review:fog"]["state"], "awaiting_evidence")
            fixture._data("knowledge.region-recorded", {"id": "OTHER", "question": "Unrelated?", "node_refs": ["R"]}, "Unrelated fog")
            coordinator.tick(); self.assertEqual(len(semantic.requests), 1)
            fixture._data("knowledge.region-recorded", {"id": "K", "question": "Relevant?", "node_refs": ["T"]}, "Relevant fog")
            coordinator.tick(); self.assertEqual(len(semantic.requests), 2)

    def test_invalid_model_output_gets_one_feedback_bound_repair_attempt(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-semantic-repair-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); semantic = SemanticScript(invalid_first=True)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, semantic, fixture.config)
            for _ in range(8): coordinator.tick()
            self.assertEqual(len(semantic.requests), 2)
            repaired = list(semantic.requests.values())[1]
            self.assertEqual(repaired["request"]["repair_feedback"]["repair_attempt"], 1)

    def test_actionable_invalidation_cannot_be_resolved_by_no_change_without_review(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-actionable-review-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); coordinator = AutomaticCoordinator(
                fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            row = {"request": {"request": {"review_requests": [{"scope": "source_invalidation"}]}}}
            with self.assertRaisesRegex(Refusal, "applied keep-route"):
                coordinator._apply_semantic(row, {"disposition": "no_change"}, [])


if __name__ == "__main__":
    unittest.main()
