"""Machine-readable payload descriptors for every ZAP domain event."""
from __future__ import annotations

from types import MappingProxyType

ID = {"type": "string", "pattern": "^[A-Za-z0-9._:-]+$"}
TEXT = {"type": "string", "minLength": 1}
TEXTS = {"type": "array", "items": TEXT}
IDS = {"type": "array", "items": ID, "uniqueItems": True}
SHA = {"type": "string", "pattern": "^[0-9a-f]{64}$"}
INT = {"type": "integer", "minimum": 0}


def obj(schema, fields):
    return {
        "type": "object", "additionalProperties": False,
        "required": ["schema", *fields],
        "properties": {"schema": {"const": schema}, **fields},
    }


def nested(fields):
    return {
        "type": "object", "additionalProperties": False,
        "required": list(fields), "properties": fields,
    }


OWNER = nested({"work_id": ID, "role": {"enum": ["implementation", "verification", "integration", "acceptance"]}})
OBLIGATION = nested({
    "id": ID, "statement": TEXT, "essential": {"type": "boolean"},
    "source_refs": TEXTS, "owners": {"type": "array", "items": OWNER},
})
DISPOSITION = nested({
    "obligation_id": ID,
    "disposition": {"enum": ["retained", "replaced", "excluded", "unattainable"]},
    "successor_ids": IDS, "unmet_portion": {"type": "string"}, "reason": TEXT,
})
CONTRACT = nested({
    "schema": {"const": "zap-task-contract/1"}, "contract_id": ID, "work_id": ID,
    "title": TEXT, "goal": TEXT, "read_subjects": TEXTS, "write_subjects": TEXTS,
    "resources": TEXTS, "steps": TEXTS, "positive_cases": TEXTS,
    "negative_cases": TEXTS, "checks": TEXTS, "acceptance": TEXTS,
    "safe_stop": TEXT, "integration_owner": ID,
    "delivery_route": {"type": "array", "items": {"enum": ["direct", "prototype", "functional", "productized"]}},
    "required_stage": {"enum": ["prototype", "functional", "productized"]},
    "source_handles": TEXTS, "obligation_ids": IDS,
})
WORK_NODE = nested({
    "id": ID, "parent": ID, "title": TEXT,
    "kind": {"enum": ["portfolio", "campaign", "phase", "workstream", "group", "atom", "gate", "horizon"]},
    "state": {"const": "planned"}, "order": INT, "depends_on": IDS,
    "acceptance": TEXTS, "required_stage": {"enum": ["prototype", "functional", "productized"]},
})
EDGE = nested({"node_id": ID, "depends_on": IDS})
ASSIGNMENT = nested({"work_id": ID, "role": {"enum": ["implementation", "verification", "integration", "acceptance"]}})
COVERAGE = nested({"obligation_id": ID, "assignments": {"type": "array", "items": ASSIGNMENT}})
APPLIES_TO = nested({
    "outcome_id": ID, "obligation_ids": IDS, "work_ids": IDS,
    "stage": {"enum": ["prototype", "functional", "productized"]}, "scope": TEXT,
})
METHOD = nested({
    "argv": TEXTS, "target": TEXT, "toolchain": TEXT, "environment": TEXT,
    "subjects": TEXTS, "cases": TEXTS,
})
SOURCE_CAPTURE = nested({"source_id": ID, "sha256": SHA})
JOB_CAPTURE = nested({"job_id": ID, "status": TEXT, "attempt_id": {"oneOf": [ID, {"type": "null"}]}})
CAPTURES = nested({
    "base_sha256": SHA, "zap_revision": INT, "domain_revision": INT,
    "intent_id": {"oneOf": [ID, {"type": "null"}]},
    "outcome_id": {"oneOf": [ID, {"type": "null"}]}, "policy_revision": {"type": "integer", "minimum": 1},
    "source_captures": {"type": "array", "items": SOURCE_CAPTURE},
    "jobs": {"type": "array", "items": JOB_CAPTURE},
})
KNOWLEDGE = nested({
    "before": {"oneOf": [{"type": "null"}, nested({"review_id": ID, "sha256": SHA})]},
    "after": nested({"region_ids": IDS, "revision": INT, "sha256": SHA, "regions": {"type": "object"}}),
    "new_region_ids": IDS, "affected_dependencies": IDS, "closure_complete": {"type": "boolean"},
})
ALTERNATIVE = nested({
    "id": ID, "description": TEXT, "value": TEXT, "feasibility": TEXT,
    "remaining_cost": TEXT, "risks": TEXTS, "unknowns": TEXTS,
})
DECISION = nested({
    "kind": {"enum": ["keep_route", "reorder", "research", "replace_method", "pivot_outcome", "wait", "owner_proposal"]},
    "rationale": TEXT,
})
WORK_CHANGE = nested({
    "work_id": ID, "operation": {"enum": ["retain", "reprioritize", "supersede", "drop", "revalidate"]},
    "order": {"oneOf": [INT, {"type": "null"}]}, "successor_ids": IDS, "reason": TEXT,
})
JOB_RECONCILIATION = nested({
    "job_id": ID, "action": {"enum": ["continue", "finish_compatible", "drain", "preserve_candidate", "revalidate"]},
    "safe_boundary": TEXT, "reason": TEXT,
})
TRANSITION = nested({
    "intent_id": {"oneOf": [ID, {"type": "null"}]},
    "outcome_id": {"oneOf": [ID, {"type": "null"}]},
    "obligation_dispositions": {"type": "array", "items": DISPOSITION},
    "ownership_changes": {"type": "array", "items": nested({
        "obligation_id": ID, "from_work_id": ID,
        "assignments": {"type": "array", "items": ASSIGNMENT}, "reason": TEXT,
    })},
    "work_changes": {"type": "array", "items": WORK_CHANGE},
    "preserved_evidence_ids": IDS,
    "preserved_stage_acceptance_ids": IDS,
    "preserved_work_acceptance_ids": IDS,
    "preserved_integration_acceptance_ids": IDS,
    "job_reconciliation": {"type": "array", "items": JOB_RECONCILIATION},
    "tradeoffs": TEXTS, "preserved_benefits": TEXTS,
})
SPARSE_REVIEW_TRANSITION_SCHEMA = obj("zap-domain/sparse-review-transition/1", {
    "intent_id": {"oneOf": [ID, {"type": "null"}]}, "outcome_id": ID,
    "changed_dispositions": {"type": "array", "items": DISPOSITION},
    "ownership_changes": {"type": "array", "items": nested({
        "obligation_id": ID, "from_work_id": ID, "assignments": {"type": "array", "items": ASSIGNMENT}, "reason": TEXT,
    })},
    "work_changes": {"type": "array", "items": WORK_CHANGE}, "preserved_evidence_ids": IDS,
    "preserved_stage_acceptance_ids": IDS, "preserved_work_acceptance_ids": IDS,
    "preserved_integration_acceptance_ids": IDS,
    "job_reconciliation": {"type": "array", "items": JOB_RECONCILIATION},
    "tradeoffs": TEXTS, "preserved_benefits": TEXTS,
})
OBLIGATION_RESULT = nested({
    "obligation_id": ID,
    "result": {"enum": ["accepted", "retained_unmet", "replaced", "excluded", "unattainable"]},
    "unmet_portion": {"type": "string"}, "successor_ids": IDS, "evidence_ids": IDS,
})


DOMAIN_EVENT_SCHEMAS = MappingProxyType({
    "domain.intent-proposed": obj("zap-domain/intent-proposed/1", {
        "intent_id": ID, "revision": {"type": "integer", "minimum": 1},
        "previous_intent_id": {"oneOf": [ID, {"type": "null"}]}, "summary": TEXT,
        "beneficiaries": TEXTS, "values": TEXTS, "constraints": TEXTS, "source_refs": TEXTS,
    }),
    "domain.intent-adopted": obj("zap-domain/intent-adopted/1", {"intent_id": ID}),
    "domain.outcome-proposed": obj("zap-domain/outcome-proposed/1", {
        "outcome_id": ID, "revision": {"type": "integer", "minimum": 1},
        "previous_outcome_id": {"oneOf": [ID, {"type": "null"}]}, "intent_id": ID,
        "summary": TEXT, "benefits": TEXTS, "guarantees": TEXTS, "tradeoffs": TEXTS,
        "obligations": {"type": "array", "items": OBLIGATION},
    }),
    "domain.outcome-adopted": obj("zap-domain/outcome-adopted/1", {
        "outcome_id": ID, "obligation_dispositions": {"type": "array", "items": DISPOSITION},
    }),
    "domain.task-contract-replaced": obj("zap-domain/task-contract-replaced/1", {
        "work_id": ID, "expected_version": INT, "contract": CONTRACT,
    }),
    "domain.work-renamed": obj("zap-domain/work-renamed/1", {"work_id": ID, "expected_title": TEXT, "new_title": TEXT}),
    "domain.work-transitioned": obj("zap-domain/work-transitioned/1", {
        "work_id": ID,
        "from_state": {"enum": ["planned", "ready", "active", "candidate", "accepted", "blocked", "deferred", "dropped", "superseded"]},
        "to_state": {"enum": ["planned", "ready", "active", "candidate", "accepted", "blocked", "deferred", "dropped", "superseded"]},
        "successor_ids": IDS,
    }),
    "domain.work-dispatched": obj("zap-domain/work-dispatched/1", {
        "work_id": ID, "from_state": {"const": "ready"}, "job_id": ID,
    }),
    "domain.plan-lowered": obj("zap-domain/plan-lowered/1", {
        "parent_id": ID, "nodes": {"type": "array", "items": WORK_NODE},
        "edges": {"type": "array", "items": EDGE}, "coverage": {"type": "array", "items": COVERAGE},
        "contracts": {"type": "array", "items": CONTRACT}, "integration_owner": ID,
    }),
    "domain.deferral-created": obj("zap-domain/deferral-created/1", {
        "deferral_id": ID, "outcome_id": ID, "obligation_ids": IDS, "work_ids": IDS,
        "scope": TEXT, "reason": TEXT, "current_guarantees": TEXTS,
        "responsible_party": TEXT, "closure_requirement": TEXT,
    }),
    "domain.deferral-transferred": obj("zap-domain/deferral-transferred/1", {
        "deferral_id": ID, "from_responsible_party": TEXT, "to_responsible_party": TEXT,
        "to_work_ids": IDS, "reason": TEXT,
    }),
    "domain.deferral-closed": obj("zap-domain/deferral-closed/1", {"deferral_id": ID, "evidence_ids": IDS, "reason": TEXT}),
    "domain.deferral-inapplicable": obj("zap-domain/deferral-inapplicable/1", {"deferral_id": ID, "outcome_id": ID, "reason": TEXT}),
    "domain.evidence-adjudicated": obj("zap-domain/evidence-adjudicated/1", {
        "evidence_id": ID, "expected_revision": {"type": "integer", "minimum": -1},
        "disposition": {"enum": ["accepted", "rejected", "inapplicable"]},
        "applies_to": APPLIES_TO, "source_refs": IDS, "method": METHOD, "limitations": TEXTS,
    }),
    "domain.stage-accepted": obj("zap-domain/stage-accepted/1", {
        "stage_acceptance_id": ID, "work_id": ID,
        "stage": {"enum": ["prototype", "functional", "productized"]}, "outcome_id": ID,
        "evidence_ids": IDS, "obligation_ids": IDS, "scope": TEXT, "summary": TEXT,
    }),
    "domain.integration-accepted": obj("zap-domain/integration-accepted/1", {
        "integration_id": ID, "work_id": ID, "child_work_ids": IDS, "legacy_child_ids": IDS,
        "outcome_id": ID, "evidence_ids": IDS, "obligation_ids": IDS, "summary": TEXT,
    }),
    "domain.work-accepted": obj("zap-domain/work-accepted/1", {
        "acceptance_id": ID, "work_id": ID, "outcome_id": ID, "stage_acceptance_id": ID,
        "evidence_ids": IDS, "obligation_ids": IDS, "integration_acceptance_ids": IDS, "summary": TEXT,
    }),
    "domain.fact-promotion-recorded": obj("zap-domain/fact-promotion-recorded/1", {
        "promotion_id": ID, "fact_id": ID, "target_handle": TEXT, "content_sha256": SHA,
        "evidence_ids": IDS, "adapter_receipt": TEXT, "summary": TEXT,
    }),
    "domain.review-proposed": obj("zap-domain/review-proposed/1", {
        "review_id": ID, "previous_review_id": {"oneOf": [ID, {"type": "null"}]},
        "signals": TEXTS, "captures": CAPTURES, "knowledge": KNOWLEDGE,
        "alternatives": {"type": "array", "items": ALTERNATIVE}, "chosen": ID,
        "decision": DECISION, "transition": TRANSITION, "next_trigger": TEXT,
    }),
    "domain.review-applied": obj("zap-domain/review-applied/1", {"review_id": ID, "expected_domain_revision": INT}),
    "domain.campaign-closed": obj("zap-domain/campaign-closed/1", {
        "closure_id": ID, "classification": {"enum": ["original", "revised", "partial", "unreachable"]},
        "active_outcome_id": ID, "actual_benefit": TEXT,
        "obligation_results": {"type": "array", "items": OBLIGATION_RESULT},
        "acceptance_ids": IDS, "integration_acceptance_ids": IDS, "deferral_ids": IDS,
        "promotion_ids": IDS, "final_gate_evidence_ids": IDS, "summary": TEXT,
    }),
})

__all__ = ("DOMAIN_EVENT_SCHEMAS", "SPARSE_REVIEW_TRANSITION_SCHEMA")
