"""Fail-closed composition of every ZAP reducer and application route."""
from __future__ import annotations

import copy
from dataclasses import dataclass
from types import MappingProxyType
from typing import Any, Mapping

from .control import (
    CONTROL_EVENT_SCHEMAS, CONTROL_HANDLERS, CONTROL_VALUE_SCHEMAS,
    COORDINATOR_EVENT_KINDS, DATA_EVENT_KINDS, INTERNAL_EVENT_KINDS,
    OWNER_EVENT_KINDS,
)
from .control_trust import CredentialAuthority, Principal
from .cli_profile_schema import RUNTIME_PROFILE_JSON_SCHEMA
from .domain import (
    DOMAIN_ACTION_KINDS, DOMAIN_CAPABILITIES, DOMAIN_DATA_KINDS,
    DOMAIN_EVENT_SCHEMAS, DOMAIN_HANDLERS, DOMAIN_OPERATIONS,
)
from .knowledge import (
    KNOWLEDGE_EVENT_ROUTES, KNOWLEDGE_EVENT_SCHEMAS, KNOWLEDGE_HANDLERS,
    KNOWLEDGE_SCHEMA_DIALECT, KNOWLEDGE_SCHEMA_VERSION,
)
from .migration import MIGRATION_OPERATIONS
from .records import CORE_HANDLERS, HandlerSpec, compose_handlers
from .projection_cache import PROJECTION_CACHE_OPERATIONS, ProjectionCache
from .recovery import RECOVERY_OPERATIONS
from .runtime import (
    RUNTIME_ACTION_KINDS, RUNTIME_CAPABILITIES, RUNTIME_DATA_KINDS,
    RUNTIME_EVENT_ROUTES, RUNTIME_EVENT_SCHEMAS, RUNTIME_HANDLERS,
    RUNTIME_OBSERVATION_KINDS,
)
from .service import ApplicationService, SERVICE_REQUEST_SCHEMAS
from .snapshots import SNAPSHOT_OPERATIONS
from .storage import STORAGE_OPERATIONS


CORE_EVENT_SCHEMAS = MappingProxyType({
    "node.classified": {"required": ["node_id", "work_type", "maturity"]},
    "knowledge.region-recorded": {"required": ["id", "question", "node_refs"]},
    "evidence.recorded": {"required": ["id", "claim", "subject", "result", "artifact_refs", "node_refs"]},
    "fact.recorded": {"required": ["id", "statement", "status", "node_refs", "evidence_refs", "source_refs"]},
    "decision.recorded": {"required": ["id", "question", "alternatives", "chosen", "rationale", "authority_ref", "consequences", "node_refs", "evidence_refs"]},
    "approach.declared": {"required": ["id", "problem_id", "description"]},
    "approach.verdict": {"required": ["approach_id", "outcome", "evidence_refs"]},
    "plan.refined": {"required": ["parent_id", "nodes", "edges", "coverage"]},
})


def _knowledge_routes() -> tuple[set[str], dict[str, str], set[str]]:
    data: set[str] = set()
    actions: dict[str, str] = {}
    observations: set[str] = set()
    for kind, descriptor in KNOWLEDGE_EVENT_ROUTES.items():
        route = descriptor.get("route")
        if route == "agent_data":
            data.add(kind)
        elif route == "effect_adapter":
            observations.add(kind)
        elif route == "trusted_service":
            action = descriptor.get("action")
            if not isinstance(action, str):
                raise RuntimeError(f"knowledge action route lacks action: {kind}")
            actions[kind] = action
        else:
            raise RuntimeError(f"unknown knowledge route for {kind}: {route!r}")
    return data, actions, observations


def _compose_routes() -> tuple[frozenset[str], Mapping[str, str], frozenset[str]]:
    knowledge_data, knowledge_actions, knowledge_observations = _knowledge_routes()
    data = set(DOMAIN_DATA_KINDS) | knowledge_data | set(RUNTIME_DATA_KINDS)
    actions = {**dict(DOMAIN_ACTION_KINDS), **knowledge_actions, **dict(RUNTIME_ACTION_KINDS)}
    observations = knowledge_observations | set(RUNTIME_OBSERVATION_KINDS)
    groups = [data, set(actions), observations]
    if any(groups[i] & groups[j] for i in range(len(groups)) for j in range(i + 1, len(groups))):
        raise RuntimeError("ZAP event route sets overlap")
    extension_handlers = set(DOMAIN_HANDLERS) | set(KNOWLEDGE_HANDLERS) | set(RUNTIME_HANDLERS)
    if data | set(actions) | observations != extension_handlers:
        missing = sorted(extension_handlers - data - set(actions) - observations)
        extra = sorted(data | set(actions) | observations - extension_handlers)
        raise RuntimeError(f"ZAP event routes do not cover handlers: missing={missing}, extra={extra}")
    return frozenset(data), MappingProxyType(actions), frozenset(observations)


ENGINE_DATA_KINDS, ENGINE_ACTION_KINDS, ENGINE_OBSERVATION_KINDS = _compose_routes()
ENGINE_CAPTURE_OBSERVATION_KINDS = frozenset(
    kind for kind, descriptor in KNOWLEDGE_EVENT_ROUTES.items()
    if descriptor.get("route") == "effect_adapter"
)
ENGINE_HANDLERS = compose_handlers(
    CORE_HANDLERS, CONTROL_HANDLERS, DOMAIN_HANDLERS, KNOWLEDGE_HANDLERS,
    RUNTIME_HANDLERS,
)


def _event_descriptors() -> dict[str, dict[str, Any]]:
    schemas: dict[str, Any] = {}
    for source in (
        CORE_EVENT_SCHEMAS, CONTROL_EVENT_SCHEMAS, DOMAIN_EVENT_SCHEMAS,
        KNOWLEDGE_EVENT_SCHEMAS, RUNTIME_EVENT_SCHEMAS,
    ):
        overlap = set(schemas) & set(source)
        if overlap:
            raise RuntimeError(f"duplicate event descriptors: {sorted(overlap)}")
        schemas.update(copy.deepcopy(dict(source)))
    if set(schemas) != set(ENGINE_HANDLERS):
        raise RuntimeError("event descriptors do not exactly cover composed handlers")
    result = {}
    for kind, raw in sorted(schemas.items()):
        if kind in INTERNAL_EVENT_KINDS:
            route, action = "internal", None
        elif kind in OWNER_EVENT_KINDS:
            route, action = "owner_control", None
        elif kind in COORDINATOR_EVENT_KINDS:
            route, action = "coordinator_control", None
        elif kind in DATA_EVENT_KINDS or kind in ENGINE_DATA_KINDS or kind in CORE_EVENT_SCHEMAS and kind != "plan.refined":
            route, action = "agent_data", None
        elif kind == "plan.refined":
            route, action = "draft_data_or_action", "plan.lower"
        elif kind in ENGINE_ACTION_KINDS:
            route, action = "action", ENGINE_ACTION_KINDS[kind]
        elif kind in ENGINE_OBSERVATION_KINDS:
            knowledge_route = KNOWLEDGE_EVENT_ROUTES.get(kind, {}).get("route")
            route, action = ("effect_adapter" if knowledge_route == "effect_adapter" else "trusted_observation"), None
        else:
            raise RuntimeError(f"descriptor route missing for {kind}")
        result[kind] = {
            "descriptor_dialect": KNOWLEDGE_SCHEMA_DIALECT if kind in KNOWLEDGE_EVENT_SCHEMAS else "zap-payload-descriptor/1",
            "descriptor_version": KNOWLEDGE_SCHEMA_VERSION if kind in KNOWLEDGE_EVENT_SCHEMAS else 1,
            "payload": raw,
            "route": route,
            "action": action,
        }
    return result


ENGINE_EVENT_DESCRIPTORS = MappingProxyType(_event_descriptors())


@dataclass
class Engine:
    store: Any
    trust: CredentialAuthority
    host_principal: Principal | None = None
    projection_cache: ProjectionCache | None = None

    def __post_init__(self) -> None:
        if self.projection_cache is None:
            self.projection_cache = ProjectionCache()
        self.handlers: Mapping[str, HandlerSpec] = ENGINE_HANDLERS
        self.service = ApplicationService(
            self.store,
            self.handlers,
            self.trust,
            self.host_principal,
            action_kinds=ENGINE_ACTION_KINDS,
            data_kinds=ENGINE_DATA_KINDS,
            observation_kinds=ENGINE_OBSERVATION_KINDS,
            loader=self.projection_cache.load,
            recorder=self.projection_cache.record,
        )

    def load(self) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any] | None]:
        return self.projection_cache.load(self.store, self.handlers)

    def capabilities(self) -> dict[str, Any]:
        state, _events, pending = self.load()
        return {
            "schema": "zap-capabilities/1",
            "campaign_id": state["plan"]["plan_id"],
            "base_sha256": state["base_sha256"],
            "revision": state["revision"],
            "cursor": state["revision"],
            "projection_schema": state["projection_schema"],
            "pending_tail": copy.deepcopy(pending),
            "handlers": sorted(self.handlers),
            "events": copy.deepcopy(dict(ENGINE_EVENT_DESCRIPTORS)),
            "value_schemas": copy.deepcopy(dict(CONTROL_VALUE_SCHEMAS)),
            "routes": self.service.route_descriptors(),
            "roles": {
                "reader": ["read"],
                "agent": ["agent_data"],
                "coordinator": ["read", "trusted_observation", "delegated_action", "coordinator_control"],
                "owner": ["read", "trusted_observation", "delegated_action", "coordinator_control", "owner_control"],
            },
            "domain": copy.deepcopy(dict(DOMAIN_CAPABILITIES)),
            "runtime": copy.deepcopy(dict(RUNTIME_CAPABILITIES)),
            "operations": {
                "storage": copy.deepcopy(STORAGE_OPERATIONS),
                "snapshots": copy.deepcopy(SNAPSHOT_OPERATIONS),
                "migration": copy.deepcopy(MIGRATION_OPERATIONS),
                "recovery": copy.deepcopy(RECOVERY_OPERATIONS),
                "service": copy.deepcopy(dict(SERVICE_REQUEST_SCHEMAS)),
                "domain": {key: copy.deepcopy(dict(value)) for key, value in DOMAIN_OPERATIONS.items()},
                "runtime_profile_schema": copy.deepcopy(RUNTIME_PROFILE_JSON_SCHEMA),
                "projection_cache": copy.deepcopy(PROJECTION_CACHE_OPERATIONS),
            },
        }


def build_engine(
    store: Any,
    trust: CredentialAuthority | None = None,
    host_principal: Principal | None = None,
    projection_cache: ProjectionCache | None = None,
) -> Engine:
    return Engine(store, trust or CredentialAuthority(), host_principal, projection_cache)
