"""Exact nested JSON payload schemas for the public knowledge event registry."""
from __future__ import annotations

from types import MappingProxyType

KNOWLEDGE_SCHEMA_DIALECT = "https://json-schema.org/draft/2020-12/schema"
KNOWLEDGE_SCHEMA_VERSION = 1
BASE_ID = "https://vibevm.org/schema/zap-knowledge-event-1/"

ID = {"type": "string", "pattern": "^[A-Za-z0-9._:-]+$"}
SHA = {"type": "string", "pattern": "^[0-9A-Fa-f]{64}$"}
HASH64 = {"type": "string", "minLength": 64, "maxLength": 64}
TEXT = {"type": "string", "pattern": "\\S"}
STRING = {"type": "string"}
STRINGS = {"type": "array", "items": TEXT}
ENDPOINT_KINDS = ["decision", "evidence", "fact", "node", "obligation", "outcome", "source", "task"]
REGION_STATES = ["bounded", "evidenced", "invalidated", "unexamined"]
RELEVANCE = ["irrelevant", "relevant", "unknown"]


def obj(properties, *, required=None, **keywords):
    fields = list(properties) if required is None else list(required)
    return {"type": "object", "additionalProperties": False, "required": fields, "properties": properties, **keywords}


ENDPOINT = obj({"kind": {"enum": ENDPOINT_KINDS}, "id": ID})
SCOPE = obj({
    "kind": {"enum": ["project", "subjects", "unassessed"]},
    "subjects": {"type": "array", "items": ENDPOINT, "uniqueItems": True},
}, allOf=[{
    "if": {"properties": {"kind": {"enum": ["project", "unassessed"]}}},
    "then": {"properties": {"subjects": {"maxItems": 0}}},
}])
SOURCE = obj({
    "schema": {"const": "zap-source/1"}, "id": ID,
    "source_kind": {"enum": ["file", "vibevm_xml_spec"]},
    "root": TEXT, "path": TEXT, "content_sha256": SHA,
    "bytes": {"type": "integer", "minimum": 0}, "applicability_scope": SCOPE,
})
OBSERVED = obj({
    "status": {"enum": ["changed", "current", "unavailable"]},
    "sha256": {"oneOf": [HASH64, {"type": "null"}]},
    "bytes": {"oneOf": [{"type": "integer", "minimum": 0}, {"type": "null"}]},
    "detail": {"oneOf": [STRING, {"type": "null"}]},
}, oneOf=[
    {"properties": {"status": {"const": "unavailable"}, "sha256": {"type": "null"},
                    "bytes": {"type": "null"}, "detail": {"type": "string"}}},
    {"properties": {"status": {"enum": ["changed", "current"]}, "sha256": HASH64,
                    "bytes": {"type": "integer", "minimum": 0}, "detail": {"type": "null"}}},
])
NATIVE_FACT = obj({
    "id": ID, "address": TEXT, "marker": ID, "text": TEXT,
    "normative_status": {"oneOf": [STRING, {"type": "null"}]},
    "source_id": ID, "source_sha256": SHA,
    "observation_status": {"const": "unobserved"}, "acceptance_status": {"const": "unassessed"},
})
REGION = obj({"id": ID, "question": TEXT, "node_refs": STRINGS, "relevance": {"enum": RELEVANCE}})


def event(kind, properties, **keywords):
    return {
        "$schema": KNOWLEDGE_SCHEMA_DIALECT,
        "$id": BASE_ID + kind.removeprefix("knowledge.") + ".json",
        "x-zap-schema-version": KNOWLEDGE_SCHEMA_VERSION,
        "x-zap-state-validation": "Reducer additionally checks references, current revisions, cycles, and cross-field identity bindings.",
        **obj(properties, **keywords),
    }


KNOWLEDGE_EVENT_SCHEMAS = MappingProxyType({
    "knowledge.source-recorded": event("knowledge.source-recorded", {"source": SOURCE}),
    "knowledge.native-facts-recorded": event("knowledge.native-facts-recorded", {
        "capture": obj({"source": SOURCE, "facts": {"type": "array", "items": NATIVE_FACT}}),
    }),
    "knowledge.source-recaptured": event("knowledge.source-recaptured", {"previous_sha256": SHA, "source": SOURCE}),
    "knowledge.source-observation-recorded": event("knowledge.source-observation-recorded", {
        "source_id": ID, "observed": OBSERVED, "claim": TEXT, "artifact_refs": STRINGS,
    }),
    "knowledge.source-observed": event("knowledge.source-observed", {"source_id": ID, "observed": OBSERVED}),
    "knowledge.dependency-recorded": event("knowledge.dependency-recorded", {
        "id": ID, "prerequisite": ENDPOINT, "dependent": ENDPOINT,
        "relation": {"enum": ["affects", "consumes", "depends_on", "derived_from", "supports", "verifies"]},
    }),
    "knowledge.closure-assessed": event("knowledge.closure-assessed", {
        "subject": ENDPOINT, "status": {"enum": ["complete", "incomplete", "unknown"]},
        "boundary": {"type": "array", "items": ENDPOINT}, "missing": {"type": "array", "items": ENDPOINT},
        "evidence_refs": STRINGS, "basis": TEXT,
    }, allOf=[{
        "if": {"properties": {"status": {"const": "complete"}}},
        "then": {"properties": {"missing": {"maxItems": 0}, "evidence_refs": {"minItems": 1}}},
    }]),
    "knowledge.applicability-assessed": event("knowledge.applicability-assessed", {
        "source_id": ID, "status": {"enum": ["applicable", "not_applicable", "unknown"]},
        "scope": SCOPE, "evidence_refs": STRINGS, "basis": TEXT,
    }, allOf=[{
        "if": {"properties": {"status": {"const": "applicable"}}},
        "then": {"properties": {"evidence_refs": {"minItems": 1},
                                "scope": {"properties": {"kind": {"enum": ["project", "subjects"]}}}}},
    }]),
    "knowledge.region-transitioned": event("knowledge.region-transitioned", {
        "region_id": ID, "from": {"enum": REGION_STATES}, "to": {"enum": REGION_STATES},
        "evidence_refs": STRINGS, "reason": TEXT,
    }, allOf=[{
        "if": {"properties": {"to": {"const": "evidenced"}}},
        "then": {"properties": {"evidence_refs": {"minItems": 1}}},
    }]),
    "knowledge.region-relevance-set": event("knowledge.region-relevance-set", {
        "region_id": ID, "relevance": {"enum": RELEVANCE}, "reason": TEXT,
    }),
    "knowledge.region-split": event("knowledge.region-split", {
        "region_id": ID, "children": {"type": "array", "minItems": 2, "items": REGION}, "reason": TEXT,
    }),
    "knowledge.region-merged": event("knowledge.region-merged", {
        "region_ids": {"type": "array", "minItems": 2, "uniqueItems": True, "items": ID},
        "merged": REGION, "reason": TEXT,
    }),
})

__all__ = ("KNOWLEDGE_EVENT_SCHEMAS", "KNOWLEDGE_SCHEMA_DIALECT", "KNOWLEDGE_SCHEMA_VERSION")
