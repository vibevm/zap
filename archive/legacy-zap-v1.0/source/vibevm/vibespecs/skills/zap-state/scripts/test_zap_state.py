"""Small fixture tests for the ZAP reference kernel; no product execution."""
import base64
import contextlib
import copy
import io
import json
from pathlib import Path
import runpy
import tempfile
import tomllib
import unittest

M = runpy.run_path(str(Path(__file__).with_name("zap_state.py")))
RULES_PATH = Path(__file__).parents[3] / "examples/zap/stop-rules.json"

PLAN = '''schema = 1
plan_id = "fixture"
revision = 7
root_node = "R"
current_node = "T"
unknown_field = { nested = ["keep", "order"] }
[[mandate]]
id = "OWNER"
text = "Keep the complete requirement."
disposition = "owned"
nodes = ["R", "G", "T", "U", "HOLD"]
[[node]]
id = "R"
parent = ""
title = "Result"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["OWNER"]
acceptance = ["Complete behavior"]
evidence = []
[[node]]
id = "HOLD"
parent = "R"
title = "Explicit owner start"
kind = "gate"
state = "blocked"
order = 1
depends_on = []
mandates = ["OWNER"]
acceptance = ["Wait"]
evidence = []
[[node]]
id = "G"
parent = "R"
title = "Group"
kind = "group"
state = "planned"
order = 2
depends_on = ["HOLD"]
mandates = ["OWNER"]
acceptance = ["Both cases"]
evidence = []
[[node]]
id = "T"
parent = "G"
title = "One case"
kind = "atom"
state = "planned"
order = 3
depends_on = []
mandates = ["OWNER"]
acceptance = ["Required case", "Required refusal"]
evidence = []
contract = "task:G#T"
contract_sha256 = "kept-verbatim"
extension = { exact = [2, 1] }
[[node]]
id = "U"
parent = "G"
title = "Second case"
kind = "atom"
state = "planned"
order = 4
depends_on = ["T"]
mandates = ["OWNER"]
acceptance = ["Other case"]
evidence = []
'''


def task(key):
    return {"id": key, "title": key, "goal": "Behavior", "read_paths": ["input"],
            "write_paths": ["output"], "steps": ["Produce"], "positive_cases": ["Works"],
            "negative_cases": ["Refuses"], "checks": ["SELECT BEFORE DISPATCH: scoped proof"],
            "acceptance": ["Behavior proven"], "safe_stop": "Candidate", "commit_subject": "feat: fixture",
            "notes": ["Preserve me"], "future_extension": {"order": [2, 1]}}


class KernelTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-kernel-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.plan = self.root / "plan.toml"
        self.plan.write_bytes(PLAN.encode())
        self.tasks = self.root / "tasks"
        self.tasks.mkdir()
        self.group = {"id": "G", "tasks": [task("T"), task("U")], "unknown_group_field": "retained in capture"}
        (self.tasks / "G.json").write_text(json.dumps(self.group), encoding="utf-8")
        self.store = self.root / "store"
        self.receipt = M["import_mup"](self.plan, self.tasks, self.store)

    def state(self):
        return M["load_store"](self.store)[0]

    def command(self, kind, payload, event_id=None, base_revision=None):
        revision = self.state()["revision"] if base_revision is None else base_revision
        return {"event_id": event_id or f"event-{revision + 1}", "base_revision": revision,
                "kind": kind, "reason": {"summary": "Fixture decision; not hidden reasoning"}, "payload": payload}

    def record(self, kind, payload, **kwargs):
        return M["record"](self.store, self.command(kind, payload, **kwargs))

    def evidence(self, key="E"):
        return self.record("evidence.recorded", {"id": key, "claim": "Measured hypothesis", "subject": "fixture:bytes:one",
                                                "result": "observed_fail", "artifact_refs": ["fixture:trace"], "node_refs": ["T"]})

    def evaluate(self, **assessment):
        return M["evaluate_stop"](self.state(), {"rules": json.loads(RULES_PATH.read_text(encoding="utf-8")),
                                               "assessment": {"phase": "before_action", **assessment}})

    def test_import_preserves_raw_and_semantic_unknown_fields_without_touching_input(self):
        before = {p: p.read_bytes() for p in [self.plan, self.tasks / "G.json"]}
        state = self.state()
        self.assertEqual(state["plan"], tomllib.loads(PLAN))
        self.assertEqual(state["task_contracts"], {t["id"]: t for t in self.group["tasks"]})
        base = M["parse"]((self.store / "base.json").read_bytes(), tagged=True)
        self.assertEqual(base64.b64decode(base["sources"]["plan"]["raw_base64"]), self.plan.read_bytes())
        self.assertEqual(base64.b64decode(base["sources"]["tasks"][0]["raw_base64"]), (self.tasks / "G.json").read_bytes())
        self.assertEqual(before, {p: p.read_bytes() for p in before})
        self.assertTrue(all(c == {"work_type": "unclassified", "maturity": "unspecified", "assertion_status": "declared"} for c in state["classifications"].values()))
        self.assertEqual(state["execution_mode"], "draft")
        self.assertEqual(M["frontier"](state), [])

    def test_tagged_dates_and_reserved_unknown_mapping_roundtrip(self):
        plan = tomllib.loads(PLAN + '\nrecorded = 1979-05-27T07:32:00Z\nreserved = { "$zap_type" = "literal", value = "keep" }\n')
        self.assertEqual(M["parse"](M["packed"](plan), tagged=True), plan)

    def test_existing_output_and_source_intersection_refuse_before_any_write(self):
        for out in (self.store, self.tasks / "nested-output", self.root):
            with self.subTest(out=out), self.assertRaises(M["Refusal"]):
                M["import_mup"](self.plan, self.tasks, out)

    def test_duplicate_nodes_tasks_and_completion_cycle_are_refused(self):
        plan = tomllib.loads(PLAN)
        plan["node"].append(copy.deepcopy(plan["node"][-1]))
        with self.assertRaises(M["Refusal"]):
            M["validate_plan"](plan)
        nodes = M["validate_plan"](tomllib.loads(PLAN))[0]
        with self.assertRaises(M["Refusal"]):
            M["validate_tasks"]([self.group, self.group], nodes)
        plan = tomllib.loads(PLAN)
        next(n for n in plan["node"] if n["id"] == "T")["depends_on"] = ["G"]
        with self.assertRaisesRegex(M["Refusal"], "cycle"):
            M["validate_plan"](plan)

    def test_replay_and_retry_are_deterministic_and_stale_or_changed_id_refuses(self):
        command = self.command("node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"})
        first = M["record"](self.store, command)
        before = (self.store / "events.jsonl").read_bytes()
        retry = M["record"](self.store, command)
        self.assertTrue(retry["idempotent"])
        self.assertEqual(first["event"], retry["event"])
        self.assertEqual(before, (self.store / "events.jsonl").read_bytes())
        self.assertEqual(M["packed"](self.state()), M["packed"](self.state()))
        self.assertEqual(self.state()["classifications"]["T"]["assertion_status"], "declared")
        self.assertEqual(next(n for n in self.state()["plan"]["node"] if n["id"] == "T")["state"], "planned")
        changed = copy.deepcopy(command)
        changed["payload"]["maturity"] = "functional"
        with self.assertRaisesRegex(M["Refusal"], "another command"):
            M["record"](self.store, changed)
        stale = copy.deepcopy(command)
        stale["event_id"] = "different"
        with self.assertRaisesRegex(M["Refusal"], "base revision"):
            M["record"](self.store, stale)

    def test_agent_cannot_forge_owner_activation_or_acceptance_through_patch(self):
        before = (self.store / "events.jsonl").read_bytes()
        for kind, payload in (("owner.activated", {"actor": "owner"}),
                              ("node.patched", {"node_id": "T", "state": "accepted"}),
                              ("node.classified", {"node_id": "T", "work_type": "change", "maturity": "productized", "owner_contract": {"active": True}})):
            with self.subTest(kind=kind), self.assertRaises(M["Refusal"]):
                self.record(kind, payload)
        self.assertEqual(before, (self.store / "events.jsonl").read_bytes())
        self.assertIsNone(self.state()["owner_contract"])

    def test_fact_and_decision_records_link_sources_without_promoting_truth_or_authority(self):
        self.evidence()
        self.record("knowledge.region-recorded", {"id": "K", "question": "Which ordering is valid?", "node_refs": ["T"]})
        self.record("fact.recorded", {"id": "F", "statement": "Implementation done is merely an assertion", "status": "asserted",
                                      "node_refs": ["T"], "evidence_refs": ["E"], "source_refs": ["fixture:spec"]})
        decision = {"id": "D", "question": "Which approach?", "alternatives": [{"id": "a", "description": "First"}, {"id": "b", "description": "Second"}],
                    "chosen": "a", "rationale": "Bounded experiment", "authority_ref": "owner:claimed-not-authenticated",
                    "consequences": ["Need follow-up proof"], "node_refs": ["T"], "evidence_refs": ["E"]}
        self.record("decision.recorded", decision)
        state = self.state()
        self.assertEqual(state["facts"]["F"]["adjudication"], "unverified")
        self.assertFalse(state["decisions"]["D"]["activates_contract"])
        self.assertTrue(any(r["relation"] == "supported_by" and r["target_id"] == "E" for r in state["relations"]))
        bad = {"id": "bad", "statement": "done", "status": "verified", "node_refs": ["T"], "evidence_refs": [], "source_refs": []}
        with self.assertRaises(M["Refusal"]):
            self.record("fact.recorded", bad)
        decision["id"], decision["node_refs"] = "other", ["MISSING"]
        with self.assertRaises(M["Refusal"]):
            self.record("decision.recorded", decision)

    def test_refinement_is_additive_and_requires_complete_parent_coverage(self):
        state = self.state()
        parent = next(n for n in state["plan"]["node"] if n["id"] == "T")
        child = {**copy.deepcopy(parent), "id": "T.1", "parent": "T", "title": "Bounded child"}
        child.pop("extension")
        payload = {"parent_id": "T", "nodes": [child], "edges": [],
                   "coverage": [{"acceptance_index": 0, "node_ids": ["T.1"]}, {"acceptance_index": 1, "node_ids": ["T.1"]}]}
        missing = copy.deepcopy(payload)
        missing["coverage"].pop()
        with self.assertRaises(M["Refusal"]):
            self.record("plan.refined", missing)
        self.record("plan.refined", payload)
        after = self.state()
        self.assertEqual(next(n for n in after["plan"]["node"] if n["id"] == "T"), parent)
        self.assertEqual(len(after["plan"]["node"]), len(state["plan"]["node"]) + 1)
        self.assertEqual(after["plan"]["revision"], 7)
        self.assertEqual(after["revision"], 1)
        self.assertEqual(M["frontier"](after), [])

    def test_new_refinement_node_cannot_smuggle_authority_or_unknown_patch_domains(self):
        child = {**copy.deepcopy(next(n for n in self.state()["plan"]["node"] if n["id"] == "T")), "id": "T.1", "parent": "T"}
        child.pop("extension")
        for key in ("owner_contract", "stop_rules", "future_extension"):
            payload = {"parent_id": "T", "nodes": [{**child, key: {"active": True}}], "edges": [],
                       "coverage": [{"acceptance_index": 0, "node_ids": ["T.1"]}, {"acceptance_index": 1, "node_ids": ["T.1"]}]}
            with self.subTest(key=key), self.assertRaises(M["Refusal"]):
                self.record("plan.refined", payload)
        self.assertEqual(self.state()["revision"], 0)

    def test_pending_tail_is_visible_and_append_never_truncates_it(self):
        journal = self.store / "events.jsonl"
        with journal.open("ab") as stream:
            stream.write(b'{"seq":1')
        before = journal.read_bytes()
        state, events, pending = M["load_store"](self.store)
        self.assertEqual(state["revision"], 0)
        self.assertEqual(len(events), 1)
        self.assertEqual(pending["bytes"], 8)
        with self.assertRaisesRegex(M["Refusal"], "incomplete final"):
            self.record("knowledge.region-recorded", {"id": "K", "question": "Unknown", "node_refs": ["T"]})
        self.assertEqual(journal.read_bytes(), before)

    def test_corrupt_committed_journal_line_and_base_drift_refuse(self):
        journal = self.store / "events.jsonl"
        original = journal.read_bytes()
        journal.write_bytes(original + b'{"seq":1}\n')
        with self.assertRaises(M["Refusal"]):
            self.state()
        journal.write_bytes(original)
        base = self.store / "base.json"
        base.write_bytes(base.read_bytes() + b" ")
        with self.assertRaisesRegex(M["Refusal"], "import receipt"):
            self.state()

    def test_existing_lock_is_never_stolen(self):
        lock = self.store / "writer.lock"
        lock.write_bytes(b"other owner")
        with self.assertRaisesRegex(M["Refusal"], "writer lock exists"):
            self.record("knowledge.region-recorded", {"id": "K", "question": "Unknown", "node_refs": ["T"]})
        self.assertEqual(lock.read_bytes(), b"other owner")

    def test_stop_three_valued_logic_and_unknown_public_data_effect(self):
        for flag, expected in ((True, "pause"), (False, "clear"), (None, "needs_evidence")):
            with self.subTest(flag=flag):
                result = self.evaluate(changes_public_format=True, migration_affects_user_data=flag)
                self.assertEqual(result["policy_result"], expected)
                self.assertFalse(result["action_admitted"])
        state = self.state()
        unknown = {"eq": {"field": "missing", "value": True}}
        false = {"eq": {"field": "present", "value": True}}
        for expression, expected in (({"not": unknown}, None), ({"all": [unknown, false]}, False),
                                     ({"any": [unknown, false]}, None), ({"any": [unknown, {"not": false}]}, True)):
            self.assertIs(M["expression"](expression, {"present": False}, state)[0], expected)
        with self.assertRaises(M["Refusal"]):
            self.evaluate(changes_public_format="yes", migration_affects_user_data=True)

    def test_failed_approaches_count_semantic_failures_not_retries_or_provider_errors(self):
        self.evidence()
        for key, problem, description in (("A", "T", "First architecture"), ("B", "U", "Other problem")):
            self.record("approach.declared", {"id": key, "problem_id": problem, "description": description})
            self.record("approach.verdict", {"approach_id": key, "outcome": "failed", "evidence_refs": ["E"]})
        self.assertEqual(self.evaluate(changes_public_format=False)["policy_result"], "clear")
        self.record("approach.declared", {"id": "C", "problem_id": "T", "description": "Second architecture"})
        self.record("approach.verdict", {"approach_id": "C", "outcome": "infrastructure_failure", "evidence_refs": []})
        self.assertEqual(self.evaluate(changes_public_format=False)["policy_result"], "clear")
        command = self.command("approach.verdict", {"approach_id": "C", "outcome": "failed", "evidence_refs": ["E"]})
        M["record"](self.store, command)
        M["record"](self.store, command)
        result = self.evaluate(changes_public_format=False, current_problem="different")
        self.assertEqual(result["matched_rules"], ["two-failed-architectural-approaches"])
        self.assertEqual(result["scope"], "run")
        self.assertEqual(result["policy_result"], "pause")
        self.assertEqual(len([a for a in self.state()["approaches"].values() if a["problem_id"] == "T" and a["outcome"] == "failed"]), 2)
        with self.assertRaisesRegex(M["Refusal"], "renamed duplicate"):
            self.record("approach.declared", {"id": "NEW-ID", "problem_id": "T", "description": " FIRST  architecture "})

    def test_after_action_stop_cannot_claim_it_prevented_the_effect(self):
        result = self.evaluate(phase="after_action", changes_public_format=True, migration_affects_user_data=True)
        self.assertEqual(result["policy_result"], "too_late")
        self.assertTrue(result["action_already_occurred"])
        self.assertFalse(result["prevented_action"])
        self.assertFalse(result["owner_contract_activated"])

    def test_unavailable_or_inconclusive_evidence_cannot_confirm_semantic_failure(self):
        self.record("approach.declared", {"id": "A", "problem_id": "T", "description": "Architecture"})
        for result in ("unavailable", "inconclusive"):
            self.record("evidence.recorded", {"id": result, "claim": "No usable observation", "subject": "fixture",
                                             "result": result, "artifact_refs": [], "node_refs": ["T"]})
            for outcome in ("failed", "succeeded"):
                with self.subTest(result=result, outcome=outcome), self.assertRaisesRegex(M["Refusal"], "observed result"):
                    self.record("approach.verdict", {"approach_id": "A", "outcome": outcome, "evidence_refs": [result]})
        self.assertEqual(self.state()["approaches"]["A"]["outcome"], "unresolved")
        self.assertEqual(self.evaluate(changes_public_format=False)["matched_rules"], [])
        self.record("evidence.recorded", {"id": "counterexample", "claim": "A successful probe refutes the design", "subject": "fixture",
                                         "result": "observed_pass", "artifact_refs": ["fixture:counterexample"], "node_refs": ["T"]})
        self.record("approach.verdict", {"approach_id": "A", "outcome": "failed", "evidence_refs": ["counterexample"]})
        self.assertEqual(self.state()["approaches"]["A"]["outcome"], "failed")

    def test_cli_queries_are_readonly_and_failures_are_machine_readable(self):
        before = {p.name: p.read_bytes() for p in self.store.iterdir()}
        for args in (["inspect", "--store", str(self.store)], ["events", "--store", str(self.store), "--after", "0"]):
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = M["main"](args)
            self.assertEqual(code, 0)
            self.assertTrue(json.loads(stdout.getvalue())["ok"])
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.store.iterdir()})
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = M["main"](["activate", "--as-owner"])
        self.assertEqual(code, 2)
        self.assertFalse(json.loads(stdout.getvalue())["dispatch_allowed"])

    def test_future_event_cursor_is_a_gap_not_an_empty_success(self):
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = M["main"](["events", "--store", str(self.store), "--after", "100"])
        self.assertEqual(code, 2)
        self.assertEqual(json.loads(stdout.getvalue())["code"], "CURSOR")


if __name__ == "__main__":
    unittest.main()
