"""Real JSON subprocess coordinator and Codex bridge-profile tests."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock

from zaplib.common import Refusal, packed, parse, sha
from zaplib.coordinator_adapter import COORDINATOR_REQUEST_SCHEMA, JsonProcessCoordinatorAdapter, command_contracts_for, provider_response_json_schema
from zaplib.provider_bridge import main as bridge_main
from zaplib.transport import ProcessTransport

BRIDGE_FIXTURE = '''import json, pathlib, sys, hashlib
request = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
raw = json.dumps(request, ensure_ascii=True, separators=(",", ":"), sort_keys=True).encode("utf-8")
response = {"schema":"zap-coordinator-response/1","request_id":request["request_id"],"request_kind":request["request_kind"],
"state_revision":request["state_revision"],"request_sha256":hashlib.sha256(raw).hexdigest(),"disposition":"select",
"rationale":"real subprocess selection","selection":{"work_id":"T"},"commands":[]}
print(json.dumps(response, ensure_ascii=True, separators=(",", ":"), sort_keys=True))
'''


def request():
    return {"schema": COORDINATOR_REQUEST_SCHEMA, "request_id": "semantic:real:1", "request_kind": "selection",
            "state_revision": 4, "base_sha256": "a" * 64, "request": {"frontier": [{"work_id": "T"}]}}


class CoordinatorAdapterTests(unittest.TestCase):
    def test_request_contracts_are_authoritative_scoped_and_exclude_control(self):
        contracts = command_contracts_for("acceptance")
        self.assertEqual(set(contracts), {"domain.evidence-adjudicated", "domain.stage-accepted",
                                          "domain.integration-accepted", "domain.work-accepted"})
        self.assertTrue(all(row["payload_schema"]["additionalProperties"] is False for row in contracts.values()))
        self.assertFalse(any(kind.startswith("control.") for kind in contracts))
        review_contracts = command_contracts_for("review")
        self.assertTrue({"domain.task-contract-replaced", "domain.deferral-created", "domain.deferral-transferred",
                         "domain.deferral-closed", "domain.deferral-inapplicable"} <= set(review_contracts))
        self.assertFalse(any(kind.startswith("control.") for kind in review_contracts))
        review = review_contracts["domain.review-proposed"]
        self.assertEqual(review["sparse_transition_schema"]["properties"]["schema"]["const"],
                         "zap-domain/sparse-review-transition/1")
        child = review_contracts["knowledge.region-split"]["payload_schema"]["properties"]["children"]["items"]
        self.assertEqual(set(child["required"]), {"id", "question", "node_refs", "relevance"})
        self.assertEqual(set(child["properties"]["relevance"]["enum"]), {"relevant", "irrelevant", "unknown"})

    def test_provider_schema_closes_request_specific_selection_and_command_shapes(self):
        selection = provider_response_json_schema(request())
        self.assertEqual(selection["properties"]["commands"]["maxItems"], 0)
        acceptance_request = {**request(), "request_kind": "acceptance"}
        acceptance = provider_response_json_schema(acceptance_request)
        self.assertEqual(acceptance["properties"]["selection"], {"type": "null"})
        self.assertEqual(acceptance["properties"]["schema"], {"type": "string", "const": "zap-provider-coordinator-output/1"})

    def test_process_transport_refuses_verification_cwd_outside_allowed_workspace(self):
        with tempfile.TemporaryDirectory(prefix="zap-verification-cwd-") as temporary:
            root = Path(temporary); allowed = root / "allowed"; outside = root / "outside"; allowed.mkdir(); outside.mkdir()
            script = allowed / "check.py"; script.write_text("raise SystemExit(0)\n", encoding="utf-8")
            transport = ProcessTransport(root / "transport", [allowed])
            with self.assertRaises(Refusal):
                transport.submit("verification:outside", argv=(sys.executable, "-B", str(script), "{packet_file}"), cwd=outside, packet="{}")
            self.assertEqual(list((root / "transport" / "jobs").iterdir()), [])

    def test_real_json_process_adapter_submits_and_polls_without_blocking(self):
        with tempfile.TemporaryDirectory(prefix="zap-coordinator-process-") as temporary:
            root = Path(temporary); script = root / "bridge.py"; script.write_text(BRIDGE_FIXTURE, encoding="utf-8")
            transport = ProcessTransport(root / "transport", [root])
            adapter = JsonProcessCoordinatorAdapter(transport, argv=(sys.executable, "-B", str(script), "{packet_file}"), cwd=root)
            value = request(); submitted = adapter.submit(value["request_id"], value)
            self.assertTrue(submitted["accepted"])
            observed = None
            for _ in range(100):
                observed = adapter.poll(value["request_id"], value)
                if observed["ready"]: break
                time.sleep(0.03)
            self.assertTrue(observed["ok"])
            self.assertEqual(observed["response"]["selection"], {"work_id": "T"})
            self.assertTrue(adapter.submit(value["request_id"], value)["idempotent"])

    def test_codex_bridge_uses_verified_sol_xhigh_argv_stdin_and_schema(self):
        with tempfile.TemporaryDirectory(prefix="zap-codex-bridge-test-") as temporary:
            root = Path(temporary); packet = root / "request.json"; value = request(); packet.write_bytes(packed(value))
            launcher = root / "codexrunner.ps1"; launcher.write_text("# fixture\n", encoding="utf-8")
            observed = {}

            def fake_run(command, **kwargs):
                observed.update({"command": command, **kwargs})
                output = Path(command[command.index("--output-last-message") + 1])
                response = {"schema": "zap-provider-coordinator-output/1", "disposition": "select",
                            "rationale": "mocked provider decision", "selection": {"work_id": "T"}, "commands": []}
                output.write_bytes(packed(response))
                return type("Completed", (), {"returncode": 0, "stdout": "", "stderr": ""})()

            stdout = mock.Mock()
            stdout.buffer = mock.Mock()
            with mock.patch("zaplib.provider_bridge.subprocess.run", side_effect=fake_run), \
                 mock.patch("zaplib.provider_bridge.shutil.which", return_value="/portable/pwsh"), \
                 mock.patch("zaplib.provider_bridge.sys.stdout", stdout):
                code = bridge_main(["--launcher", str(launcher), str(packet)])
            self.assertEqual(code, 0)
            command = observed["command"]
            self.assertIn("gpt-5.6-sol", command)
            self.assertIn("model_reasoning_effort=\"xhigh\"", command)
            self.assertIn("--strict-config", command)
            self.assertIn("--ignore-user-config", command)
            self.assertEqual(command[:5], ["/portable/pwsh", "-NoProfile", "-NonInteractive", "-File", str(launcher)])
            self.assertEqual(command[-1], "-")
            self.assertFalse(observed["shell"])
            self.assertIn("payload_json", observed["input"])
            stdout.buffer.write.assert_called_once()

    def test_powershell_bridge_refuses_when_pwsh_is_missing(self):
        with tempfile.TemporaryDirectory(prefix="zap-codex-pwsh-m-test-") as temporary:
            root = Path(temporary); packet = root / "request.json"; packet.write_bytes(packed(request()))
            launcher = root / "codexrunner.ps1"; launcher.write_text("# fixture\n", encoding="utf-8")
            with mock.patch("zaplib.provider_bridge.shutil.which", return_value=None), \
                 self.assertRaisesRegex(RuntimeError, "PowerShell 7"):
                bridge_main(["--launcher", str(launcher), str(packet)])


if __name__ == "__main__":
    unittest.main()
