from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from test_service import make_charter, make_store
from test_runtime_support import RuntimeFixture
from zaplib.engine import build_engine
from zaplib.common import sha
from zaplib.cli_runtime import build_automatic_coordinator
from zaplib.cli_promotion import promote_from_cli
from zaplib.control import ACTION_CLASS_SET
from zaplib.control_trust import CredentialAuthority, Principal
from zaplib.domain import domain_state
from zaplib.runtime import codex_sol_xhigh_profile


SCRIPTS = Path(__file__).resolve().parent


def run_cli(script: Path, *args: str) -> tuple[int, dict]:
    result = subprocess.run(
        [sys.executable, "-B", str(script), *args],
        text=True, capture_output=True, check=False, timeout=15,
    )
    try:
        body = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise AssertionError(f"CLI did not emit JSON: stdout={result.stdout!r} stderr={result.stderr!r}") from exc
    return result.returncode, body


class PublicCliTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        parent = self.root / "fixture"
        parent.mkdir()
        self.store = make_store(parent)
        self.zap = SCRIPTS / "zap.py"
        self.legacy = SCRIPTS / "zap_state.py"

    def tearDown(self):
        self.temp.cleanup()

    def test_legacy_entrypoint_and_new_capabilities_share_composed_engine(self):
        code, inspected = run_cli(self.legacy, "inspect", "--store", str(self.store))
        self.assertEqual(code, 0)
        self.assertEqual(inspected["state"]["revision"], 0)
        code, capabilities = run_cli(self.zap, "capabilities", "--store", str(self.store))
        self.assertEqual(code, 0)
        self.assertIn("runtime.job-claimed", capabilities["handlers"])
        self.assertEqual(capabilities["events"]["knowledge.source-recorded"]["route"], "effect_adapter")

    def test_init_and_migrate_create_draft_stores_without_execution(self):
        plan = self.root / "fixture" / "plan.toml"
        tasks = self.root / "fixture" / "tasks"
        code, initialized = run_cli(
            self.zap, "init", "--plan", str(plan), "--tasks-dir", str(tasks),
            "--out", str(self.root / "initialized"),
        )
        self.assertEqual(code, 0)
        self.assertEqual(initialized["execution_mode"], "draft")
        self.assertFalse(initialized["owner_contract_activated"])
        code, migrated = run_cli(
            self.zap, "migrate-mup", "--plan", str(plan), "--tasks-dir", str(tasks),
            "--out", str(self.root / "migrated"),
        )
        self.assertEqual(code, 0)
        self.assertFalse(migrated["charter_activated"])
        self.assertFalse(migrated["commands_executed"])

    def test_public_record_is_data_only_and_privileged_kind_refuses(self):
        command_path = self.root / "command.json"
        command_path.write_text(json.dumps({
            "event_id": "classify-cli", "base_revision": 0,
            "kind": "node.classified", "reason": {"summary": "classify"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"},
        }), encoding="utf-8")
        code, result = run_cli(
            self.zap, "record", "--store", str(self.store),
            "--command", str(command_path),
        )
        self.assertEqual(code, 0)
        self.assertFalse(result["idempotent"])
        command_path.write_text(json.dumps({
            "event_id": "forged-cli", "base_revision": 1,
            "kind": "domain.outcome-adopted", "reason": {"summary": "forged"},
            "payload": {},
        }), encoding="utf-8")
        code, result = run_cli(
            self.zap, "record", "--store", str(self.store),
            "--command", str(command_path),
        )
        self.assertEqual(code, 2)
        self.assertEqual(result["code"], "AUTHORIZATION")

    def test_explicit_trust_bootstrap_and_trusted_source_capture(self):
        trust_dir = self.root / "trust"
        code, bootstrap = run_cli(
            self.zap, "trust-bootstrap", "--store", str(self.store),
            "--trust-dir", str(trust_dir),
        )
        self.assertEqual(code, 0)
        self.assertFalse(bootstrap["secrets_in_output"])
        source_root = self.root / "source-root"
        source_root.mkdir()
        source = source_root / "source.txt"
        source.write_text("source bytes", encoding="utf-8")
        code, captured = run_cli(
            self.zap, "capture-source", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--credential-id", "coordinator-credential",
            "--credential-file", bootstrap["credential_files"]["coordinator"],
            "--artifact-store", str(self.root / "private-artifacts"),
            "--path", str(source), "--allowed-root", str(source_root),
            "--source-id", "source-cli",
        )
        self.assertEqual(code, 0)
        self.assertEqual(captured["capture"]["source"]["id"], "source-cli")
        self.assertNotIn("source bytes", json.dumps(captured))
        captured_revision = build_engine(self.store).load()[0]["revision"]
        code, retried = run_cli(
            self.zap, "capture-source", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--credential-id", "coordinator-credential",
            "--credential-file", bootstrap["credential_files"]["coordinator"],
            "--artifact-store", str(self.root / "private-artifacts"),
            "--path", str(source), "--allowed-root", str(source_root),
            "--source-id", "source-cli",
        )
        self.assertEqual(code, 0)
        self.assertTrue(retried["idempotent"])
        self.assertTrue(retried["already_current"])
        self.assertEqual(build_engine(self.store).load()[0]["revision"], captured_revision)
        code, detail = run_cli(
            self.zap, "detail", "--store", str(self.store),
            "--kind", "source", "--id", "source-cli",
        )
        self.assertEqual(code, 0)
        self.assertNotIn("root", detail["detail"]["entity"])
        state = build_engine(self.store).load()[0]
        forged_command = self.root / "forged-observation.json"
        forged_command.write_text(json.dumps({
            "event_id": "forged-cli-capture", "base_revision": state["revision"],
            "kind": "knowledge.source-recorded", "reason": {"summary": "forged bytes"},
            "payload": {"source": {**captured["capture"]["source"], "id": "forged-cli"}},
        }), encoding="utf-8")
        code, refused = run_cli(
            self.zap, "observe", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--credential-id", "coordinator-credential",
            "--credential-file", bootstrap["credential_files"]["coordinator"],
            "--command", str(forged_command),
        )
        self.assertEqual(code, 2)
        self.assertEqual(refused["code"], "OBSERVATION")
        self.assertNotIn("forged-cli", build_engine(self.store).load()[0]["extensions"]["knowledge"]["sources"])
        spec = source_root / "facts.xml"
        spec.write_text(
            '<?xml version="1.0" encoding="UTF-8"?><spec xmlns="https://vibevm.org/spec/1">'
            '<title id="root">Facts</title><p><CLAIM fact="true" status="spec/plan">Bound claim</CLAIM></p></spec>',
            encoding="utf-8",
        )
        native_args = (
            self.zap, "capture-source", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--credential-id", "coordinator-credential",
            "--credential-file", bootstrap["credential_files"]["coordinator"],
            "--artifact-store", str(self.root / "private-artifacts"),
            "--path", str(spec), "--allowed-root", str(source_root),
            "--source-id", "native-cli", "--kind", "vibevm_xml_spec",
        )
        code, native = run_cli(*native_args)
        self.assertEqual(code, 0)
        self.assertTrue(native["capture"]["capture"]["facts"])
        native_fact_id = native["capture"]["capture"]["facts"][0]["id"]
        spec.write_text(spec.read_text(encoding="utf-8").replace("Bound claim", "Revised claim"), encoding="utf-8")
        code, recaptured = run_cli(*native_args)
        self.assertEqual(code, 0)
        self.assertEqual(recaptured["event"]["event"]["kind"], "knowledge.source-recaptured")
        code, stale_fact = run_cli(
            self.zap, "detail", "--store", str(self.store), "--kind", "fact", "--id", native_fact_id,
        )
        self.assertEqual(code, 0)
        self.assertTrue(stale_fact["detail"]["availability"]["stale"])

    def test_charter_prepare_then_data_draft_and_owner_activation(self):
        state = build_engine(self.store).load()[0]
        charter_path = self.root / "charter.json"
        charter_path.write_text(json.dumps(make_charter(state)), encoding="utf-8")
        code, prepared = run_cli(
            self.zap, "charter-prepare", "--store", str(self.store),
            "--charter", str(charter_path), "--out-dir", str(self.root / "prepared"),
        )
        self.assertEqual(code, 0)
        code, _draft = run_cli(
            self.zap, "record", "--store", str(self.store),
            "--command", prepared["draft_command"],
        )
        self.assertEqual(code, 0)
        code, trust = run_cli(
            self.zap, "trust-bootstrap", "--store", str(self.store),
            "--trust-dir", str(self.root / "activate-trust"),
        )
        self.assertEqual(code, 0)
        code, _activated = run_cli(
            self.zap, "control", "--store", str(self.store),
            "--trust-config", trust["trust_config"],
            "--credential-id", "owner-credential",
            "--credential-file", trust["credential_files"]["owner"],
            "--command", prepared["activation_command"],
        )
        self.assertEqual(code, 0)
        code, inspected = run_cli(self.zap, "inspect", "--store", str(self.store))
        self.assertEqual(code, 0)
        self.assertEqual(inspected["state"]["execution_mode"], "active")

    def test_projected_skill_copy_resolves_its_own_runtime(self):
        projected = self.root / "project" / ".codex" / "skills" / "zap-state"
        projected.parent.mkdir(parents=True)
        shutil.copytree(SCRIPTS.parent, projected)
        script = projected / "scripts" / "zap.py"
        code, capabilities = run_cli(script, "capabilities", "--store", str(self.store))
        self.assertEqual(code, 0)
        self.assertEqual(capabilities["schema"], "zap-capabilities/1")

    def test_zap_run_wrapper_finds_sibling_projected_runtime(self):
        skills = self.root / "projected" / ".codex" / "skills"
        skills.mkdir(parents=True)
        shutil.copytree(SCRIPTS.parent, skills / "zap-state")
        shutil.copytree(SCRIPTS.parent.parent / "zap-run", skills / "zap-run")
        wrapper = skills / "zap-run" / "scripts" / "zap-run.py"
        result = subprocess.run(
            [
                sys.executable, "-B", str(wrapper), "--", "capabilities",
                "--store", str(self.store),
            ],
            cwd=self.root / "projected", text=True, capture_output=True,
            check=False, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["schema"], "zap-capabilities/1")

    def test_pending_tail_repair_requires_owner_and_preserves_quarantine(self):
        code, bootstrap = run_cli(
            self.zap, "trust-bootstrap", "--store", str(self.store),
            "--trust-dir", str(self.root / "repair-trust"),
        )
        self.assertEqual(code, 0)
        tail = b'{"incomplete"'
        with (self.store / "events.jsonl").open("ab") as stream:
            stream.write(tail)
        common = (
            "repair-pending-tail", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--repair-id", "repair-cli",
            "--expected-tail-sha256", sha(tail),
        )
        code, refused = run_cli(
            self.zap, *common,
            "--credential-id", "coordinator-credential",
            "--credential-file", bootstrap["credential_files"]["coordinator"],
        )
        self.assertEqual(code, 2)
        self.assertEqual(refused["code"], "AUTHORIZATION")
        code, repaired = run_cli(
            self.zap, *common,
            "--credential-id", "owner-credential",
            "--credential-file", bootstrap["credential_files"]["owner"],
        )
        self.assertEqual(code, 0)
        self.assertTrue(Path(repaired["quarantine"]).joinpath("original-events.jsonl").is_file())
        self.assertTrue(Path(repaired["quarantine"]).joinpath("pending-tail.bin").is_file())

    def test_tick_wires_real_coordinator_but_draft_import_starts_nothing(self):
        code, bootstrap = run_cli(
            self.zap, "trust-bootstrap", "--store", str(self.store),
            "--trust-dir", str(self.root / "runtime-trust"),
        )
        self.assertEqual(code, 0)
        profile = {
            "schema": "zap-runtime-profile/1",
            "transport": {
                "root": str(self.root / "transport"), "allowed_workspace_roots": [str(self.root)],
                "inherited_environment": [], "environment_allowlist": [],
                "allow_process_termination": False,
            },
            "semantic_transport": {
                "root": str(self.root / "semantic"), "allowed_workspace_roots": [str(self.root)],
                "inherited_environment": [], "environment_allowlist": [],
                "allow_process_termination": False,
            },
            "semantic": {
                "kind": "json-process", "argv": [sys.executable, "{packet_file}"],
                "cwd": str(self.root), "launcher": None,
            },
            "artifact_capture": {"allowed_root": str(self.root)},
            "worker": {
                "argv": [sys.executable, "{packet_file}"], "cwd": str(self.root),
                "standing_rule_paths": [], "environment": {}, "resource_capacities": {},
                "review_capacity": 1, "integration_capacity": 1, "branch_for_work": {},
                "stop_mode": "cooperative", "terminate_after_seconds": None,
            },
            "verifications": {},
            "runtime": {"transient_backoff_ns": 0, "idle_poll_seconds": 0},
        }
        profile_path = self.root / "profile.json"
        profile_path.write_text(json.dumps(profile), encoding="utf-8")
        code, tick = run_cli(
            self.zap, "tick", "--store", str(self.store),
            "--trust-config", bootstrap["trust_config"],
            "--host-principal-id", "coordinator-principal",
            "--profile", str(profile_path),
            "--artifact-store", str(self.root / "runtime-artifacts"),
        )
        self.assertEqual(code, 0)
        self.assertEqual(tick["execution_mode"], "draft")
        self.assertEqual(tick["actions"], [])
        self.assertEqual(tick["active_jobs"], [])
        self.assertTrue((self.root / "runtime-artifacts" / "catalog.json").is_file())

    def test_codex_runtime_profile_preserves_the_provider_auth_environment(self):
        state = build_engine(self.store).load()[0]
        principal = Principal("host", "coordinator", state["plan"]["plan_id"])
        engine = build_engine(self.store, host_principal=principal)
        profile = {
            "schema": "zap-runtime-profile/1",
            "transport": {
                "root": str(self.root / "transport"), "allowed_workspace_roots": [str(self.root)],
                "inherited_environment": None, "environment_allowlist": [],
                "allow_process_termination": False,
            },
            "semantic_transport": {
                "root": str(self.root / "semantic"), "allowed_workspace_roots": [str(self.root)],
                "inherited_environment": None, "environment_allowlist": [],
                "allow_process_termination": False,
            },
            "semantic": {
                "kind": "codex-sol-xhigh", "argv": [], "cwd": str(self.root),
                "launcher": str(self.root / "codexrunner"),
            },
            "worker": {
                "kind": "codex-sol-xhigh", "launcher": str(self.root / "codexrunner"),
                "argv": [], "cwd": str(self.root),
                "standing_rule_paths": [], "environment": {}, "resource_capacities": {},
                "review_capacity": 1, "integration_capacity": 1, "branch_for_work": {},
                "stop_mode": "cooperative", "terminate_after_seconds": None,
            },
            "verifications": {},
            "runtime": {"transient_backoff_ns": 0, "idle_poll_seconds": 0},
        }
        profile_path = self.root / "codex-profile.json"
        profile_path.write_text(json.dumps(profile), encoding="utf-8")
        coordinator = build_automatic_coordinator(engine, profile_path)
        expected = set(codex_sol_xhigh_profile(cwd=self.root, launcher=self.root / "codexrunner").inherited_environment)
        self.assertEqual(set(coordinator.semantic.transport.inherited_environment), expected)
        self.assertEqual(set(coordinator.config.worker.required_inherited_environment), expected)
        self.assertEqual(set(coordinator.transport.inherited_environment), expected)
        self.assertIn("CODEXRUNNER_SANDBOXED", coordinator.transport.environment_allowlist)
        self.assertEqual(coordinator.config.worker.environment["CODEXRUNNER_SANDBOXED"], "1")
        self.assertIn("USERPROFILE" if os.name == "nt" else "HOME", expected)

    def test_fact_promotion_records_the_real_domain_receipt(self):
        fixture_root = self.root / "promotion-fixture"
        fixture_root.mkdir()
        fixture = RuntimeFixture(fixture_root)
        state = fixture.state()
        domain = domain_state(state)
        obligations = sorted(
            row["obligation_id"] for row in domain["ownership"]
            if row["work_id"] == "T" and domain["obligations"][row["obligation_id"]]["status"] == "active"
        )
        fixture._apply("domain.evidence-adjudicated", {
            "schema": "zap-domain/evidence-adjudicated/1", "evidence_id": "SOURCE-E",
            "expected_revision": -1, "disposition": "accepted",
            "applies_to": {"outcome_id": "OUTCOME", "obligation_ids": obligations, "work_ids": ["T"],
                           "stage": "functional", "scope": "promotion fixture"},
            "source_refs": ["S"],
            "method": {"argv": ["python", "-B", "verify.py"], "target": "T", "toolchain": "python",
                       "environment": "isolated", "subjects": ["source:S"], "cases": ["positive"]},
            "limitations": [],
        }, "evidence.adjudicate", "Accept promotion proof", [{"source_id": "S", "sha256": state["extensions"]["knowledge"]["sources"]["S"]["content_sha256"]}])
        fixture._data("fact.recorded", {
            "id": "PROMOTABLE", "statement": "Fixture proof is portable", "status": "observed",
            "node_refs": ["T"], "evidence_refs": ["SOURCE-E"], "source_refs": ["S"],
        }, "Record promotable fact")
        binding, token = CredentialAuthority.issue(
            "owner-promotion", "owner-promotion", "owner", "runtime-fixture",
            action_classes=ACTION_CLASS_SET,
        )
        engine = build_engine(fixture.store, CredentialAuthority([binding]))
        project = fixture_root / "project"
        (project / "facts").mkdir(parents=True)
        result = promote_from_cli(
            engine, credential_id="owner-promotion", credential=token,
            project_root=str(project), artifact_store=str(fixture_root / "artifacts"),
            promotion_id="promotion-cli", fact_id="PROMOTABLE", source_refs=["S"],
            target="facts/PROMOTABLE.json", proof_refs=["SOURCE-E"],
            authorization_ref="owner-promotion",
        )
        self.assertTrue(result["ok"])
        promotion = domain_state(engine.load()[0])["promotions"]["promotion-cli"]
        self.assertEqual(promotion["evidence_ids"], ["SOURCE-E"])
        self.assertEqual(promotion["effect"], "confirmed_by_external_adapter_receipt")

    def test_sparse_review_builder_materializes_every_obligation_without_appending(self):
        fixture_root = self.root / "sparse-fixture"
        fixture_root.mkdir()
        fixture = RuntimeFixture(fixture_root)
        fixture._data("domain.outcome-proposed", {
            "schema": "zap-domain/outcome-proposed/1", "outcome_id": "OUTCOME-2", "revision": 2,
            "previous_outcome_id": "OUTCOME", "intent_id": "INTENT", "summary": "Refined verified tasks",
            "benefits": ["proof"], "guarantees": ["checked"], "tradeoffs": [], "obligations": [],
        }, "Propose sparse-review target")
        before = fixture.state()
        active = {
            key for key, row in domain_state(before)["obligations"].items()
            if row["status"] == "active"
        }
        request = {
            "schema": "zap-domain/sparse-review-transition/1", "intent_id": None,
            "outcome_id": "OUTCOME-2", "changed_dispositions": [], "ownership_changes": [],
            "work_changes": [], "preserved_evidence_ids": [], "preserved_stage_acceptance_ids": [],
            "preserved_work_acceptance_ids": [], "preserved_integration_acceptance_ids": [],
            "job_reconciliation": [], "tradeoffs": [], "preserved_benefits": ["proof"],
        }
        request_path = fixture_root / "sparse.json"
        request_path.write_text(json.dumps(request), encoding="utf-8")
        code, result = run_cli(
            self.zap, "materialize-review-transition", "--store", str(fixture.store),
            "--request", str(request_path),
        )
        self.assertEqual(code, 0)
        dispositions = result["transition"]["obligation_dispositions"]
        self.assertEqual({row["obligation_id"] for row in dispositions}, active)
        self.assertTrue(all(row["disposition"] == "retained" for row in dispositions))
        self.assertEqual(fixture.state()["revision"], before["revision"])


if __name__ == "__main__":
    unittest.main()
