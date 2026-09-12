"""Evidence applicability, achieved stages, acceptance and promotion metadata."""
from __future__ import annotations

from typing import Any

from .common import exact, identity, need
from .domain_model import (
    STAGES, active_obligations, digest, domain_handler, effective_nodes, integer,
    inherited_dependencies, nonblank_list, owned_obligations, require_action, require_refs, rows, strict,
    text, unique_ids, work_is_accepted,
)


def _applicability(state, refs):
    from .sources import current_applicability
    result = current_applicability(state, refs)
    need(isinstance(result, dict) and result.get("status") in {"applicable", "stale", "unknown"},
         "DOMAIN_EVIDENCE", "invalid source applicability result")
    return result


def _applies_to(value):
    exact(value, {"outcome_id", "obligation_ids", "work_ids", "stage", "scope"})
    identity(value["outcome_id"])
    unique_ids(value["obligation_ids"], "applicable obligations", nonempty=True)
    unique_ids(value["work_ids"], "applicable work", nonempty=True)
    need(value["stage"] in STAGES, "DOMAIN_VALUE", "invalid applicable stage")
    text(value["scope"], "evidence scope")


def _method(value):
    exact(value, {"argv", "target", "toolchain", "environment", "subjects", "cases"})
    nonblank_list(value["argv"], "verification argv", nonempty=True)
    for field in ("target", "toolchain", "environment"):
        text(value[field], f"verification {field}")
    nonblank_list(value["subjects"], "verification subjects", nonempty=True)
    nonblank_list(value["cases"], "verification cases", nonempty=True)


def _evidence_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/evidence-adjudicated/1", {
        "evidence_id", "expected_revision", "disposition", "applies_to",
        "source_refs", "method", "limitations",
    })(value)
    identity(value["evidence_id"])
    integer(value["expected_revision"], "evidence adjudication revision", minimum=-1)
    need(value["disposition"] in {"accepted", "rejected", "inapplicable"},
         "DOMAIN_VALUE", "invalid evidence disposition")
    _applies_to(value["applies_to"])
    unique_ids(value["source_refs"], "evidence sources", nonempty=True)
    _method(value["method"])
    nonblank_list(value["limitations"], "evidence limitations")
    return value


def _apply_evidence(state, domain, payload, event_id):
    require_action(state, "evidence.adjudicate")
    evidence = state.get("evidence", {}).get(payload["evidence_id"])
    need(evidence is not None, "REFERENCE", "core evidence record missing")
    current = domain["evidence_adjudications"].get(payload["evidence_id"])
    current_revision = current["revision"] if current else -1
    need(payload["expected_revision"] == current_revision, "DOMAIN_STALE", "evidence adjudication revision differs")
    applies = payload["applies_to"]
    need(applies["outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "evidence outcome is not active")
    require_refs(applies["obligation_ids"], set(domain["obligations"]), "evidence obligations", nonempty=True)
    require_refs(applies["work_ids"], set(effective_nodes(state, domain)), "evidence work", nonempty=True)
    need(set(applies["work_ids"]) <= set(evidence.get("node_refs", [])),
         "DOMAIN_EVIDENCE", "core evidence does not identify every claimed work subject")
    applicability = _applicability(state, payload["source_refs"])
    if payload["disposition"] == "accepted":
        need(evidence.get("result") in {"observed_pass", "observed_fail"},
             "DOMAIN_EVIDENCE", "accepted evidence requires an observed result")
        need(evidence.get("artifact_refs"), "DOMAIN_EVIDENCE", "accepted evidence needs a durable artifact reference")
        need(applicability["status"] == "applicable" and not applicability.get("incomplete_closure"),
             "DOMAIN_EVIDENCE", "stale, unknown or incomplete source closure cannot be accepted")
    version = {**payload, "revision": current_revision + 1, "event_id": event_id,
               "applicability_at_adjudication": applicability}
    history = [] if current is None else list(current["history"]) + [
        {key: val for key, val in current.items() if key != "history"}
    ]
    domain["evidence_adjudications"][payload["evidence_id"]] = {**version, "history": history}


def accepted_evidence(state, domain, evidence_ids, *, work_id=None, stage=None,
                      obligation_ids=frozenset(), require_pass=False):
    for key in evidence_ids:
        row = domain["evidence_adjudications"].get(key)
        need(row and row["disposition"] == "accepted", "DOMAIN_EVIDENCE", "evidence is not centrally accepted")
        if require_pass:
            need(state["evidence"][key]["result"] == "observed_pass",
                 "DOMAIN_EVIDENCE", "positive acceptance requires observed-pass evidence")
        applies = row["applies_to"]
        need(applies["outcome_id"] == domain["active_outcome_id"],
             "DOMAIN_EVIDENCE", "evidence applies to another outcome revision")
        if work_id is not None:
            need(work_id in applies["work_ids"], "DOMAIN_EVIDENCE", "evidence does not apply to work")
        if stage is not None:
            need(stage == applies["stage"], "DOMAIN_EVIDENCE", "evidence does not apply to stage")
        need(set(obligation_ids) <= set(applies["obligation_ids"]), "DOMAIN_EVIDENCE", "evidence does not cover obligations")
        current = _applicability(state, row["source_refs"])
        need(current["status"] == "applicable" and not current.get("incomplete_closure"),
             "DOMAIN_EVIDENCE", "accepted evidence is no longer applicable")


def _stage_payload(value):
    value = strict("zap-domain/stage-accepted/1", {
        "stage_acceptance_id", "work_id", "stage", "outcome_id", "evidence_ids",
        "obligation_ids", "scope", "summary",
    })(value)
    identity(value["stage_acceptance_id"]); identity(value["work_id"]); identity(value["outcome_id"])
    need(value["stage"] in STAGES, "DOMAIN_VALUE", "invalid achieved stage")
    unique_ids(value["evidence_ids"], "stage evidence", nonempty=True)
    unique_ids(value["obligation_ids"], "stage obligations", nonempty=True)
    text(value["scope"], "stage scope"); text(value["summary"], "stage summary")
    return value


def _apply_stage(state, domain, payload, event_id):
    require_action(state, "stage.accept")
    key = payload["stage_acceptance_id"]
    need(key not in domain["stages"], "DUPLICATE", "stage acceptance exists")
    need(payload["work_id"] in effective_nodes(state, domain), "REFERENCE", "stage work missing")
    need(payload["outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "stage outcome is not active")
    require_refs(payload["obligation_ids"], active_obligations(domain), "stage obligations", nonempty=True)
    need(set(payload["obligation_ids"]) <= owned_obligations(domain, payload["work_id"]),
         "DOMAIN_EVIDENCE", "stage obligations are not owned by the work")
    need(set(payload["obligation_ids"]) <= owned_obligations(domain, payload["work_id"]),
         "DOMAIN_ACCEPTANCE", "stage obligations are not owned by work")
    accepted_evidence(state, domain, payload["evidence_ids"], work_id=payload["work_id"],
                      stage=payload["stage"], obligation_ids=payload["obligation_ids"], require_pass=True)
    domain["stages"][key] = {**payload, "event_id": event_id, "status": "accepted"}


def _integration_payload(value):
    value = strict("zap-domain/integration-accepted/1", {
        "integration_id", "work_id", "child_work_ids", "legacy_child_ids", "outcome_id",
        "evidence_ids", "obligation_ids", "summary",
    })(value)
    for field in ("integration_id", "work_id", "outcome_id"):
        identity(value[field])
    unique_ids(value["child_work_ids"], "integration children", nonempty=True)
    unique_ids(value["legacy_child_ids"], "legacy integration children")
    unique_ids(value["evidence_ids"], "integration evidence", nonempty=True)
    unique_ids(value["obligation_ids"], "integration obligations", nonempty=True)
    text(value["summary"], "integration summary")
    return value


def _apply_integration(state, domain, payload, event_id):
    require_action(state, "work.accept")
    key = payload["integration_id"]
    need(key not in domain["integration_acceptances"], "DUPLICATE", "integration acceptance exists")
    nodes = effective_nodes(state, domain)
    need(payload["work_id"] in nodes, "REFERENCE", "integration work missing")
    children = {node_id for node_id, node in nodes.items() if node.get("parent") == payload["work_id"]}
    need(set(payload["child_work_ids"]) == children, "DOMAIN_ACCEPTANCE", "integration must cover every direct child")
    legacy = set(payload["legacy_child_ids"])
    need(legacy <= children, "REFERENCE", "legacy child missing")
    for child in children - legacy:
        need(any(row["work_id"] == child and row["outcome_id"] == domain["active_outcome_id"]
                 for row in domain["acceptances"].values()),
             "DOMAIN_ACCEPTANCE", "child lacks current central acceptance")
    for child in legacy:
        need(child in domain["legacy_acceptance"], "DOMAIN_ACCEPTANCE", "legacy child assertion missing")
    require_refs(payload["obligation_ids"], active_obligations(domain), "integration obligations", nonempty=True)
    accepted_evidence(state, domain, payload["evidence_ids"], work_id=payload["work_id"],
                      obligation_ids=payload["obligation_ids"], require_pass=True)
    domain["integration_acceptances"][key] = {**payload, "event_id": event_id, "status": "accepted",
                                               "legacy_inputs_are_assertions": bool(legacy)}


def _work_accept_payload(value):
    value = strict("zap-domain/work-accepted/1", {
        "acceptance_id", "work_id", "outcome_id", "stage_acceptance_id", "evidence_ids",
        "obligation_ids", "integration_acceptance_ids", "summary",
    })(value)
    for field in ("acceptance_id", "work_id", "outcome_id", "stage_acceptance_id"):
        identity(value[field])
    unique_ids(value["evidence_ids"], "work evidence", nonempty=True)
    unique_ids(value["obligation_ids"], "work obligations", nonempty=True)
    unique_ids(value["integration_acceptance_ids"], "integration acceptance ids")
    text(value["summary"], "acceptance summary")
    return value


def _apply_work_accept(state, domain, payload, event_id):
    require_action(state, "work.accept")
    nodes = effective_nodes(state, domain)
    key = payload["work_id"]
    revalidating = domain["work_updates"].get(key, {}).get("revalidation_required")
    need(key in nodes and (nodes[key]["state"] == "candidate" or (nodes[key]["state"] == "accepted" and revalidating)),
         "DOMAIN_ACCEPTANCE", "only candidate or explicitly revalidated work can be accepted")
    need(payload["acceptance_id"] not in domain["acceptances"], "DUPLICATE", "acceptance identity exists")
    need(payload["outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "acceptance outcome is not active")
    obligations = owned_obligations(domain, key, active_only=True)
    need(set(payload["obligation_ids"]) == obligations, "DOMAIN_ACCEPTANCE", "acceptance must cover every current work obligation")
    history = domain["task_contracts"].get(key)
    need(history is not None, "DOMAIN_CONTRACT", "task contract missing")
    contract_version = next(
        (row for row in history["versions"] if row["version"] == history["active_version"]), None
    )
    need(contract_version is not None, "DOMAIN_CONTRACT", "active task contract version missing")
    need(contract_version["schema"] == "zap-task-contract/1", "DOMAIN_CONTRACT", "legacy contract must be explicitly versioned before new acceptance")
    required_stage = contract_version["contract"]["required_stage"]
    stage = domain["stages"].get(payload["stage_acceptance_id"])
    need(stage and stage["work_id"] == key and stage["stage"] == required_stage,
         "DOMAIN_ACCEPTANCE", "required achieved stage lacks central acceptance")
    accepted_evidence(state, domain, payload["evidence_ids"], work_id=key,
                      obligation_ids=obligations, require_pass=True)
    need(all(work_is_accepted(state, domain, dep) for dep in inherited_dependencies(nodes, key)),
         "DOMAIN_ACCEPTANCE", "work prerequisites are not accepted or explicitly resolved")
    children = {node_id for node_id, node in nodes.items() if node.get("parent") == key}
    integrations = [domain["integration_acceptances"].get(i) for i in payload["integration_acceptance_ids"]]
    if children:
        need(integrations and any(set(row["child_work_ids"]) == children for row in integrations if row),
             "DOMAIN_ACCEPTANCE", "parent work lacks complete integration acceptance")
    domain["acceptances"][payload["acceptance_id"]] = {
        **payload, "event_id": event_id, "status": "accepted", "contract_version": history["active_version"],
    }
    domain["work_updates"].setdefault(key, {})["state"] = "accepted"
    domain["work_updates"][key].pop("revalidation_required", None)


def _promotion_payload(value):
    value = strict("zap-domain/fact-promotion-recorded/1", {
        "promotion_id", "fact_id", "target_handle", "content_sha256", "evidence_ids",
        "adapter_receipt", "summary",
    })(value)
    identity(value["promotion_id"]); identity(value["fact_id"])
    text(value["target_handle"], "promotion target"); digest(value["content_sha256"])
    unique_ids(value["evidence_ids"], "promotion evidence", nonempty=True)
    text(value["adapter_receipt"], "adapter receipt"); text(value["summary"], "promotion summary")
    return value


def _apply_promotion(state, domain, payload, event_id):
    require_action(state, "fact.promote")
    need(payload["promotion_id"] not in domain["promotions"], "DUPLICATE", "promotion exists")
    fact = state.get("facts", {}).get(payload["fact_id"])
    need(fact is not None, "REFERENCE", "core fact missing")
    need(fact.get("status") == "observed", "DOMAIN_EVIDENCE", "only an observed fact can be promoted")
    need(set(payload["evidence_ids"]) == set(fact.get("evidence_refs", [])),
         "DOMAIN_EVIDENCE", "promotion must use the fact's exact evidence set")
    accepted_evidence(state, domain, payload["evidence_ids"])
    proven_work = {work_id for evidence_id in payload["evidence_ids"]
                   for work_id in domain["evidence_adjudications"][evidence_id]["applies_to"]["work_ids"]}
    need(set(fact.get("node_refs", [])) <= proven_work,
         "DOMAIN_EVIDENCE", "promotion evidence does not apply to every fact subject")
    domain["promotions"][payload["promotion_id"]] = {
        **payload, "event_id": event_id, "effect": "confirmed_by_external_adapter_receipt",
    }


DOMAIN_PROOF_HANDLERS = {
    spec.kind: spec for spec in (
        domain_handler("domain.evidence-adjudicated", _evidence_payload, _apply_evidence),
        domain_handler("domain.stage-accepted", _stage_payload, _apply_stage),
        domain_handler("domain.integration-accepted", _integration_payload, _apply_integration),
        domain_handler("domain.work-accepted", _work_accept_payload, _apply_work_accept),
        domain_handler("domain.fact-promotion-recorded", _promotion_payload, _apply_promotion),
    )
}
