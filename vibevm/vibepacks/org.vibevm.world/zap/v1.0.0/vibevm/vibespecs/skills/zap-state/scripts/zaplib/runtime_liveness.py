"""Semantic no-action suppression and evidence-driven review reopening."""
from __future__ import annotations

from .common import Refusal, packed, sha
from .domain import domain_frontier
from .runtime_model import runtime_state
from .runtime_packets import active_contract, source_captures
from .runtime_semantic import closure_view, policy_binding, scoped_semantic_context, semantic_job_view, semantic_scope_sha256, semantic_verification_view


def reopen_changed_reviews(coordinator, actions):
    state, _ = coordinator._load(); runtime = runtime_state(state)
    for review_id, review in runtime["review_requests"].items():
        if review["state"] != "awaiting_evidence" or not review.get("semantic_request_id"):
            continue
        request = runtime["semantic_requests"].get(review["semantic_request_id"])
        if not request:
            continue
        prior_body = request["request"]["request"]; prior_hash = prior_body.get("scope_sha256")
        jobs = [semantic_job_view(job) for job in runtime["jobs"].values() if job["work_id"] in review["work_ids"]]
        job_ids = {job["job_id"] for job in jobs}
        checks = [semantic_verification_view(row) for row in runtime["verification_jobs"].values() if row["work_job_id"] in job_ids]
        current_body = {"review_requests": [review], "jobs": jobs, "verification_jobs": checks,
                        **scoped_semantic_context(state, [review], jobs, checks)}
        current_hash = semantic_scope_sha256(current_body)
        if prior_hash is None or current_hash == prior_hash:
            continue
        payload = {"schema": "zap-runtime/review-reopened/1", "review_request_id": review_id,
                   "prior_semantic_request_id": review["semantic_request_id"], "prior_scope_sha256": prior_hash,
                   "current_scope_sha256": current_hash}
        coordinator._observe("runtime.review-reopened", payload, f"Reopen review after scoped evidence changed {review_id}",
                             f"review-reopen:{review_id}:{current_hash}")
        actions.append({"kind": "review_reopened", "review_request_id": review_id})


def selection_basis(body):
    return sha(packed({key: body.get(key) for key in ("frontier", "active_jobs", "policy_binding")}))


def repair_feedback(runtime, request_kind, operation_basis):
    rows = [row for row in runtime["resource_waits"].values() if row.get("operation_kind") == "semantic"
            and row.get("classification") == "model_response_invalid" and row.get("diagnostic", {}).get("operation_basis") == operation_basis
            and runtime["semantic_requests"].get(row.get("request_id"), {}).get("request_kind") == request_kind]
    if not rows:
        return None
    row = rows[-1]
    return {key: row.get("diagnostic", {}).get(key) for key in ("repair_attempt", "validator_feedback", "provider_response_sha256")}


def semantic_wait_input_changed(state, request_row):
    body = request_row["request"]["request"]; kind = request_row["request_kind"]
    if body.get("policy_binding") != policy_binding(state):
        return True
    runtime = runtime_state(state)
    if kind == "selection":
        rows = []
        for work_id in domain_frontier(state):
            try:
                version, contract, contract_hash = active_contract(state, work_id)
            except Refusal:
                continue
            contract_view = {key: contract[key] for key in ("title", "goal", "read_subjects", "write_subjects", "resources", "integration_owner", "required_stage", "source_handles", "obligation_ids")}
            rows.append({"work_id": work_id, "contract_version": version, "contract_sha256": contract_hash,
                         "contract": contract_view, "source_captures": source_captures(state, contract["source_handles"])})
        current = {"frontier": rows, "active_jobs": [semantic_job_view(job) for job in runtime["jobs"].values() if job["state"] in {"claimed", "dispatched", "starting", "running", "stop_requested", "stopping", "unknown_effect"}],
                   "policy_binding": policy_binding(state)}
        return selection_basis(current) != body.get("selection_basis_sha256")
    if kind == "closure":
        return sha(packed(closure_view(state))) != body.get("closure_basis_sha256")
    reviews = [runtime["review_requests"].get(row["review_request_id"]) for row in body.get("review_requests", [])]
    if any(row is None for row in reviews):
        return True
    job_ids = [row["job_id"] for row in body.get("jobs", [])]; check_ids = [row["verification_id"] for row in body.get("verification_jobs", [])]
    if any(key not in runtime["jobs"] for key in job_ids) or any(key not in runtime["verification_jobs"] for key in check_ids):
        return True
    jobs = [semantic_job_view(runtime["jobs"][key]) for key in job_ids]
    checks = [semantic_verification_view(runtime["verification_jobs"][key]) for key in check_ids]
    current = {"review_requests": reviews, "jobs": jobs, "verification_jobs": checks,
               **scoped_semantic_context(state, reviews, jobs, checks)}
    return semantic_scope_sha256(current) != body.get("scope_sha256")


__all__ = ("reopen_changed_reviews", "repair_feedback", "selection_basis", "semantic_wait_input_changed")
