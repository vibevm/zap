"""Bounded semantic request scheduling for the automatic coordinator."""
from .common import Refusal, packed, sha
from .control import active_policy
from .coordinator_adapter import COMMANDS_BY_REQUEST, command_contracts_for
from .domain import domain_frontier
from .runtime_artifacts import attach_artifact_content
from .runtime_liveness import repair_feedback, selection_basis
from .runtime_model import ACTIVE_JOB_STATES, runtime_state
from .runtime_packets import active_contract, source_captures
from .runtime_semantic import (closure_view, completion_ready, policy_binding, scoped_semantic_context,
                               semantic_job_view, semantic_scope_sha256, semantic_verification_view)


def schedule_semantic(coordinator, actions):
    state, _ = coordinator._load(); runtime = runtime_state(state)
    if any(row["state"] in {"requested", "submitted"} for row in runtime["semantic_requests"].values()): return
    if any(row["state"] == "waiting" and row.get("operation_kind") == "semantic" for row in runtime["resource_waits"].values()): return
    reviews = [row for row in runtime["review_requests"].values() if row["state"] == "pending"]
    if reviews:
        candidate_jobs = [job for job in runtime["jobs"].values() if any(job["work_id"] in review["work_ids"] for review in reviews)]
        if candidate_jobs and all(review["scope"] == "candidate_acceptance" for review in reviews):
            kind = "acceptance"
        elif any(review["scope"] in {"source_invalidation", "contract_changed", "native_facts_changed"} for review in reviews):
            kind = "reassessment"
        else:
            kind = "review"
        verification_jobs = [row for row in runtime["verification_jobs"].values() if any(row["work_job_id"] == job["job_id"] for job in candidate_jobs)]
        job_views = [semantic_job_view(row) for row in candidate_jobs]
        verification_views = [attach_artifact_content(semantic_verification_view(row), coordinator.config.artifact_reader) for row in verification_jobs]
        body = {"review_requests": reviews, "jobs": job_views, "verification_jobs": verification_views,
                **scoped_semantic_context(state, reviews, job_views, verification_views),
                "policy_binding": policy_binding(state), "allowed_commands": sorted(COMMANDS_BY_REQUEST[kind]),
                "command_contracts": command_contracts_for(kind)}
        body["scope_sha256"] = semantic_scope_sha256(body); body["repair_feedback"] = repair_feedback(runtime, kind, body["scope_sha256"])
        coordinator._semantic_request(kind, body, sha(packed({"reviews": [row["review_request_id"] for row in reviews]}))[:12], actions); return
    if any(row["state"] == "awaiting_evidence" for row in runtime["review_requests"].values()): return
    frontier = domain_frontier(state)
    if frontier and active_policy(state) is not None:
        rows = []
        for work_id in frontier:
            try:
                version, contract, contract_hash = active_contract(state, work_id)
                contract_view = {key: contract[key] for key in ("title", "goal", "read_subjects", "write_subjects", "resources", "integration_owner", "required_stage", "source_handles", "obligation_ids")}
                rows.append({"work_id": work_id, "contract_version": version, "contract_sha256": contract_hash,
                             "contract": contract_view, "source_captures": source_captures(state, contract["source_handles"])})
            except Refusal: continue
        if rows:
            body = {"frontier": rows, "active_jobs": [semantic_job_view(job) for job in runtime["jobs"].values() if job["state"] in ACTIVE_JOB_STATES],
                    "resource_waits": [{key: wait.get(key) for key in ("wait_id", "classification", "capability", "operation_kind", "next_retry_ns")}
                                       for wait in runtime["resource_waits"].values() if wait["state"] == "waiting"],
                    "policy_binding": policy_binding(state), "allowed_commands": [], "command_contracts": {}}
            body["selection_basis_sha256"] = selection_basis(body); body["repair_feedback"] = repair_feedback(runtime, "selection", body["selection_basis_sha256"])
            if any(row["request_kind"] == "selection" and row["state"] in {"no_action", "rejected"} and
                   row["request"]["request"].get("selection_basis_sha256") == body["selection_basis_sha256"] for row in runtime["semantic_requests"].values()): return
            coordinator._semantic_request("selection", body, "frontier", actions)
    elif active_policy(state) is not None and completion_ready(state):
        current_closure_view = closure_view(state); closure_hash = sha(packed(current_closure_view))
        if any(row["request_kind"] == "closure" and row["request"]["request"].get("closure_basis_sha256") == closure_hash and
               row["state"] in {"requested", "submitted", "no_action", "rejected", "applied"} for row in runtime["semantic_requests"].values()): return
        body = {"closure_view": current_closure_view, "closure_basis_sha256": closure_hash,
                "allowed_classifications": ["original", "revised"], "policy_binding": policy_binding(state),
                "allowed_commands": sorted(COMMANDS_BY_REQUEST["closure"]), "command_contracts": command_contracts_for("closure")}
        body["repair_feedback"] = repair_feedback(runtime, "closure", closure_hash)
        coordinator._semantic_request("closure", body, "success", actions)


__all__ = ("schedule_semantic",)
