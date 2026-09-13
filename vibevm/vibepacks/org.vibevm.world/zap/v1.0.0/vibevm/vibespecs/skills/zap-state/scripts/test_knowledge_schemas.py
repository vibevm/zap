"""Machine contract coverage for exact nested knowledge event payloads."""
from __future__ import annotations

import json
import unittest

from zaplib.engine import ENGINE_EVENT_DESCRIPTORS
from zaplib.knowledge import (
    KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_EVENT_SCHEMAS, KNOWLEDGE_HANDLERS,
    KNOWLEDGE_SCHEMA_DIALECT, KNOWLEDGE_SCHEMA_VERSION,
)


class KnowledgeSchemaTests(unittest.TestCase):
    def test_every_handler_has_one_strict_versioned_json_schema_and_route(self):
        self.assertEqual(set(KNOWLEDGE_EVENT_SCHEMAS), set(KNOWLEDGE_HANDLERS))
        self.assertEqual(set(KNOWLEDGE_EVENT_SCHEMAS), set(KNOWLEDGE_EVENT_ROUTES))
        for kind, schema in KNOWLEDGE_EVENT_SCHEMAS.items():
            self.assertEqual(schema["$schema"], KNOWLEDGE_SCHEMA_DIALECT)
            self.assertEqual(schema["x-zap-schema-version"], KNOWLEDGE_SCHEMA_VERSION)
            self.assertEqual(schema["type"], "object")
            self.assertFalse(schema["additionalProperties"])
            self.assertEqual(set(schema["required"]), set(schema["properties"]))
            descriptor = ENGINE_EVENT_DESCRIPTORS[kind]
            self.assertEqual(descriptor["descriptor_dialect"], KNOWLEDGE_SCHEMA_DIALECT)
            self.assertEqual(descriptor["descriptor_version"], KNOWLEDGE_SCHEMA_VERSION)
            self.assertEqual(descriptor["payload"], schema)
        json.dumps(dict(KNOWLEDGE_EVENT_SCHEMAS))

    def test_source_observation_and_adjudication_schemas_are_nested(self):
        observed = KNOWLEDGE_EVENT_SCHEMAS["knowledge.source-observed"]["properties"]["observed"]
        self.assertEqual(set(observed["required"]), {"status", "sha256", "bytes", "detail"})
        self.assertFalse(observed["additionalProperties"])
        self.assertEqual(observed["properties"]["status"]["enum"], ["changed", "current", "unavailable"])
        source = KNOWLEDGE_EVENT_SCHEMAS["knowledge.source-recorded"]["properties"]["source"]
        self.assertEqual(source["properties"]["schema"]["const"], "zap-source/1")
        self.assertEqual(source["properties"]["applicability_scope"]["properties"]["kind"]["enum"],
                         ["project", "subjects", "unassessed"])
        closure = KNOWLEDGE_EVENT_SCHEMAS["knowledge.closure-assessed"]
        self.assertEqual(closure["properties"]["status"]["enum"], ["complete", "incomplete", "unknown"])
        self.assertEqual(closure["properties"]["boundary"]["items"]["properties"]["kind"]["enum"],
                         ["decision", "evidence", "fact", "node", "obligation", "outcome", "source", "task"])

    def test_region_command_schemas_expose_exact_model_writable_shapes(self):
        transition = KNOWLEDGE_EVENT_SCHEMAS["knowledge.region-transitioned"]
        self.assertEqual(transition["properties"]["from"]["enum"],
                         ["bounded", "evidenced", "invalidated", "unexamined"])
        self.assertEqual(transition["properties"]["to"]["enum"],
                         ["bounded", "evidenced", "invalidated", "unexamined"])
        self.assertIn("allOf", transition)
        relevance = KNOWLEDGE_EVENT_SCHEMAS["knowledge.region-relevance-set"]
        self.assertEqual(relevance["properties"]["relevance"]["enum"],
                         ["irrelevant", "relevant", "unknown"])
        split = KNOWLEDGE_EVENT_SCHEMAS["knowledge.region-split"]["properties"]["children"]
        self.assertEqual(split["minItems"], 2)
        self.assertEqual(set(split["items"]["required"]), {"id", "question", "node_refs", "relevance"})
        merged = KNOWLEDGE_EVENT_SCHEMAS["knowledge.region-merged"]
        self.assertTrue(merged["properties"]["region_ids"]["uniqueItems"])
        self.assertEqual(set(merged["properties"]["merged"]["required"]),
                         {"id", "question", "node_refs", "relevance"})


if __name__ == "__main__":
    unittest.main()
