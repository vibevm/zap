"""Explicit protected trust bootstrap and loading outside campaign storage."""
from __future__ import annotations

import os
from pathlib import Path
import stat
from typing import Any, Iterable

from .common import exact, identity, need, packed, parse, string
from .control import (
    ACTION_CLASSES, COORDINATOR_EVENT_KINDS, OWNER_EVENT_KINDS,
)
from .control_trust import CredentialAuthority, CredentialBinding, Principal
from .storage import safe_path, write_new

TRUST_SCHEMA = "zap-trust/1"


def _outside_store(path: Path, store: Path) -> None:
    try:
        path.relative_to(store)
    except ValueError:
        return
    raise ValueError("trust material must be outside campaign storage")


def _protect(path: Path, directory: bool = False) -> None:
    try:
        path.chmod(0o700 if directory else 0o600)
    except OSError:
        pass


def _write_secret(path: Path, value: str) -> None:
    write_new(path, (value + "\n").encode("utf-8"))
    _protect(path)


def bootstrap_trust(
    store: str | os.PathLike[str],
    state: dict[str, Any],
    trust_dir: str | os.PathLike[str],
) -> dict[str, Any]:
    store_path = safe_path(store)
    target = Path(trust_dir).absolute()
    _outside_store(target, store_path)
    need(not target.exists(), "OUTPUT_EXISTS", "trust bootstrap requires a fresh directory")
    target.mkdir(parents=True, exist_ok=False)
    _protect(target, directory=True)
    specifications = (
        ("owner", "owner", OWNER_EVENT_KINDS | COORDINATOR_EVENT_KINDS, ACTION_CLASSES),
        ("coordinator", "coordinator", COORDINATOR_EVENT_KINDS, ACTION_CLASSES),
        ("reader", "reader", (), ()),
    )
    rows, token_paths = [], {}
    try:
        for name, role, control_kinds, action_classes in specifications:
            credential_id = f"{name}-credential"
            binding, token = CredentialAuthority.issue(
                credential_id, f"{name}-principal", role,
                state["plan"]["plan_id"], control_kinds=control_kinds,
                action_classes=action_classes,
            )
            token_path = target / f"{credential_id}.token"
            _write_secret(token_path, token)
            token_paths[name] = str(token_path)
            rows.append({
                "credential_id": binding.credential_id,
                "principal_id": binding.principal.principal_id,
                "role": binding.principal.role,
                "campaign_id": binding.principal.campaign_id,
                "secret": binding.secret,
                "control_kinds": sorted(binding.principal.control_kinds),
                "action_classes": sorted(binding.principal.action_classes),
            })
        config = {
            "schema": TRUST_SCHEMA,
            "campaign_id": state["plan"]["plan_id"],
            "base_sha256": state["base_sha256"],
            "credentials": rows,
        }
        config_path = target / "trust.json"
        write_new(config_path, packed(config) + b"\n")
        _protect(config_path)
    except Exception:
        # A partially created trust directory contains secrets and is never
        # treated as usable. Leave it for the operator to inspect/remove.
        raise
    return {
        "ok": True,
        "schema": TRUST_SCHEMA,
        "trust_config": str(config_path),
        "credential_files": token_paths,
        "campaign_id": config["campaign_id"],
        "base_sha256": config["base_sha256"],
        "secrets_in_output": False,
    }


def _check_protection(path: Path) -> None:
    need(path.is_file(), "CREDENTIAL", "trust file is not a regular file")
    if os.name != "nt":
        mode = stat.S_IMODE(path.stat().st_mode)
        need(mode & 0o077 == 0, "CREDENTIAL", "trust file must not be group/world accessible")


def load_trust(
    config_path: str | os.PathLike[str],
    state: dict[str, Any],
    *,
    store: str | os.PathLike[str] | None = None,
) -> tuple[CredentialAuthority, dict[str, Principal]]:
    path = safe_path(config_path)
    if store is not None:
        _outside_store(path, safe_path(store))
    _check_protection(path)
    value = parse(path.read_bytes(), tagged=True)
    exact(value, {"schema", "campaign_id", "base_sha256", "credentials"})
    need(value["schema"] == TRUST_SCHEMA, "CREDENTIAL", "unknown trust configuration")
    need(value["campaign_id"] == state["plan"]["plan_id"] and value["base_sha256"] == state["base_sha256"], "CREDENTIAL_SCOPE", "trust configuration belongs to another campaign/base")
    need(isinstance(value["credentials"], list), "CREDENTIAL", "trust credentials must be a list")
    bindings, principals = [], {}
    for row in value["credentials"]:
        exact(row, {"credential_id", "principal_id", "role", "campaign_id", "secret", "control_kinds", "action_classes"})
        principal = Principal(
            principal_id=identity(row["principal_id"]),
            role=row["role"],
            campaign_id=identity(row["campaign_id"]),
            control_kinds=frozenset(row["control_kinds"]),
            action_classes=frozenset(row["action_classes"]),
        )
        need(principal.campaign_id == value["campaign_id"], "CREDENTIAL_SCOPE", "credential row belongs to another campaign")
        need(principal.principal_id not in principals, "DUPLICATE", "duplicate principal id")
        binding = CredentialBinding(identity(row["credential_id"]), string(row["secret"], "credential"), principal)
        bindings.append(binding)
        principals[principal.principal_id] = principal
    return CredentialAuthority(bindings), principals


def read_credential(path: str | os.PathLike[str]) -> str:
    target = safe_path(path)
    _check_protection(target)
    try:
        value = target.read_text(encoding="utf-8").strip()
    except UnicodeDecodeError as exc:
        raise ValueError("credential file must be UTF-8") from exc
    return string(value, "credential")


def select_principal(principals: dict[str, Principal], principal_id: str) -> Principal:
    principal_id = identity(principal_id)
    principal = principals.get(principal_id)
    need(principal is not None, "PRINCIPAL", "configured host principal does not exist")
    return principal
