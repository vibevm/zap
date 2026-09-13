"""Selective source and goal-change invalidation tests."""
from __future__ import annotations

import copy
from pathlib import Path
import tempfile
import unittest

from zaplib.domain import domain_state
from zaplib.runtime import AutomaticCoordinator, runtime_state
from zaplib.sources import capture_vibevm_facts
from test_runtime_support import RuntimeFixture


class RuntimeInvalidationTests(unittest.TestCase):
    def test_changed_native_xml_facts_emit_bound_observation_and_reassessment(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-native-facts-") as temporary:
            fixture = RuntimeFixture(Path(temporary)); root = Path(temporary); xml = root / "facts.xml"
            xml.write_text('<spec xmlns="https://vibevm.org/spec/1"><p><FACT fact="true" status="spec/plan">One</FACT></p></spec>', encoding="utf-8")
            capture = capture_vibevm_facts(xml, root, source_id="N")
            fixture._observe("knowledge.native-facts-recorded", {"capture": capture}, "Capture native facts")
            source_hash = capture["source"]["content_sha256"]; captures = [{"source_id": "N", "sha256": source_hash}]
            scope = {"kind": "subjects", "subjects": [{"kind": "node", "id": "T"}]}
            fixture._apply("knowledge.applicability-assessed", {"source_id": "N", "status": "applicable", "scope": scope,
                           "evidence_refs": ["SOURCE-E"], "basis": "Native spec checked for T"}, "evidence.adjudicate", "Adjudicate native source", captures)
            fixture._apply("knowledge.dependency-recorded", {"id": "native-to-T", "prerequisite": {"kind": "source", "id": "N"},
                           "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "evidence.adjudicate", "Bind native source to T", captures)
            fixture._apply("knowledge.closure-assessed", {"subject": {"kind": "source", "id": "N"}, "status": "complete",
                           "boundary": [{"kind": "task", "id": "T"}], "missing": [], "evidence_refs": ["SOURCE-E"],
                           "basis": "Native source boundary"}, "evidence.adjudicate", "Close native source boundary", captures)
            domain = domain_state(fixture.state()); history = domain["task_contracts"]["T"]
            contract = copy.deepcopy(next(row for row in history["versions"] if row["version"] == 1)["contract"])
            contract["source_handles"] = ["N"]; contract["read_subjects"] = [str(xml)]
            fixture._apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                           "expected_version": 1, "contract": contract}, "task.update", "Bind T to native facts", captures)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            coordinator.tick()
            xml.write_text('<spec xmlns="https://vibevm.org/spec/1"><p><FACT fact="true" status="spec/plan">Two</FACT></p></spec>', encoding="utf-8")
            coordinator.tick()
            runtime = runtime_state(fixture.state())
            native = [row for row in runtime["observations"].values() if row.get("kind") == "native_facts_refresh"]
            self.assertEqual(len(native), 1)
            self.assertEqual(native[0]["capture"]["facts"][0]["text"], "Two")
            self.assertTrue(any(row["scope"] == "native_facts_changed" and row["work_ids"] == ["T"] for row in runtime["review_requests"].values()))

    def test_unrelated_append_rebinds_exact_selection_basis_with_an_explicit_event(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-rebind-") as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.2)
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            coordinator.tick()
            request_id = next(iter(runtime_state(fixture.state())["semantic_requests"]))
            fixture._data("knowledge.region-recorded", {"id": "UNRELATED", "question": "Unrelated fog?", "node_refs": ["R"]}, "Record unrelated fog")
            result = coordinator.tick()
            runtime = runtime_state(fixture.state())
            self.assertTrue(runtime["semantic_requests"][request_id]["rebindings"])
            self.assertTrue(runtime["jobs"])
            self.assertTrue(any(action["kind"] == "semantic_rebound" for action in result["actions"]))

    def test_source_drift_requests_review_for_known_affected_work_only(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-source-drift-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            state = fixture.state(); source = state["extensions"]["knowledge"]["sources"]["S"]
            captures = [{"source_id": "S", "sha256": source["content_sha256"]}]
            fixture._apply("knowledge.dependency-recorded", {"id": "source-to-T", "prerequisite": {"kind": "source", "id": "S"},
                           "dependent": {"kind": "task", "id": "T"}, "relation": "affects"}, "evidence.adjudicate", "Bind source to T", captures)
            fixture._apply("knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S"}, "status": "complete",
                           "boundary": [{"kind": "task", "id": "T"}], "missing": [], "evidence_refs": ["SOURCE-E"],
                           "basis": "Rebound after dependency"}, "evidence.adjudicate", "Rebind source closure", captures)
            fixture.source_path.write_text("changed", encoding="utf-8")
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            coordinator.tick()
            reviews = runtime_state(fixture.state())["review_requests"].values()
            source_reviews = [row for row in reviews if row["scope"] == "source_invalidation"]
            self.assertEqual(len(source_reviews), 1)
            self.assertEqual(source_reviews[0]["work_ids"], ["T"])
            self.assertEqual(runtime_state(fixture.state())["jobs"], {})

    def test_changed_goal_stales_prior_semantic_selection_without_touching_other_contract(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-goal-drift-") as temporary:
            fixture = RuntimeFixture(Path(temporary))
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport, fixture.semantic, fixture.config)
            coordinator.tick()
            runtime = runtime_state(fixture.state())
            old_request = next(iter(runtime["semantic_requests"]))
            domain = domain_state(fixture.state()); history = domain["task_contracts"]["T"]
            contract = copy.deepcopy(next(row for row in history["versions"] if row["version"] == history["active_version"])["contract"])
            contract["goal"] = "Changed goal requiring a fresh semantic choice"
            source = fixture.state()["extensions"]["knowledge"]["sources"]["S"]
            fixture._apply("domain.task-contract-replaced", {"schema": "zap-domain/task-contract-replaced/1", "work_id": "T",
                           "expected_version": 1, "contract": contract}, "task.update", "Change only T goal",
                           [{"source_id": "S", "sha256": source["content_sha256"]}])
            coordinator.tick()
            runtime = runtime_state(fixture.state())
            self.assertEqual(runtime["semantic_requests"][old_request]["state"], "stale")
            self.assertEqual(runtime["jobs"], {})
            domain = domain_state(fixture.state())
            self.assertEqual(domain["task_contracts"]["T"]["active_version"], 2)
            self.assertEqual(domain["task_contracts"]["U"]["active_version"], 1)
            self.assertEqual(list((Path(temporary) / "transport" / "jobs").iterdir()), [])


if __name__ == "__main__":
    unittest.main()
