"""Bound project-fact promotion and truthful failure tests."""
from __future__ import annotations

import hashlib
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, parse
from zaplib.knowledge_promotion import build_promotion_proposal, portable_accepted_proofs, promote_fact
from zaplib.sources import capture_source
from zaplib.storage import load_store
from test_knowledge_support import HANDLERS, append, create_store


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-promotion-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = create_store(self.root)
        self.project = self.root / "project"
        (self.project / "facts").mkdir(parents=True)
        source = self.root / "source.txt"
        source.write_text("captured", encoding="utf-8")
        append(self.store, "knowledge.source-recorded", {"source": capture_source(source, self.root, source_id="S")}, "source")
        append(self.store, "evidence.recorded", {"id": "E", "claim": "Observed fact", "subject": "source:S", "result": "observed_pass",
               "artifact_refs": ["trace:E"], "node_refs": ["T"]}, "evidence")
        append(self.store, "fact.recorded", {"id": "F", "statement": "Promotable project fact", "status": "observed",
               "node_refs": ["T"], "evidence_refs": ["E"], "source_refs": ["S"]}, "fact")
        scope = {"kind": "subjects", "subjects": [{"kind": "node", "id": "T"}]}
        append(self.store, "knowledge.applicability-assessed", {"source_id": "S", "status": "applicable", "scope": scope,
               "evidence_refs": ["E"], "basis": "Checked for T"}, "applicability")
        append(self.store, "knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S"}, "status": "complete",
               "boundary": [{"kind": "node", "id": "T"}], "missing": [], "evidence_refs": ["E"],
               "basis": "Relevant local inputs bounded"}, "closure")

    @staticmethod
    def authorize(proposal, _state):
        return {"authorized": True, "action": "fact.promote", "promotion_id": proposal["promotion_id"],
                "accepted_proof_refs": ["E"], "authorization_ref": "control:grant-1"}

    def proposal(self, target: str = "facts/F.json"):
        state = load_store(self.store, HANDLERS)[0]
        applicability = {"status": "applicable", "refs": ["S"], "stale_refs": [], "unknown_refs": [], "incomplete_closure": False}
        state["extensions"]["domain"] = {"evidence_adjudications": {"E": {
            "schema": "zap-domain/evidence-adjudicated/1", "evidence_id": "E", "expected_revision": state["revision"],
            "disposition": "accepted", "applies_to": {"outcome_id": "OUT", "obligation_ids": ["O"], "work_ids": ["T"],
            "stage": "functional", "scope": "local"}, "source_refs": ["S"],
            "method": {"argv": ["python", "-B", "-m", "unittest"], "target": "T", "toolchain": "python-3.11",
                       "environment": "fixture", "subjects": ["source:S"], "cases": ["positive"]},
            "limitations": ["fixture scope"], "revision": state["revision"], "event_id": "adjudication-E",
            "applicability_at_adjudication": applicability,
            "source_captures_at_adjudication": [{"source_id": "S", "sha256": state["extensions"]["knowledge"]["sources"]["S"]["content_sha256"]}],
            "history": []}}}
        return state, build_promotion_proposal(state, promotion_id="promotion-1", fact_id="F", source_refs=["S"], target=target)

    def test_portable_proof_refuses_a_changed_adjudication_source_witness(self):
        state, _proposal = self.proposal()
        state["extensions"]["domain"]["evidence_adjudications"]["E"]["source_captures_at_adjudication"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(Refusal, "no longer current"):
            portable_accepted_proofs(state, ["E"], ["S"])

    def test_success_writes_bound_artifact_and_preserves_effect_receipt(self):
        state, proposal = self.proposal()
        effects = []

        def writer(effect):
            effects.append(effect)
            return {"event_id": "promotion-effect-1"}

        result = promote_fact(state, proposal, self.project, authorization=self.authorize, event_writer=writer)
        self.assertTrue(result["ok"])
        artifact = parse(Path(result["target"]).read_bytes(), tagged=True)
        self.assertEqual(artifact["fact_id"], "F")
        self.assertEqual(artifact["accepted_proof_refs"], ["E"])
        self.assertEqual(artifact["portable_proofs"][0]["observation"]["subject"], "source:S")
        self.assertEqual(artifact["portable_proofs"][0]["method"]["argv"], ["python", "-B", "-m", "unittest"])
        self.assertEqual(artifact["portable_proofs"][0]["source_captures_at_adjudication"][0]["source_id"], "S")
        self.assertEqual(artifact["source_reobservations"][0]["observed"]["status"], "current")
        self.assertEqual(artifact["sources"][0]["content_sha256"], state["extensions"]["knowledge"]["sources"]["S"]["content_sha256"])
        self.assertEqual(effects[0]["kind"], "fact.promotion-effect-recorded")
        self.assertEqual(result["event_receipt"], {"event_id": "promotion-effect-1"})

    def test_receipt_failure_rolls_back_new_artifact_truthfully(self):
        state, proposal = self.proposal("facts/rollback.json")

        def fail(_effect):
            raise RuntimeError("journal unavailable")

        result = promote_fact(state, proposal, self.project, authorization=self.authorize, event_writer=fail)
        self.assertFalse(result["ok"])
        self.assertEqual(result["artifact_status"], "rolled_back")
        self.assertFalse((self.project / "facts" / "rollback.json").exists())
        self.assertFalse(result["event_recorded"])

    def test_receipt_failure_never_deletes_a_foreign_replacement(self):
        state, proposal = self.proposal("facts/foreign.json")
        target = self.project / "facts" / "foreign.json"

        def replace_then_fail(_effect):
            target.unlink()
            target.write_bytes(b"foreign replacement")
            raise RuntimeError("journal unavailable")

        result = promote_fact(state, proposal, self.project, authorization=self.authorize, event_writer=replace_then_fail)
        self.assertFalse(result["ok"])
        self.assertEqual(result["artifact_status"], "present_unreceipted")
        self.assertEqual(target.read_bytes(), b"foreign replacement")

    def test_unaccepted_stale_outside_and_unexpected_existing_targets_refuse(self):
        state, proposal = self.proposal("facts/refuse.json")

        def no_proof(value, _state):
            grant = self.authorize(value, _state)
            grant["accepted_proof_refs"] = []
            return grant

        with self.assertRaisesRegex(Refusal, "accepted proof"):
            promote_fact(state, proposal, self.project, authorization=no_proof, event_writer=lambda _: {})

        def fake_proof(value, _state):
            grant = self.authorize(value, _state)
            grant["accepted_proof_refs"] = ["MISSING"]
            return grant

        with self.assertRaisesRegex(Refusal, "does not exist"):
            promote_fact(state, proposal, self.project, authorization=fake_proof, event_writer=lambda _: {})

        (self.project / "facts" / "refuse.json").write_bytes(b"unexpected")
        with self.assertRaisesRegex(Refusal, "unexpected content"):
            promote_fact(state, proposal, self.project, authorization=self.authorize, event_writer=lambda _: {})

        with self.assertRaises(Refusal):
            build_promotion_proposal(state, promotion_id="outside", fact_id="F", source_refs=["S"], target="../outside.json")

        actual_change = self.root / "source.txt"
        actual_change.write_text("changed without an event", encoding="utf-8")
        state_fresh, proposal_fresh = self.proposal("facts/actual-change.json")
        with self.assertRaisesRegex(Refusal, "changed or is unavailable"):
            promote_fact(state_fresh, proposal_fresh, self.project, authorization=self.authorize, event_writer=lambda _: {})
        actual_change.write_text("captured", encoding="utf-8")

        append(self.store, "knowledge.source-observed", {"source_id": "S", "observed": {"status": "changed",
               "sha256": hashlib.sha256(b"changed").hexdigest(), "bytes": 7, "detail": None}}, "changed")
        changed = load_store(self.store, HANDLERS)[0]
        with self.assertRaisesRegex(Refusal, "state binding"):
            promote_fact(changed, proposal, self.project, authorization=self.authorize, event_writer=lambda _: {})


if __name__ == "__main__":
    unittest.main()
