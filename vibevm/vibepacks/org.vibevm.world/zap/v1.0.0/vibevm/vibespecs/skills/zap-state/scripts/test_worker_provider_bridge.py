"""Ready Codex Sol/xhigh coding-worker bridge tests without a model call."""
from __future__ import annotations

import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from zaplib.common import packed, parse
from zaplib.runtime import codex_sol_xhigh_worker_profile
from zaplib.worker_provider_bridge import main as worker_bridge_main


def packet():
    return {"schema": "zap-worker-packet/1", "job_id": "job:T:1", "attempt_id": "attempt:T:1", "work_id": "T",
            "contract_sha256": "a" * 64, "contract": {"goal": "bounded"}, "read_subjects": ["input"], "write_subjects": ["output"],
            "resources": ["worker"], "integration_owner": "T", "standing_rule_paths": ["rules.xml"],
            "verification_bindings": ["check-T"], "safe_boundary": "preserve candidate", "source_captures": []}


class WorkerProviderBridgeTests(unittest.TestCase):
    def test_ready_worker_profile_preserves_provider_auth_homes_and_packet_placeholder(self):
        with tempfile.TemporaryDirectory(prefix="zap-worker-profile-") as temporary:
            launcher = Path(temporary) / "codexrunner"
            launcher.write_text("test launcher", encoding="utf-8")
            profile = codex_sol_xhigh_worker_profile(cwd=temporary, launcher=launcher, standing_rule_paths=["rules.xml"])
            self.assertEqual(profile.argv.count("{packet_file}"), 1)
            self.assertIn("worker_provider_bridge.py", " ".join(profile.argv))
            self.assertIn("workspace-write", profile.argv)
            home = "USERPROFILE" if os.name == "nt" else "HOME"
            self.assertIn(home, profile.required_inherited_environment)
            self.assertEqual(profile.standing_rule_paths, ("rules.xml",))

    def test_bridge_injects_worker_identity_and_never_self_accepts(self):
        with tempfile.TemporaryDirectory(prefix="zap-worker-bridge-") as temporary:
            root = Path(temporary); packet_path = root / "packet.json"; packet_path.write_bytes(packed(packet()))
            launcher = root / "codexrunner.ps1"; launcher.write_text("# test launcher", encoding="utf-8")
            observed = {}

            def fake_run(command, **kwargs):
                observed.update({"command": command, **kwargs})
                output = Path(command[command.index("--output-last-message") + 1])
                output.write_bytes(packed({"schema": "zap-provider-worker-output/1", "status": "candidate", "summary": "Implemented bounded work",
                                           "artifacts": ["output"], "evidence": ["local observation"], "checks_run": [],
                                           "safe_boundary": "candidate preserved"}))
                return type("Completed", (), {"returncode": 0, "stdout": "", "stderr": ""})()

            stdout = mock.Mock(); stdout.buffer = mock.Mock()
            with mock.patch("zaplib.worker_provider_bridge.subprocess.run", side_effect=fake_run), \
                    mock.patch("zaplib.worker_provider_bridge.shutil.which", return_value="/portable/pwsh"), \
                    mock.patch("zaplib.worker_provider_bridge.sys.stdout", stdout):
                code = worker_bridge_main(["--launcher", str(launcher), "--sandbox", "workspace-write",
                                           "--approval-policy", "auto-review", str(packet_path)])
            self.assertEqual(code, 0)
            raw = stdout.buffer.write.call_args.args[0]
            candidate = parse(raw)
            self.assertEqual((candidate["job_id"], candidate["attempt_id"], candidate["work_id"]), ("job:T:1", "attempt:T:1", "T"))
            self.assertFalse(candidate["accepted"])
            self.assertIn("gpt-5.6-sol", observed["command"])
            self.assertEqual(observed["command"][:5],
                             ["/portable/pwsh", "-NoProfile", "-NonInteractive", "-File", str(launcher)])
            self.assertIn("model_reasoning_effort=\"xhigh\"", observed["command"])
            self.assertIn("--approve-for-me", observed["command"])
            self.assertNotIn("-s", observed["command"])
            self.assertFalse(observed["shell"])
            self.assertIn("producer evidence, never central acceptance", observed["input"])

    def test_powershell_launcher_refuses_when_pwsh_is_missing(self):
        with tempfile.TemporaryDirectory(prefix="zap-worker-pwsh-") as temporary:
            root = Path(temporary)
            packet_path = root / "packet.json"; packet_path.write_bytes(packed(packet()))
            launcher = root / "codexrunner.ps1"; launcher.write_text("# test launcher", encoding="utf-8")
            with mock.patch("zaplib.worker_provider_bridge.shutil.which", return_value=None):
                with self.assertRaisesRegex(RuntimeError, "requires PowerShell 7"):
                    worker_bridge_main(["--launcher", str(launcher), "--sandbox", "workspace-write",
                                        "--approval-policy", "auto-review", str(packet_path)])


if __name__ == "__main__":
    unittest.main()
