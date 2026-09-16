"""Framework-independent authenticated application backend for ZAP clients."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any, Iterable

from .artifacts import PrivateArtifactStore, read_registered_source
from .backend_queries import ENTITY_KINDS, entity_detail, overview, search, subgraph
from .backend_sanitize import sanitize_event, sanitize_public_value
from .common import Refusal, need
from .control_trust import Principal
from .engine import ENGINE_CAPTURE_OBSERVATION_KINDS, Engine
from .domain import build_sparse_review_transition


BACKEND_CAPABILITIES = MappingProxyType({
    "schema": "zap-backend-capabilities/1",
    "descriptor_dialect": "zap-operation-descriptor/1",
    "reads": {
        "snapshot": {"required": [], "optional": ["base_sha256"]},
        "events": {"required": [], "optional": ["after", "base_sha256"]},
        "stream": {"required": [], "optional": ["after", "base_sha256", "Last-Event-ID"]},
        "overview": {"required": [], "optional": ["limit", "cursor", "states", "work_types", "knowledge_states", "entity_kinds", "base_sha256"]},
        "subgraph": {"required": ["id"], "optional": ["depth", "limit", "cursor", "base_sha256"]},
        "search": {"required": ["q"], "optional": ["kinds", "limit", "cursor", "base_sha256"]},
        "detail": {"required": ["kind", "id"], "optional": ["base_sha256"]},
        "source_content": {"required": ["id", "sha256"], "optional": ["offset", "limit", "base_sha256"]},
        "artifact_content": {"required": ["handle", "sha256"], "optional": ["offset", "limit", "base_sha256"]},
        "assessments": {"required": [], "optional": ["id", "base_sha256"]},
    },
    "commands": {
        "agent": {"route": "agent_data", "required": ["command"]},
        "control": {"route": "credentialed_control", "required": ["command"]},
        "observation": {"route": "credentialed_runtime_observation", "required": ["command"]},
        "action": {"route": "credentialed_action", "required": ["command", "action", "assessment"], "optional": ["exception_id"]},
        "tick": {"route": "exact_configured_host_principal", "required": []},
        "materialize_review_transition": {"route": "authenticated_pure_builder", "required": ["request"]},
    },
    "entity_kinds": sorted(ENTITY_KINDS),
    "page_limit": {"default": 100, "maximum": 500},
    "content_limit_bytes": {"default": 65536, "maximum": 1048576},
    "http_body_limit_bytes": {"default": 2097152, "operator_configurable": True, "zero_means_unlimited": True},
    "live_follow_seconds": {"default": 30.0, "operator_configurable": True},
})


def _public_state(state: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(state)
    knowledge = result.get("extensions", {}).get("knowledge", {})
    for source in knowledge.get("sources", {}).values():
        source.pop("root", None)
    runtime = result.get("extensions", {}).get("runtime", {})
    for job in runtime.get("jobs", {}).values():
        if "packet" in job:
            job["packet"] = {"redacted": True, "sha256": job.get("packet_sha256")}
    return sanitize_public_value(result)


class BackendApplication:
    """Read/query/mutation surface independent of its HTTP adapter."""

    def __init__(self, engine: Engine, artifacts: PrivateArtifactStore | None = None, coordinator: Any | None = None):
        need(isinstance(engine, Engine), "BACKEND", "backend requires a composed engine")
        self.engine = engine
        self.artifacts = artifacts
        self.coordinator = coordinator

    def authenticate(self, credential_id: str, credential: str) -> Principal:
        state, _events, _pending = self.engine.load()
        return self.engine.trust.authenticate(credential_id, credential, state["plan"]["plan_id"])

    def _read(self) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any] | None]:
        return self.engine.load()

    @staticmethod
    def _identity(state: dict[str, Any], requested_base: str | None = None) -> None:
        if requested_base is not None:
            need(requested_base == state["base_sha256"], "FOREIGN_BASE", "request belongs to another campaign base")

    def capabilities(self, *, requested_base: str | None = None) -> dict[str, Any]:
        result = self.engine.capabilities()
        if requested_base is not None:
            need(requested_base == result["base_sha256"], "FOREIGN_BASE", "request belongs to another campaign base")
        result["backend"] = copy.deepcopy(dict(BACKEND_CAPABILITIES))
        return result

    def snapshot(self, *, requested_base: str | None = None) -> dict[str, Any]:
        state, _events, pending = self._read()
        self._identity(state, requested_base)
        public = _public_state(state)
        return {
            "schema": "zap-backend-snapshot/1",
            "campaign_id": state["plan"]["plan_id"],
            "base_sha256": state["base_sha256"],
            "revision": state["revision"],
            "cursor": state["revision"],
            "projection_schema": state["projection_schema"],
            "state": public,
            "pending_tail": copy.deepcopy(pending),
            "mutation_admitted": pending is None,
        }

    def events(
        self,
        *,
        after: int = -1,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        state, events, pending = self._read()
        self._identity(state, requested_base)
        need(type(after) is int and -1 <= after <= state["revision"], "CURSOR_GAP", "event cursor is outside committed history")
        rows = [sanitize_event(event) for event in events if event["revision"] > after]
        return {
            "schema": "zap-event-tail/1",
            "campaign_id": state["plan"]["plan_id"],
            "base_sha256": state["base_sha256"],
            "from_cursor": after,
            "cursor": state["revision"],
            "events": rows,
            "pending_tail": copy.deepcopy(pending),
            "gap": False,
        }

    def overview(
        self,
        *,
        limit: int = 100,
        cursor: str | None = None,
        states: Iterable[str] = (),
        work_types: Iterable[str] = (),
        knowledge_states: Iterable[str] = (),
        entity_kinds: Iterable[str] = (),
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        state, _events, _pending = self._read()
        self._identity(state, requested_base)
        return overview(
            state, limit=limit, cursor=cursor, states=states,
            work_types=work_types, knowledge_states=knowledge_states,
            entity_kinds=entity_kinds,
        )

    def subgraph(
        self,
        node_id: str,
        *,
        depth: int = 1,
        limit: int = 100,
        cursor: str | None = None,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        state, _events, _pending = self._read()
        self._identity(state, requested_base)
        return subgraph(state, node_id, depth=depth, limit=limit, cursor=cursor)

    def search(
        self,
        query: str,
        *,
        kinds: Iterable[str] = (),
        limit: int = 100,
        cursor: str | None = None,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        state, _events, _pending = self._read()
        self._identity(state, requested_base)
        return search(state, query, kinds=kinds, limit=limit, cursor=cursor)

    def detail(
        self,
        kind: str,
        entity_id: str,
        *,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        state, events, _pending = self._read()
        self._identity(state, requested_base)
        return entity_detail(state, events, kind, entity_id)

    def source_content(
        self,
        source_id: str,
        expected_sha256: str,
        *,
        offset: int = 0,
        limit: int = 65536,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        need(self.artifacts is not None, "CONTENT", "private artifact store is not configured")
        state, _events, _pending = self._read()
        self._identity(state, requested_base)
        result = read_registered_source(
            state, self.artifacts, source_id, expected_sha256,
            offset=offset, limit=limit,
        )
        return {**result, "campaign_id": state["plan"]["plan_id"], "base_sha256": state["base_sha256"],
                "revision": state["revision"], "cursor": state["revision"]}

    def artifact_content(
        self,
        handle: str,
        expected_sha256: str,
        *,
        offset: int = 0,
        limit: int = 65536,
        requested_base: str | None = None,
    ) -> dict[str, Any]:
        need(self.artifacts is not None, "CONTENT", "private artifact store is not configured")
        state, _events, _pending = self._read()
        self._identity(state, requested_base)
        result = self.artifacts.read(handle, expected_sha256, offset=offset, limit=limit)
        return {**result, "campaign_id": state["plan"]["plan_id"], "base_sha256": state["base_sha256"],
                "revision": state["revision"], "cursor": state["revision"]}

    def submit_agent(self, command: Any) -> dict[str, Any]:
        return self.engine.service.submit_agent(command)

    def assessment_status(self, request_id: str | None = None,
                          *, requested_base: str | None = None) -> dict[str, Any]:
        need(self.coordinator is not None, "RUNTIME", "automatic coordinator is not configured")
        state, _events, pending = self._read()
        self._identity(state, requested_base)
        provider = self.coordinator.config.assessment_provider
        need(callable(getattr(provider, "status", None)), "RUNTIME", "inspectable assessment provider is not configured")
        return {"campaign_id": state["plan"]["plan_id"], "base_sha256": state["base_sha256"],
                "revision": state["revision"], "cursor": state["revision"],
                "pending_tail": copy.deepcopy(pending), **provider.status(request_id)}

    def materialize_review_transition(self, request: Any) -> dict[str, Any]:
        state, _events, pending = self._read()
        transition = build_sparse_review_transition(state, request)
        return {
            "schema": "zap-domain/materialized-review-transition/1",
            "campaign_id": state["plan"]["plan_id"], "base_sha256": state["base_sha256"],
            "revision": state["revision"], "cursor": state["revision"],
            "pending_tail": copy.deepcopy(pending), "transition": transition,
        }

    def submit_control(self, command: Any, credential_id: str, credential: str) -> dict[str, Any]:
        return self.engine.service.submit_control(command, credential_id=credential_id, credential=credential)

    def submit_observation(self, command: Any, credential_id: str, credential: str) -> dict[str, Any]:
        kind = command.get("kind") if isinstance(command, dict) else None
        if isinstance(kind, str):
            need(kind not in ENGINE_CAPTURE_OBSERVATION_KINDS, "OBSERVATION", "source capture must use the guarded byte-capture adapter")
        return self.engine.service.submit_observation(command, credential_id=credential_id, credential=credential)

    def apply_action(
        self,
        command: Any,
        action: Any,
        assessment: Any,
        credential_id: str,
        credential: str,
        *,
        exception_id: str | None = None,
    ) -> dict[str, Any]:
        return self.engine.service.apply_control_action(
            command, action, assessment,
            credential_id=credential_id, credential=credential,
            exception_id=exception_id,
        )

    def tick(self, principal: Principal) -> dict[str, Any]:
        need(self.coordinator is not None, "RUNTIME", "automatic coordinator is not configured")
        host = self.engine.host_principal
        need(host is not None and principal == host, "PRINCIPAL_SCOPE", "tick credential must match the configured runtime host principal")
        return self.coordinator.tick()
