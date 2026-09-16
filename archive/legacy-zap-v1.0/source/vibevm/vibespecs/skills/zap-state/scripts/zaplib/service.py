"""Trusted application boundary for ZAP control and privileged product events."""
from __future__ import annotations

import copy
from types import MappingProxyType
from typing import Any, Callable, Iterable, Mapping
import uuid

from .common import Refusal, exact, identity, need, packed, sha, string
from .control import (
    ACTION_CLASS_SET,
    CONTROL_EVENT_SCHEMAS,
    CONTROL_HANDLERS,
    CONTROL_VALUE_SCHEMAS,
    COORDINATOR_EVENT_KINDS,
    DATA_EVENT_KINDS,
    INTERNAL_EVENT_KINDS,
    OWNER_EVENT_KINDS,
    PRIVILEGED_EVENT_KINDS,
    active_policy,
    assess_action,
    control_state,
    validate_action,
    validate_assessment,
)
from .records import HandlerSpec, State, validate_command_envelope
from .storage import load_store, record
from .control_trust import CredentialAuthority, CredentialBinding, Principal

CORE_DATA_KINDS = frozenset({
    "node.classified",
    "knowledge.region-recorded",
    "evidence.recorded",
    "fact.recorded",
    "decision.recorded",
    "approach.declared",
    "approach.verdict",
})
CONDITIONAL_ACTION_KINDS = MappingProxyType({"plan.refined": "plan.lower"})


LoadStore = Callable[[Any, Mapping[str, HandlerSpec]], tuple[State, list[dict[str, Any]], dict[str, Any] | None]]
Record = Callable[[Any, Any, Mapping[str, HandlerSpec]], dict[str, Any]]


class ApplicationService:
    """Authorize first, then delegate CAS and append to the common storage cell.

    The object itself belongs inside a trusted application host. An untrusted
    route receives only ``submit_agent`` or ``submit_control``; it is never
    handed ``submit_host`` with a configured host principal.
    """

    def __init__(
        self,
        store: Any,
        handlers: Mapping[str, HandlerSpec],
        trust: CredentialAuthority,
        host_principal: Principal | None = None,
        *,
        action_kinds: Mapping[str, str] | None = None,
        data_kinds: Iterable[str] = (),
        observation_kinds: Iterable[str] = (),
        loader: LoadStore = load_store,
        recorder: Record = record,
    ):
        need(isinstance(handlers, Mapping), "HANDLER", "application service needs an explicit handler registry")
        missing = sorted(set(CONTROL_HANDLERS) - set(handlers))
        need(not missing, "HANDLER", f"application registry lacks control handlers: {', '.join(missing)}")
        need(isinstance(trust, CredentialAuthority), "CREDENTIAL", "application service needs trusted credential configuration")
        if host_principal is not None:
            need(isinstance(host_principal, Principal), "PRINCIPAL", "invalid configured host principal")
        mapped: dict[str, str] = dict(CONDITIONAL_ACTION_KINDS)
        for kind, action_class in (action_kinds or {}).items():
            identity(kind)
            need(kind in handlers, "HANDLER", f"action kind has no handler: {kind}")
            need(action_class in ACTION_CLASS_SET, "ACTION", f"unknown action class for {kind}")
            need(kind not in CONTROL_HANDLERS, "ACTION", "control events cannot be product action kinds")
            mapped[kind] = action_class
        for kind in mapped:
            need(kind in handlers, "HANDLER", f"action kind has no handler: {kind}")
        data = set(CORE_DATA_KINDS)
        for kind in data_kinds:
            identity(kind)
            need(kind in handlers, "HANDLER", f"data kind has no handler: {kind}")
            need(kind not in CONTROL_HANDLERS and kind not in mapped, "HANDLER", f"handler kind has conflicting route classification: {kind}")
            data.add(kind)
        observations: set[str] = set()
        for kind in observation_kinds:
            identity(kind)
            need(kind in handlers, "HANDLER", f"observation kind has no handler: {kind}")
            need(kind not in CONTROL_HANDLERS and kind not in mapped and kind not in data, "HANDLER", f"handler kind has conflicting route classification: {kind}")
            observations.add(kind)
        unclassified = sorted(set(handlers) - set(CONTROL_HANDLERS) - set(mapped) - data - observations)
        need(not unclassified, "HANDLER", f"unclassified extension handlers: {', '.join(unclassified)}")
        self.store = store
        self.handlers = handlers
        self.trust = trust
        self.host_principal = host_principal
        self.action_kinds: Mapping[str, str] = MappingProxyType(mapped)
        self.data_kinds = frozenset(data)
        self.observation_kinds = frozenset(observations)
        self._loader = loader
        self._recorder = recorder

    def _load(self) -> State:
        state, _events, pending = self._loader(self.store, self.handlers)
        need(pending is None, "PENDING_TAIL", "incomplete journal tail prevents control admission")
        return state

    def load_projection(self) -> tuple[State, list[dict[str, Any]], dict[str, Any] | None]:
        """Return the configured loader's detached projection and tail boundary."""
        return self._loader(self.store, self.handlers)

    def _load_all(self) -> tuple[State, list[dict[str, Any]]]:
        state, events, pending = self._loader(self.store, self.handlers)
        need(pending is None, "PENDING_TAIL", "incomplete journal tail prevents control admission")
        return state, events

    @staticmethod
    def _logical_command_hash(command: Mapping[str, Any]) -> str:
        return sha(packed({key: command[key] for key in ("event_id", "kind", "reason", "payload")}))

    @staticmethod
    def _command_shape(command: Any) -> tuple[str, str]:
        exact(command, {"event_id", "base_revision", "kind", "reason", "payload"})
        event_id = identity(command["event_id"])
        kind = identity(command["kind"])
        need(type(command["base_revision"]) is int and command["base_revision"] >= 0, "STALE", "base revision must be nonnegative")
        exact(command["reason"], {"summary"}, {"evidence_refs", "decision_ref"})
        string(command["reason"]["summary"], "reason summary")
        return event_id, kind

    def _new_or_exact_retry(self, state: State, events: list[dict[str, Any]], command: Any) -> tuple[str, dict[str, Any] | None]:
        event_id, kind = self._command_shape(command)
        existing = next((event for event in events if event.get("event_id") == event_id), None)
        if existing is not None:
            need(existing.get("command_sha256") == sha(packed(command)), "IDEMPOTENCY", "event id reused for another command")
        else:
            validate_command_envelope(state, command)
        return kind, existing

    @staticmethod
    def _campaign(state: State) -> str:
        plan = state.get("plan")
        need(isinstance(plan, dict), "CONTROL_STATE", "state has no plan")
        return identity(plan.get("plan_id"))

    def _check_principal(self, principal: Principal, state: State) -> None:
        need(isinstance(principal, Principal), "PRINCIPAL", "invalid trusted principal")
        need(principal.campaign_id == self._campaign(state), "PRINCIPAL_SCOPE", "principal belongs to another campaign")

    def _authenticate(self, credential_id: str, credential: str, state: State) -> Principal:
        return self.trust.authenticate(credential_id, credential, self._campaign(state))

    @staticmethod
    def _authorize_control(principal: Principal, kind: str, *, internal: bool = False) -> None:
        need(kind in CONTROL_HANDLERS, "CONTROL", "event is not a control command")
        if kind in INTERNAL_EVENT_KINDS:
            need(internal, "AUTHORIZATION", "service-internal control event cannot be submitted directly")
            return
        need(kind in principal.control_kinds, "AUTHORIZATION", f"principal is not scoped for {kind}")
        if kind in OWNER_EVENT_KINDS:
            need(principal.role == "owner", "AUTHORIZATION", f"{kind} requires owner authority")
        elif kind in COORDINATOR_EVENT_KINDS:
            need(principal.role in {"owner", "coordinator"}, "AUTHORIZATION", f"{kind} requires coordinator authority")
        else:
            need(kind in DATA_EVENT_KINDS, "AUTHORIZATION", "unknown control authority class")

    def submit_agent(self, command: Any) -> dict[str, Any]:
        """Append a data-only command from the public worker/agent route."""
        state, events = self._load_all()
        _event_id, kind = self._command_shape(command)
        need(kind not in PRIVILEGED_EVENT_KINDS, "AUTHORIZATION", "agent route cannot submit privileged control commands")
        need(kind not in self.observation_kinds, "AUTHORIZATION", "agent route cannot submit trusted transport/source observations")
        draft_refinement = kind == "plan.refined" and active_policy(state) is None
        need(kind not in self.action_kinds or draft_refinement, "AUTHORIZATION", "agent route cannot submit privileged product transitions")
        need(kind not in CONTROL_HANDLERS or kind in DATA_EVENT_KINDS, "AUTHORIZATION", "agent route cannot establish control authority")
        need(kind in self.data_kinds or kind in DATA_EVENT_KINDS or draft_refinement, "AUTHORIZATION", "agent route accepts only explicitly classified data commands")
        self._new_or_exact_retry(state, events, command)
        return self._recorder(self.store, command, self.handlers)

    def submit_control(self, command: Any, *, credential_id: str, credential: str) -> dict[str, Any]:
        """Append one control command through an opaque credential channel."""
        state, events = self._load_all()
        _event_id, kind = self._command_shape(command)
        principal = self._authenticate(credential_id, credential, state)
        self._authorize_control(principal, kind)
        self._new_or_exact_retry(state, events, command)
        return self._recorder(self.store, command, self.handlers)

    def _authorize_observation(self, principal: Principal, kind: str) -> None:
        need(kind in self.observation_kinds, "OBSERVATION", "event is not a registered trusted observation")
        need(principal.role in {"owner", "coordinator"}, "AUTHORIZATION", "trusted observation requires coordinator authority")

    def submit_observation(self, command: Any, *, credential_id: str, credential: str) -> dict[str, Any]:
        """Append an existing-operation/source observation through control auth."""
        state, events = self._load_all()
        _event_id, kind = self._command_shape(command)
        principal = self._authenticate(credential_id, credential, state)
        self._authorize_observation(principal, kind)
        self._new_or_exact_retry(state, events, command)
        return self._recorder(self.store, command, self.handlers)

    def submit_host_observation(self, command: Any) -> dict[str, Any]:
        """Trusted-host observation route; it grants no product transition."""
        state, events = self._load_all()
        _event_id, kind = self._command_shape(command)
        need(self.host_principal is not None, "PRINCIPAL", "no trusted host principal is configured")
        self._check_principal(self.host_principal, state)
        self._authorize_observation(self.host_principal, kind)
        self._new_or_exact_retry(state, events, command)
        return self._recorder(self.store, command, self.handlers)

    def authorize_read(self, *, credential_id: str, credential: str) -> Principal:
        """Authenticate a campaign-scoped read credential for a backend/viewer."""
        state = self._load()
        principal = self._authenticate(credential_id, credential, state)
        need(principal.role in {"reader", "owner", "coordinator"}, "AUTHORIZATION", "principal has no read scope")
        return principal

    def route_descriptors(self) -> dict[str, Any]:
        """Return the service's explicit non-secret handler classification."""
        return {
            "schema": "zap-service-routes/1",
            "data_kinds": sorted(self.data_kinds),
            "action_kinds": dict(sorted(self.action_kinds.items())),
            "observation_kinds": sorted(self.observation_kinds),
            "control_kinds": {
                kind: copy.deepcopy(CONTROL_EVENT_SCHEMAS[kind])
                for kind in sorted(CONTROL_EVENT_SCHEMAS)
            },
            "value_schemas": {
                kind: copy.deepcopy(CONTROL_VALUE_SCHEMAS[kind])
                for kind in sorted(CONTROL_VALUE_SCHEMAS)
            },
        }

    def submit_host(self, command: Any) -> dict[str, Any]:
        """Append one control command as the configured trusted host principal."""
        state, events = self._load_all()
        _event_id, kind = self._command_shape(command)
        need(self.host_principal is not None, "PRINCIPAL", "no trusted host principal is configured")
        self._check_principal(self.host_principal, state)
        self._authorize_control(self.host_principal, kind)
        self._new_or_exact_retry(state, events, command)
        return self._recorder(self.store, command, self.handlers)

    def apply_control_action(
        self,
        command: Any,
        action: Any,
        assessment: Any,
        *,
        credential_id: str,
        credential: str,
        exception_id: str | None = None,
    ) -> dict[str, Any]:
        """Assess, admit and append one exact privileged product transition."""
        state = self._load()
        principal = self._authenticate(credential_id, credential, state)
        return self._apply_action(principal, state, command, action, assessment, exception_id)

    def apply_host_action(
        self,
        command: Any,
        action: Any,
        assessment: Any,
        *,
        exception_id: str | None = None,
    ) -> dict[str, Any]:
        """Trusted-host variant of ``apply_control_action``."""
        state = self._load()
        need(self.host_principal is not None, "PRINCIPAL", "no trusted host principal is configured")
        self._check_principal(self.host_principal, state)
        return self._apply_action(self.host_principal, state, command, action, assessment, exception_id)

    def _apply_action(
        self,
        principal: Principal,
        state: State,
        command: Any,
        action: Any,
        assessment: Any,
        exception_id: str | None,
    ) -> dict[str, Any]:
        _event_id, command_kind = self._command_shape(command)
        expected_action = self.action_kinds.get(command_kind)
        need(expected_action is not None, "ACTION", "command kind is not registered as a privileged product transition")
        checked_action = validate_action(action)
        checked_assessment = validate_assessment(assessment)
        need(checked_action["action_class"] == expected_action, "ACTION", "command kind/action class binding differs")
        need(expected_action in principal.action_classes, "AUTHORIZATION", f"principal is not scoped for {expected_action}")
        need(sha(packed(command["payload"])) == checked_action["payload_sha256"], "ACTION_HASH", "action payload hash differs from command")
        need(checked_action["campaign_id"] == principal.campaign_id, "PRINCIPAL_SCOPE", "action belongs to another campaign")
        logical_hash = self._logical_command_hash(command)

        state, events = self._load_all()
        self._check_principal(principal, state)
        control = control_state(state)
        existing_event = next((event for event in events if event.get("event_id") == command["event_id"]), None)
        if existing_event is not None:
            existing_hash = sha(packed({key: existing_event[key] for key in ("event_id", "kind", "reason", "payload")}))
            need(existing_hash == logical_hash, "IDEMPOTENCY", "product event id belongs to another logical command")
            matching = [row for row in control["admissions"].values() if row.get("product_event_id") == command["event_id"]]
            need(
                len(matching) == 1
                and matching[0]["product_command_sha256"] == logical_hash
                and matching[0]["action_sha256"] == sha(packed(checked_action)),
                "AUTHORIZATION",
                "committed product event lacks its exact action reservation",
            )
            return {
                "ok": True,
                "idempotent": True,
                "assessment": None,
                "admission_id": None,
                "product": {"ok": True, "idempotent": True, "event": existing_event, "revision": state["revision"]},
                "policy": active_policy(state),
            }

        reservations = [
            row for row in control["admissions"].values()
            if row.get("product_event_id") == command["event_id"]
        ]
        need(len(reservations) <= 1, "CONTROL_STATE", "multiple reservations target one product event")
        if reservations:
            reservation = reservations[0]
            need(exception_id == reservation["exception_id"], "EXCEPTION", "reservation retry must name the same exception binding")
            need(
                reservation["product_command_sha256"] == logical_hash
                and reservation["action_sha256"] == sha(packed(checked_action)),
                "AUTHORIZATION",
                "existing reservation belongs to another logical action",
            )
            grant = control.get("pending_grant")
            grant_is_current = (
                isinstance(grant, dict)
                and grant.get("admission_id") == reservation["admission_id"]
                and grant.get("for_revision") == state["revision"]
            )
            report = control["assessments"].get(reservation["assessment_id"])
            if not grant_is_current:
                # State moved after durable admission. Re-evaluate every current
                # policy/source/job precondition before rebinding the exact
                # reservation; a new failure or source invalidation can block it.
                fresh_assessment = copy.deepcopy(checked_assessment)
                fresh_assessment["assessment_id"] = identity(f"reassessment:{uuid.uuid4()}")
                assess_action(state, checked_action, fresh_assessment)
                assessment_command = {
                    "event_id": f"assessment:{uuid.uuid4()}",
                    "base_revision": state["revision"],
                    "kind": "control.action-assessed",
                    "reason": {"summary": f"Reassess reserved action {checked_action['action_id']}"},
                    "payload": {"action": checked_action, "assessment": fresh_assessment},
                }
                self._recorder(self.store, assessment_command, self.handlers)
                state = self._load()
                report = control_state(state)["assessments"].get(fresh_assessment["assessment_id"])
                need(isinstance(report, dict), "ASSESSMENT", "fresh reservation assessment is missing")
                if report["policy_result"] == "needs_evidence":
                    raise Refusal("NEEDS_EVIDENCE", "reserved action needs new evidence before rebind")
                if report["policy_result"] == "too_late":
                    raise Refusal("TOO_LATE", "reserved action crossed its protected assessment boundary")
                if reservation["pause_id"] is None and report["policy_result"] == "pause":
                    raise Refusal("PAUSED", "a new sticky pause invalidated the earlier reservation")
                if report["policy_result"] == "clear" and not report["eligible_for_admission"]:
                    raise Refusal("ASSESSMENT", "reserved action is not eligible for admission")
                rebind_command = {
                    "event_id": f"rebind:{uuid.uuid4()}",
                    "base_revision": state["revision"],
                    "kind": "control.action-reservation-rebound",
                    "reason": {"summary": f"Rebind exact action reservation {reservation['admission_id']}"},
                    "payload": {
                        "admission_id": reservation["admission_id"],
                        "assessment_id": fresh_assessment["assessment_id"],
                        "product_event_id": command["event_id"],
                        "product_command_sha256": logical_hash,
                    },
                }
                self._recorder(self.store, rebind_command, self.handlers)
                state = self._load()
            product_command = copy.deepcopy(command)
            product_command["base_revision"] = state["revision"]
            result = self._recorder(self.store, product_command, self.handlers)
            return {
                "ok": True,
                "assessment": report,
                "admission_id": reservation["admission_id"],
                "product": result,
                "policy": active_policy(self._load()),
            }

        # No committed event or durable reservation matches this request. It is
        # a new logical command and must satisfy the shared current CAS.
        validate_command_envelope(state, command)
        # A pure preflight gives a refusal before any append for malformed,
        # wrong-policy, wrong-base, or wrong-source-capture input. The durable
        # assessment below repeats it against the exact CAS revision.
        assess_action(state, checked_action, checked_assessment)

        assessment_command = {
            "event_id": f"assessment:{uuid.uuid4()}",
            "base_revision": state["revision"],
            "kind": "control.action-assessed",
            "reason": {"summary": f"Assess exact action {checked_action['action_id']}"},
            "payload": {"action": checked_action, "assessment": checked_assessment},
        }
        self._recorder(self.store, assessment_command, self.handlers)
        state = self._load()
        report = control_state(state)["assessments"].get(checked_assessment["assessment_id"])
        need(isinstance(report, dict), "ASSESSMENT", "durable assessment is missing")
        if report["policy_result"] == "needs_evidence":
            raise Refusal("NEEDS_EVIDENCE", "affected action needs evidence before admission")
        if report["policy_result"] == "too_late":
            raise Refusal("TOO_LATE", "assessment happened after the protected action boundary")
        if report["policy_result"] == "pause":
            need(exception_id is not None, "PAUSED", "sticky pause blocks action without an exact one-shot exception")
        else:
            need(report["eligible_for_admission"] and exception_id is None, "ASSESSMENT", "action is not eligible for ordinary admission")

        admission_id = identity(f"admission:{uuid.uuid4()}")
        admission_command = {
            "event_id": f"admit:{uuid.uuid4()}",
            "base_revision": state["revision"],
            "kind": "control.action-admitted",
            "reason": {"summary": f"Admit exact action {checked_action['action_id']}"},
            "payload": {
                "admission_id": admission_id,
                "action": checked_action,
                "assessment_id": checked_assessment["assessment_id"],
                "exception_id": exception_id,
                "command_kind": command_kind,
                "product_event_id": command["event_id"],
                "product_command_sha256": logical_hash,
            },
        }
        self._recorder(self.store, admission_command, self.handlers)
        state = self._load()
        grant = control_state(state)["pending_grant"]
        need(isinstance(grant, dict) and grant["admission_id"] == admission_id and grant["for_revision"] == state["revision"], "AUTHORIZATION", "exact action grant was not established")

        product_command = copy.deepcopy(command)
        product_command["base_revision"] = state["revision"]
        result = self._recorder(self.store, product_command, self.handlers)
        return {
            "ok": True,
            "assessment": report,
            "admission_id": admission_id,
            "product": result,
            "policy": active_policy(self._load()),
        }


SERVICE_REQUEST_SCHEMAS: Mapping[str, dict[str, Any]] = MappingProxyType({
    "agent.submit": {
        "required": ["command"],
        "route": "agent",
        "authority": "data-only",
        "credential": False,
    },
    "control.submit": {
        "required": ["credential_id", "credential", "command"],
        "route": "credentialed-control",
        "authority": "event-schema-route",
        "credential": True,
    },
    "observation.submit": {
        "required": ["credential_id", "credential", "command"],
        "route": "credentialed-observation",
        "authority": "coordinator",
        "credential": True,
    },
    "observation.host-submit": {
        "required": ["command"],
        "route": "trusted-host-observation",
        "authority": "configured-host-principal",
        "credential": False,
    },
    "action.apply": {
        "required": ["credential_id", "credential", "command", "action", "assessment"],
        "optional": ["exception_id"],
        "route": "credentialed-control",
        "authority": "action-class",
        "credential": True,
    },
    "read.authorize": {
        "required": ["credential_id", "credential"],
        "route": "credentialed-read",
        "authority": "campaign-reader",
        "credential": True,
    },
})
