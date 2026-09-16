"""Machine-readable schema for the public runtime profile."""
from __future__ import annotations

ID = {"type": "string", "pattern": "^[A-Za-z0-9._:-]+$"}

RUNTIME_PROFILE_JSON_SCHEMA = {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://vibevm.org/schema/zap-runtime-profile-1.json",
    "title": "ZAP runtime profile",
    "type": "object",
    "additionalProperties": False,
    "required": ["schema", "transport", "semantic_transport", "semantic", "worker", "verifications", "runtime"],
    "properties": {
        "schema": {"const": "zap-runtime-profile/1"},
        "transport": {"$ref": "#/$defs/transport"},
        "semantic_transport": {"$ref": "#/$defs/transport"},
        "semantic": {"$ref": "#/$defs/semantic"},
        "worker": {"$ref": "#/$defs/worker"},
        "verifications": {
            "type": "object",
            "propertyNames": {"minLength": 1},
            "additionalProperties": {"type": "array", "items": {"$ref": "#/$defs/verification"}},
        },
        "artifact_capture": {
            "type": "object", "additionalProperties": False,
            "required": ["allowed_root"], "properties": {"allowed_root": {"$ref": "#/$defs/nonempty"}},
        },
        "assessment_transport": {
            "allOf": [
                {"$ref": "#/$defs/transport"},
                {"properties": {"allow_process_termination": {"const": False}}},
            ],
        },
        "assessment": {
            "type": "object", "additionalProperties": False,
            "required": ["kind", "argv", "cwd"],
            "properties": {
                "kind": {"const": "json-process"},
                "argv": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"},
                         "contains": {"const": "{packet_file}"}, "minContains": 1, "maxContains": 1},
                "cwd": {"$ref": "#/$defs/nonempty"},
                "observation_paths": {"$ref": "#/$defs/strings"},
            },
        },
        "runtime": {
            "type": "object", "additionalProperties": False,
            "required": ["transient_backoff_ns", "idle_poll_seconds"],
            "properties": {
                "transient_backoff_ns": {"type": "integer", "minimum": 0},
                "idle_poll_seconds": {"type": "number", "minimum": 0},
            },
        },
    },
    "allOf": [
        {"if": {"properties": {"verifications": {"minProperties": 1}}},
         "then": {"required": ["artifact_capture"]}},
        {"if": {"required": ["assessment"]}, "then": {"required": ["assessment_transport"]}},
        {"if": {"required": ["assessment_transport"]}, "then": {"required": ["assessment"]}},
    ],
    "$defs": {
        "nonempty": {"type": "string", "minLength": 1},
        "strings": {"type": "array", "items": {"$ref": "#/$defs/nonempty"}},
        "string_map": {"type": "object", "propertyNames": {"minLength": 1}, "additionalProperties": {"type": "string"}},
        "transport": {
            "type": "object", "additionalProperties": False,
            "required": ["root", "allowed_workspace_roots", "inherited_environment", "environment_allowlist", "allow_process_termination"],
            "properties": {
                "root": {"$ref": "#/$defs/nonempty"},
                "allowed_workspace_roots": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"}},
                "inherited_environment": {"oneOf": [{"type": "null"}, {"$ref": "#/$defs/strings"}]},
                "environment_allowlist": {"$ref": "#/$defs/strings"},
                "allow_process_termination": {"type": "boolean"},
            },
        },
        "semantic": {
            "type": "object", "additionalProperties": False,
            "required": ["kind", "argv", "cwd", "launcher"],
            "properties": {
                "kind": {"enum": ["json-process", "codex-sol-xhigh"]},
                "argv": {"$ref": "#/$defs/strings"}, "cwd": {"$ref": "#/$defs/nonempty"},
                "launcher": {"oneOf": [{"type": "null"}, {"$ref": "#/$defs/nonempty"}]},
            },
            "allOf": [
                {"if": {"properties": {"kind": {"const": "codex-sol-xhigh"}}}, "then": {"properties": {"argv": {"maxItems": 0}}}},
                {"if": {"properties": {"kind": {"const": "json-process"}}}, "then": {"properties": {"launcher": {"type": "null"}, "argv": {"minItems": 1, "contains": {"const": "{packet_file}"}, "minContains": 1, "maxContains": 1}}}},
            ],
        },
        "worker": {
            "type": "object", "additionalProperties": False,
            "required": ["argv", "cwd", "standing_rule_paths", "environment", "resource_capacities", "review_capacity", "integration_capacity", "branch_for_work", "stop_mode", "terminate_after_seconds"],
            "properties": {
                "kind": {"enum": ["argv", "codex-sol-xhigh"], "default": "argv"},
                "launcher": {"oneOf": [{"type": "null"}, {"$ref": "#/$defs/nonempty"}]},
                "argv": {"$ref": "#/$defs/strings"}, "cwd": {"$ref": "#/$defs/nonempty"},
                "standing_rule_paths": {"$ref": "#/$defs/strings"},
                "environment": {"$ref": "#/$defs/string_map"},
                "resource_capacities": {"type": "object", "propertyNames": {"minLength": 1}, "additionalProperties": {"type": "integer", "minimum": 1}},
                "review_capacity": {"type": "integer", "minimum": 1},
                "integration_capacity": {"type": "integer", "minimum": 1},
                "branch_for_work": {"type": "object", "propertyNames": {"minLength": 1}, "additionalProperties": {"$ref": "#/$defs/nonempty"}},
                "stop_mode": {"enum": ["cooperative", "terminate"]},
                "terminate_after_seconds": {"oneOf": [{"type": "null"}, {"type": "number", "exclusiveMinimum": 0}]},
            },
            "allOf": [
                {"if": {"properties": {"kind": {"const": "codex-sol-xhigh"}}, "required": ["kind"]},
                 "then": {"properties": {"argv": {"maxItems": 0}, "environment": {"maxProperties": 0}, "stop_mode": {"const": "cooperative"}, "terminate_after_seconds": {"type": "null"}}}},
                {"if": {"anyOf": [{"not": {"required": ["kind"]}}, {"properties": {"kind": {"const": "argv"}}}]},
                 "then": {"properties": {"launcher": {"type": "null"}, "argv": {"minItems": 1, "contains": {"const": "{packet_file}"}, "minContains": 1, "maxContains": 1}}}},
                {"if": {"properties": {"stop_mode": {"const": "cooperative"}}}, "then": {"properties": {"terminate_after_seconds": {"type": "null"}}}},
                {"if": {"properties": {"stop_mode": {"const": "terminate"}}}, "then": {"properties": {"terminate_after_seconds": {"type": "number", "exclusiveMinimum": 0}}}},
            ],
        },
        "verification": {
            "type": "object", "additionalProperties": False,
            "required": ["check_id", "argv", "cwd", "target", "toolchain", "environment_label", "subjects", "cases", "source_refs", "environment"],
            "properties": {
                "check_id": ID,
                "argv": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"}, "contains": {"const": "{packet_file}"}, "minContains": 1, "maxContains": 1},
                "cwd": {"$ref": "#/$defs/nonempty"}, "target": {"$ref": "#/$defs/nonempty"},
                "toolchain": {"$ref": "#/$defs/nonempty"}, "environment_label": {"$ref": "#/$defs/nonempty"},
                "subjects": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"}},
                "cases": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"}},
                "source_refs": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/nonempty"}},
                "environment": {"$ref": "#/$defs/string_map"},
            },
        },
    },
}

__all__ = ("RUNTIME_PROFILE_JSON_SCHEMA",)
