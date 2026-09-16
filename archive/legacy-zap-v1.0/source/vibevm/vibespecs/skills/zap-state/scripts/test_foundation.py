"""Compatibility and extension-boundary tests for the modular ZAP foundation."""
from __future__ import annotations

import base64
import contextlib
import hashlib
import importlib
import io
import json
from pathlib import Path
import runpy
import tempfile
import tomllib
import unittest

M = runpy.run_path(str(Path(__file__).with_name("zap_state.py")))
Z = importlib.import_module("zaplib")

PLAN = '''schema = 1
plan_id = "foundation"
revision = 4
root_node = "R"
current_node = "T"
future_plan_field = { retain = [2, 1] }
[[mandate]]
id = "M"
text = "Keep it"
disposition = "owned"
nodes = ["R", "G", "T"]
[[node]]
id = "R"
parent = ""
title = "Root"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["M"]
acceptance = ["Done"]
evidence = []
[[node]]
id = "G"
parent = "R"
title = "Group"
kind = "group"
state = "planned"
order = 1
depends_on = []
mandates = ["M"]
acceptance = ["Done"]
evidence = []
[[node]]
id = "T"
parent = "G"
title = "Task"
kind = "atom"
state = "planned"
order = 2
depends_on = []
mandates = ["M"]
acceptance = ["Done"]
evidence = []
future_node_field = { retain = true }
'''


def task() -> dict:
    return {"id": "T", "title": "Task", "goal": "Goal", "read_paths": ["in"], "write_paths": ["out"],
            "steps": ["step"], "positive_cases": ["yes"], "negative_cases": ["no"], "checks": ["check"],
            "acceptance": ["accept"], "safe_stop": "stop", "commit_subject": "feat: test", "notes": ["note"],
            "future_task_field": {"retain": [2, 1]}}


def legacy_packed(value) -> bytes:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True, allow_nan=False).encode("utf-8")


class FoundationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-foundation-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.plan = self.root / "plan.toml"
        self.plan.write_bytes(PLAN.encode("utf-8"))
        self.tasks = self.root / "tasks"
        self.tasks.mkdir()
        self.group = {"id": "G", "tasks": [task()], "future_group_field": "raw-only"}
        self.task_raw = json.dumps(self.group, ensure_ascii=False, indent=2).encode("utf-8")
        self.task_path = self.tasks / "G.json"
        self.task_path.write_bytes(self.task_raw)
        self.store = self.root / "store"
        M["import_mup"](self.plan, self.tasks, self.store)

    def expected_base(self) -> dict:
        plan_raw = self.plan.read_bytes()
        capture_plan = {"path": str(self.plan.resolve()), "sha256": hashlib.sha256(plan_raw).hexdigest(),
                        "raw_base64": base64.b64encode(plan_raw).decode("ascii")}
        capture_task = {"path": str(self.task_path.resolve()), "sha256": hashlib.sha256(self.task_raw).hexdigest(),
                        "raw_base64": base64.b64encode(self.task_raw).decode("ascii")}
        return {"schema": "zap/1", "plan": tomllib.loads(PLAN), "task_contracts": {"T": task()},
                "sources": {"plan": capture_plan, "tasks": [capture_task]}}

    def command(self, kind: str, payload: dict, revision: int = 0, event_id: str = "extension-1") -> dict:
        return {"event_id": event_id, "base_revision": revision, "kind": kind,
                "reason": {"summary": "Recorded deterministic effect"}, "payload": payload}

    def test_compatibility_shim_exports_complete_legacy_helper_surface(self):
        helpers = {"Refusal", "need", "exact", "string", "strings", "identity", "wire", "unwire", "packed", "parse", "sha",
                   "acyclic", "validate_plan", "validate_tasks", "safe_path", "write_new", "capture", "import_mup", "initial_state",
                   "refs", "relations", "add_record", "refine", "apply_command", "load_store", "writer_lock", "record", "frontier",
                   "expression", "evaluate_stop", "JsonParser", "main"}
        self.assertEqual(helpers - M.keys(), set())

    def test_import_base_bytes_match_legacy_encoding_and_raw_captures_exactly(self):
        expected = legacy_packed(self.expected_base()) + b"\n"
        actual = (self.store / "base.json").read_bytes()
        self.assertEqual(actual, expected)
        self.assertEqual(hashlib.sha256(actual).hexdigest(), M["load_store"](self.store)[0]["base_sha256"])
        base = M["parse"](actual, tagged=True)
        self.assertEqual(base64.b64decode(base["sources"]["plan"]["raw_base64"]), self.plan.read_bytes())
        self.assertEqual(base64.b64decode(base["sources"]["tasks"][0]["raw_base64"]), self.task_raw)
        self.assertEqual(base["plan"]["future_plan_field"], {"retain": [2, 1]})
        self.assertEqual(base["task_contracts"]["T"]["future_task_field"], {"retain": [2, 1]})

    def test_manually_formed_legacy_store_replays_without_migration(self):
        legacy = self.root / "legacy"
        legacy.mkdir()
        base_raw = legacy_packed(self.expected_base()) + b"\n"
        (legacy / "base.json").write_bytes(base_raw)
        receipt = {"seq": 0, "revision": 0, "previous_revision": None, "event_id": "legacy-import",
                   "kind": "store.imported", "base_sha256": hashlib.sha256(base_raw).hexdigest()}
        (legacy / "events.jsonl").write_bytes(legacy_packed(receipt) + b"\n")
        state, events, pending = Z.load_store(legacy)
        self.assertEqual((state["schema"], state["revision"], len(events), pending), ("zap/1", 0, 1, None))
        self.assertEqual(state["projection_schema"], "zap-projection/1")
        self.assertEqual(state["capabilities"], {"zap.core": 1, "zap.extensions": 1})
        self.assertEqual(state["extensions"], {})
        before = (legacy / "base.json").read_bytes()
        Z.record(legacy, self.command("node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"}))
        self.assertEqual((legacy / "base.json").read_bytes(), before)
        self.assertEqual(Z.load_store(legacy)[0]["classifications"]["T"]["maturity"], "prototype")

    def test_explicit_extension_registry_replays_and_receives_detached_state_payload(self):
        seen = {}

        def apply_marker(state, payload, event_id):
            seen["payload"] = payload
            state["extensions"].setdefault("runtime", {})["marker"] = {"value": payload["value"], "event_id": event_id}
            state["execution_mode"] = "extension_active"
            payload["value"] = "handler-local-mutation"

        spec = Z.HandlerSpec("runtime.marker-recorded", Z.strict_payload({"value"}), apply_marker)
        handlers = Z.compose_handlers(Z.CORE_HANDLERS, spec)
        command = self.command(spec.kind, {"value": "kept"})
        state = Z.load_store(self.store)[0]
        after = Z.apply_command(state, command, handlers)
        self.assertEqual(state["extensions"], {})
        self.assertEqual(command["payload"], {"value": "kept"})
        self.assertEqual(after["extensions"]["runtime"]["marker"], {"value": "kept", "event_id": "extension-1"})
        result = Z.record(self.store, command, handlers)
        self.assertEqual(result["execution_mode"], "extension_active")
        replayed = Z.load_store(self.store, handlers)[0]
        self.assertEqual(replayed["extensions"]["runtime"]["marker"]["value"], "kept")
        self.assertEqual(replayed["execution_mode"], "extension_active")
        retry = Z.record(self.store, command, handlers)
        self.assertTrue(retry["idempotent"])
        self.assertEqual(retry["execution_mode"], "extension_active")
        with self.assertRaises(Z.Refusal):
            Z.apply_command(state, self.command(spec.kind, {"value": "x", "extra": True}), handlers)
        self.assertEqual(seen["payload"]["value"], "handler-local-mutation")

    def test_every_extension_effect_is_followed_by_global_graph_validation(self):
        def smuggle_acceptance(state, _payload, _event_id):
            next(node for node in state["plan"]["node"] if node["id"] == "T")["state"] = "accepted"

        handlers = Z.compose_handlers(Z.CORE_HANDLERS, Z.HandlerSpec("runtime.invalid-acceptance", Z.strict_payload(set()), smuggle_acceptance))
        before = Z.load_store(self.store)[0]
        with self.assertRaisesRegex(Z.Refusal, "accepted node lacks"):
            Z.apply_command(before, self.command("runtime.invalid-acceptance", {}), handlers)
        self.assertEqual(next(node for node in before["plan"]["node"] if node["id"] == "T")["state"], "planned")

    def test_deep_valid_plan_uses_iterative_graph_validation(self):
        count = 1100
        ids = ["R"] + [f"N{i}" for i in range(1, count)]
        nodes = []
        for index, key in enumerate(ids):
            nodes.append({"id": key, "parent": "" if index == 0 else ids[index - 1], "title": key,
                          "kind": "campaign" if index == 0 else "group", "state": "planned", "order": index,
                          "depends_on": [], "mandates": ["M"], "acceptance": ["done"], "evidence": []})
        plan = {"schema": 1, "plan_id": "deep", "revision": 0, "root_node": "R", "current_node": ids[-1],
                "mandate": [{"id": "M", "text": "Deep", "disposition": "owned", "nodes": ids}], "node": nodes}
        self.assertEqual(len(Z.validate_plan(plan)[0]), count)

    def test_cli_extension_is_explicit_and_machine_readable(self):
        def configure(parser):
            parser.add_argument("--value", required=True)

        extension = Z.CliExtension("echo-extension", configure, lambda args, context: {"ok": True, "value": args.value, "handler_count": len(context.handlers)})
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = Z.main(["echo-extension", "--value", "visible"], extensions=[extension])
        result = json.loads(stdout.getvalue())
        self.assertEqual((code, result["ok"], result["value"]), (0, True, "visible"))
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = Z.main(["missing-extension"])
        self.assertEqual((code, json.loads(stdout.getvalue())["code"]), (2, "ARGUMENT"))


if __name__ == "__main__":
    unittest.main()
