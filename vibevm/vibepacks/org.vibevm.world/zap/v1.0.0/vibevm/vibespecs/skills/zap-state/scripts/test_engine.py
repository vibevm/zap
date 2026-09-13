from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from test_service import make_store
from zaplib.common import Refusal
from zaplib.cli_runtime import load_runtime_profile
from zaplib.engine import (
    ENGINE_ACTION_KINDS, ENGINE_DATA_KINDS, ENGINE_EVENT_DESCRIPTORS,
    ENGINE_HANDLERS, ENGINE_OBSERVATION_KINDS, build_engine,
)
from zaplib.runtime import runtime_state


class EngineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        parent = Path(self.temp.name) / "fixture"
        parent.mkdir()
        self.store = make_store(parent)
        self.engine = build_engine(self.store)

    def tearDown(self):
        self.temp.cleanup()

    def test_every_handler_has_one_schema_and_one_fail_closed_route(self):
        self.assertEqual(set(ENGINE_HANDLERS), set(ENGINE_EVENT_DESCRIPTORS))
        extension = set(ENGINE_DATA_KINDS) | set(ENGINE_ACTION_KINDS) | set(ENGINE_OBSERVATION_KINDS)
        self.assertFalse(set(ENGINE_DATA_KINDS) & set(ENGINE_ACTION_KINDS))
        self.assertFalse(set(ENGINE_DATA_KINDS) & set(ENGINE_OBSERVATION_KINDS))
        self.assertFalse(set(ENGINE_ACTION_KINDS) & set(ENGINE_OBSERVATION_KINDS))
        self.assertIn("domain.intent-proposed", extension)
        self.assertEqual(ENGINE_ACTION_KINDS["runtime.job-claimed"], "work.dispatch")
        self.assertIn("runtime.job-result-observed", ENGINE_OBSERVATION_KINDS)
        self.assertIn("knowledge.source-recorded", ENGINE_OBSERVATION_KINDS)
        self.assertNotIn("knowledge.source-recorded", ENGINE_ACTION_KINDS)

    def test_capabilities_are_generated_from_actual_registries_and_json_serializable(self):
        capabilities = self.engine.capabilities()
        self.assertEqual(capabilities["handlers"], sorted(ENGINE_HANDLERS))
        self.assertEqual(capabilities["events"]["runtime.job-result-observed"]["route"], "trusted_observation")
        self.assertEqual(capabilities["events"]["knowledge.source-recorded"]["route"], "effect_adapter")
        self.assertEqual(capabilities["events"]["knowledge.source-recorded"]["route"], "effect_adapter")
        self.assertEqual(capabilities["events"]["domain.work-dispatched"]["action"], "work.dispatch")
        self.assertEqual(capabilities["events"]["control.charter-activated"]["route"], "owner_control")
        self.assertEqual(capabilities["events"]["plan.refined"]["route"], "draft_data_or_action")
        self.assertIn("zap-action/1", capabilities["value_schemas"])
        sparse = capabilities["operations"]["domain"]["domain.materialize-review-transition"]
        self.assertEqual(sparse["input_schema"]["properties"]["schema"]["const"], "zap-domain/sparse-review-transition/1")
        profile = capabilities["operations"]["runtime_profile_schema"]
        self.assertEqual(profile["properties"]["schema"]["const"], "zap-runtime-profile/1")
        example_path = Path(__file__).resolve().parents[3] / "examples" / "zap" / "runtime-profile.json"
        example = load_runtime_profile(example_path)
        self.assertEqual(set(example), set(profile["required"]) | {"artifact_capture"})
        self.assertEqual(set(example["worker"]), set(profile["$defs"]["worker"]["required"]) | {"kind", "launcher"})
        json.dumps(capabilities)

    def test_import_never_activates_or_starts_runtime(self):
        state, events, pending = self.engine.load()
        self.assertEqual(state["execution_mode"], "draft")
        self.assertIsNone(state["owner_contract"])
        self.assertEqual(runtime_state(state)["jobs"], {})
        self.assertEqual(len(events), 1)
        self.assertIsNone(pending)

    def test_agent_route_cannot_submit_action_or_trusted_capture(self):
        state = self.engine.load()[0]
        action = {
            "event_id": "forged-action", "base_revision": state["revision"],
            "kind": "domain.outcome-adopted", "reason": {"summary": "forged"},
            "payload": {},
        }
        with self.assertRaisesRegex(Refusal, "privileged product"):
            self.engine.service.submit_agent(action)
        observation = {
            "event_id": "forged-source", "base_revision": state["revision"],
            "kind": "knowledge.source-recorded", "reason": {"summary": "forged"},
            "payload": {},
        }
        with self.assertRaisesRegex(Refusal, "trusted transport"):
            self.engine.service.submit_agent(observation)


if __name__ == "__main__":
    unittest.main()
