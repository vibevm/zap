"""Formal deferral lifecycle for ZAP domain obligations."""
from __future__ import annotations

from typing import Any

from .common import identity, need
from .domain_model import (
    active_obligations, domain_handler, effective_nodes, nonblank_list,
    require_action, require_refs, strict, text, unique_ids,
)
from .domain_proof import accepted_evidence


def _deferral_payload(value: Any) -> dict[str, Any]:
    value = strict("zap-domain/deferral-created/1", {
        "deferral_id", "outcome_id", "obligation_ids", "work_ids", "scope", "reason",
        "current_guarantees", "responsible_party", "closure_requirement",
    })(value)
    identity(value["deferral_id"]); identity(value["outcome_id"])
    unique_ids(value["obligation_ids"], "deferral obligations", nonempty=True)
    unique_ids(value["work_ids"], "deferral work", nonempty=True)
    for field in ("scope", "reason", "responsible_party", "closure_requirement"):
        text(value[field], field)
    nonblank_list(value["current_guarantees"], "current guarantees", nonempty=True)
    return value


def _apply_created(state, domain, payload, event_id):
    require_action(state, "task.update")
    key = payload["deferral_id"]
    need(key not in domain["deferrals"], "DUPLICATE", "deferral exists")
    need(payload["outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "deferral outcome is not active")
    require_refs(payload["obligation_ids"], active_obligations(domain), "deferral obligations", nonempty=True)
    require_refs(payload["work_ids"], set(effective_nodes(state, domain)), "deferral work", nonempty=True)
    domain["deferrals"][key] = {**payload, "status": "open", "event_id": event_id, "history": []}


def _validate_transfer(value):
    value = strict("zap-domain/deferral-transferred/1", {
        "deferral_id", "from_responsible_party", "to_responsible_party", "to_work_ids", "reason",
    })(value)
    identity(value["deferral_id"])
    for field in ("from_responsible_party", "to_responsible_party", "reason"):
        text(value[field], field)
    unique_ids(value["to_work_ids"], "transfer work", nonempty=True)
    return value


def _apply_transfer(state, domain, payload, event_id):
    require_action(state, "task.update")
    row = domain["deferrals"].get(payload["deferral_id"])
    need(row and row["status"] == "open", "DOMAIN_DEFERRAL", "open deferral missing")
    need(row["responsible_party"] == payload["from_responsible_party"], "DOMAIN_STALE", "responsible party differs")
    require_refs(payload["to_work_ids"], set(effective_nodes(state, domain)), "transfer work", nonempty=True)
    row["history"].append({"kind": "transferred", "event_id": event_id, **payload})
    row["responsible_party"] = payload["to_responsible_party"]
    row["work_ids"] = payload["to_work_ids"]


def _validate_close(value):
    value = strict("zap-domain/deferral-closed/1", {"deferral_id", "evidence_ids", "reason"})(value)
    identity(value["deferral_id"])
    unique_ids(value["evidence_ids"], "closure evidence", nonempty=True)
    text(value["reason"], "closure reason")
    return value


def _apply_closed(state, domain, payload, event_id):
    require_action(state, "task.update")
    row = domain["deferrals"].get(payload["deferral_id"])
    need(row and row["status"] == "open", "DOMAIN_DEFERRAL", "open deferral missing")
    accepted_evidence(state, domain, payload["evidence_ids"],
                      obligation_ids=row["obligation_ids"], require_pass=True)
    row["status"] = "closed"
    row["closed_event_id"] = event_id
    row["closure_reason"] = payload["reason"]
    row["closure_evidence_ids"] = payload["evidence_ids"]


def _validate_inapplicable(value):
    value = strict("zap-domain/deferral-inapplicable/1", {"deferral_id", "outcome_id", "reason"})(value)
    identity(value["deferral_id"]); identity(value["outcome_id"])
    text(value["reason"], "inapplicable reason")
    return value


def _apply_inapplicable(state, domain, payload, event_id):
    require_action(state, "task.update")
    row = domain["deferrals"].get(payload["deferral_id"])
    need(row and row["status"] == "open", "DOMAIN_DEFERRAL", "open deferral missing")
    need(payload["outcome_id"] == domain["active_outcome_id"], "DOMAIN_STALE", "outcome is not active")
    need(all(domain["obligations"][key]["status"] != "active" for key in row["obligation_ids"]),
         "DOMAIN_DEFERRAL", "active obligations cannot become inapplicable")
    row["status"] = "inapplicable"
    row["inapplicable_event_id"] = event_id
    row["inapplicable_reason"] = payload["reason"]


DOMAIN_DEFERRAL_HANDLERS = {
    spec.kind: spec for spec in (
        domain_handler("domain.deferral-created", _deferral_payload, _apply_created),
        domain_handler("domain.deferral-transferred", _validate_transfer, _apply_transfer),
        domain_handler("domain.deferral-closed", _validate_close, _apply_closed),
        domain_handler("domain.deferral-inapplicable", _validate_inapplicable, _apply_inapplicable),
    )
}

__all__ = ("DOMAIN_DEFERRAL_HANDLERS",)
