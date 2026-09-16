"""Adaptive-review proposal/application and truthful campaign closure."""
from __future__ import annotations

from typing import Any

from .common import Refusal, exact, identity, need
from .domain_graph import _adopt_intent, _adopt_outcome, _disposition_rows
from .domain_model import (
    active_obligations, boolean, digest, domain_handler, effective_nodes, integer,
    domain_state, nonblank_list, optional_identity, owned_obligations, require_action, rows,
    strict, text, unique_ids, validation_generation,
)
from .domain_proof import accepted_evidence
from .domain_reuse import (
    acceptance_for_outcome, apply_preserved_reuse, integration_for_outcome,
    validate_preserved_candidates, validate_preserved_current,
)

REVIEW_KINDS = {"keep_route", "reorder", "research", "replace_method", "pivot_outcome", "wait", "owner_proposal"}
WORK_OPERATIONS = {"retain", "reprioritize", "supersede", "drop", "revalidate"}
JOB_ACTIONS = {"continue", "finish_compatible", "drain", "preserve_candidate", "revalidate"}
CLOSURE_CLASSES = {"original", "revised", "partial", "unreachable"}


def _builder_policy(state):
    from .control import active_policy
    return active_policy(state)


def _capture(value):
    exact(value, {
        "base_sha256", "zap_revision", "domain_revision", "intent_id", "outcome_id",
        "policy_revision", "source_captures", "jobs",
    })
    digest(value["base_sha256"], "captured base")
    integer(value["zap_revision"], "captured ZAP revision")
    integer(value["domain_revision"], "captured domain revision")
    optional_identity(value["intent_id"], "captured intent")
    optional_identity(value["outcome_id"], "captured outcome")
    integer(value["policy_revision"], "captured policy revision", minimum=1)
    source_ids = set()
    for source in rows(value["source_captures"], "review source captures"):
        exact(source, {"source_id", "sha256"})
        key = identity(source["source_id"]); digest(source["sha256"])
        need(key not in source_ids, "DUPLICATE", "duplicate review source")
        source_ids.add(key)
    job_ids = set()
    for job in rows(value["jobs"], "review jobs"):
        exact(job, {"job_id", "status", "attempt_id"})
        key = identity(job["job_id"]); optional_identity(job["attempt_id"], "job attempt")
        text(job["status"], "job status")
        need(key not in job_ids, "DUPLICATE", "duplicate review job")
        job_ids.add(key)


def _knowledge(value):
    exact(value, {"before", "after", "new_region_ids", "affected_dependencies", "closure_complete"})
    if value["before"] is not None:
        exact(value["before"], {"review_id", "sha256"})
        identity(value["before"]["review_id"]); digest(value["before"]["sha256"], "prior knowledge snapshot")
    exact(value["after"], {"region_ids", "revision", "sha256", "regions"})
    unique_ids(value["after"]["region_ids"], "knowledge snapshot regions")
    integer(value["after"]["revision"], "knowledge snapshot revision")
    digest(value["after"]["sha256"], "knowledge snapshot")
    need(isinstance(value["after"]["regions"], dict)
         and set(value["after"]["regions"]) == set(value["after"]["region_ids"]),
         "DOMAIN_REVIEW", "knowledge snapshot rows differ from selected regions")
    unique_ids(value["new_region_ids"], "new knowledge regions")
    need(set(value["new_region_ids"]) <= set(value["after"]["region_ids"]),
         "DOMAIN_REVIEW", "new regions must be in the captured knowledge snapshot")
    unique_ids(value["affected_dependencies"], "affected dependencies")
    boolean(value["closure_complete"], "dependency closure completeness")


def _alternative(row):
    exact(row, {"id", "description", "value", "feasibility", "remaining_cost", "risks", "unknowns"})
    identity(row["id"])
    for field in ("description", "value", "feasibility", "remaining_cost"):
        text(row[field], f"alternative {field}")
    nonblank_list(row["risks"], "alternative risks")
    nonblank_list(row["unknowns"], "alternative unknowns")


def _work_change(row):
    exact(row, {"work_id", "operation", "order", "successor_ids", "reason"})
    identity(row["work_id"])
    need(row["operation"] in WORK_OPERATIONS, "DOMAIN_VALUE", "invalid review work operation")
    if row["order"] is not None:
        integer(row["order"], "review work order")
    unique_ids(row["successor_ids"], "review work successors")
    text(row["reason"], "work change reason")
    need((row["operation"] == "reprioritize") == (row["order"] is not None),
         "DOMAIN_VALUE", "only reprioritize supplies order")
    need((row["operation"] == "supersede") == bool(row["successor_ids"]),
         "DOMAIN_VALUE", "only supersede supplies successors")


def _transition(value):
    exact(value, {
        "intent_id", "outcome_id", "obligation_dispositions", "ownership_changes", "work_changes", "preserved_evidence_ids",
        "preserved_stage_acceptance_ids", "preserved_work_acceptance_ids", "preserved_integration_acceptance_ids",
        "job_reconciliation", "tradeoffs", "preserved_benefits",
    })
    optional_identity(value["intent_id"], "transition intent")
    optional_identity(value["outcome_id"], "transition outcome")
    _disposition_rows(value["obligation_dispositions"])
    ownership = rows(value["ownership_changes"], "ownership changes")
    for row in ownership:
        exact(row, {"obligation_id", "from_work_id", "assignments", "reason"})
        identity(row["obligation_id"]); identity(row["from_work_id"]); text(row["reason"], "ownership change reason")
        for assignment in rows(row["assignments"], "ownership assignments"):
            exact(assignment, {"work_id", "role"})
            identity(assignment["work_id"])
            need(assignment["role"] in {"implementation", "verification", "integration", "acceptance"},
                 "DOMAIN_VALUE", "invalid ownership role")
        need(len({(assignment["work_id"], assignment["role"]) for assignment in row["assignments"]})
             == len(row["assignments"]), "DUPLICATE", "duplicate ownership assignment")
    need(len({(row["obligation_id"], row["from_work_id"]) for row in ownership}) == len(ownership),
         "DUPLICATE", "duplicate ownership change")
    changes = rows(value["work_changes"], "review work changes")
    for row in changes:
        _work_change(row)
    need(len({row["work_id"] for row in changes}) == len(changes), "DUPLICATE", "duplicate review work change")
    unique_ids(value["preserved_evidence_ids"], "preserved evidence")
    unique_ids(value["preserved_stage_acceptance_ids"], "preserved stages")
    unique_ids(value["preserved_work_acceptance_ids"], "preserved work acceptances")
    unique_ids(value["preserved_integration_acceptance_ids"], "preserved integration acceptances")
    jobs = rows(value["job_reconciliation"], "job reconciliation")
    for row in jobs:
        exact(row, {"job_id", "action", "safe_boundary", "reason"})
        identity(row["job_id"]); need(row["action"] in JOB_ACTIONS, "DOMAIN_VALUE", "invalid job reconciliation")
        text(row["safe_boundary"], "job safe boundary"); text(row["reason"], "job reconciliation reason")
    need(len({row["job_id"] for row in jobs}) == len(jobs), "DUPLICATE", "duplicate reconciled job")
    nonblank_list(value["tradeoffs"], "transition tradeoffs")
    nonblank_list(value["preserved_benefits"], "preserved benefits", nonempty=True)


def build_sparse_review_transition(state: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
    """Expand sparse changed dispositions into the exact auditable review transition."""
    request = strict("zap-domain/sparse-review-transition/1", {
        "intent_id", "outcome_id", "changed_dispositions", "ownership_changes", "work_changes",
        "preserved_evidence_ids", "preserved_stage_acceptance_ids", "preserved_work_acceptance_ids",
        "preserved_integration_acceptance_ids", "job_reconciliation", "tradeoffs", "preserved_benefits",
    })(request)
    optional_identity(request["intent_id"], "transition intent")
    outcome_id = optional_identity(request["outcome_id"], "transition outcome")
    need(outcome_id is not None, "DOMAIN_OUTCOME", "sparse outcome transition requires an outcome")
    changes = _disposition_rows(request["changed_dispositions"])
    domain = domain_state(state)
    policy = _builder_policy(state)
    need(policy is not None and policy["adaptation"]["allow_target_revision"],
         "DOMAIN_POLICY", "sparse outcome revision is not delegated")
    outcome = domain["outcome_revisions"].get(outcome_id)
    need(outcome and outcome["status"] == "proposed" and outcome["previous_outcome_id"] == domain["active_outcome_id"],
         "DOMAIN_OUTCOME", "sparse transition outcome is not the next proposal")
    active = active_obligations(domain)
    changed = {row["obligation_id"]: row for row in changes}
    need(set(changed) <= active, "REFERENCE", "sparse disposition names a non-current obligation")
    allowed = set(policy["adaptation"]["allowed_dispositions"])
    mutable = set(policy["adaptation"]["mutable_obligations"])
    essential = set(policy["adaptation"]["essential_obligations"])
    need("retained" in allowed, "DOMAIN_POLICY", "retaining unchanged obligations is not delegated")
    proposed = {row["id"] for row in outcome["obligations"]}
    for obligation_id, row in changed.items():
        need(row["disposition"] in allowed, "DOMAIN_POLICY", "sparse disposition is not delegated")
        if obligation_id not in mutable or obligation_id in essential:
            need(row["disposition"] == "retained", "DOMAIN_POLICY",
                 "immutable or essential sparse obligation must be retained")
        need(set(row["successor_ids"]) <= proposed, "REFERENCE", "sparse replacement successor missing")
    dispositions = []
    for obligation_id in sorted(active):
        dispositions.append(changed.get(obligation_id, {
            "obligation_id": obligation_id, "disposition": "retained", "successor_ids": [],
            "unmet_portion": "", "reason": "Unchanged obligation retained by sparse expansion.",
        }))
    transition = {
        "intent_id": request["intent_id"], "outcome_id": outcome_id,
        "obligation_dispositions": dispositions, "ownership_changes": request["ownership_changes"],
        "work_changes": request["work_changes"], "preserved_evidence_ids": request["preserved_evidence_ids"],
        "preserved_stage_acceptance_ids": request["preserved_stage_acceptance_ids"],
        "preserved_work_acceptance_ids": request["preserved_work_acceptance_ids"],
        "preserved_integration_acceptance_ids": request["preserved_integration_acceptance_ids"],
        "job_reconciliation": request["job_reconciliation"], "tradeoffs": request["tradeoffs"],
        "preserved_benefits": request["preserved_benefits"],
    }
    _transition(transition)
    validate_preserved_candidates(state, domain, transition, domain["active_outcome_id"], outcome_id)
    return transition


def _review_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/review-proposed/1", {
        "review_id", "previous_review_id", "signals", "captures", "knowledge",
        "alternatives", "chosen", "decision", "transition", "next_trigger",
    })(value)
    identity(value["review_id"]); optional_identity(value["previous_review_id"], "previous review")
    nonblank_list(value["signals"], "review signals", nonempty=True)
    _capture(value["captures"]); _knowledge(value["knowledge"])
    alternatives = rows(value["alternatives"], "review alternatives", nonempty=True)
    for row in alternatives:
        _alternative(row)
    ids = [row["id"] for row in alternatives]
    need(len(ids) == len(set(ids)), "DUPLICATE", "duplicate review alternative")
    need(value["chosen"] in ids, "DOMAIN_REVIEW", "chosen alternative missing")
    exact(value["decision"], {"kind", "rationale"})
    need(value["decision"]["kind"] in REVIEW_KINDS, "DOMAIN_VALUE", "invalid review decision")
    text(value["decision"]["rationale"], "review rationale")
    _transition(value["transition"]); text(value["next_trigger"], "next review trigger")
    pivot = value["decision"]["kind"] == "pivot_outcome"
    need(pivot == (value["transition"]["outcome_id"] is not None), "DOMAIN_REVIEW", "pivot must name one outcome")
    return value


def _apply_review_proposed(state, domain, payload, event_id):
    key = payload["review_id"]
    need(key not in domain["reviews"], "DUPLICATE", "review identity exists")
    need(payload["previous_review_id"] == domain["last_applied_review_id"],
         "DOMAIN_STALE", "review is not based on last applied review")
    capture = payload["captures"]
    need(capture["base_sha256"] == state["base_sha256"] and capture["zap_revision"] == state["revision"]
         and capture["domain_revision"] == domain["revision"], "DOMAIN_STALE", "review capture differs")
    need(capture["intent_id"] == domain["active_intent_id"] and capture["outcome_id"] == domain["active_outcome_id"],
         "DOMAIN_STALE", "review target capture differs")
    knowledge = payload["knowledge"]
    previous = payload["previous_review_id"]
    if previous is None:
        need(knowledge["before"] is None, "DOMAIN_REVIEW", "initial review cannot invent a prior knowledge view")
    else:
        prior = domain["reviews"][previous]["knowledge"]["after"]
        need(knowledge["before"] == {"review_id": previous, "sha256": prior["sha256"]},
             "DOMAIN_STALE", "prior knowledge view differs from the applied review")
        need(not set(knowledge["new_region_ids"]) & set(prior["region_ids"]),
             "DOMAIN_REVIEW", "new region already existed in the prior review view")
    _check_knowledge(state, knowledge["after"])
    domain["reviews"][key] = {**payload, "status": "proposed", "event_id": event_id}


def _runtime_jobs(state):
    runtime = state.get("extensions", {}).get("runtime")
    if runtime is None:
        return {}
    need(isinstance(runtime, dict) and isinstance(runtime.get("jobs", {}), dict), "DOMAIN_STORE", "invalid runtime jobs")
    return runtime.get("jobs", {})


def _check_jobs(state, captured):
    jobs = _runtime_jobs(state)
    need(not captured or jobs, "DOMAIN_STALE", "captured jobs unavailable")
    for row in captured:
        current = jobs.get(row["job_id"])
        need(current and current.get("state", current.get("status")) == row["status"]
             and current.get("attempt_id") == row["attempt_id"],
             "DOMAIN_STALE", "captured job changed")


def _check_sources(state, captured):
    if not captured:
        return
    from .sources import compare_source_captures
    result = compare_source_captures(state, captured)
    need(result["status"] == "current", "DOMAIN_STALE", "captured review source changed or disappeared")


def _check_knowledge(state, captured):
    from .knowledge import knowledge_snapshot
    snapshot = knowledge_snapshot(state, captured["region_ids"])
    current = {"region_ids": list(captured["region_ids"]), **snapshot}
    need(current == captured,
         "DOMAIN_STALE", "captured knowledge regions changed or were fabricated")


def _apply_work_changes(state, domain, changes):
    nodes = effective_nodes(state, domain)
    for change in changes:
        key = change["work_id"]
        need(key in nodes, "REFERENCE", "review work missing")
        operation = change["operation"]
        update = domain["work_updates"].setdefault(key, {})
        if operation == "reprioritize":
            update["order"] = change["order"]
        elif operation == "revalidate":
            update["revalidation_required"] = True
            update["revalidation_from_generation"] = validation_generation(domain, key)
        elif operation == "supersede":
            need(set(change["successor_ids"]) <= set(nodes) - {key}, "REFERENCE", "review successor missing")
            need(not owned_obligations(domain, key), "DOMAIN_OBLIGATION", "superseded work still owns active obligations")
            update["state"] = "superseded"; domain["work_successors"][key] = change["successor_ids"]
        elif operation == "drop":
            need(not owned_obligations(domain, key), "DOMAIN_OBLIGATION", "dropped work still owns active obligations")
            update["state"] = "dropped"; update["dependency_resolved"] = True


def _apply_ownership_changes(state, domain, changes):
    nodes = effective_nodes(state, domain)
    for change in changes:
        obligation_id = change["obligation_id"]
        need(obligation_id in domain["obligations"], "REFERENCE", "ownership obligation missing")
        before = len(domain["ownership"])
        domain["ownership"] = [
            row for row in domain["ownership"]
            if not (row["obligation_id"] == obligation_id and row["work_id"] == change["from_work_id"])
        ]
        need(len(domain["ownership"]) < before, "REFERENCE", "ownership source link missing")
        assignments = change["assignments"]
        need(assignments or domain["obligations"][obligation_id]["status"] != "active",
             "DOMAIN_OBLIGATION", "active obligation must retain an owner")
        for assignment in assignments:
            need(assignment["work_id"] in nodes, "REFERENCE", "ownership target work missing")
            relation = {
                "type": "obligation.owned_by", "obligation_id": obligation_id,
                "work_id": assignment["work_id"], "role": assignment["role"],
                "outcome_id": domain["active_outcome_id"],
            }
            if relation not in domain["ownership"]:
                domain["ownership"].append(relation)
        domain["obligations"][obligation_id]["owner_work_ids"] = sorted({
            row["work_id"] for row in domain["ownership"] if row["obligation_id"] == obligation_id
        })


def _apply_review(state, domain, payload, event_id):
    policy = require_action(state, "adaptive.apply")
    review = domain["reviews"].get(payload["review_id"])
    need(review and review["status"] == "proposed", "DOMAIN_REVIEW", "review proposal missing")
    need(payload["expected_domain_revision"] == domain["revision"], "DOMAIN_STALE", "domain revision differs")
    capture = review["captures"]
    need(domain["revision"] == capture["domain_revision"] + 1, "DOMAIN_STALE", "domain changed after review proposal")
    need(policy["revision"] == capture["policy_revision"], "DOMAIN_STALE", "charter policy changed")
    need(domain["active_intent_id"] == capture["intent_id"] and domain["active_outcome_id"] == capture["outcome_id"],
         "DOMAIN_STALE", "review target changed")
    _check_sources(state, capture["source_captures"])
    _check_knowledge(state, review["knowledge"]["after"])
    _check_jobs(state, capture["jobs"])
    transition = review["transition"]
    from_outcome = capture["outcome_id"]
    need({row["job_id"] for row in transition["job_reconciliation"]} == {row["job_id"] for row in capture["jobs"]},
         "DOMAIN_REVIEW", "every captured job needs reconciliation")
    if review["decision"]["kind"] == "pivot_outcome":
        validate_preserved_candidates(state, domain, transition, from_outcome, transition["outcome_id"])
        if transition["intent_id"] is not None:
            _adopt_intent(state, domain, transition["intent_id"], event_id, action="adaptive.apply")
        _adopt_outcome(state, domain, transition["outcome_id"], transition["obligation_dispositions"], event_id,
                       action_checked=True)
    else:
        need(transition["intent_id"] is None and not transition["obligation_dispositions"],
             "DOMAIN_REVIEW", "non-pivot cannot revise intent or dispose obligations")
    _apply_ownership_changes(state, domain, transition["ownership_changes"])
    _apply_work_changes(state, domain, transition["work_changes"])
    if review["decision"]["kind"] == "pivot_outcome":
        apply_preserved_reuse(state, domain, review, event_id, from_outcome, domain["active_outcome_id"])
    else:
        validate_preserved_current(state, domain, review)
    review["status"] = "applied"; review["applied_event_id"] = event_id
    review["job_effect_status"] = "planned_not_performed_by_reducer"
    domain["last_applied_review_id"] = payload["review_id"]


def _review_apply_payload(value):
    value = strict("zap-domain/review-applied/1", {"review_id", "expected_domain_revision"})(value)
    identity(value["review_id"]); integer(value["expected_domain_revision"], "expected domain revision")
    return value


def _obligation_result(row):
    exact(row, {"obligation_id", "result", "unmet_portion", "successor_ids", "evidence_ids"})
    identity(row["obligation_id"])
    need(row["result"] in {"accepted", "retained_unmet", "replaced", "excluded", "unattainable"},
         "DOMAIN_VALUE", "invalid obligation result")
    text(row["unmet_portion"], "obligation unmet portion") if row["result"] != "accepted" else None
    unique_ids(row["successor_ids"], "closure successors")
    unique_ids(row["evidence_ids"], "closure evidence", nonempty=row["result"] == "accepted")


def _closure_payload(value):
    value = strict("zap-domain/campaign-closed/1", {
        "closure_id", "classification", "active_outcome_id", "actual_benefit", "obligation_results",
        "acceptance_ids", "integration_acceptance_ids", "deferral_ids", "promotion_ids",
        "final_gate_evidence_ids", "summary",
    })(value)
    identity(value["closure_id"]); identity(value["active_outcome_id"])
    need(value["classification"] in CLOSURE_CLASSES, "DOMAIN_VALUE", "invalid closure classification")
    text(value["actual_benefit"], "actual benefit"); text(value["summary"], "closure summary")
    results = rows(value["obligation_results"], "obligation results", nonempty=True)
    for row in results:
        _obligation_result(row)
    need(len({row["obligation_id"] for row in results}) == len(results), "DUPLICATE", "duplicate closure obligation")
    for field in ("acceptance_ids", "integration_acceptance_ids", "deferral_ids", "promotion_ids", "final_gate_evidence_ids"):
        unique_ids(value[field], field, nonempty=field == "final_gate_evidence_ids")
    return value


def _apply_closure(state, domain, payload, event_id):
    require_action(state, "campaign.close")
    need(domain["closure"] is None, "DUPLICATE", "campaign already closed")
    need(payload["active_outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "closure outcome is not active")
    result_by_id = {row["obligation_id"]: row for row in payload["obligation_results"]}
    need(set(result_by_id) == set(domain["obligations"]), "DOMAIN_CLOSURE", "every historical obligation needs an outcome")
    accepted_obligations = set()
    for acceptance_id in domain["acceptances"]:
        try:
            row, _ = acceptance_for_outcome(state, domain, acceptance_id, domain["active_outcome_id"])
            accepted_obligations.update(row["obligation_ids"])
        except Refusal:
            continue
    for key, obligation in domain["obligations"].items():
        result = result_by_id[key]
        if obligation["status"] == "active":
            if result["result"] == "accepted":
                need(key in accepted_obligations, "DOMAIN_CLOSURE", "obligation lacks central work acceptance")
                accepted_evidence(state, domain, result["evidence_ids"], obligation_ids={key}, require_pass=True)
            else:
                need(result["result"] == "retained_unmet", "DOMAIN_CLOSURE", "active obligation result is false")
                if payload["classification"] == "unreachable":
                    need(result["evidence_ids"], "DOMAIN_CLOSURE", "unattainable gap needs observed evidence")
                    accepted_evidence(state, domain, result["evidence_ids"], obligation_ids={key})
        else:
            need(result["result"] == obligation["status"], "DOMAIN_CLOSURE", "historical disposition differs")
            expected = obligation["dispositions"][-1]["successor_ids"] if obligation["dispositions"] else []
            need(result["successor_ids"] == expected, "DOMAIN_CLOSURE", "closure successors differ")
    success = payload["classification"] in {"original", "revised"}
    if success:
        need(all(row["result"] == "accepted" or domain["obligations"][row["obligation_id"]]["status"] != "active"
                 for row in payload["obligation_results"]), "DOMAIN_CLOSURE", "successful closure has unmet active obligations")
    need((payload["classification"] == "original") == (domain["active_outcome_id"] == domain["original_outcome_id"])
         if success else True, "DOMAIN_CLOSURE", "success classification does not match outcome history")
    need(set(payload["deferral_ids"]) == set(domain["deferrals"]), "DOMAIN_CLOSURE", "every deferral needs closure disposition")
    need(all(row["status"] != "open" for row in domain["deferrals"].values()), "DOMAIN_CLOSURE", "open deferral blocks closure")
    need(set(payload["acceptance_ids"]) <= set(domain["acceptances"]), "REFERENCE", "closure acceptance missing")
    need(set(payload["integration_acceptance_ids"]) <= set(domain["integration_acceptances"]), "REFERENCE", "closure integration missing")
    for acceptance_id in payload["acceptance_ids"]:
        acceptance_for_outcome(state, domain, acceptance_id, domain["active_outcome_id"])
    for integration_id in payload["integration_acceptance_ids"]:
        integration_for_outcome(state, domain, integration_id, domain["active_outcome_id"])
    need(set(payload["promotion_ids"]) <= set(domain["promotions"]), "REFERENCE", "closure promotion missing")
    accepted_evidence(state, domain, payload["final_gate_evidence_ids"],
                      obligation_ids=active_obligations(domain), require_pass=success, collective=True)
    domain["closure"] = {**payload, "event_id": event_id, "status": "closed"}


DOMAIN_ADAPTIVE_HANDLERS = {
    spec.kind: spec for spec in (
        domain_handler("domain.review-proposed", _review_payload, _apply_review_proposed),
        domain_handler("domain.review-applied", _review_apply_payload, _apply_review),
        domain_handler("domain.campaign-closed", _closure_payload, _apply_closure),
    )
}
