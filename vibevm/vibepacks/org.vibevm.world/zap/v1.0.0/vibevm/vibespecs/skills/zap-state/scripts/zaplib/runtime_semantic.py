"""Relevant-scope semantic revalidation and evidence-backed closure views."""
from __future__ import annotations

import copy
from pathlib import Path

from .common import Refusal, exact, need, packed, parse, sha
from .control import active_policy
from .domain import build_sparse_review_transition, current_acceptance_coverage, domain_frontier, domain_state
from .knowledge import knowledge_snapshot, knowledge_state
from .runtime_model import runtime_state
from .runtime_packets import active_contract, source_captures
from .sources import compare_source_captures


def materialize_model_payload(state, kind, payload, *, expected_review_capture=None, request_knowledge=None):
    """Expand sparse review input and refresh only explicitly rebound CAS fields."""
    result = copy.deepcopy(payload)
    transition = result.get("transition") if isinstance(result, dict) else None
    if kind == "domain.review-proposed" and isinstance(transition, dict) and transition.get("schema") == "zap-domain/sparse-review-transition/1":
        result["transition"] = build_sparse_review_transition(state, transition)
    if kind == "domain.review-proposed" and expected_review_capture is not None:
        captures = result.get("captures")
        need(isinstance(captures, dict) and all(captures.get(key) == value for key, value in expected_review_capture.items()),
             "COORDINATOR_STALE", "semantic review capture was not bound to its request")
        captures["zap_revision"] = state["revision"]
        captures["domain_revision"] = domain_state(state)["revision"]
        declared = result.get("knowledge"); exact(declared, {"before", "after", "new_region_ids", "affected_dependencies", "closure_complete"})
        need(isinstance(declared["after"], dict) and isinstance(declared["after"].get("region_ids"), list)
             and isinstance(declared["new_region_ids"], list), "COORDINATOR", "semantic review knowledge selection differs")
        known = set((request_knowledge or {}).get("snapshot", {}).get("region_ids", []))
        available = set(knowledge_state(state)["regions"])
        region_ids = declared["after"]["region_ids"]; new_ids = declared["new_region_ids"]
        need(len(region_ids) == len(set(region_ids)) and len(new_ids) == len(set(new_ids)) and set(new_ids) <= set(region_ids),
             "COORDINATOR", "semantic review region identities differ")
        need(set(region_ids) <= available and set(region_ids) <= known | set(new_ids),
             "COORDINATOR", "semantic review named an uncaptured or missing region")
        need(not known.intersection(new_ids), "COORDINATOR", "semantic review marked a captured region as new")
        current_domain = domain_state(state); prior_id = current_domain["last_applied_review_id"]
        declared["before"] = None if prior_id is None else {"review_id": prior_id,
            "sha256": current_domain["reviews"][prior_id]["knowledge"]["after"]["sha256"]}
        declared["after"] = {"region_ids": list(region_ids), **knowledge_snapshot(state, region_ids)}
    return result


def semantic_job_view(job):
    """Return the bounded job facts needed for semantic review or acceptance."""
    keys = ("job_id", "attempt_id", "work_id", "state", "contract_version", "contract_sha256",
            "source_captures", "result", "result_sha256", "last_status_sha256")
    result = {key: copy.deepcopy(job[key]) for key in keys if key in job}
    worker_result = job.get("result") or {}; stdout = worker_result.get("stdout")
    if worker_result.get("state") == "succeeded" and isinstance(stdout, dict):
        raw = Path(stdout["path"]).read_bytes()
        need(len(raw) == stdout["bytes"] and sha(raw) == stdout["sha256"], "RUNTIME_STALE", "worker candidate output identity differs")
        report = parse(raw); need(isinstance(report, dict), "RUNTIME_VALUE", "worker candidate report must be a JSON object")
        result["candidate_report"] = report
    return result


def semantic_verification_view(row):
    keys = ("verification_id", "work_job_id", "work_id", "state", "evidence_id", "plan",
            "source_captures", "result", "result_sha256")
    result = {key: copy.deepcopy(row[key]) for key in keys if key in row}
    artifact = row.get("artifact_source")
    if artifact:
        result["artifact_source"] = copy.deepcopy(artifact)
        result.setdefault("source_captures", []).append({"source_id": artifact["source_id"], "sha256": artifact["sha256"]})
        result["source_captures"] = sorted(result["source_captures"], key=lambda item: item["source_id"])
        result["plan"]["source_refs"] = sorted(set(result["plan"]["source_refs"]) | {artifact["source_id"]})
    return result


def semantic_scope_sha256(body):
    """Hash only the scoped facts that may change a delayed semantic decision."""
    domain = copy.deepcopy(body["domain"]); domain.pop("revision", None); domain.pop("preservation_candidates", None)
    reviews = [{key: copy.deepcopy(row[key]) for key in ("review_request_id", "trigger_ids", "work_ids", "scope", "summary") if key in row}
               for row in body.get("review_requests", [])]
    verifications = copy.deepcopy(body.get("verification_jobs", []))
    for row in verifications:
        row.pop("artifact_content", None)
    return sha(packed({"domain": domain, "source_refs": body.get("source_refs", []),
                       "source_captures": body.get("source_captures", []), "source_descriptors": body.get("source_descriptors", []),
                       "knowledge": body.get("knowledge", {}), "review_requests": reviews,
                       "jobs": body.get("jobs", []), "verification_jobs": verifications}))


def scoped_semantic_context(state, review_requests, jobs, verification_jobs):
    """Build a relevant campaign view without serializing full domain/knowledge history."""
    domain = domain_state(state)
    work_ids = {work_id for row in review_requests for work_id in row.get("work_ids", [])}
    work_ids.update(row["work_id"] for row in jobs)
    ownership = [copy.deepcopy(row) for row in domain["ownership"] if row["work_id"] in work_ids]
    obligation_ids = {row["obligation_id"] for row in ownership}
    for work_id in work_ids:
        history = domain["task_contracts"].get(work_id)
        if history:
            active = next((row for row in history["versions"] if row["version"] == history["active_version"]), None)
            if active and active.get("schema") == "zap-task-contract/1":
                obligation_ids.update(active["contract"]["obligation_ids"])
    active_outcome = domain["outcome_revisions"].get(domain["active_outcome_id"])
    proposed_outcomes = [copy.deepcopy(row) for row in domain["outcome_revisions"].values()
                         if row.get("status") == "proposed" and row.get("previous_outcome_id") == domain["active_outcome_id"]]
    relevant_acceptances = {key: copy.deepcopy(row) for key, row in domain["acceptances"].items()
                            if row.get("work_id") in work_ids or obligation_ids.intersection(row.get("obligation_ids", []))}
    relevant_integrations = {key: copy.deepcopy(row) for key, row in domain["integration_acceptances"].items()
                             if row.get("work_id") in work_ids or obligation_ids.intersection(row.get("obligation_ids", []))}
    evidence_ids = {evidence_id for row in relevant_acceptances.values() for evidence_id in row.get("evidence_ids", [])}
    evidence_ids.update(evidence_id for row in relevant_integrations.values() for evidence_id in row.get("evidence_ids", []))
    evidence_ids.update(row["evidence_id"] for row in verification_jobs if row.get("evidence_id"))
    relevant_evidence = {key: copy.deepcopy(row) for key, row in domain["evidence_adjudications"].items()
                         if key in evidence_ids or work_ids.intersection(row.get("applies_to", {}).get("work_ids", []))}
    source_ids = {source_id for row in relevant_evidence.values() for source_id in row.get("source_refs", [])}
    source_ids.update(source_id for row in verification_jobs for source_id in row.get("plan", {}).get("source_refs", []))
    contracts = {}
    for work_id in sorted(work_ids):
        history = domain["task_contracts"].get(work_id)
        if not history:
            continue
        active = next((row for row in history["versions"] if row["version"] == history["active_version"]), None)
        if active:
            contracts[work_id] = copy.deepcopy(active)
            source_ids.update(active.get("contract", {}).get("source_handles", []))
    knowledge = knowledge_state(state)
    sources = knowledge["sources"]
    node_rows = {row["id"]: copy.deepcopy(row) for row in state["plan"]["node"] if row["id"] in work_ids}
    node_rows.update({key: copy.deepcopy(row) for key, row in domain["work_nodes"].items() if key in work_ids})
    stages = {key: copy.deepcopy(row) for key, row in domain["stages"].items() if row.get("work_id") in work_ids}
    deferrals = {key: copy.deepcopy(row) for key, row in domain["deferrals"].items()
                 if work_ids.intersection(row.get("work_ids", [])) or obligation_ids.intersection(row.get("obligation_ids", []))}
    last_review = domain["reviews"].get(domain["last_applied_review_id"])
    region_ids = sorted(key for key, row in knowledge["regions"].items() if work_ids.intersection(row.get("node_refs", [])))
    unknown_regions = {key: copy.deepcopy(row) for key, row in state.get("unknown_regions", {}).items()
                       if work_ids.intersection(row.get("node_refs", []))}
    endpoint_ids = {f"task:{key}" for key in work_ids} | {f"source:{key}" for key in source_ids}
    endpoint_ids |= {f"obligation:{key}" for key in obligation_ids}
    dependencies = {}
    changed = True
    while changed:
        changed = False
        for key, fact in knowledge["native_facts"].items():
            endpoint = f"fact:{key}"
            if f"source:{fact.get('source_id')}" in endpoint_ids and endpoint not in endpoint_ids:
                endpoint_ids.add(endpoint); changed = True
        for key, edge in knowledge["dependencies"].items():
            before = f"{edge['prerequisite']['kind']}:{edge['prerequisite']['id']}"
            after = f"{edge['dependent']['kind']}:{edge['dependent']['id']}"
            if key not in dependencies and (before in endpoint_ids or after in endpoint_ids):
                dependencies[key] = copy.deepcopy(edge); prior = len(endpoint_ids); endpoint_ids.update((before, after)); changed = changed or len(endpoint_ids) != prior
    source_ids.update(key.split(":", 1)[1] for key in endpoint_ids if key.startswith("source:"))
    native_facts = {key: copy.deepcopy(row) for key, row in knowledge["native_facts"].items() if f"fact:{key}" in endpoint_ids}
    source_rows = [{key: copy.deepcopy(source[key]) for key in ("id", "handle", "source_kind", "content_sha256", "capture_status", "applicability_scope") if key in source}
                   for source_id in sorted(source_ids) if (source := sources.get(source_id)) is not None]
    closures = {key: copy.deepcopy(row) for key, row in knowledge["closures"].items() if key in endpoint_ids}
    applicability = {key: copy.deepcopy(row) for key, row in knowledge["applicability"].items() if key in source_ids}
    invalidations = {key: copy.deepcopy(row) for key, row in knowledge["invalidations"].items()
                     if f"{row['cause']['kind']}:{row['cause']['id']}" in endpoint_ids or any(
                         f"{item['kind']}:{item['id']}" in endpoint_ids for item in row.get("affected", []))}
    knowledge_view = {"snapshot": {"region_ids": region_ids, **knowledge_snapshot(state, region_ids)},
                      "unknown_regions": unknown_regions, "dependencies": dependencies, "closures": closures,
                      "applicability": applicability, "invalidations": invalidations, "native_facts": native_facts}
    return {
        "domain": {"revision": domain["revision"], "active_intent_id": domain["active_intent_id"],
                   "active_intent": copy.deepcopy(domain["intents"].get(domain["active_intent_id"])),
                   "active_outcome_id": domain["active_outcome_id"], "active_outcome": copy.deepcopy(active_outcome),
                   "proposed_outcomes": proposed_outcomes, "last_applied_review_id": domain["last_applied_review_id"],
                   "last_applied_review": copy.deepcopy(last_review),
                   "work_nodes": node_rows, "work_updates": {key: copy.deepcopy(value) for key, value in domain["work_updates"].items() if key in work_ids},
                   "contracts": contracts, "obligations": {key: copy.deepcopy(domain["obligations"][key]) for key in sorted(obligation_ids)},
                   "ownership": ownership, "evidence_adjudications": relevant_evidence, "stages": stages,
                   "acceptances": relevant_acceptances, "integration_acceptances": relevant_integrations, "deferrals": deferrals,
                   "preservation_candidates": {"evidence_ids": sorted(domain["evidence_adjudications"]),
                                                "stage_acceptance_ids": sorted(domain["stages"]),
                                                "work_acceptance_ids": sorted(domain["acceptances"]),
                                                "integration_acceptance_ids": sorted(domain["integration_acceptances"]) }},
        "source_refs": sorted(source_ids),
        "source_captures": [{"source_id": row["id"], "sha256": row["content_sha256"]} for row in source_rows],
        "source_descriptors": source_rows, "knowledge": knowledge_view,
    }


def policy_binding(state):
    policy = active_policy(state)
    return None if policy is None else {"charter_id": policy["charter_id"], "charter_revision": policy["revision"],
                                         "charter_sha256": policy["charter_sha256"], "outcome_id": domain_state(state)["active_outcome_id"]}


def closure_view(state):
    domain = domain_state(state); runtime = runtime_state(state); coverage = current_acceptance_coverage(state)
    work_states = {node["id"]: domain["work_updates"].get(node["id"], {}).get("state", node["state"]) for node in state["plan"]["node"]}
    work_states.update({work_id: domain["work_updates"].get(work_id, {}).get("state", node["state"]) for work_id, node in domain["work_nodes"].items()})
    return {"active_intent_id": domain["active_intent_id"], "active_outcome_id": domain["active_outcome_id"],
            "original_outcome_id": domain["original_outcome_id"], "obligations": domain["obligations"], "deferrals": domain["deferrals"],
            "acceptances": domain["acceptances"], "integration_acceptances": domain["integration_acceptances"],
            "promotions": domain["promotions"], "evidence_adjudications": domain["evidence_adjudications"],
            "current_acceptance_coverage": coverage,
            "work_states": work_states, "jobs": {job_id: {"work_id": row["work_id"], "state": row["state"], "attempt_id": row["attempt_id"]}
                                                  for job_id, row in runtime["jobs"].items()}}


def rebind_basis(row, response, state):
    body = row["request"]["request"]
    if body.get("policy_binding") != policy_binding(state):
        return None
    kind = row["request_kind"]
    if kind == "selection":
        selected = response.get("selection")
        if not isinstance(selected, dict) or selected.get("work_id") not in domain_frontier(state):
            return None
        captured = next((item for item in body["frontier"] if item["work_id"] == selected["work_id"]), None)
        if captured is None:
            return None
        try:
            version, contract, contract_hash = active_contract(state, selected["work_id"])
            captures = source_captures(state, contract["source_handles"])
        except Refusal:
            return None
        if (version != captured["contract_version"] or contract_hash != captured["contract_sha256"] or
                captures != captured.get("source_captures") or compare_source_captures(state, captures)["status"] != "current"):
            return None
        basis = {"kind": kind, "work_id": selected["work_id"], "contract_sha256": contract_hash, "source_captures": captures,
                 "policy_binding": body["policy_binding"]}
    elif kind == "acceptance":
        runtime = runtime_state(state); captured_jobs = {job["job_id"]: job for job in body["jobs"]}
        for job_id, captured in captured_jobs.items():
            current = runtime["jobs"].get(job_id)
            if current is None or any(current.get(key) != captured.get(key) for key in ("work_id", "attempt_id", "contract_sha256", "result_sha256")):
                return None
        captured_verifications = {item["verification_id"]: item for item in body["verification_jobs"]}
        for verification_id, captured in captured_verifications.items():
            current = runtime["verification_jobs"].get(verification_id)
            if current is None or current.get("state") != "observed" or sha(packed(current.get("result"))) != sha(packed(captured.get("result"))):
                return None
        current_reviews = [runtime["review_requests"].get(row["review_request_id"]) for row in body["review_requests"]]
        if any(row is None for row in current_reviews):
            return None
        current_jobs = [semantic_job_view(runtime["jobs"][job_id]) for job_id in captured_jobs]
        current_verifications = [semantic_verification_view(runtime["verification_jobs"][verification_id]) for verification_id in captured_verifications]
        current_body = {"review_requests": current_reviews, "jobs": current_jobs, "verification_jobs": current_verifications,
                        **scoped_semantic_context(state, current_reviews, current_jobs, current_verifications)}
        if semantic_scope_sha256(current_body) != body.get("scope_sha256"):
            return None
        basis = {"kind": kind, "jobs": sorted(captured_jobs), "verifications": sorted(captured_verifications), "policy_binding": body["policy_binding"]}
    elif kind in {"review", "reassessment"}:
        runtime = runtime_state(state); captured_ids = [item["review_request_id"] for item in body["review_requests"]]
        if any(review_id not in runtime["review_requests"] or runtime["review_requests"][review_id]["state"] != "in_progress" for review_id in captured_ids):
            return None
        if any(item["state"] == "pending" and item["review_request_id"] not in captured_ids for item in runtime["review_requests"].values()):
            return None
        current_reviews = [runtime["review_requests"][review_id] for review_id in captured_ids]
        job_ids = [row["job_id"] for row in body["jobs"]]
        verification_ids = [row["verification_id"] for row in body["verification_jobs"]]
        if any(job_id not in runtime["jobs"] for job_id in job_ids) or any(key not in runtime["verification_jobs"] for key in verification_ids):
            return None
        current_jobs = [semantic_job_view(runtime["jobs"][job_id]) for job_id in job_ids]
        current_verifications = [semantic_verification_view(runtime["verification_jobs"][key]) for key in verification_ids]
        current_body = {"review_requests": current_reviews, "jobs": current_jobs, "verification_jobs": current_verifications,
                        **scoped_semantic_context(state, current_reviews, current_jobs, current_verifications)}
        if semantic_scope_sha256(current_body) != body.get("scope_sha256"):
            return None
        basis = {"kind": kind, "reviews": captured_ids, "last_applied_review_id": domain_state(state)["last_applied_review_id"],
                 "policy_binding": body["policy_binding"]}
    elif kind == "closure":
        current_hash = sha(packed(closure_view(state)))
        if current_hash != body["closure_basis_sha256"]:
            return None
        basis = {"kind": kind, "closure_basis_sha256": current_hash, "policy_binding": body["policy_binding"]}
    else:
        return None
    return sha(packed(basis))


def completion_ready(state):
    domain = domain_state(state); runtime = runtime_state(state)
    if domain["closure"] is not None or domain["active_outcome_id"] is None or any(row["status"] == "open" for row in domain["deferrals"].values()):
        return False
    active = {key for key, row in domain["obligations"].items() if row["status"] == "active"}
    covered = set(current_acceptance_coverage(state)["obligation_ids"])
    return bool(active) and active <= covered and all(job["state"] in {"accepted", "retry_released"} for job in runtime["jobs"].values())
