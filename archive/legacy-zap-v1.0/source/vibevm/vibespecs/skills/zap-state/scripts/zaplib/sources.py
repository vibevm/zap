"""Captured source handles, native XML facts, and applicability projection."""
from __future__ import annotations

import copy
import os
from pathlib import Path
import string as ascii_string
from typing import Any
import xml.etree.ElementTree as ET

from .common import exact, identity, need, sha, string, strings
from .storage import safe_path

SOURCE_SCHEMA = "zap-source/1"
SPEC_NAMESPACE = "https://vibevm.org/spec/1"
ENDPOINT_KINDS = {"source", "fact", "evidence", "decision", "task", "node", "obligation", "outcome"}
APPLICABILITY_RESULTS = {"applicable", "not_applicable", "unknown"}


def validate_endpoint(value: Any) -> dict[str, str]:
    exact(value, {"kind", "id"})
    need(value["kind"] in ENDPOINT_KINDS, "ENDPOINT", "unknown knowledge endpoint kind")
    identity(value["id"])
    return copy.deepcopy(value)


def endpoint_key(endpoint: dict[str, str]) -> str:
    endpoint = validate_endpoint(endpoint)
    return f"{endpoint['kind']}:{endpoint['id']}"


def validate_scope(value: Any) -> dict[str, Any]:
    exact(value, {"kind", "subjects"})
    need(value["kind"] in {"unassessed", "project", "subjects"}, "SOURCE_SCOPE", "unknown applicability scope kind")
    need(isinstance(value["subjects"], list), "SOURCE_SCOPE", "scope subjects must be a list")
    subjects = [validate_endpoint(subject) for subject in value["subjects"]]
    need(len({endpoint_key(subject) for subject in subjects}) == len(subjects), "DUPLICATE", "duplicate scope subject")
    need(value["kind"] == "subjects" or not subjects, "SOURCE_SCOPE", "only subjects scope may name subjects")
    return {"kind": value["kind"], "subjects": subjects}


def validate_source_descriptor(value: Any) -> dict[str, Any]:
    exact(value, {"schema", "id", "source_kind", "root", "path", "content_sha256", "bytes", "applicability_scope"})
    need(value["schema"] == SOURCE_SCHEMA, "SOURCE", "unknown captured source schema")
    identity(value["id"])
    need(value["source_kind"] in {"file", "vibevm_xml_spec"}, "SOURCE", "unknown captured source kind")
    string(value["root"], "source root")
    string(value["path"], "source path")
    need(isinstance(value["content_sha256"], str) and len(value["content_sha256"]) == 64 and
         all(character in ascii_string.hexdigits for character in value["content_sha256"]), "SOURCE", "invalid source hash")
    need(type(value["bytes"]) is int and value["bytes"] >= 0, "SOURCE", "invalid source size")
    result = copy.deepcopy(value)
    result["applicability_scope"] = validate_scope(value["applicability_scope"])
    return result


def _inside(path: Path, root: Path) -> Path:
    try:
        return path.relative_to(root)
    except ValueError as exc:
        raise ValueError(f"source is outside allowed root: {path}") from exc


def capture_source(
    path: str | os.PathLike[str],
    allowed_root: str | os.PathLike[str],
    *,
    source_id: str | None = None,
    source_kind: str = "file",
    applicability_scope: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Capture one regular file without assigning truth, acceptance, or authority."""
    root = safe_path(allowed_root)
    target = safe_path(path)
    relative = _inside(target, root)
    need(target.is_file(), "SOURCE", "captured source must be a regular file")
    raw = target.read_bytes()
    locator = relative.as_posix()
    stable_id = source_id or f"source:{sha(locator.encode('utf-8'))}"
    descriptor = {
        "schema": SOURCE_SCHEMA,
        "id": identity(stable_id),
        "source_kind": source_kind,
        "root": str(root),
        "path": locator,
        "content_sha256": sha(raw),
        "bytes": len(raw),
        "applicability_scope": {"kind": "unassessed", "subjects": []} if applicability_scope is None else applicability_scope,
    }
    return validate_source_descriptor(descriptor)


def observe_source(descriptor: dict[str, Any], allowed_root: str | os.PathLike[str] | None = None) -> dict[str, Any]:
    """Produce a deterministic observation payload for a later journal event."""
    descriptor = validate_source_descriptor(descriptor)
    root = safe_path(allowed_root if allowed_root is not None else descriptor["root"])
    need(str(root) == str(safe_path(descriptor["root"])), "SOURCE", "source root binding differs")
    try:
        target = safe_path(root / descriptor["path"])
        _inside(target, root)
        raw = target.read_bytes()
    except (OSError, ValueError) as exc:
        return {"source_id": descriptor["id"], "observed": {"status": "unavailable", "sha256": None, "bytes": None, "detail": type(exc).__name__}}
    observed_hash = sha(raw)
    status = "current" if observed_hash == descriptor["content_sha256"] and len(raw) == descriptor["bytes"] else "changed"
    return {"source_id": descriptor["id"], "observed": {"status": status, "sha256": observed_hash, "bytes": len(raw), "detail": None}}


def capture_vibevm_facts(
    path: str | os.PathLike[str],
    allowed_root: str | os.PathLike[str],
    *,
    source_id: str | None = None,
    applicability_scope: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Capture fact markers only from a native namespaced VibeVM XML spec."""
    target = safe_path(path)
    need(target.suffix.casefold() == ".xml", "SOURCE_FORMAT", "native fact capture accepts XML specs only")
    raw = target.read_bytes()
    need(b"<!DOCTYPE" not in raw.upper() and b"<!ENTITY" not in raw.upper(), "SOURCE_FORMAT", "DTD/entity declarations are not accepted")
    try:
        root_element = ET.fromstring(raw)
    except ET.ParseError as exc:
        raise ValueError(f"invalid XML source: {exc}") from exc
    prefix = f"{{{SPEC_NAMESPACE}}}"
    need(root_element.tag == prefix + "spec", "SOURCE_FORMAT", "native facts require the declared VibeVM spec root and namespace")
    descriptor = capture_source(target, allowed_root, source_id=source_id, source_kind="vibevm_xml_spec", applicability_scope=applicability_scope)
    facts = []
    markers: set[str] = set()
    for element in root_element.iter():
        if not isinstance(element.tag, str) or not element.tag.startswith(prefix) or element.attrib.get("fact") != "true":
            continue
        marker = identity(element.tag[len(prefix):])
        need(marker not in markers, "DUPLICATE", f"duplicate native fact marker {marker}")
        markers.add(marker)
        normative_status = element.attrib.get("status")
        if normative_status is not None:
            string(normative_status, "native fact status")
        text = " ".join("".join(element.itertext()).split())
        string(text, "native fact text")
        facts.append({
            "id": identity(f"{descriptor['id']}:{marker}"),
            "address": f"{descriptor['path']}#{marker}",
            "marker": marker,
            "text": text,
            "normative_status": normative_status,
            "source_id": descriptor["id"],
            "source_sha256": descriptor["content_sha256"],
            "observation_status": "unobserved",
            "acceptance_status": "unassessed",
        })
    return {"source": descriptor, "facts": facts}


def current_applicability(state: dict[str, Any], refs: list[str]) -> dict[str, Any]:
    """Return the exact aggregate applicability wire consumed by domain B."""
    refs = strings(refs, "source refs")
    need(len(set(refs)) == len(refs), "DUPLICATE", "duplicate source reference")
    for source_id in refs:
        identity(source_id)
    knowledge = state.get("extensions", {}).get("knowledge", {})
    sources = knowledge.get("sources", {})
    assessments = knowledge.get("applicability", {})
    closures = knowledge.get("closures", {})
    stale_refs: list[str] = []
    unknown_refs: list[str] = []
    incomplete = False
    if not refs:
        return {"status": "unknown", "refs": [], "stale_refs": [], "unknown_refs": [], "incomplete_closure": True}
    for source_id in refs:
        source = sources.get(source_id)
        if source is None:
            unknown_refs.append(source_id)
            incomplete = True
            continue
        capture_status = source.get("capture_status")
        assessment_record = assessments.get(source_id, {})
        assessment = assessment_record.get("status")
        if capture_status == "changed" or assessment == "not_applicable":
            stale_refs.append(source_id)
        elif capture_status != "current" or assessment != "applicable" or assessment_record.get("source_sha256") != source.get("content_sha256"):
            unknown_refs.append(source_id)
        closure = closures.get(f"source:{source_id}")
        if not closure or closure.get("status") != "complete":
            incomplete = True
            if source_id not in stale_refs and source_id not in unknown_refs:
                unknown_refs.append(source_id)
    status = "stale" if stale_refs else "unknown" if unknown_refs or incomplete else "applicable"
    return {"status": status, "refs": list(refs), "stale_refs": stale_refs, "unknown_refs": unknown_refs, "incomplete_closure": incomplete}


def compare_source_captures(state: dict[str, Any], captures: list[dict[str, str]]) -> dict[str, Any]:
    """Purely compare adaptive-review `{source_id,sha256}` captures to state."""
    need(isinstance(captures, list), "SOURCE", "source captures must be a list")
    validated = []
    seen: set[str] = set()
    for capture in captures:
        exact(capture, {"source_id", "sha256"})
        source_id = identity(capture["source_id"])
        need(source_id not in seen, "DUPLICATE", "duplicate source capture")
        seen.add(source_id)
        need(isinstance(capture["sha256"], str) and len(capture["sha256"]) == 64 and
             all(character in ascii_string.hexdigits for character in capture["sha256"]), "SOURCE", "invalid captured source hash")
        validated.append({"source_id": source_id, "sha256": capture["sha256"]})
    sources = state.get("extensions", {}).get("knowledge", {}).get("sources", {})
    stale = []
    unknown = []
    if not validated:
        return {"status": "unknown", "captures": [], "stale_captures": [], "unknown_captures": []}
    for capture in validated:
        source = sources.get(capture["source_id"])
        if source is None or source.get("capture_status") not in {"current", "changed"}:
            unknown.append(capture)
        elif source.get("capture_status") == "changed" or source.get("content_sha256") != capture["sha256"]:
            stale.append(capture)
    status = "stale" if stale else "unknown" if unknown else "current"
    return {"status": status, "captures": validated, "stale_captures": stale, "unknown_captures": unknown}
