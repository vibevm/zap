"""Persistent automatic coordinator: reconcile, review, select, run, and accept."""
from __future__ import annotations

import threading
import time
from typing import Any, Callable, Mapping

from .common import Refusal, need, packed, sha
from .control import active_policy, pause_applies
from .coordinator_adapter import COORDINATOR_REQUEST_SCHEMA, SemanticCoordinatorAdapter, validate_response
from .domain import DOMAIN_ACTION_KINDS, DOMAIN_DATA_KINDS, domain_frontier, domain_state
from .runtime_artifacts import capture_verified_outputs, classify_worker_result, verification_ready_for_review
from .runtime_liveness import reopen_changed_reviews, semantic_wait_input_changed
from .runtime_model import ACTIVE_JOB_STATES, RUNTIME_ACTION_KINDS, runtime_state
from .runtime_packets import RuntimeConfig, VerificationSpec, action_for, active_contract, assessment_for, compile_verification_packet, compile_worker_packet, source_captures, worker_profile_sha256
from .runtime_reconciliation import ADMISSION_PENDING, reconcile_applied_reviews, reconciliation_blocks_progress
from .runtime_reconciliation_model import RECONCILIATION_HANDLERS
from .runtime_scheduler import schedule_semantic
from .runtime_semantic import materialize_model_payload, rebind_basis
from .runtime_sources import refresh_sources
from .sources import compare_source_captures

WAIT_CLASSES = {"rate_limit", "provider_quota", "provider_auth", "provider_unavailable", "transport_spawn_error", "configuration_error", "model_response_invalid"}
class AutomaticCoordinator:
    """One durable coordinator over a composed service and two async transports."""

    def __init__(self, store: Any, handlers: Mapping[str, Any], service: Any, transport: Any,
                 semantic: SemanticCoordinatorAdapter, config: RuntimeConfig, *,
                 clock_ns: Callable[[], int] = time.time_ns, sleeper: Callable[[float], None] = time.sleep) -> None:
        need(set(RUNTIME_ACTION_KINDS) <= set(handlers), "HANDLER", "runtime handlers are not composed")
        need(set(RECONCILIATION_HANDLERS) <= set(handlers), "HANDLER", "runtime reconciliation handlers are not composed")
        self.store = store; self.handlers = handlers; self.service = service; self.transport = transport
        self.semantic = semantic; self.config = config; self.clock_ns = clock_ns; self.sleeper = sleeper

    def _load(self) -> tuple[dict[str, Any], list[dict[str, Any]]]:
        state, events, pending = self.service.load_projection()
        need(pending is None, "PENDING_TAIL", "automatic coordinator refuses an incomplete journal")
        return state, events

    @staticmethod
    def _event_id(prefix: str, subject: str, payload: dict[str, Any]) -> str:
        safe = subject.replace("/", ":").replace("\\", ":")
        return f"{prefix}:{safe}:{sha(packed(payload))[:16]}"

    def _command(self, kind: str, payload: dict[str, Any], reason: str, event_id: str):
        state, events = self._load()
        existing = next((event for event in events if event.get("event_id") == event_id), None)
        if existing is not None:
            need(existing["kind"] == kind and existing["payload"] == payload and existing["reason"] == {"summary": reason}, "IDEMPOTENCY", "runtime event identity belongs to another effect")
            return state, existing
        return state, {"event_id": event_id, "base_revision": state["revision"], "kind": kind, "reason": {"summary": reason}, "payload": payload}

    def _data(self, kind, payload, reason, event_id):
        _state, command = self._command(kind, payload, reason, event_id)
        return {"ok": True, "idempotent": True} if "seq" in command else self.service.submit_agent(command)

    def _observe(self, kind, payload, reason, event_id):
        _state, command = self._command(kind, payload, reason, event_id)
        return {"ok": True, "idempotent": True} if "seq" in command else self.service.submit_host_observation(command)

    def _control(self, kind, payload, reason, event_id):
        _state, command = self._command(kind, payload, reason, event_id)
        return {"ok": True, "idempotent": True} if "seq" in command else self.service.submit_host(command)

    def _action(self, kind, payload, reason, event_id, action_class, *, source_rows=(), branch_id=None, run_id=None, problem_id=None):
        state, command = self._command(kind, payload, reason, event_id)
        if "seq" in command:
            return {"ok": True, "idempotent": True}
        action = action_for(state, action_id=f"action:{event_id}", action_class=action_class, payload=payload,
                            source_rows=list(source_rows), branch_id=branch_id, run_id=run_id, problem_id=problem_id)
        assessment = assessment_for(state, action, self.config, f"assessment:{event_id}:{state['revision']}", command)
        return self.service.apply_host_action(command, action, assessment)

    def _runtime_action(self, kind, payload, reason, event_id, job):
        state, _ = self._load(); reservation = runtime_state(state)["reservations"].get(job["job_id"], {})
        return self._action(kind, payload, reason, event_id, RUNTIME_ACTION_KINDS[kind], source_rows=job.get("source_captures", []),
                            branch_id=reservation.get("branch_id"), run_id=job["job_id"], problem_id=job["work_id"])

    def _reconcile_work_jobs(self, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        for job_id, job in list(runtime["jobs"].items()):
            if reconciliation_blocks_progress(state, job_id):
                continue
            if job["state"] == "claimed" and job.get("transport") is None:
                try:
                    receipt = self.transport.submit(job_id, argv=self.config.worker.argv, cwd=self.config.worker.cwd, packet=job["packet"], environment=self.config.worker.environment)
                except (OSError, Refusal):
                    continue
                payload = {"schema": "zap-runtime/job-submitted/1", "job_id": job_id, "receipt": receipt}
                self._observe("runtime.job-submitted", payload, f"Observe durable submission {job_id}", self._event_id("job-submitted", job_id, receipt))
                actions.append({"kind": "job_submitted", "job_id": job_id, "state": receipt["state"]}); continue
            if job["state"] in ACTIVE_JOB_STATES and job.get("transport") is not None:
                receipt = self.transport.reconcile(job_id)
                if sha(packed(receipt)) != job.get("last_status_sha256"):
                    payload = {"schema": "zap-runtime/job-status-observed/1", "job_id": job_id, "receipt": receipt}
                    self._observe("runtime.job-status-observed", payload, f"Reconcile transport job {job_id}", self._event_id("job-status", job_id, receipt))
                    actions.append({"kind": "job_status", "job_id": job_id, "state": receipt["state"]})
                if receipt["state"] in {"succeeded", "failed", "stopped", "interrupted"}:
                    result = classify_worker_result(self.transport.collect(job_id)); payload = {"schema": "zap-runtime/job-result-observed/1", "job_id": job_id, "receipt": result}
                    self._observe("runtime.job-result-observed", payload, f"Collect terminal job {job_id}", self._event_id("job-result", job_id, result))
                    actions.append({"kind": "job_result", "job_id": job_id, "state": result["state"]})

    def _pause(self, pause_id):
        state, _ = self._load(); policy = active_policy(state)
        pause = next((row for row in (policy or {}).get("pauses", []) if row["pause_id"] == pause_id), None)
        need(pause is not None, "PAUSE", "pause disappeared before acknowledgement"); return pause

    def _ack_pause_delivery(self, pause_id, job_id, delivery_state, receipt_sha256):
        pause = self._pause(pause_id)
        if job_id in pause["delivery"]["acknowledgements"] and pause["delivery"]["acknowledgements"][job_id]["state"] in {"delivered", "already_terminal"}:
            return
        payload = {"pause_id": pause_id, "pause_sha256": pause["pause_sha256"], "subject_id": job_id,
                   "state": delivery_state, "receipt_sha256": receipt_sha256}
        self._control("control.pause-delivery-acknowledged", payload, f"Acknowledge stop delivery for {job_id}", self._event_id("pause-delivery", job_id, payload))

    def _ack_pause_safe(self, pause_id, job_id, receipt, safe_state):
        pause = self._pause(pause_id)
        if job_id in pause["actual_safe_state"]["acknowledgements"] and pause["actual_safe_state"]["acknowledgements"][job_id]["state"] in {"safe", "completed", "not_started"}:
            return
        need(safe_state in {"safe", "completed", "not_started"}, "RUNTIME_VALUE", "safe-state verifier returned an unsupported result")
        payload = {"pause_id": pause_id, "pause_sha256": pause["pause_sha256"], "run_id": job_id, "state": safe_state, "receipt_sha256": sha(packed(receipt))}
        self._control("control.pause-safe-state-acknowledged", payload, f"Acknowledge actual safe state for {job_id}", self._event_id("pause-safe", job_id, payload))

    def _pause_jobs(self, actions):
        state, _ = self._load(); policy = active_policy(state)
        if policy is None or not policy.get("pauses"):
            return
        runtime = runtime_state(state)
        for job_id, job in runtime["jobs"].items():
            captured_pauses = [pause for pause in policy["pauses"] if job_id in pause["delivery"]["required"]]
            if job["state"] not in ACTIVE_JOB_STATES and not job.get("stop") and not captured_pauses:
                continue
            branch_id = runtime["reservations"].get(job_id, {}).get("branch_id")
            applicable = pause_applies(state, branch_id=branch_id, run_id=job_id) if job["state"] in ACTIVE_JOB_STATES else captured_pauses
            for pause in applicable:
                request_id = f"stop:{pause['pause_id']}:{job_id}"
                if self.config.worker.stop_mode == "terminate":
                    receipt = self.transport.request_stop(job_id, request_id, mode="terminate", terminate_after_seconds=self.config.worker.terminate_after_seconds)
                else:
                    receipt = self.transport.request_stop(job_id, request_id)
                payload = {"schema": "zap-runtime/stop-observed/1", "job_id": job_id, "pause_id": pause["pause_id"], "request_id": request_id, "receipt": receipt}
                self._observe("runtime.stop-observed", payload, f"Observe stop request for {job_id}", self._event_id("stop", job_id, receipt))
                actions.append({"kind": "stop_requested", "job_id": job_id, "pause_id": pause["pause_id"], "delivered": receipt.get("delivered")})
                if receipt.get("delivered"):
                    self._ack_pause_delivery(pause["pause_id"], job_id, "delivered", sha(packed(receipt)))
                elif receipt.get("state") in {"succeeded", "failed"} and job.get("result_sha256"):
                    self._ack_pause_delivery(pause["pause_id"], job_id, "already_terminal", job["result_sha256"])
                terminal = receipt.get("actual_exit") or receipt.get("state") in {"succeeded", "failed", "stopped", "interrupted"}
                if terminal:
                    proof = self.config.safe_state_verifier(self._load()[0], job, pause, receipt) if self.config.safe_state_verifier else None
                    if proof is not None:
                        need(isinstance(proof, dict) and set(proof) == {"state", "receipt"} and isinstance(proof["receipt"], dict), "RUNTIME_VALUE", "safe-state verifier returned an invalid proof")
                        self._ack_pause_safe(pause["pause_id"], job_id, proof["receipt"], proof["state"])
                    else:
                        self._ensure_interrupted_review(job, pause["pause_id"], actions)

    def _advance_results_and_waits(self, actions):
        state, _ = self._load(); runtime = runtime_state(state); now = self.clock_ns()
        for job_id, job in runtime["jobs"].items():
            if reconciliation_blocks_progress(state, job_id):
                continue
            if job["state"] == "result_ready":
                classification = job.get("result", {}).get("diagnostic", {}).get("classification")
                if classification in WAIT_CLASSES and not any(row["job_id"] == job_id and row["state"] == "waiting" for row in runtime["resource_waits"].values()):
                    hint = job["result"].get("diagnostic", {}).get("retry_after_ns"); next_retry = None if classification == "configuration_error" else hint if type(hint) is int and hint >= now else now + self.config.transient_backoff_ns
                    capability = f"worker_transport:{worker_profile_sha256(self.config.worker)}" if classification == "configuration_error" else "worker_transport"
                    payload = {"schema": "zap-runtime/resource-wait-recorded/1", "wait_id": f"wait:{job['attempt_id']}", "job_id": job_id, "capability": capability, "classification": classification, "observed_at_ns": now, "affected_operations": [job["attempt_id"]], "next_retry_ns": next_retry}
                    self._observe("runtime.resource-wait-recorded", payload, f"Persist transient wait for {job_id}", self._event_id("resource-wait", job_id, payload)); actions.append({"kind": "resource_wait", "job_id": job_id, "classification": classification, "next_retry_ns": next_retry})
                elif job.get("result", {}).get("state") == "failed" or classification in {"worker_blocked", "worker_stopped", "worker_result_invalid"}:
                    review_id = f"review:worker-failure:{job['attempt_id']}"
                    if review_id not in runtime["review_requests"]:
                        payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": review_id,
                                   "trigger_ids": [job_id], "work_ids": [job["work_id"]], "scope": "worker_failure",
                                   "summary": "Worker failed; classify and revise or retry through an explicit review"}
                        self._observe("runtime.review-requested", payload, f"Request review for failed worker {job_id}", review_id)
                        actions.append({"kind": "worker_failure_review", "job_id": job_id})
                elif job.get("result", {}).get("state") not in {"stopped", "interrupted"}:
                    reservation = runtime["reservations"].get(job_id, {})
                    if pause_applies(state, branch_id=reservation.get("branch_id"), run_id=job_id):
                        continue
                    try:
                        version, _contract, contract_hash = active_contract(state, job["work_id"])
                    except Refusal:
                        continue
                    if version != job["contract_version"] or contract_hash != job["contract_sha256"]:
                        continue
                    payload = {"schema": "zap-runtime/job-candidate-recorded/1", "job_id": job_id, "work_id": job["work_id"], "result_sha256": sha(packed(job["result"]))}
                    try:
                        self._runtime_action("runtime.job-candidate-recorded", payload, f"Record worker result as candidate for {job['work_id']}", f"candidate:{job_id}", job); actions.append({"kind": "candidate", "job_id": job_id, "work_id": job["work_id"]})
                    except Refusal as exc:
                        if exc.code not in ADMISSION_PENDING: raise
            elif job["state"] == "waiting":
                wait = next((row for row in runtime["resource_waits"].values() if row["job_id"] == job_id and row["state"] == "waiting"), None)
                profile_changed = wait and wait["classification"] == "configuration_error" and wait["capability"] != f"worker_transport:{worker_profile_sha256(self.config.worker)}"
                if wait and ((wait["next_retry_ns"] is not None and now >= wait["next_retry_ns"]) or profile_changed) and not pause_applies(state, branch_id=runtime["reservations"][job_id].get("branch_id"), run_id=job_id):
                    payload = {"schema": "zap-runtime/retry-released/1", "wait_id": wait["wait_id"], "job_id": job_id, "work_id": job["work_id"], "observed_now_ns": now}
                    self._runtime_action("runtime.retry-released", payload, f"Release persisted retry for {job['work_id']}", f"retry:{wait['wait_id']}", job); actions.append({"kind": "retry_released", "job_id": job_id})

    def _verification_spec(self, work_id, check_id):
        found = [spec for spec in self.config.verifications.get(work_id, ()) if spec.check_id == check_id]
        need(len(found) == 1, "RUNTIME_CONTRACT", "configured verification binding changed or is missing"); return found[0]

    def _reconcile_verifications(self, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        for verification_id, row in runtime["verification_jobs"].items():
            spec = self._verification_spec(runtime["jobs"][row["work_job_id"]]["work_id"], row["plan"]["check_id"])
            if row["state"] == "claimed":
                receipt = self.transport.submit(verification_id, argv=spec.argv, cwd=spec.cwd, packet=row["packet"], environment=spec.environment)
                payload = {"schema": "zap-runtime/verification-submitted/1", "verification_id": verification_id, "receipt": receipt}
                self._observe("runtime.verification-submitted", payload, f"Observe verification submission {verification_id}", self._event_id("verification-submitted", verification_id, receipt)); actions.append({"kind": "verification_submitted", "verification_id": verification_id})
            elif row["state"] in ACTIVE_JOB_STATES | {"prepared"}:
                result = self.transport.collect(verification_id)
                if result["ready"]:
                    evidence = {"id": row["evidence_id"], "claim": f"Configured check {spec.check_id}", "subject": spec.target, "result": "observed_pass" if result["state"] == "succeeded" else "observed_fail", "artifact_refs": [f"transport:{result['stdout']['sha256']}", f"transport:{result['stderr']['sha256']}"], "node_refs": [runtime["jobs"][row["work_job_id"]]["work_id"]]}
                    payload = {"schema": "zap-runtime/verification-result-observed/1", "verification_id": verification_id, "receipt": result, "evidence": evidence}
                    self._observe("runtime.verification-result-observed", payload, f"Collect configured verification {verification_id}", self._event_id("verification-result", verification_id, result)); actions.append({"kind": "verification_result", "verification_id": verification_id, "result": evidence["result"]})

    def _ensure_review_request(self, job, trigger_ids, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        if f"review:{job['attempt_id']}" in runtime["review_requests"]: return
        payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": f"review:{job['attempt_id']}", "trigger_ids": sorted(set(trigger_ids)), "work_ids": [job["work_id"]], "scope": "candidate_acceptance", "summary": "Independent configured checks and semantic acceptance required"}
        self._observe("runtime.review-requested", payload, f"Request review for {job['work_id']}", f"review-request:{job['attempt_id']}"); actions.append({"kind": "review_requested", "work_id": job["work_id"]})

    def _ensure_interrupted_review(self, job, pause_id, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        if any(row["state"] == "pending" and row["scope"] == "interrupted_needs_reconciliation" and job["work_id"] in row["work_ids"] for row in runtime["review_requests"].values()):
            return
        payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": f"review:stop:{pause_id}:{job['attempt_id']}",
                   "trigger_ids": [pause_id, job["job_id"]], "work_ids": [job["work_id"]], "scope": "interrupted_needs_reconciliation",
                   "summary": "Process exit is observed; safe boundary and external effects still require reconciliation"}
        self._observe("runtime.review-requested", payload, f"Request interrupted-effect reconciliation for {job['work_id']}", f"review-stop:{pause_id}:{job['job_id']}")
        actions.append({"kind": "interrupted_needs_reconciliation", "job_id": job["job_id"], "pause_id": pause_id})

    def _schedule_verifications(self, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        for job_id, job in runtime["jobs"].items():
            if reconciliation_blocks_progress(state, job_id): continue
            if job["state"] not in {"candidate", "verification", "review_pending"}: continue
            specs = self.config.verifications.get(job["work_id"], ()); existing = {row["plan"]["check_id"]: row for row in runtime["verification_jobs"].values() if row["work_job_id"] == job_id}
            failed = next((row for row in existing.values() if row["state"] == "observed" and row.get("result", {}).get("state") != "succeeded"), None)
            if failed is not None:
                review_id = f"review:verification-failure:{failed['verification_id']}"
                if review_id not in runtime["review_requests"]:
                    payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": review_id,
                               "trigger_ids": [failed["evidence_id"]], "work_ids": [job["work_id"]], "scope": "verification_failure",
                               "summary": "Configured verification failed; refine or retry through an explicit review"}
                    self._observe("runtime.review-requested", payload, f"Request review for failed verification {failed['verification_id']}", review_id)
                    actions.append({"kind": "verification_failure_review", "verification_id": failed["verification_id"]})
                continue
            next_spec = next((spec for spec in specs if spec.check_id not in existing), None)
            if next_spec is not None and all(row["state"] == "observed" for row in existing.values()):
                blocked_review_id = f"review:verification:{job['attempt_id']}:{next_spec.check_id}"
                if blocked_review_id in runtime["review_requests"]:
                    continue
                packet = compile_verification_packet(state, job, next_spec); verification_id = f"verify:{job['attempt_id']}:{next_spec.check_id}"
                payload = {"schema": "zap-runtime/verification-claimed/1", "verification_id": verification_id, "work_job_id": job_id, "evidence_id": f"evidence:{verification_id}", "plan": next_spec.public_plan(), "packet": packet, "packet_sha256": sha(packet.encode("utf-8"))}
                try:
                    self._runtime_action("runtime.verification-claimed", payload, f"Claim configured verification {next_spec.check_id}", f"verification-claim:{verification_id}", job)
                    actions.append({"kind": "verification_claimed", "verification_id": verification_id})
                except Refusal as exc:
                    if exc.code not in ADMISSION_PENDING:
                        raise
                    actions.append({"kind": "verification_blocked", "verification_id": verification_id, "code": exc.code})
                    review = {"schema": "zap-runtime/review-requested/1", "review_request_id": blocked_review_id,
                              "trigger_ids": [f"verification-admission:{verification_id}"], "work_ids": [job["work_id"]],
                              "scope": "verification_admission", "summary": f"Verification admission blocked: {exc.code}"}
                    self._observe("runtime.review-requested", review, f"Request review for blocked verification {verification_id}", blocked_review_id)
            elif (not specs or all(verification_ready_for_review(existing.get(spec.check_id, {}), artifact_capture_required=self.config.artifact_capture is not None)
                                   for spec in specs)) and job["state"] != "accepted":
                trigger_ids = [existing[spec.check_id]["evidence_id"] for spec in specs if spec.check_id in existing]; self._ensure_review_request(job, trigger_ids or [f"candidate:{job_id}"], actions)

    def _discover_review_triggers(self, actions):
        state, _ = self._load(); runtime = runtime_state(state); domain = domain_state(state); knowledge = state.get("extensions", {}).get("knowledge", {}); used = {trigger for row in runtime["review_requests"].values() for trigger in row["trigger_ids"]}
        for trigger_id, invalidation in knowledge.get("invalidations", {}).items():
            if trigger_id in used: continue
            work_ids = {row["id"] for row in invalidation.get("affected", []) if row["kind"] in {"task", "node"}}
            cause = invalidation.get("cause", {})
            if cause.get("kind") == "source":
                work_ids.update(work_id for evidence in domain["evidence_adjudications"].values() if cause.get("id") in evidence.get("source_refs", [])
                                for work_id in evidence.get("applies_to", {}).get("work_ids", []))
            work_ids = sorted(work_ids)
            if not work_ids: continue
            payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": f"review:invalidation:{trigger_id}", "trigger_ids": [trigger_id], "work_ids": work_ids, "scope": "source_invalidation", "summary": "Changed source affects known work closure"}
            self._observe("runtime.review-requested", payload, "Request adaptive review for source invalidation", f"review-invalidation:{trigger_id}"); actions.append({"kind": "review_requested", "trigger": trigger_id, "work_ids": work_ids})
        for trigger_id, observation in runtime["observations"].items():
            if observation.get("kind") != "native_facts_refresh" or trigger_id in used:
                continue
            source_id = observation["source_id"]; work_ids = []
            for work_id, history in domain["task_contracts"].items():
                row = next((item for item in history["versions"] if item["version"] == history["active_version"]), None)
                if row and row["schema"] == "zap-task-contract/1" and source_id in row["contract"]["source_handles"]:
                    work_ids.append(work_id)
            if work_ids:
                payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": f"review:native:{trigger_id}",
                           "trigger_ids": [trigger_id], "work_ids": sorted(work_ids), "scope": "native_facts_changed",
                           "summary": "Native VibeVM fact markers changed and require semantic reassessment"}
                self._observe("runtime.review-requested", payload, "Request native-facts reassessment", f"review-native:{trigger_id}")
                actions.append({"kind": "review_requested", "trigger": trigger_id, "work_ids": sorted(work_ids)})
        for job in runtime["jobs"].values():
            if job["state"] in {"accepted", "retry_released"}:
                continue
            try:
                version, _contract, contract_hash = active_contract(state, job["work_id"])
            except Refusal:
                continue
            trigger_id = f"contract-drift:{job['job_id']}:{version}"
            if (version == job["contract_version"] and contract_hash == job["contract_sha256"]) or trigger_id in used:
                continue
            payload = {"schema": "zap-runtime/review-requested/1", "review_request_id": f"review:{trigger_id}",
                       "trigger_ids": [trigger_id], "work_ids": [job["work_id"]], "scope": "contract_changed",
                       "summary": "Active job contract or goal changed and requires reconciliation"}
            self._observe("runtime.review-requested", payload, f"Request review for changed contract {job['work_id']}", f"review:{trigger_id}")
            actions.append({"kind": "review_requested", "trigger": trigger_id, "work_ids": [job["work_id"]]})

    def _semantic_request(self, request_kind, body, subject, actions):
        state, _ = self._load(); request_id = f"semantic:{request_kind}:{subject}:{runtime_state(state)['revision'] + 1}"
        request = {"schema": COORDINATOR_REQUEST_SCHEMA, "request_id": request_id, "request_kind": request_kind, "state_revision": state["revision"] + 1, "base_sha256": state["base_sha256"], "request": body}
        payload = {"schema": "zap-runtime/semantic-requested/1", "request_id": request_id, "request_kind": request_kind, "state_revision": request["state_revision"], "request_sha256": sha(packed(request)), "request": request}
        self._observe("runtime.semantic-requested", payload, f"Persist semantic {request_kind} request", f"semantic-request:{request_id}"); self.semantic.submit(request_id, request); actions.append({"kind": "semantic_requested", "request_id": request_id, "request_kind": request_kind})

    def _record_semantic_result(self, row, response, outcome, actions):
        payload = {"schema": "zap-runtime/semantic-result-recorded/1", "request_id": row["request_id"], "response": response, "response_sha256": sha(packed(response)), "outcome": outcome}
        self._observe("runtime.semantic-result-recorded", payload, f"Record semantic result {outcome}", self._event_id("semantic-result", row["request_id"], payload)); actions.append({"kind": "semantic_result", "request_id": row["request_id"], "outcome": outcome})

    def _poll_semantic(self, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        for request_id, row in runtime["semantic_requests"].items():
            if row["state"] not in {"requested", "submitted"}: continue
            request = row["request"]; self.semantic.submit(request_id, request); result = self.semantic.poll(request_id, request)
            if not result["ready"]: continue
            if not result.get("ok"):
                diagnostic = result.get("diagnostic") if isinstance(result.get("diagnostic"), dict) else {}
                classification = diagnostic.get("classification")
                if classification not in WAIT_CLASSES:
                    classification = "provider_unavailable"
                now = self.clock_ns(); hint = diagnostic.get("retry_after_ns")
                operation_basis = request["request"].get("scope_sha256") or request["request"].get("selection_basis_sha256") or request["request"].get("closure_basis_sha256") or sha(packed(request["request"]))
                if classification == "model_response_invalid":
                    prior = sum(wait.get("classification") == classification and wait.get("diagnostic", {}).get("operation_basis") == operation_basis
                                for wait in runtime["resource_waits"].values())
                    diagnostic.update({"operation_basis": operation_basis, "repair_attempt": prior + 1})
                next_retry = None if classification == "configuration_error" or classification == "model_response_invalid" and diagnostic["repair_attempt"] > 1 else hint if type(hint) is int and hint >= now else now + self.config.transient_backoff_ns
                payload = {"schema": "zap-runtime/semantic-wait-recorded/1", "wait_id": f"wait:{request_id}", "request_id": request_id,
                           "classification": classification, "observed_at_ns": now, "next_retry_ns": next_retry, "diagnostic": diagnostic}
                self._observe("runtime.semantic-wait-recorded", payload, f"Persist semantic provider wait {request_id}", f"semantic-wait:{request_id}")
                actions.append({"kind": "semantic_wait", "request_id": request_id, "classification": classification, "next_retry_ns": next_retry})
                continue
            response = validate_response(request, result["response"]); current, _ = self._load()
            if current["revision"] != response["state_revision"]:
                basis_sha256 = rebind_basis(row, response, current)
                if basis_sha256 is None:
                    self._record_semantic_result(row, response, "stale", actions)
                    continue
                response_sha256 = sha(packed(response))
                if not any(binding["basis_sha256"] == basis_sha256 and binding["response_sha256"] == response_sha256 for binding in row.get("rebindings", [])):
                    rebound_event = {"schema": "zap-runtime/semantic-rebound/1", "request_id": request_id,
                                     "response_sha256": response_sha256, "from_revision": response["state_revision"],
                                     "to_revision": current["revision"], "basis_sha256": basis_sha256}
                    self._observe("runtime.semantic-rebound", rebound_event, f"Rebind unchanged semantic basis for {request_id}", self._event_id("semantic-rebound", request_id, rebound_event))
                    actions.append({"kind": "semantic_rebound", "request_id": request_id, "to_revision": current["revision"]})
            try:
                outcome = self._apply_semantic(row, response, actions)
            except Refusal as exc:
                if exc.code == "ASSESSMENT_PENDING":
                    actions.append({"kind": "assessment_pending", "request_id": request_id})
                    continue
                outcome = "rejected"
                actions.append({"kind": "semantic_rejected", "request_id": request_id, "code": exc.code, "message": str(exc)})
            self._record_semantic_result(row, response, outcome, actions)

    def _release_semantic_waits(self, actions):
        state, _ = self._load(); runtime = runtime_state(state); now = self.clock_ns()
        for wait_id, wait in runtime["resource_waits"].items():
            if wait["state"] != "waiting" or wait.get("operation_kind") != "semantic":
                continue
            if wait["next_retry_ns"] is None:
                profile = getattr(self.semantic, "profile_sha256", None)
                request_row = runtime["semantic_requests"].get(wait["request_id"])
                profile_same = not callable(profile) or wait.get("diagnostic", {}).get("profile_sha256") == profile()
                if profile_same and (request_row is None or not semantic_wait_input_changed(state, request_row)):
                    continue
            elif now < wait["next_retry_ns"]:
                continue
            payload = {"schema": "zap-runtime/semantic-wait-released/1", "wait_id": wait_id,
                       "request_id": wait["request_id"], "observed_now_ns": now}
            self._observe("runtime.semantic-wait-released", payload, f"Release semantic provider wait {wait['request_id']}", f"semantic-wait-release:{wait_id}")
            actions.append({"kind": "semantic_wait_released", "request_id": wait["request_id"]})

    def _apply_semantic(self, row, response, actions):
        if response["disposition"] in {"needs_evidence", "wait", "no_change"}:
            body = row["request"]["request"]
            actionable = any(review["scope"] in {"source_invalidation", "contract_changed", "native_facts_changed"} for review in body.get("review_requests", []))
            need(not (response["disposition"] == "no_change" and actionable), "COORDINATOR_REVIEW", "actionable reassessment requires an applied keep-route or pivot review")
            return "no_action"
        if response["disposition"] == "select":
            work_id = response["selection"]["work_id"]; allowed = {item["work_id"] for item in row["request"]["request"]["frontier"]}; need(work_id in allowed, "COORDINATOR", "selected work was not in captured frontier")
            state, _ = self._load(); domain = domain_state(state); node = next((node for node in state["plan"]["node"] if node["id"] == work_id), domain["work_nodes"].get(work_id)); current_state = domain["work_updates"].get(work_id, {}).get("state", node["state"])
            _version, contract, _hash = active_contract(state, work_id); captures = source_captures(state, contract["source_handles"]); branch_id = self.config.worker.branch_for_work.get(work_id)
            if current_state == "planned":
                payload = {"schema": "zap-domain/work-transitioned/1", "work_id": work_id, "from_state": "planned", "to_state": "ready", "successor_ids": []}
                self._action("domain.work-transitioned", payload, f"Ready selected work {work_id}", f"ready:{row['request_id']}", "plan.lower", source_rows=captures, branch_id=branch_id, problem_id=work_id); state, _ = self._load()
            runtime = runtime_state(state); response_sha256 = sha(packed(response)); prepared = runtime["prepared_claims"].get(row["request_id"])
            if prepared is None:
                number = 1 + sum(attempt["work_id"] == work_id for attempt in runtime["attempts"].values()); decision_suffix = response_sha256[:8]
                job_id, attempt_id = f"job:{work_id}:{number}:{decision_suffix}", f"attempt:{work_id}:{number}:{decision_suffix}"
                _packet, claim = compile_worker_packet(state, work_id, job_id, attempt_id, self.config.worker,
                                                       semantic_request_id=row["request_id"], semantic_response_sha256=response_sha256)
                prep = {"schema": "zap-runtime/job-claim-prepared/1", "request_id": row["request_id"],
                        "response_sha256": response_sha256, "claim": claim}
                self._observe("runtime.job-claim-prepared", prep, f"Prepare stable claim for selected work {work_id}", f"claim-prepared:{row['request_id']}")
                actions.append({"kind": "job_claim_prepared", "job_id": job_id, "work_id": work_id})
            else:
                need(prepared["response_sha256"] == response_sha256 and prepared["claim"]["work_id"] == work_id,
                     "COORDINATOR_STALE", "prepared selection claim differs")
                claim = prepared["claim"]; job_id = claim["job_id"]
            self._action("runtime.job-claimed", claim, f"Atomically claim selected work {work_id}", f"claim:{job_id}", "work.dispatch", source_rows=claim["source_captures"], branch_id=claim["reservation"]["branch_id"], run_id=job_id, problem_id=work_id); actions.append({"kind": "job_claimed", "job_id": job_id, "work_id": work_id}); return "applied"
        for index, draft in enumerate(response["commands"]): self._apply_model_command(row, index, draft, actions)
        return "applied"

    def _apply_model_command(self, row, index, draft, actions):
        request_id = row["request_id"]
        kind, payload, event_id = draft["kind"], draft["payload"], f"model:{request_id}:{index}"
        if kind == "domain.review-proposed":
            state, _ = self._load(); body = row["request"]["request"]
            expected = {"base_sha256": row["request"]["base_sha256"], "zap_revision": row["state_revision"],
                        "domain_revision": body["domain"]["revision"], "intent_id": body["domain"]["active_intent_id"],
                        "outcome_id": body["domain"]["active_outcome_id"],
                        "policy_revision": body["policy_binding"]["charter_revision"],
                        "source_captures": body["source_captures"]}
            payload = materialize_model_payload(state, kind, payload, expected_review_capture=expected,
                                                request_knowledge=body["knowledge"])
        if kind in DOMAIN_DATA_KINDS or kind.startswith("knowledge.region-"):
            self._data(kind, payload, draft["reason"], event_id)
            if kind == "domain.review-proposed":
                state, _ = self._load(); review_id = payload["review_id"]; apply_payload = {"schema": "zap-domain/review-applied/1", "review_id": review_id, "expected_domain_revision": domain_state(state)["revision"]}
                self._action("domain.review-applied", apply_payload, f"Apply validated semantic review {review_id}", f"model-apply:{request_id}:{index}", "adaptive.apply")
        else:
            action_class = DOMAIN_ACTION_KINDS.get(kind); need(action_class is not None, "COORDINATOR", "model command has no privileged route")
            state, _ = self._load(); runtime = runtime_state(state); work_id = payload.get("work_id"); job = next((job for job in runtime["jobs"].values() if job["work_id"] == work_id), None); captures = job.get("source_captures", []) if job else []; branch_id = runtime["reservations"].get(job["job_id"], {}).get("branch_id") if job else None
            if kind == "domain.evidence-adjudicated":
                captures = source_captures(state, payload["source_refs"])
            self._action(kind, payload, draft["reason"], event_id, action_class, source_rows=captures, branch_id=branch_id, run_id=job["job_id"] if job else None, problem_id=work_id)
            if kind == "domain.work-accepted" and job:
                state, _ = self._load(); observed = {"schema": "zap-runtime/job-acceptance-observed/1", "job_id": job["job_id"], "acceptance_id": payload["acceptance_id"], "domain_revision": domain_state(state)["revision"]}
                self._observe("runtime.job-acceptance-observed", observed, f"Observe central acceptance {payload['acceptance_id']}", f"acceptance-observed:{job['job_id']}")
        actions.append({"kind": "model_command", "command_kind": kind, "event_id": event_id})

    def _reset_stopped_after_resume(self, actions):
        state, _ = self._load(); runtime = runtime_state(state)
        for job_id, job in runtime["jobs"].items():
            if job["state"] == "result_ready" and job.get("stop") is not None and job.get("result", {}).get("state") in {"stopped", "interrupted"} and not pause_applies(state, branch_id=runtime["reservations"][job_id].get("branch_id"), run_id=job_id):
                payload = {"schema": "zap-runtime/job-reset-ready/1", "job_id": job_id, "work_id": job["work_id"], "reason": "Owner pause resumed after safe stop"}
                self._runtime_action("runtime.job-reset-ready", payload, f"Ready stopped work {job['work_id']} after resume", f"reset:{job_id}", job); actions.append({"kind": "stopped_job_reset", "job_id": job_id})

    def tick(self):
        actions = []; self._reconcile_work_jobs(actions); self._reconcile_verifications(actions); capture_verified_outputs(self, actions); refresh_sources(self, actions); self._pause_jobs(actions); reconcile_applied_reviews(self, actions); self._advance_results_and_waits(actions); self._reset_stopped_after_resume(actions); self._schedule_verifications(actions); self._discover_review_triggers(actions); reopen_changed_reviews(self, actions); self._release_semantic_waits(actions); self._poll_semantic(actions); schedule_semantic(self, actions)
        state, _ = self._load(); runtime = runtime_state(state); domain = domain_state(state)
        closure = domain["closure"]; classification = closure.get("classification") if isinstance(closure, dict) else None
        return {"ok": True, "revision": state["revision"], "runtime_revision": runtime["revision"], "execution_mode": state["execution_mode"], "actions": actions,
                "active_jobs": sorted(job_id for job_id, row in runtime["jobs"].items() if row["state"] in ACTIVE_JOB_STATES),
                "frontier": domain_frontier(state), "closure": closure, "closure_classification": classification,
                "campaign_finished": closure is not None, "campaign_complete": classification in {"original", "revised"}}

    def run(self, *, stop_event: threading.Event | None = None, max_ticks: int | None = None):
        ticks = 1; last = self.tick()
        while (max_ticks is None or ticks < max_ticks) and not (stop_event and stop_event.is_set()) and not last["campaign_finished"]:
            self.sleeper(self.config.idle_poll_seconds); last = self.tick(); ticks += 1
        return {**last, "ticks": ticks}
