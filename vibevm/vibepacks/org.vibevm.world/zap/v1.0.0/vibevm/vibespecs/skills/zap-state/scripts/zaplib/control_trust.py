"""Opaque campaign-scoped credentials supplied by a trusted application host."""
from __future__ import annotations

from dataclasses import dataclass, field
import hmac
import secrets
from types import MappingProxyType
from typing import Iterable, Mapping

from .common import identity, need, string
from .control import ACTION_CLASS_SET, CONTROL_HANDLERS


@dataclass(frozen=True)
class Principal:
    """Authority fixed by trusted startup or successful credential checking."""

    principal_id: str
    role: str
    campaign_id: str
    control_kinds: frozenset[str] = field(default_factory=frozenset)
    action_classes: frozenset[str] = field(default_factory=frozenset)

    def __post_init__(self) -> None:
        identity(self.principal_id)
        identity(self.campaign_id)
        need(self.role in {"owner", "coordinator", "reader"}, "PRINCIPAL", "trusted principal role must be owner, coordinator, or reader")
        need(self.control_kinds <= set(CONTROL_HANDLERS), "PRINCIPAL", "principal contains unknown control kinds")
        need(self.action_classes <= ACTION_CLASS_SET, "PRINCIPAL", "principal contains unknown action classes")
        if self.role == "reader":
            need(not self.control_kinds and not self.action_classes, "PRINCIPAL", "reader principal cannot carry command authority")


@dataclass(frozen=True, repr=False)
class CredentialBinding:
    """One protected opaque credential and its explicit scope."""

    credential_id: str
    secret: str = field(repr=False)
    principal: Principal

    def __post_init__(self) -> None:
        identity(self.credential_id)
        string(self.secret, "credential secret")

    def __repr__(self) -> str:
        return f"CredentialBinding(credential_id={self.credential_id!r}, secret='***', principal={self.principal!r})"


class CredentialAuthority:
    """In-memory view of protected credential configuration supplied at startup."""

    def __init__(self, bindings: Iterable[CredentialBinding] = ()):
        table: dict[str, CredentialBinding] = {}
        for binding in bindings:
            need(isinstance(binding, CredentialBinding), "CREDENTIAL", "invalid credential binding")
            need(binding.credential_id not in table, "DUPLICATE", "duplicate credential id")
            table[binding.credential_id] = binding
        self._bindings: Mapping[str, CredentialBinding] = MappingProxyType(table)

    @staticmethod
    def issue(
        credential_id: str,
        principal_id: str,
        role: str,
        campaign_id: str,
        *,
        control_kinds: Iterable[str] = (),
        action_classes: Iterable[str] = (),
    ) -> tuple[CredentialBinding, str]:
        """Generate one opaque token for a protected startup configuration."""
        token = secrets.token_urlsafe(32)
        principal = Principal(
            principal_id=principal_id,
            role=role,
            campaign_id=campaign_id,
            control_kinds=frozenset(control_kinds),
            action_classes=frozenset(action_classes),
        )
        return CredentialBinding(credential_id=credential_id, secret=token, principal=principal), token

    def authenticate(self, credential_id: str, secret: str, campaign_id: str) -> Principal:
        key = identity(credential_id)
        string(secret, "credential")
        binding = self._bindings.get(key)
        # compare_digest is still exercised for an unknown id; callers receive
        # one credential refusal surface rather than an existence oracle.
        expected = binding.secret if binding is not None else secrets.token_urlsafe(32)
        valid = hmac.compare_digest(secret.encode("utf-8"), expected.encode("utf-8"))
        need(binding is not None and valid, "CREDENTIAL", "control credential was refused")
        need(binding.principal.campaign_id == campaign_id, "CREDENTIAL_SCOPE", "credential belongs to another campaign")
        return binding.principal
