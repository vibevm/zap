"""Shared constants, refusals, validation primitives, and wire encoding."""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import math
import re
from typing import Any

SCHEMA = "zap/1"
PROJECTION_SCHEMA = "zap-projection/1"
CAPABILITIES = {"zap.core": 1, "zap.extensions": 1}
ID = re.compile(r"^[A-Za-z0-9._:-]+$")
STATES = {"planned", "ready", "active", "candidate", "accepted", "blocked", "deferred", "dropped", "superseded"}
KINDS = {"portfolio", "campaign", "phase", "workstream", "group", "atom", "gate", "horizon"}
TERMINAL = {"accepted", "deferred", "dropped", "superseded"}
WORK_TYPES = {"evidence", "decision", "change", "verification", "integration"}
MATURITY = {"unspecified", "prototype", "functional", "productized"}
TASK_FIELDS = {"id", "title", "goal", "read_paths", "write_paths", "steps", "positive_cases", "negative_cases", "checks", "acceptance", "safe_stop", "commit_subject", "notes"}
COLLECTIONS = ("unknown_regions", "evidence", "facts", "decisions", "approaches")


class Refusal(ValueError):
    """Expected invalid-input failure carrying a stable machine code."""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def need(value: Any, code: str, message: str) -> None:
    if not value:
        raise Refusal(code, message)


def exact(value: Any, required: set[str], optional: set[str] | tuple[str, ...] = ()) -> None:
    optional_keys = set(optional)
    need(
        isinstance(value, dict) and required <= set(value) <= required | optional_keys,
        "FIELDS",
        f"expected fields {sorted(required)}; optional {sorted(optional_keys)}",
    )


def string(value: Any, label: str, empty: bool = False) -> str:
    need(isinstance(value, str) and (empty or value.strip()), "VALUE", f"invalid {label}")
    return value


def strings(value: Any, label: str) -> list[str]:
    need(isinstance(value, list) and all(isinstance(item, str) and item.strip() for item in value), "VALUE", f"invalid {label}")
    return value


def identity(value: Any) -> str:
    need(isinstance(value, str) and ID.fullmatch(value), "IDENTITY", "invalid stable identity")
    return value


def wire(value: Any) -> Any:
    """Return the canonical JSON representation, including uncommon TOML values."""
    if isinstance(value, (dt.datetime, dt.date, dt.time)):
        return {"$zap_type": type(value).__name__, "value": value.isoformat()}
    if isinstance(value, float) and not math.isfinite(value):
        return {"$zap_type": "float", "value": repr(value)}
    if isinstance(value, dict):
        items = {key: wire(item) for key, item in value.items()}
        return {"$zap_type": "mapping", "value": list(items.items())} if "$zap_type" in value else items
    if isinstance(value, list):
        return [wire(item) for item in value]
    return value


def unwire(value: Any) -> Any:
    if isinstance(value, list):
        return [unwire(item) for item in value]
    if isinstance(value, dict):
        if "$zap_type" in value:
            exact(value, {"$zap_type", "value"})
            kind, raw = value["$zap_type"], value["value"]
            if kind == "mapping":
                return {key: unwire(item) for key, item in raw}
            if kind in {"datetime", "date", "time"}:
                return getattr(dt, kind).fromisoformat(raw)
            if kind == "float" and raw in {"nan", "inf", "-inf"}:
                return float(raw)
            raise Refusal("ENCODING", "unknown tagged TOML value")
        return {key: unwire(item) for key, item in value.items()}
    return value


def packed(value: Any) -> bytes:
    return json.dumps(wire(value), ensure_ascii=True, separators=(",", ":"), sort_keys=True, allow_nan=False).encode("utf-8")


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        need(key not in result, "DUPLICATE", f"duplicate JSON member {key}")
        result[key] = value
    return result


def parse(raw: bytes | str, tagged: bool = False) -> Any:
    value = json.loads(
        raw,
        object_pairs_hook=unique_object,
        parse_constant=lambda _: (_ for _ in ()).throw(Refusal("ENCODING", "non-JSON number")),
    )
    return unwire(value) if tagged else value


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()
