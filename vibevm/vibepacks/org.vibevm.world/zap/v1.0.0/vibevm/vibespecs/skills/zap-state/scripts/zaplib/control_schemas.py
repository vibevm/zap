"""Machine-readable schemas for ZAP control events and shared control values."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any, Mapping

from .control_model import (
    ACTION_CLASSES, ACTION_SCHEMA, ASSESSMENT_SCHEMA, CHARTER_SCHEMA,
    STOP_POLICY_SCHEMA,
)

_ID_FIELD = {"type": "string", "format": "stable-id"}
_HASH_FIELD = {"type": "string", "pattern": "^[0-9a-f]{64}$"}
_POSITIVE_REVISION = {"type": "integer", "minimum": 1}
_STRING_FIELD = {"type": "string", "min_length": 1}


def _event_schema(properties: dict[str, Any], route: str) -> dict[str, Any]:
    return {
        "type": "object",
        "required": list(properties),
        "properties": copy.deepcopy(properties),
        "additional_properties": False,
        "route": route,
    }


CONTROL_EVENT_SCHEMAS: Mapping[str, Mapping[str, Any]] = MappingProxyType({
    "control.charter-drafted": _event_schema({"charter": {"$ref": "zap-charter/1"}}, "agent-data"),
    "control.charter-activated": _event_schema({
        "charter_id": _ID_FIELD, "charter_revision": _POSITIVE_REVISION,
        "charter_sha256": _HASH_FIELD, "campaign_id": _ID_FIELD,
        "base_sha256": _HASH_FIELD,
    }, "owner"),
    "control.charter-amended": _event_schema({"charter": {"$ref": "zap-charter/1"}}, "owner"),
    "control.action-assessed": _event_schema({
        "action": {"$ref": "zap-action/1"},
        "assessment": {"$ref": "zap-assessment/1"},
    }, "coordinator"),
    "control.action-admitted": _event_schema({
        "admission_id": _ID_FIELD, "action": {"$ref": "zap-action/1"},
        "assessment_id": _ID_FIELD, "exception_id": {"type": ["string", "null"], "format": "stable-id"},
        "command_kind": _ID_FIELD, "product_event_id": _ID_FIELD,
        "product_command_sha256": _HASH_FIELD,
    }, "internal"),
    "control.action-reservation-rebound": _event_schema({
        "admission_id": _ID_FIELD, "assessment_id": _ID_FIELD,
        "product_event_id": _ID_FIELD, "product_command_sha256": _HASH_FIELD,
    }, "internal"),
    "control.owner-stop-requested": _event_schema({
        "pause_id": _ID_FIELD, "campaign_id": _ID_FIELD,
        "base_sha256": _HASH_FIELD, "charter_revision": _POSITIVE_REVISION,
        "reason": _STRING_FIELD, "drain_targets": {"type": "array", "items": _ID_FIELD, "sorted_unique": True},
    }, "owner"),
    "control.pause-delivery-acknowledged": _event_schema({
        "pause_id": _ID_FIELD, "pause_sha256": _HASH_FIELD, "subject_id": _ID_FIELD,
        "state": {"enum": ["delivered", "unreachable"]}, "receipt_sha256": _HASH_FIELD,
    }, "coordinator"),
    "control.pause-safe-state-acknowledged": _event_schema({
        "pause_id": _ID_FIELD, "pause_sha256": _HASH_FIELD, "run_id": _ID_FIELD,
        "state": {"enum": ["safe", "completed", "not_started", "unknown_effect"]},
        "receipt_sha256": _HASH_FIELD,
    }, "coordinator"),
    "control.pause-resumed": _event_schema({
        "pause_id": _ID_FIELD, "pause_sha256": _HASH_FIELD, "decision": _STRING_FIELD,
    }, "owner"),
    "control.action-exception-granted": _event_schema({
        "exception_id": _ID_FIELD, "pause_id": _ID_FIELD, "pause_sha256": _HASH_FIELD,
        "action_id": _ID_FIELD, "action_class": {"enum": list(ACTION_CLASSES)},
        "payload_sha256": _HASH_FIELD, "source_captures_sha256": _HASH_FIELD,
        "charter_revision": _POSITIVE_REVISION, "reason": _STRING_FIELD,
    }, "owner"),
    "control.approach-outcome-recorded": _event_schema({
        "record_id": _ID_FIELD, "problem_id": _ID_FIELD, "approach_id": _ID_FIELD,
        "strategy_sha256": _HASH_FIELD,
        "outcome": {"enum": ["failed", "succeeded", "inconclusive", "provider_error", "retry"]},
        "evidence_refs": {"type": "array", "items": _ID_FIELD, "sorted_unique": True},
    }, "coordinator"),
    "control.approach-epoch-advanced": _event_schema({
        "problem_id": _ID_FIELD, "expected_epoch": {"type": "integer", "minimum": 0},
        "new_epoch": _POSITIVE_REVISION, "reason": _STRING_FIELD,
    }, "owner"),
})


CONTROL_VALUE_SCHEMAS: Mapping[str, Mapping[str, Any]] = MappingProxyType({
    ACTION_SCHEMA: {
        "type": "object",
        "required": ["schema", "action_id", "action_class", "campaign_id", "base_sha256", "charter_revision", "payload_sha256", "source_captures", "branch_id", "run_id", "problem_id"],
        "properties": {
            "schema": {"const": ACTION_SCHEMA}, "action_id": _ID_FIELD,
            "action_class": {"enum": list(ACTION_CLASSES)}, "campaign_id": _ID_FIELD,
            "base_sha256": _HASH_FIELD, "charter_revision": _POSITIVE_REVISION,
            "payload_sha256": _HASH_FIELD,
            "source_captures": {"type": "array", "items": {"type": "object", "required": ["source_id", "sha256"], "properties": {"source_id": _ID_FIELD, "sha256": _HASH_FIELD}, "additional_properties": False}, "sorted_unique_by": "source_id"},
            "branch_id": {"type": ["string", "null"], "format": "stable-id"},
            "run_id": {"type": ["string", "null"], "format": "stable-id"},
            "problem_id": {"type": ["string", "null"], "format": "stable-id"},
        },
        "additional_properties": False,
    },
    ASSESSMENT_SCHEMA: {
        "type": "object",
        "required": ["schema", "assessment_id", "policy_id", "policy_revision", "phase", "values", "drain_targets"],
        "properties": {
            "schema": {"const": ASSESSMENT_SCHEMA}, "assessment_id": _ID_FIELD,
            "policy_id": _ID_FIELD, "policy_revision": _POSITIVE_REVISION,
            "phase": {"enum": ["before_action", "after_action"]},
            "values": {"type": "object", "value_type": ["string", "boolean", "integer", "number", "null"]},
            "drain_targets": {"type": "array", "items": _ID_FIELD, "sorted_unique": True},
        },
        "additional_properties": False,
    },
    CHARTER_SCHEMA: {
        "type": "object",
        "required": ["schema", "charter_id", "campaign_id", "base_sha256", "revision", "parent_sha256", "intent", "expected_outcome", "delegation", "legacy_authority", "stop_policy"],
        "properties": {
            "schema": {"const": CHARTER_SCHEMA}, "charter_id": _ID_FIELD,
            "campaign_id": _ID_FIELD, "base_sha256": _HASH_FIELD,
            "revision": _POSITIVE_REVISION,
            "parent_sha256": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "intent": _STRING_FIELD,
            "intent_binding": {"type": ["object", "null"], "required": ["intent_id", "sha256"], "properties": {"intent_id": _ID_FIELD, "sha256": _HASH_FIELD}, "additional_properties": False},
            "expected_outcome": {"type": "object", "required": ["outcome_id", "summary"]},
            "delegation": {"type": "object", "required": ["allowed_actions", "adaptation"]},
            "legacy_authority": {"type": "array", "sorted_unique_by": "id"},
            "stop_policy": {"$ref": STOP_POLICY_SCHEMA},
        },
        "additional_properties": False,
    },
    STOP_POLICY_SCHEMA: {
        "type": "object",
        "required": ["schema", "policy_id", "revision", "rules"],
        "properties": {
            "schema": {"const": STOP_POLICY_SCHEMA}, "policy_id": _ID_FIELD,
            "revision": _POSITIVE_REVISION,
            "rules": {"type": "array", "sorted_unique_by": "id"},
        },
        "additional_properties": False,
    },
})
