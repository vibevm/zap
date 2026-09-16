"""Atomic import and non-destructive migration-report tests."""
from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from zaplib.common import Refusal, parse, sha
from zaplib.migration import migrate_mup
from zaplib.storage import import_mup, load_store
from test_knowledge_support import create_inputs


class MigrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zap-migration-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_migration_preserves_sources_constraints_unknown_fields_and_holds_execution(self):
        plan, tasks = create_inputs(self.root)
        sources = [plan, tasks / "G.json"]
        before = {path: path.read_bytes() for path in sources}
        store = self.root / "store"
        result = migrate_mup(plan, tasks, store)
        self.assertTrue(result["source_unchanged"])
        self.assertFalse(result["charter_activated"])
        self.assertFalse(result["commands_executed"])
        self.assertEqual(before, {path: path.read_bytes() for path in sources})
        report = parse(Path(result["migration_report"]).read_bytes(), tagged=True)
        self.assertEqual(report["preservation"]["node_count"], 4)
        self.assertEqual(report["preservation"]["task_count"], 2)
        unknown = {row["path"] for row in report["unknown_fields"]}
        self.assertIn("plan.future_top", unknown)
        self.assertIn("plan.node[2].future_node", unknown)
        self.assertIn("task_contracts.T.future_task", unknown)
        self.assertIn("sources.tasks[0].future_group", unknown)
        node_t = next(row for row in report["mapping"]["nodes"] if row["source_id"] == "T")
        self.assertEqual(node_t["constraints"]["acceptance"], ["Proof"])
        self.assertEqual(load_store(store)[0]["execution_mode"], "draft")

    def test_atomic_import_crash_points_never_publish_a_partial_store(self):
        for point in ("after_base", "after_events"):
            case = self.root / point
            case.mkdir()
            plan, tasks = create_inputs(case)
            out = case / "store"

            def fail(observed, wanted=point):
                if observed == wanted:
                    raise RuntimeError(wanted)

            with self.subTest(point=point), self.assertRaises(RuntimeError):
                import_mup(plan, tasks, out, fault=fail)
            self.assertFalse(out.exists())
            self.assertEqual(list(case.glob(".store.import-*")), [])

        published = self.root / "after_publish"
        published.mkdir()
        plan, tasks = create_inputs(published)
        out = published / "store"

        def fail_after_publish(point):
            if point == "after_publish":
                raise RuntimeError(point)

        with self.assertRaises(RuntimeError):
            import_mup(plan, tasks, out, fault=fail_after_publish)
        state, events, pending = load_store(out)
        self.assertEqual((state["revision"], len(events), pending), (0, 1, None))
        self.assertEqual(sha((out / "base.json").read_bytes()), state["base_sha256"])

    def test_migration_resumes_exact_store_and_report_without_overwrite(self):
        case = self.root / "resume"
        case.mkdir()
        plan, tasks = create_inputs(case)
        out = case / "store"

        def crash(point):
            if point == "after_import":
                raise RuntimeError(point)

        with self.assertRaises(RuntimeError):
            migrate_mup(plan, tasks, out, fault=crash)
        self.assertTrue(out.is_dir())
        self.assertFalse((out / "migration-report.json").exists())
        recovered = migrate_mup(plan, tasks, out)
        self.assertTrue(recovered["store_recovered"])
        self.assertTrue(recovered["report_created"])
        report_bytes = (out / "migration-report.json").read_bytes()
        repeated = migrate_mup(plan, tasks, out)
        self.assertTrue(repeated["store_recovered"])
        self.assertFalse(repeated["report_created"])
        self.assertEqual((out / "migration-report.json").read_bytes(), report_bytes)

    def test_migration_detects_source_set_change_and_aba_capture(self):
        added = self.root / "added"
        added.mkdir()
        plan, tasks = create_inputs(added)

        def add_source(point):
            if point == "after_import":
                (tasks / "late.json").write_text("{}", encoding="utf-8")

        with self.assertRaisesRegex(Refusal, "path-set"):
            migrate_mup(plan, tasks, added / "store", fault=add_source)

        aba = self.root / "aba"
        aba.mkdir()
        plan, tasks = create_inputs(aba)
        task_path = tasks / "G.json"
        original = task_path.read_bytes()
        changed = original.replace(b'"future_group": "raw"', b'"future_group": "changed"')

        def aba_source(point):
            if point == "after_source_snapshot":
                task_path.write_bytes(changed)
            elif point == "after_import":
                task_path.write_bytes(original)

        with self.assertRaisesRegex(Refusal, "capture does not match"):
            migrate_mup(plan, tasks, aba / "store", fault=aba_source)
        self.assertEqual(task_path.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
