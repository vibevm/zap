"""Real-process tests for adaptive review job reconciliation."""
from __future__ import annotations

from pathlib import Path
import tempfile
import time
import unittest

from zaplib.common import Refusal, packed, sha
from zaplib.domain import current_acceptance_coverage, domain_state
from zaplib.knowledge import knowledge_snapshot
from zaplib.records import compose_handlers
from zaplib.runtime import AutomaticCoordinator, runtime_state
from zaplib.runtime_reconciliation import (
    RECONCILIATION_ACTION_KINDS, RECONCILIATION_DATA_KINDS, RECONCILIATION_HANDLERS,
    RECONCILIATION_EVENT_ROUTES, RECONCILIATION_EVENT_SCHEMAS, RECONCILIATION_OBSERVATION_KINDS,
    reconcile_applied_reviews, reconciliation_blocks_progress,
)
from zaplib.service import ApplicationService
from test_runtime_support import RuntimeFixture, ScriptedSemanticAdapter, tick_until


class HoldAcceptanceAdapter(ScriptedSemanticAdapter):
    def poll(self, request_id, request):
        if request["request_kind"] != "acceptance":
            return super().poll(request_id, request)
        response = {"schema": "zap-coordinator-response/1", "request_id": request_id,
            "request_kind": "acceptance", "state_revision": request["state_revision"],
            "request_sha256": sha(packed(request)), "disposition": "needs_evidence",
            "rationale": "Fixture leaves fresh proof for the trusted test", "selection": None, "commands": []}
        return {"ready": True, "ok": True, "response": response, "response_sha256": sha(packed(response))}


def add_reconciliation(fixture):
    if not set(RECONCILIATION_HANDLERS) <= set(fixture.handlers):
        fixture.handlers = compose_handlers(fixture.handlers, RECONCILIATION_HANDLERS)
    prior = fixture.service
    fixture.service = ApplicationService(
        fixture.store, fixture.handlers, prior.trust, prior.host_principal,
        action_kinds={**dict(prior.action_kinds), **dict(RECONCILIATION_ACTION_KINDS)},
        data_kinds=set(prior.data_kinds) | set(RECONCILIATION_DATA_KINDS),
        observation_kinds=set(prior.observation_kinds) | set(RECONCILIATION_OBSERVATION_KINDS),
    )


def applied_review(fixture, actions, *, work_changes=(), review_id="RECON-1"):
    state = fixture.state(); domain = domain_state(state); runtime = runtime_state(state)
    jobs = [runtime["jobs"][job_id] for job_id in sorted(actions)]
    captures = sorted({capture["source_id"]: capture for job in jobs for capture in job["source_captures"]}.values(),
                      key=lambda row: row["source_id"])
    payload = {"schema": "zap-domain/review-proposed/1", "review_id": review_id,
        "previous_review_id": domain["last_applied_review_id"], "signals": ["job disposition required"],
        "captures": {"base_sha256": state["base_sha256"], "zap_revision": state["revision"],
            "domain_revision": domain["revision"], "intent_id": domain["active_intent_id"],
            "outcome_id": domain["active_outcome_id"], "policy_revision": 1,
            "source_captures": captures, "jobs": [{"job_id": job["job_id"], "status": job["state"],
                                                     "attempt_id": job["attempt_id"]} for job in jobs]},
        "knowledge": {"before": None if domain["last_applied_review_id"] is None else {
                "review_id": domain["last_applied_review_id"],
                "sha256": domain["reviews"][domain["last_applied_review_id"]]["knowledge"]["after"]["sha256"]},
            "after": {"region_ids": [], **knowledge_snapshot(state, [])}, "new_region_ids": [],
            "affected_dependencies": [], "closure_complete": True},
        "alternatives": [{"id": "chosen", "description": "Apply exact job disposition", "value": "preserves value",
            "feasibility": "verified", "remaining_cost": "bounded", "risks": [], "unknowns": []}],
        "chosen": "chosen", "decision": {"kind": "replace_method", "rationale": "captured job needs reconciliation"},
        "transition": {"intent_id": None, "outcome_id": None, "obligation_dispositions": [],
            "ownership_changes": [], "work_changes": list(work_changes), "preserved_evidence_ids": [],
            "preserved_stage_acceptance_ids": [], "preserved_work_acceptance_ids": [],
            "preserved_integration_acceptance_ids": [], "job_reconciliation": [{"job_id": job_id,
                "action": action, "safe_boundary": "worker checkpoint", "reason": f"test {action}"}
                for job_id, action in sorted(actions.items())], "tradeoffs": [], "preserved_benefits": ["owner value"]},
        "next_trigger": "reconciliation complete"}
    fixture._data("domain.review-proposed", payload, "Propose exact job reconciliation")
    fixture._apply("domain.review-applied", {"schema": "zap-domain/review-applied/1", "review_id": review_id,
                    "expected_domain_revision": domain_state(fixture.state())["revision"]},
                   "adaptive.apply", "Apply exact job reconciliation", captures)
    return review_id


def stop_transport_jobs(fixture):
    if not (Path(fixture.store) / "base.json").exists():
        return
    runtime = runtime_state(fixture.state())
    for job_id, job in runtime["jobs"].items():
        if job["state"] in {"accepted", "retry_released", "result_ready"}:
            continue
        try:
            fixture.transport.request_stop(job_id, f"cleanup:{job_id}")
        except (OSError, Refusal):
            pass
    for _ in range(80):
        pending = []
        for job_id in runtime["jobs"]:
            try:
                if fixture.transport.reconcile(job_id).get("state") not in {"succeeded", "failed", "stopped", "interrupted"}:
                    pending.append(job_id)
            except (OSError, Refusal):
                pass
        if not pending:
            return
        time.sleep(0.02)


class RuntimeReconciliationTests(unittest.TestCase):
    def coordinator(self, fixture):
        add_reconciliation(fixture)
        self.addCleanup(stop_transport_jobs, fixture)
        return AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service,
                                    fixture.transport, fixture.semantic, fixture.config)

    def test_registry_routes_and_schemas_are_exact(self):
        self.assertEqual(set(RECONCILIATION_HANDLERS), set(RECONCILIATION_EVENT_SCHEMAS))
        self.assertEqual(set(RECONCILIATION_HANDLERS), set(RECONCILIATION_EVENT_ROUTES))
        for schema in RECONCILIATION_EVENT_SCHEMAS.values():
            self.assertFalse(schema["additionalProperties"])
            self.assertEqual(set(schema["required"]), set(schema["properties"]))

    def test_automatic_tick_executes_the_composed_reconciliation_hook(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-hook-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.5)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            review_id = applied_review(fixture, {job_id: "continue"})
            result = coordinator.tick()
            self.assertEqual("complete", runtime_state(fixture.state())["reconciliations"][review_id]["state"])
            self.assertTrue(any(row.get("kind") == "reconciliation_planned" for row in result["actions"]))
            self.assertFalse(reconciliation_blocks_progress(fixture.state(), job_id))
            stop_transport_jobs(fixture)

    def test_continue_finish_and_campaign_pause_precedence(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-continue-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=2.0)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: len(runtime_state(fixture.state())["jobs"]) == 2 and
                       all(job["state"] in {"running", "starting", "dispatched"}
                           for job in runtime_state(fixture.state())["jobs"].values()), limit=80)
            jobs = sorted(runtime_state(fixture.state())["jobs"])
            review_id = applied_review(fixture, {jobs[0]: "continue", jobs[1]: "finish_compatible"})
            actions = []; reconcile_applied_reviews(coordinator, actions)
            plan = runtime_state(fixture.state())["reconciliations"][review_id]
            self.assertEqual("complete", plan["state"])
            self.assertTrue(all(not fixture.transport.status(job_id)["stop"]["requested"] for job_id in jobs))
            stop_transport_jobs(fixture)

        with tempfile.TemporaryDirectory(prefix="zap-reconcile-pause-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.5)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            review_id = applied_review(fixture, {job_id: "continue"})
            fixture.owner_stop("PAUSE-FIRST")
            actions = []; reconcile_applied_reviews(coordinator, actions)
            item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job_id]
            self.assertEqual("pending", item["status"])
            self.assertFalse(any(row["kind"] == "job_reconciliation" for row in actions))
            stop_transport_jobs(fixture)

    def test_cooperative_drain_verifies_safe_state_and_does_not_stop_peer(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-drain-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=10.0, safe_verifier=True)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: len(runtime_state(fixture.state())["jobs"]) == 2 and
                       all(job["state"] in {"running", "starting", "dispatched"}
                           for job in runtime_state(fixture.state())["jobs"].values()), limit=80)
            jobs = sorted(runtime_state(fixture.state())["jobs"]); drained, peer = jobs
            review_id = applied_review(fixture, {drained: "drain"})
            self.assertTrue(reconciliation_blocks_progress(fixture.state(), drained))
            reconcile_applied_reviews(coordinator, [])
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service,
                                               fixture.transport, fixture.semantic, fixture.config)
            for _ in range(100):
                actions = []; coordinator._reconcile_work_jobs(actions); reconcile_applied_reviews(coordinator, actions)
                if runtime_state(fixture.state())["reconciliations"][review_id]["state"] == "complete":
                    break
                time.sleep(0.02)
            plan = runtime_state(fixture.state())["reconciliations"][review_id]
            self.assertEqual("complete", plan["state"])
            history = plan["items"][drained]["history"]
            modes = [row["transport"]["mode"] for row in history
                     if (row.get("transport") or {}).get("schema") == "zap-transport-stop/1"]
            self.assertEqual(["cooperative"], sorted(set(modes)))
            self.assertIn(history[-1]["safe_state"]["status"], {"safe", "completed"})
            self.assertEqual(1, fixture.transport.status(drained)["stop"]["request_count"])
            self.assertFalse(fixture.transport.status(peer)["stop"]["requested"])
            stop_transport_jobs(fixture)

    def test_terminal_process_without_task_proof_still_needs_reconciliation(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-no-proof-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.05, safe_verifier=False)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            for _ in range(80):
                coordinator._reconcile_work_jobs([])
                if runtime_state(fixture.state())["jobs"][job_id]["state"] == "result_ready":
                    break
                time.sleep(0.02)
            self.assertEqual("result_ready", runtime_state(fixture.state())["jobs"][job_id]["state"])
            review_id = applied_review(fixture, {job_id: "drain"})
            reconcile_applied_reviews(coordinator, [])
            item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job_id]
            self.assertEqual("needs_reconcile", item["status"])
            self.assertEqual("needs_reconcile", item["history"][-1]["safe_state"]["status"])
            self.assertFalse(item["history"][-1]["transport"]["delivery"]["delivered"])
            self.assertFalse(item["history"][-1]["transport"]["termination"]["sent"])

        with tempfile.TemporaryDirectory(prefix="zap-reconcile-no-safe-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.1, safe_verifier=False)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            for _ in range(80):
                coordinator._reconcile_work_jobs([])
                if runtime_state(fixture.state())["jobs"][job_id]["state"] == "result_ready":
                    break
                time.sleep(0.02)
            review_id = applied_review(fixture, {job_id: "drain"})
            reconcile_applied_reviews(coordinator, [])
            item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job_id]
            self.assertEqual("needs_reconcile", item["status"])
            self.assertEqual("needs_reconcile", item["history"][-1]["safe_state"]["status"])
            self.assertFalse((item["history"][-1]["transport"].get("termination") or {}).get("sent", False))

    def test_preserve_candidate_keeps_same_attempt_and_refuses_changed_source(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-candidate-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.1)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            for _ in range(80):
                coordinator._reconcile_work_jobs([])
                if runtime_state(fixture.state())["jobs"][job_id]["state"] == "result_ready":
                    break
                time.sleep(0.02)
            job = runtime_state(fixture.state())["jobs"][job_id]
            self.assertEqual("result_ready", job["state"])
            review_id = applied_review(fixture, {job["job_id"]: "preserve_candidate"})
            actions = []; reconcile_applied_reviews(coordinator, actions)
            item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job["job_id"]]
            self.assertEqual("complete", item["status"])
            self.assertEqual(job["attempt_id"], runtime_state(fixture.state())["jobs"][job["job_id"]]["attempt_id"])
            stop_transport_jobs(fixture)

    def test_preserve_candidate_records_failed_and_blocked_output_as_rework_only(self):
        scripts = {
            "blocked": "import json; print(json.dumps({'schema':'zap-worker-candidate/1','status':'blocked'}))",
            "failed": "raise SystemExit(3)",
        }
        for label, script in scripts.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory(
                    prefix=f"zap-reconcile-{label}-", ignore_cleanup_errors=True) as temporary:
                fixture = RuntimeFixture(Path(temporary), worker_seconds=0.05)
                fixture.worker_script.write_text(script, encoding="utf-8")
                coordinator = self.coordinator(fixture)
                tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
                job_id = next(iter(runtime_state(fixture.state())["jobs"]))
                for _ in range(80):
                    coordinator._reconcile_work_jobs([])
                    if runtime_state(fixture.state())["jobs"][job_id]["state"] == "result_ready":
                        break
                    time.sleep(0.02)
                job = runtime_state(fixture.state())["jobs"][job_id]
                self.assertEqual("result_ready", job["state"])
                review_id = applied_review(fixture, {job_id: "preserve_candidate"}, review_id=f"RECON-{label}")
                actions = []; reconcile_applied_reviews(coordinator, actions)
                item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job_id]
                self.assertEqual("needs_reconcile", item["status"])
                self.assertEqual("result_ready", runtime_state(fixture.state())["jobs"][job_id]["state"])
                self.assertIn("rework artifact", item["history"][-1]["detail"])

    def test_pause_after_revalidation_release_defers_readiness_across_restart(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-release-pause-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.05, safe_verifier=True)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job_id = next(iter(runtime_state(fixture.state())["jobs"]))
            for _ in range(80):
                coordinator._reconcile_work_jobs([])
                if runtime_state(fixture.state())["jobs"][job_id]["state"] == "result_ready":
                    break
                time.sleep(0.02)
            job = runtime_state(fixture.state())["jobs"][job_id]
            review_id = applied_review(fixture, {job_id: "revalidate"}, work_changes=[{
                "work_id": job["work_id"], "operation": "revalidate", "order": None,
                "successor_ids": [], "reason": "pause race fixture"}], review_id="RECON-PAUSE")
            reconcile_applied_reviews(coordinator, [])
            reconcile_applied_reviews(coordinator, [])
            self.assertEqual("released", runtime_state(fixture.state())["reconciliations"][review_id]["items"][job_id]["status"])
            fixture.owner_stop("PAUSE-AFTER-RELEASE")
            coordinator = AutomaticCoordinator(fixture.store, fixture.handlers, fixture.service,
                                               fixture.transport, fixture.semantic, fixture.config)
            actions = []; reconcile_applied_reviews(coordinator, actions)
            state = fixture.state(); domain = domain_state(state)
            self.assertEqual("released", runtime_state(state)["reconciliations"][review_id]["items"][job_id]["status"])
            self.assertEqual(0, domain.get("validation_generations", {}).get(job["work_id"], 0))
            self.assertNotEqual("ready", domain["work_updates"][job["work_id"]]["state"])
            self.assertFalse(any(row.get("kind") == "revalidation_released" for row in actions))

        with tempfile.TemporaryDirectory(prefix="zap-reconcile-stale-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.5)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: bool(runtime_state(fixture.state())["jobs"]), limit=30)
            job = next(iter(runtime_state(fixture.state())["jobs"].values()))
            review_id = applied_review(fixture, {job["job_id"]: "continue"})
            source = fixture.state()["extensions"]["knowledge"]["sources"]["S"]
            fixture._apply("knowledge.source-observed", {"source_id": "S", "observed": {
                "status": "changed", "sha256": "f" * 64, "bytes": source["bytes"], "detail": None}},
                "evidence.adjudicate", "Observe changed source", [{"source_id": "S", "sha256": source["content_sha256"]}])
            actions = []; reconcile_applied_reviews(coordinator, actions)
            item = runtime_state(fixture.state())["reconciliations"][review_id]["items"][job["job_id"]]
            self.assertEqual("needs_reconcile", item["status"])
            stop_transport_jobs(fixture)

    def test_accepted_work_revalidates_with_fresh_process_and_generation_bound_proof(self):
        with tempfile.TemporaryDirectory(prefix="zap-reconcile-revalidate-", ignore_cleanup_errors=True) as temporary:
            fixture = RuntimeFixture(Path(temporary), worker_seconds=0.1, safe_verifier=True)
            coordinator = self.coordinator(fixture)
            tick_until(coordinator, lambda _row: {"T", "U"} <= set(current_acceptance_coverage(fixture.state())["work_ids"]),
                       limit=100)
            state = fixture.state(); domain = domain_state(state)
            old_acceptance = next(row for row in domain["acceptances"].values() if row["work_id"] == "T")
            old_evidence = old_acceptance["evidence_ids"][0]
            old_job = next(job for job in runtime_state(state)["jobs"].values() if job["work_id"] == "T")
            review_id = applied_review(fixture, {old_job["job_id"]: "revalidate"}, work_changes=[{
                "work_id": "T", "operation": "revalidate", "order": None, "successor_ids": [],
                "reason": "fresh validation generation required"}])
            for _ in range(6):
                actions = []; reconcile_applied_reviews(coordinator, actions)
            state = fixture.state(); domain = domain_state(state)
            self.assertEqual(1, domain["validation_generations"]["T"])
            self.assertEqual("ready", domain["work_updates"]["T"]["state"])
            self.assertNotIn("T", current_acceptance_coverage(state)["work_ids"])
            with self.assertRaisesRegex(Refusal, "validation generation"):
                fixture._apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1",
                    "stage_acceptance_id": "old-proof-stage", "work_id": "T", "stage": "functional",
                    "outcome_id": domain["active_outcome_id"], "evidence_ids": [old_evidence],
                    "obligation_ids": sorted(old_acceptance["obligation_ids"]), "scope": "stale proof",
                    "summary": "must refuse"}, "stage.accept", "Try old validation proof")

            hold = HoldAcceptanceAdapter(); coordinator.semantic = hold; fixture.semantic = hold
            prior_jobs = set(runtime_state(fixture.state())["jobs"])
            tick_until(coordinator, lambda _row: any(job_id not in prior_jobs and job["work_id"] == "T"
                       and job["state"] == "review_pending" for job_id, job in runtime_state(fixture.state())["jobs"].items()),
                       limit=30)
            state = fixture.state(); runtime = runtime_state(state)
            fresh_job = next(job for job_id, job in runtime["jobs"].items() if job_id not in prior_jobs and job["work_id"] == "T")
            verification = next(row for row in runtime["verification_jobs"].values()
                                if row["work_job_id"] == fresh_job["job_id"] and row["state"] == "observed")
            evidence_id = verification["evidence_id"]; obligations = sorted(old_acceptance["obligation_ids"])
            source = state["extensions"]["knowledge"]["sources"]["S"]
            capture = [{"source_id": "S", "sha256": source["content_sha256"]}]
            plan = verification["plan"]
            fixture._apply("domain.evidence-adjudicated", {"schema": "zap-domain/evidence-adjudicated/1",
                "evidence_id": evidence_id, "expected_revision": -1, "disposition": "accepted",
                "applies_to": {"outcome_id": domain["active_outcome_id"], "obligation_ids": obligations,
                    "work_ids": ["T"], "stage": "functional", "scope": "fresh generation"},
                "source_refs": plan["source_refs"], "method": {key: plan[key] for key in
                    ("argv", "target", "toolchain", "environment", "subjects", "cases")}, "limitations": []},
                "evidence.adjudicate", "Adjudicate fresh proof", capture)
            fixture._apply("domain.stage-accepted", {"schema": "zap-domain/stage-accepted/1",
                "stage_acceptance_id": "stage:T:g1", "work_id": "T", "stage": "functional",
                "outcome_id": domain["active_outcome_id"], "evidence_ids": [evidence_id],
                "obligation_ids": obligations, "scope": "fresh generation", "summary": "fresh stage"},
                "stage.accept", "Accept fresh stage")
            fixture._apply("domain.work-accepted", {"schema": "zap-domain/work-accepted/1",
                "acceptance_id": "acceptance:T:g1", "work_id": "T", "outcome_id": domain["active_outcome_id"],
                "stage_acceptance_id": "stage:T:g1", "evidence_ids": [evidence_id],
                "obligation_ids": obligations, "integration_acceptance_ids": [], "summary": "fresh acceptance"},
                "work.accept", "Accept fresh generation")
            state = fixture.state(); domain = domain_state(state)
            self.assertIn("T", current_acceptance_coverage(state)["work_ids"])
            self.assertEqual(1, domain["acceptances"]["acceptance:T:g1"]["validation_generation"])
            self.assertEqual(review_id, domain["revalidation_history"][-1]["review_id"])


if __name__ == "__main__":
    unittest.main()
