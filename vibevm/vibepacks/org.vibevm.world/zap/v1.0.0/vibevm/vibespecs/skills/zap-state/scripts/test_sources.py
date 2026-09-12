"""Captured-source, native-fact, and applicability tests."""
from __future__ import annotations

import hashlib
from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal
from zaplib.knowledge import KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_EVENT_SCHEMAS, KNOWLEDGE_HANDLERS
from zaplib.sources import capture_source, capture_vibevm_facts, compare_source_captures, current_applicability, observe_source
from zaplib.storage import load_store
from test_knowledge_support import HANDLERS, append, create_store


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-sources-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_capture_is_bound_but_unassessed_and_outside_root_refuses(self):
        source = self.root / "source.txt"
        source.write_bytes(b"exact bytes\r\n")
        descriptor = capture_source(source, self.root, source_id="S1")
        self.assertEqual(descriptor["id"], "S1")
        self.assertEqual(descriptor["path"], "source.txt")
        self.assertEqual(descriptor["bytes"], len(b"exact bytes\r\n"))
        self.assertEqual(descriptor["applicability_scope"], {"kind": "unassessed", "subjects": []})
        source.write_bytes(b"changed")
        observation = observe_source(descriptor, self.root)
        self.assertEqual(observation["observed"]["status"], "changed")
        self.assertEqual(observation["observed"]["sha256"], hashlib.sha256(b"changed").hexdigest())
        outside = self.root.parent / "outside-source.txt"
        outside.write_bytes(b"outside")
        self.addCleanup(lambda: outside.unlink(missing_ok=True))
        with self.assertRaises((Refusal, ValueError)):
            capture_source(outside, self.root)

    def test_native_xml_reads_only_declared_vibevm_fact_markers(self):
        spec = self.root / "facts.xml"
        spec.write_text('''<?xml version="1.0"?>
<spec xmlns="https://vibevm.org/spec/1" xmlns:x="urn:foreign">
  <p><DECLARED fact="true" status="impl/done">Normative declaration</DECLARED></p>
  <p><UNMARKED status="impl/done">not a fact</UNMARKED></p>
  <x:FOREIGN fact="true" status="impl/done">not native</x:FOREIGN>
</spec>''', encoding="utf-8")
        capture = capture_vibevm_facts(spec, self.root, source_id="SPEC")
        self.assertEqual(len(capture["facts"]), 1)
        fact = capture["facts"][0]
        self.assertEqual((fact["marker"], fact["normative_status"]), ("DECLARED", "impl/done"))
        self.assertEqual((fact["observation_status"], fact["acceptance_status"]), ("unobserved", "unassessed"))
        store = create_store(self.root)
        append(store, "knowledge.native-facts-recorded", {"capture": capture}, "native-facts")
        recorded = load_store(store, HANDLERS)[0]["extensions"]["knowledge"]["native_facts"][fact["id"]]
        self.assertEqual((recorded["normative_status"], recorded["acceptance_status"]), ("impl/done", "unassessed"))
        bridge = self.root / "bridge.xml"
        bridge.write_text('<bridge xmlns="https://vibevm.org/spec/1"><CLAIM fact="true">data</CLAIM></bridge>', encoding="utf-8")
        with self.assertRaises(Refusal):
            capture_vibevm_facts(bridge, self.root)
        markdown = self.root / "bridge.md"
        markdown.write_text('@fact:CLAIM arbitrary bridge data', encoding="utf-8")
        with self.assertRaises(Refusal):
            capture_vibevm_facts(markdown, self.root)

    def test_local_applicability_ignores_unrelated_unknown_but_exposes_missing_relevant_closure(self):
        store = create_store(self.root)
        one = self.root / "one.txt"
        two = self.root / "two.txt"
        one.write_text("one", encoding="utf-8")
        two.write_text("two", encoding="utf-8")
        append(store, "knowledge.source-recorded", {"source": capture_source(one, self.root, source_id="S1")}, "source-1")
        append(store, "knowledge.source-recorded", {"source": capture_source(two, self.root, source_id="S2")}, "source-2")
        append(store, "knowledge.source-observation-recorded", {"source_id": "S1", "observed": {"status": "changed", "sha256": "0" * 64,
               "bytes": 99, "detail": None}, "claim": "Producer suspects drift", "artifact_refs": ["producer:trace"]}, "source-candidate")
        candidate_state = load_store(store, HANDLERS)[0]
        self.assertEqual(candidate_state["extensions"]["knowledge"]["sources"]["S1"]["capture_status"], "current")
        self.assertEqual(candidate_state["extensions"]["knowledge"]["invalidations"], {})
        append(store, "evidence.recorded", {"id": "E", "claim": "Checked S1", "subject": "S1 bytes",
               "result": "observed_pass", "artifact_refs": ["trace:S1"], "node_refs": ["T"]}, "evidence-1")
        scope = {"kind": "subjects", "subjects": [{"kind": "node", "id": "T"}]}
        append(store, "knowledge.applicability-assessed", {"source_id": "S1", "status": "applicable", "scope": scope,
               "evidence_refs": ["E"], "basis": "Exact source checked for task T"}, "applicable-1")
        state = load_store(store, HANDLERS)[0]
        missing = current_applicability(state, ["S1"])
        self.assertEqual(missing, {"status": "unknown", "refs": ["S1"], "stale_refs": [], "unknown_refs": ["S1"], "incomplete_closure": True})
        append(store, "knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S1"}, "status": "complete",
               "boundary": [{"kind": "node", "id": "T"}], "missing": [], "evidence_refs": ["E"],
               "basis": "Bounded inputs for this local proof"}, "closure-1")
        state = load_store(store, HANDLERS)[0]
        self.assertEqual(current_applicability(state, ["S1"]), {"status": "applicable", "refs": ["S1"], "stale_refs": [], "unknown_refs": [], "incomplete_closure": False})
        append(store, "knowledge.dependency-recorded", {"id": "new-relevant-edge", "prerequisite": {"kind": "source", "id": "S1"},
               "dependent": {"kind": "node", "id": "T"}, "relation": "affects"}, "new-relevant-edge")
        state = load_store(store, HANDLERS)[0]
        self.assertEqual(current_applicability(state, ["S1"])["status"], "unknown")
        append(store, "knowledge.closure-assessed", {"subject": {"kind": "source", "id": "S1"}, "status": "complete",
               "boundary": [{"kind": "node", "id": "T"}], "missing": [], "evidence_refs": ["E"],
               "basis": "Rebounded after the relevant edge"}, "closure-2")
        state = load_store(store, HANDLERS)[0]
        self.assertEqual(current_applicability(state, ["S1"])["status"], "applicable")
        source_hash = state["extensions"]["knowledge"]["sources"]["S1"]["content_sha256"]
        self.assertEqual(compare_source_captures(state, [{"source_id": "S1", "sha256": source_hash}])["status"], "current")
        self.assertEqual(compare_source_captures(state, [{"source_id": "S1", "sha256": "0" * 64}])["status"], "stale")
        self.assertEqual(current_applicability(state, ["S1", "S2"])["status"], "unknown")
        self.assertEqual(current_applicability(state, ["S1"])["status"], "applicable")
        self.assertEqual(set(KNOWLEDGE_EVENT_SCHEMAS), set(KNOWLEDGE_HANDLERS))
        self.assertEqual(set(KNOWLEDGE_EVENT_ROUTES), set(KNOWLEDGE_HANDLERS))
        self.assertEqual(KNOWLEDGE_EVENT_ROUTES["knowledge.source-observation-recorded"]["route"], "agent_data")
        self.assertEqual(KNOWLEDGE_EVENT_ROUTES["knowledge.applicability-assessed"]["action"], "evidence.adjudicate")
        self.assertEqual(KNOWLEDGE_EVENT_ROUTES["knowledge.dependency-recorded"]["action"], "evidence.adjudicate")
        self.assertEqual(current_applicability(state, [])["status"], "unknown")


if __name__ == "__main__":
    unittest.main()
