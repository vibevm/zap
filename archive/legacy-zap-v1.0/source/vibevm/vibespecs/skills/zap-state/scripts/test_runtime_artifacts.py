"""Generated-output identity, applicability, and stale-proof tests."""
from __future__ import annotations

from dataclasses import replace
from functools import partial
import json
from pathlib import Path
import tempfile
import unittest

from zaplib.domain import domain_state
from zaplib.artifacts import PrivateArtifactStore, capture_source_blob
from zaplib.common import Refusal, sha
from zaplib.runtime import AutomaticCoordinator, VerificationSpec, runtime_state
from zaplib.runtime_artifacts import _verified_target
from zaplib.sources import current_applicability
from test_runtime_support import RuntimeFixture, tick_until


VERIFIER = '''import hashlib, json, pathlib, sys
packet = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
target = pathlib.Path(packet["check"]["target"])
raw = target.read_bytes() if target.is_file() else b""
passed = target.is_file() and raw == b"candidate"
print(json.dumps({"schema":"zap-verification-output/1", "result":"pass" if passed else "fail",
                  "artifacts":[{"path":str(target.resolve()), "sha256":hashlib.sha256(raw).hexdigest(), "bytes":len(raw)}],
                  "summary":"exact candidate bytes checked"}))
raise SystemExit(0 if passed else 3)
'''


class RuntimeArtifactTests(unittest.TestCase):
    def test_verifier_output_uses_cwd_for_relative_target_and_rejects_same_length_tamper(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-verifier-output-") as temporary:
            root = Path(temporary); target = root / "result.txt"; target.write_text("candidate", encoding="utf-8")
            output = root / "stdout.json"
            value = {"schema": "zap-verification-output/1", "result": "pass",
                     "artifacts": [{"path": "result.txt", "sha256": sha(target.read_bytes()), "bytes": len(target.read_bytes())}],
                     "summary": "a"}
            raw = json.dumps(value, separators=(",", ":"), sort_keys=True).encode("utf-8"); output.write_bytes(raw)
            row = {"result": {"stdout": {"path": str(output), "bytes": len(raw), "sha256": sha(raw)}}}
            spec = VerificationSpec("check", ("python", "{packet_file}"), str(root), "result.txt", "python", "test",
                                    ("result.txt",), ("exact",), ("S",))
            _identity, resolved = _verified_target(row, spec)
            self.assertEqual(resolved, target.resolve())
            output.write_bytes(raw.replace(b'"summary":"a"', b'"summary":"b"'))
            with self.assertRaisesRegex(Refusal, "output identity"):
                _verified_target(row, spec)

    def _fixture(self, root):
        fixture = RuntimeFixture(root, worker_seconds=0.1)
        fixture.verify_script.write_text(VERIFIER, encoding="utf-8")
        artifacts = PrivateArtifactStore(root / "private-artifacts", create=True)
        fixture.config = replace(fixture.config, artifact_capture=partial(capture_source_blob, artifacts),
                                 artifact_reader=artifacts.read, artifact_allowed_root=str(root))
        return fixture

    def test_exact_generated_output_is_captured_assessed_and_used_for_acceptance(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-artifact-") as temporary:
            fixture = self._fixture(Path(temporary))
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport,
                                               fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: bool(domain_state(fixture.state())["acceptances"]), limit=300)
            state = fixture.state(); runtime = runtime_state(state)
            captured = [row["artifact_source"] for row in runtime["verification_jobs"].values() if row.get("artifact_source")]
            self.assertTrue(captured)
            for row in captured:
                self.assertEqual(current_applicability(state, [row["source_id"]])["status"], "applicable")
            accepted = next(iter(domain_state(state)["evidence_adjudications"].values()))
            self.assertTrue(any(source_id.startswith("artifact:") for source_id in accepted["source_refs"]))
            request = next(row["request"]["request"] for row in runtime["semantic_requests"].values() if row["request_kind"] == "acceptance")
            self.assertTrue(request["jobs"][0]["candidate_report"]["candidate"])
            self.assertEqual(request["verification_jobs"][0]["artifact_content"]["content"], "candidate")

    def test_output_changed_after_verification_stales_acceptance_response(self):
        with tempfile.TemporaryDirectory(prefix="zap-runtime-artifact-stale-") as temporary:
            fixture = self._fixture(Path(temporary))
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service, fixture.transport,
                                               fixture.semantic, fixture.config)
            tick_until(coordinator, lambda _: any(row["request_kind"] == "acceptance" and row["state"] in {"requested", "submitted"}
                                                   for row in runtime_state(fixture.state())["semantic_requests"].values()), limit=300)
            pending = next((key for key, row in runtime_state(fixture.state())["semantic_requests"].items()
                            if row["request_kind"] == "acceptance" and row["state"] in {"requested", "submitted"}))
            before = runtime_state(fixture.state())
            first = next(row for row in before["verification_jobs"].values() if row.get("artifact_source"))
            fixture.targets[before["jobs"][first["work_job_id"]]["work_id"]].write_text("changed-after-verification", encoding="utf-8")
            coordinator.tick()
            state = fixture.state(); runtime = runtime_state(state)
            self.assertEqual(runtime["semantic_requests"][pending]["state"], "stale")
            self.assertEqual(domain_state(state)["evidence_adjudications"], {})
            source_id = first["artifact_source"]["source_id"]
            self.assertEqual(state["extensions"]["knowledge"]["sources"][source_id]["capture_status"], "changed")


if __name__ == "__main__":
    unittest.main()
