"""Effect adapter for job dispositions committed by applied adaptive reviews."""
from __future__ import annotations

import copy
from typing import Any

from .common import Refusal, exact, need, packed, sha
from .control import active_policy, pause_applies
from .domain import domain_state
from .domain_model import effective_nodes, owned_obligations, validation_generation
from .runtime_model import ACTIVE_JOB_STATES, runtime_state
from .runtime_packets import active_contract, source_captures
from .runtime_reconciliation_model import (
    RECONCILIATION_ACTION_KINDS, RECONCILIATION_DATA_KINDS,
    RECONCILIATION_EVENT_ROUTES, RECONCILIATION_EVENT_SCHEMAS,
    RECONCILIATION_HANDLERS, RECONCILIATION_OBSERVATION_KINDS,
)
from .sources import compare_source_captures

FINISHED = {"complete"}
NATURAL_RESULTS = {"succeeded", "failed"}
STOP_RESULTS = {"stopped", "interrupted"}
ADMISSION_PENDING = {"PAUSED", "NEEDS_EVIDENCE", "ASSESSMENT", "ASSESSMENT_PENDING", "ASSESSMENT_PROVIDER", "ASSESSMENT_STALE"}


def _compatibility(state, job):
    reasons = []
    version = None; contract_hash = None; obligations = []
    try:
        version, contract, contract_hash = active_contract(state, job["work_id"])
        obligations = sorted(owned_obligations(domain_state(state), job["work_id"]))
        if version != job["contract_version"] or contract_hash != job["contract_sha256"]:
            reasons.append("task contract changed")
        if set(contract["obligation_ids"]) != set(obligations):
            reasons.append("owned obligations differ from the captured contract")
        current_captures = source_captures(state, contract["source_handles"])
        if current_captures != job["source_captures"]:
            reasons.append("contract source captures changed")
    except Refusal:
        current_captures = []
        reasons.append("current contract or source capture is unavailable")
    compared = compare_source_captures(state, job["source_captures"])
    if compared["status"] != "current":
        reasons.append("captured job source is stale or unknown")
    domain = domain_state(state); node = effective_nodes(state, domain).get(job["work_id"])
    if node is None or node["state"] in {"dropped", "superseded"}:
        reasons.append("work was dropped or superseded")
    if domain["work_updates"].get(job["work_id"], {}).get("revalidation_required"):
        reasons.append("work requires revalidation")
    status = "compatible" if not reasons else "unknown" if any("unavailable" in reason or "unknown" in reason for reason in reasons) else "incompatible"
    return {"status": status, "contract_version": version, "contract_sha256": contract_hash,
            "source_status": compared["status"], "obligation_ids": obligations, "reasons": reasons}


def _safe_unknown(status="unknown"):
    return {"status": status, "receipt_sha256": None, "receipt": None}


def _safe_proof(coordinator, state, job, review_id, safe_boundary, receipt):
    if receipt and receipt.get("state") == "prepared" and receipt.get("process", {}).get("active") is False:
        proof = {"transport_state": "prepared", "descriptor_sha256": receipt.get("descriptor_sha256"),
                 "nonce": receipt.get("nonce")}
        return {"status": "not_started", "receipt_sha256": sha(packed(proof)), "receipt": proof}
    verifier = coordinator.config.safe_state_verifier
    if verifier is None:
        return _safe_unknown("needs_reconcile")
    context = {"kind": "adaptive_reconciliation", "review_id": review_id,
               "job_id": job["job_id"], "safe_boundary": safe_boundary}
    proof = verifier(state, job, context, receipt or {})
    if proof is None:
        return _safe_unknown("needs_reconcile")
    need(isinstance(proof, dict) and set(proof) == {"state", "receipt"}
         and proof["state"] in {"safe", "completed", "not_started"}
         and isinstance(proof["receipt"], dict),
         "RUNTIME_VALUE", "safe-state verifier returned an invalid reconciliation proof")
    return {"status": proof["state"], "receipt_sha256": sha(packed(proof["receipt"])),
            "receipt": copy.deepcopy(proof["receipt"])}


def _progress(coordinator, review_id, job_id, phase, compatibility, *, transport=None, safe_state=None, detail, actions):
    payload = {"schema": "zap-runtime/reconciliation-progress/1", "review_id": review_id, "job_id": job_id,
               "phase": phase, "compatibility": compatibility, "transport": copy.deepcopy(transport),
               "safe_state": safe_state or _safe_unknown(), "detail": detail}
    event_id = coordinator._event_id("reconciliation-progress", f"{review_id}:{job_id}", payload)
    coordinator._observe("runtime.reconciliation-progress-observed", payload, detail, event_id)
    actions.append({"kind": "job_reconciliation", "review_id": review_id, "job_id": job_id,
                    "phase": phase, "detail": detail})


def _plan_payload(state, review):
    runtime = runtime_state(state)
    captured = {row["job_id"]: row for row in review["captures"]["jobs"]}
    items = []
    for disposition in sorted(review["transition"]["job_reconciliation"], key=lambda row: row["job_id"]):
        job = runtime["jobs"].get(disposition["job_id"])
        need(job is not None and disposition["job_id"] in captured,
             "RUNTIME_STALE", "applied review reconciliation job is missing")
        items.append({"job_id": job["job_id"], "work_id": job["work_id"], "attempt_id": job["attempt_id"],
                      "action": disposition["action"], "safe_boundary": disposition["safe_boundary"],
                      "reason": disposition["reason"], "captured_state": captured[job["job_id"]]["status"],
                      "contract_version": job["contract_version"], "contract_sha256": job["contract_sha256"],
                      "source_captures": copy.deepcopy(job["source_captures"])})
    return {"schema": "zap-runtime/reconciliation-planned/1", "review_id": review["review_id"],
            "review_event_id": review["applied_event_id"], "outcome_id": domain_state(state)["active_outcome_id"],
            "items": items}


def reconciliation_blocks_progress(state: dict[str, Any], job_id: str) -> bool:
    """Block a reviewed job before another submit/advance can outrun drain or revalidation."""
    runtime = runtime_state(state); domain = domain_state(state)
    plans = runtime.get("reconciliations", {})
    for review in domain["reviews"].values():
        if review.get("status") != "applied":
            continue
        disposition = next((row for row in review["transition"]["job_reconciliation"]
                            if row["job_id"] == job_id and row["action"] in {"drain", "revalidate"}), None)
        if disposition is None:
            continue
        item = plans.get(review["review_id"], {}).get("items", {}).get(job_id)
        if item is None or item.get("status") not in FINISHED | {"released"}:
            return True
    return False


def _paused(state, job):
    policy = active_policy(state)
    if policy is None:
        return True
    if any(row["scope"] == "campaign" for row in policy.get("pauses", [])):
        return True
    reservation = runtime_state(state)["reservations"].get(job["job_id"], {})
    return bool(pause_applies(state, branch_id=reservation.get("branch_id"), run_id=job["job_id"]))


def _terminal_receipt(coordinator, job):
    if isinstance(job.get("result"), dict):
        return copy.deepcopy(job["result"])
    try:
        receipt = coordinator.transport.reconcile(job["job_id"])
    except (OSError, Refusal):
        return None
    if receipt.get("state") in NATURAL_RESULTS | STOP_RESULTS:
        try:
            return coordinator.transport.collect(job["job_id"])
        except (OSError, Refusal):
            return receipt
    return receipt


def _stop_step(coordinator, state, review_id, item, job, compatibility, actions):
    receipt = _terminal_receipt(coordinator, job)
    if receipt is not None and receipt.get("state") == "unknown_effect":
        _progress(coordinator, review_id, job["job_id"], "needs_reconcile", compatibility,
                  transport=receipt, safe_state=_safe_unknown("needs_reconcile"),
                  detail="Unknown process effect cannot be stopped or released without reconciliation", actions=actions)
        return False
    if receipt is None or receipt.get("state") not in NATURAL_RESULTS | STOP_RESULTS | {"prepared"}:
        if job["state"] in ACTIVE_JOB_STATES or receipt is not None:
            request_id = f"reconcile:{review_id}:{job['job_id']}"
            try:
                receipt = coordinator.transport.request_stop(job["job_id"], request_id)
            except (OSError, Refusal):
                receipt = None
        if receipt is None or receipt.get("state") not in NATURAL_RESULTS | STOP_RESULTS | {"prepared"}:
            _progress(coordinator, review_id, job["job_id"], "draining", compatibility,
                      transport=receipt, detail="Cooperative stop remains pending", actions=actions)
            return False
    safe_state = _safe_proof(coordinator, state, job, review_id, item["safe_boundary"], receipt)
    if safe_state["status"] not in {"safe", "completed", "not_started"}:
        _progress(coordinator, review_id, job["job_id"], "needs_reconcile", compatibility,
                  transport=receipt, safe_state=safe_state,
                  detail="Process state is terminal but task effects still need reconciliation", actions=actions)
        return False
    phase = "safe_to_release" if item["action"] == "revalidate" else "complete"
    _progress(coordinator, review_id, job["job_id"], phase, compatibility,
              transport=receipt, safe_state=safe_state,
              detail="Task safe boundary was independently verified", actions=actions)
    return True


def _release_revalidation(coordinator, state, review_id, item, job, actions):
    domain = domain_state(state); node = effective_nodes(state, domain)[job["work_id"]]
    payload = {"schema": "zap-runtime/reconciliation-revalidation-released/1", "review_id": review_id,
               "job_id": job["job_id"], "work_id": job["work_id"], "expected_job_state": job["state"],
               "expected_work_state": node["state"], "from_generation": validation_generation(domain, job["work_id"])}
    event_id = coordinator._event_id("revalidation-release", f"{review_id}:{job['job_id']}", payload)
    coordinator._action("runtime.reconciliation-revalidation-released", payload,
                        f"Release reconciled attempt {job['job_id']}", event_id, "plan.lower",
                        branch_id=runtime_state(state)["reservations"].get(job["job_id"], {}).get("branch_id"),
                        run_id=job["job_id"], problem_id=job["work_id"])
    actions.append({"kind": "revalidation_released", "review_id": review_id, "job_id": job["job_id"]})


def _ready_revalidation(coordinator, state, review_id, item, actions):
    payload = {"schema": "zap-domain/work-revalidation-readied/1", "work_id": item["work_id"],
               "review_id": review_id, "job_id": item["job_id"], "from_generation": item["from_generation"],
               "expected_state": item["expected_work_state"]}
    event_id = f"revalidation-ready:{review_id}:{item['job_id']}"
    coordinator._action("domain.work-revalidation-readied", payload,
                        f"Ready {item['work_id']} after safe revalidation release", event_id, "plan.lower",
                        run_id=item["job_id"], problem_id=item["work_id"])
    state, _ = coordinator._load(); compatibility = _compatibility(state, runtime_state(state)["jobs"][item["job_id"]])
    _progress(coordinator, review_id, item["job_id"], "complete", compatibility,
              detail="Fresh validation generation is ready", actions=actions)


def reconcile_applied_reviews(coordinator, actions) -> None:
    """Materialize and execute every unapplied job disposition from an applied review."""
    state, _ = coordinator._load(); domain = domain_state(state); runtime = runtime_state(state)
    blocked_jobs = set()
    for review in sorted(domain["reviews"].values(),
                         key=lambda row: (row["captures"]["domain_revision"], row["review_id"])):
        if review.get("status") != "applied" or not review["transition"]["job_reconciliation"]:
            continue
        if review["review_id"] not in runtime.get("reconciliations", {}):
            payload = _plan_payload(state, review)
            coordinator._observe("runtime.reconciliation-planned", payload,
                                 f"Persist job reconciliation for review {review['review_id']}",
                                 f"reconciliation-plan:{review['review_id']}:{review['applied_event_id']}")
            actions.append({"kind": "reconciliation_planned", "review_id": review["review_id"]})
            state, _ = coordinator._load(); runtime = runtime_state(state)
        plan = runtime["reconciliations"][review["review_id"]]
        for job_id in sorted(plan["items"]):
            if job_id in blocked_jobs:
                continue
            state, _ = coordinator._load(); runtime = runtime_state(state)
            item = runtime["reconciliations"][review["review_id"]]["items"][job_id]
            if item["status"] == "complete":
                continue
            job = runtime["jobs"].get(job_id)
            need(job is not None, "RUNTIME_STALE", "reconciliation job disappeared")
            if _paused(state, job):
                continue
            try:
                if item["status"] == "released":
                    _ready_revalidation(coordinator, state, review["review_id"], item, actions)
                    continue
                compatibility = _compatibility(state, job)
                action = item["action"]
                if item["status"] == "safe_to_release":
                    _release_revalidation(coordinator, state, review["review_id"], item, job, actions)
                elif action in {"drain", "revalidate"}:
                    _stop_step(coordinator, state, review["review_id"], item, job, compatibility, actions)
                elif compatibility["status"] != "compatible":
                    _progress(coordinator, review["review_id"], job_id, "needs_reconcile", compatibility,
                              detail=f"{action} refused because captured work is no longer compatible", actions=actions)
                elif action in {"continue", "finish_compatible"}:
                    if (job.get("result") or {}).get("state") in STOP_RESULTS or job["state"] == "unknown_effect":
                        _progress(coordinator, review["review_id"], job_id, "needs_reconcile", compatibility,
                                  detail=f"{action} cannot accept a stopped or unknown-effect job", actions=actions)
                    else:
                        _progress(coordinator, review["review_id"], job_id, "complete", compatibility,
                                  detail=f"{action} remains compatible with current contract and sources", actions=actions)
                elif action == "preserve_candidate":
                    result = job.get("result") or {}
                    if job["state"] == "result_ready" and (result.get("state") != "succeeded"
                            or result.get("diagnostic", {}).get("classification") != "success"):
                        _progress(coordinator, review["review_id"], job_id, "needs_reconcile", compatibility,
                                  transport=result, detail="Failed, blocked, stopped or invalid producer output was preserved only as a rework artifact",
                                  actions=actions)
                    elif job["state"] == "result_ready":
                        payload = {"schema": "zap-runtime/job-candidate-recorded/1", "job_id": job_id,
                                   "work_id": job["work_id"], "result_sha256": sha(packed(result))}
                        coordinator._runtime_action("runtime.job-candidate-recorded", payload,
                                                    f"Preserve candidate from review {review['review_id']}",
                                                    f"reconciliation-candidate:{review['review_id']}:{job_id}", job)
                        state, _ = coordinator._load(); job = runtime_state(state)["jobs"][job_id]
                    if job["state"] in {"candidate", "verification", "review_pending", "accepted"}:
                        _progress(coordinator, review["review_id"], job_id, "complete", compatibility,
                                  detail="Compatible candidate remains available without rerun", actions=actions)
            except Refusal as exc:
                if exc.code not in ADMISSION_PENDING:
                    raise
                actions.append({"kind": "reconciliation_admission_pending", "review_id": review["review_id"],
                                "job_id": job_id, "code": exc.code})
        state, _ = coordinator._load(); current_plan = runtime_state(state)["reconciliations"][review["review_id"]]
        blocked_jobs.update(job_id for job_id, item in current_plan["items"].items() if item["status"] != "complete")


__all__ = (
    "RECONCILIATION_ACTION_KINDS", "RECONCILIATION_DATA_KINDS", "RECONCILIATION_EVENT_ROUTES",
    "RECONCILIATION_EVENT_SCHEMAS", "RECONCILIATION_HANDLERS", "RECONCILIATION_OBSERVATION_KINDS",
    "reconcile_applied_reviews", "reconciliation_blocks_progress",
)
