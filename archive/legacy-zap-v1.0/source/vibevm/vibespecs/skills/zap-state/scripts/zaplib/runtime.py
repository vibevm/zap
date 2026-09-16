"""Public composition facade for the persistent automatic ZAP coordinator."""
from types import MappingProxyType

from .runtime_loop import AutomaticCoordinator
from .runtime_model import (
    ACTIVE_JOB_STATES, RUNTIME_ACTION_KINDS as _RUNTIME_ACTION_KINDS,
    RUNTIME_DATA_KINDS as _RUNTIME_DATA_KINDS, RUNTIME_EVENT_ROUTES as _RUNTIME_EVENT_ROUTES,
    RUNTIME_EVENT_SCHEMAS as _RUNTIME_EVENT_SCHEMAS, RUNTIME_HANDLERS as _RUNTIME_HANDLERS,
    RUNTIME_OBSERVATION_KINDS as _RUNTIME_OBSERVATION_KINDS, RUNTIME_SCHEMA,
    runtime_frontier, runtime_state,
)
from .runtime_reconciliation_model import (
    RECONCILIATION_ACTION_KINDS, RECONCILIATION_DATA_KINDS, RECONCILIATION_EVENT_ROUTES,
    RECONCILIATION_EVENT_SCHEMAS, RECONCILIATION_HANDLERS, RECONCILIATION_OBSERVATION_KINDS,
)
from .runtime_packets import RuntimeConfig, VerificationSpec, WorkerProfile, codex_sol_xhigh_worker_profile
from .coordinator_adapter import (
    COORDINATOR_REQUEST_SCHEMA, COORDINATOR_RESPONSE_JSON_SCHEMA,
    COORDINATOR_RESPONSE_SCHEMA, CoordinatorProcessProfile,
    JsonProcessCoordinatorAdapter, SemanticCoordinatorAdapter,
    bind_provider_response, codex_sol_xhigh_adapter, codex_sol_xhigh_profile, command_contracts_for,
    provider_response_json_schema, validate_request, validate_response,
)

RUNTIME_HANDLERS = MappingProxyType({**_RUNTIME_HANDLERS, **RECONCILIATION_HANDLERS})
RUNTIME_EVENT_SCHEMAS = MappingProxyType({**_RUNTIME_EVENT_SCHEMAS, **RECONCILIATION_EVENT_SCHEMAS})
RUNTIME_EVENT_ROUTES = MappingProxyType({**_RUNTIME_EVENT_ROUTES, **RECONCILIATION_EVENT_ROUTES})
RUNTIME_ACTION_KINDS = MappingProxyType({**_RUNTIME_ACTION_KINDS, **RECONCILIATION_ACTION_KINDS})
RUNTIME_DATA_KINDS = frozenset(_RUNTIME_DATA_KINDS | RECONCILIATION_DATA_KINDS)
RUNTIME_OBSERVATION_KINDS = frozenset(_RUNTIME_OBSERVATION_KINDS | RECONCILIATION_OBSERVATION_KINDS)
RUNTIME_CAPABILITIES = MappingProxyType({"schema": RUNTIME_SCHEMA, "version": 1, "events": tuple(sorted(RUNTIME_HANDLERS)),
    "data_kinds": tuple(sorted(RUNTIME_DATA_KINDS)), "action_kinds": dict(RUNTIME_ACTION_KINDS),
    "observation_kinds": tuple(sorted(RUNTIME_OBSERVATION_KINDS))})

__all__ = (
    "ACTIVE_JOB_STATES", "AutomaticCoordinator", "COORDINATOR_REQUEST_SCHEMA",
    "COORDINATOR_RESPONSE_JSON_SCHEMA", "COORDINATOR_RESPONSE_SCHEMA",
    "CoordinatorProcessProfile", "JsonProcessCoordinatorAdapter",
    "RECONCILIATION_ACTION_KINDS", "RECONCILIATION_DATA_KINDS", "RECONCILIATION_EVENT_ROUTES",
    "RECONCILIATION_EVENT_SCHEMAS", "RECONCILIATION_HANDLERS", "RECONCILIATION_OBSERVATION_KINDS",
    "RUNTIME_ACTION_KINDS", "RUNTIME_CAPABILITIES", "RUNTIME_DATA_KINDS",
    "RUNTIME_EVENT_ROUTES", "RUNTIME_EVENT_SCHEMAS", "RUNTIME_HANDLERS",
    "RUNTIME_OBSERVATION_KINDS", "RUNTIME_SCHEMA", "RuntimeConfig",
    "SemanticCoordinatorAdapter", "VerificationSpec", "WorkerProfile",
    "codex_sol_xhigh_adapter", "codex_sol_xhigh_profile", "runtime_frontier", "runtime_state",
    "codex_sol_xhigh_worker_profile",
    "bind_provider_response", "command_contracts_for", "provider_response_json_schema",
    "validate_request", "validate_response",
)
