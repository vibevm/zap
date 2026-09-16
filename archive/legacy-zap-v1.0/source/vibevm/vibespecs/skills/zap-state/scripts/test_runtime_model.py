"""Public runtime registry and coordinator wire tests."""
from __future__ import annotations

import copy
import os
import tempfile
from pathlib import Path
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.coordinator_adapter import COORDINATOR_REQUEST_SCHEMA, COORDINATOR_RESPONSE_SCHEMA, codex_sol_xhigh_adapter, codex_sol_xhigh_profile, validate_response
from zaplib.runtime import RUNTIME_ACTION_KINDS, RUNTIME_DATA_KINDS, RUNTIME_EVENT_ROUTES, RUNTIME_EVENT_SCHEMAS, RUNTIME_HANDLERS, RUNTIME_OBSERVATION_KINDS


class RuntimeModelTests(unittest.TestCase):
    def test_every_runtime_handler_has_one_machine_route(self):
        self.assertEqual(set(RUNTIME_HANDLERS), set(RUNTIME_EVENT_SCHEMAS))
        self.assertEqual(set(RUNTIME_HANDLERS), set(RUNTIME_EVENT_ROUTES))
        self.assertEqual(set(RUNTIME_DATA_KINDS) | set(RUNTIME_ACTION_KINDS) | set(RUNTIME_OBSERVATION_KINDS), set(RUNTIME_HANDLERS))
        self.assertFalse(set(RUNTIME_DATA_KINDS) & set(RUNTIME_ACTION_KINDS))
        self.assertFalse(set(RUNTIME_OBSERVATION_KINDS) & (set(RUNTIME_DATA_KINDS) | set(RUNTIME_ACTION_KINDS)))

    def test_coordinator_response_is_exact_and_cannot_activate_control(self):
        request = {"schema": COORDINATOR_REQUEST_SCHEMA, "request_id": "semantic:selection:1", "request_kind": "selection",
                   "state_revision": 7, "base_sha256": "a" * 64, "request": {"frontier": [{"work_id": "T"}]}}
        response = {"schema": COORDINATOR_RESPONSE_SCHEMA, "request_id": request["request_id"], "request_kind": "selection",
                    "state_revision": 7, "request_sha256": sha(packed(request)), "disposition": "select", "rationale": "Highest value ready work",
                    "selection": {"work_id": "T"}, "commands": []}
        self.assertEqual(validate_response(request, response), response)
        forged = copy.deepcopy(response)
        forged.update({"request_kind": "review", "disposition": "commands", "selection": None,
                       "commands": [{"kind": "control.charter-activated", "payload": {}, "reason": "forge"}]})
        request["request_kind"] = "review"; forged["request_sha256"] = sha(packed(request))
        with self.assertRaises(Refusal):
            validate_response(request, forged)

        closure_request = {"schema": COORDINATOR_REQUEST_SCHEMA, "request_id": "semantic:closure:1", "request_kind": "closure",
                           "state_revision": 8, "base_sha256": "a" * 64, "request": {"closure_view": {}}}
        partial = {"schema": COORDINATOR_RESPONSE_SCHEMA, "request_id": closure_request["request_id"], "request_kind": "closure",
                   "state_revision": 8, "request_sha256": sha(packed(closure_request)), "disposition": "commands", "rationale": "abandon",
                   "selection": None, "commands": [{"kind": "domain.campaign-closed", "reason": "partial",
                   "payload": {"classification": "partial"}}]}
        with self.assertRaises(Refusal):
            validate_response(closure_request, partial)

    def test_codex_profile_is_ready_for_process_transport_without_secrets(self):
        with tempfile.TemporaryDirectory(prefix="zap-profile-") as temporary:
            launcher = Path(temporary) / "codexrunner"
            launcher.write_text("test launcher", encoding="utf-8")
            profile = codex_sol_xhigh_profile(cwd=temporary, launcher=launcher)
            self.assertEqual(profile.argv.count("{packet_file}"), 1)
            self.assertIn("provider_bridge.py", " ".join(profile.argv))
            home = "USERPROFILE" if os.name == "nt" else "HOME"
            self.assertIn(home, profile.inherited_environment)
            self.assertNotIn("credential", " ".join(profile.argv).casefold())
            adapter = codex_sol_xhigh_adapter(Path(temporary) / "transport", [temporary], cwd=temporary, launcher=launcher)
            self.assertIn(home, adapter.transport.inherited_environment)


if __name__ == "__main__":
    unittest.main()
