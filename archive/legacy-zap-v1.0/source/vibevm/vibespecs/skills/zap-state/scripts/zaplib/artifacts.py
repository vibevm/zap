"""Private content-addressed source/artifact storage with guarded handle reads."""
from __future__ import annotations

import base64
import copy
import os
from pathlib import Path
from typing import Any, Iterable
import uuid

from .common import exact, identity, need, packed, parse, sha
from .sources import capture_source, capture_vibevm_facts
from .storage import safe_path, write_new, writer_lock

CATALOG_SCHEMA = "zap-private-artifacts/1"
ENTRY_KINDS = {"source", "artifact", "transport_output", "promotion"}
DEFAULT_READ_LIMIT = 64 * 1024
MAX_READ_LIMIT = 1024 * 1024


def _empty_catalog() -> dict[str, Any]:
    return {"schema": CATALOG_SCHEMA, "entries": {}}


def _hash(value: Any) -> str:
    need(isinstance(value, str) and len(value) == 64 and set(value) <= set("0123456789abcdef"), "HASH", "invalid content hash")
    return value


def _guarded_file(path: str | os.PathLike[str], allowed_root: str | os.PathLike[str]) -> tuple[Path, Path, str]:
    root = safe_path(allowed_root)
    source = safe_path(path)
    need(root.is_dir() and source.is_file(), "PATH", "capture needs an existing regular file and root")
    try:
        relative = source.relative_to(root)
    except ValueError as exc:
        raise ValueError("source escapes allowed root") from exc
    need(not relative.is_absolute() and ".." not in relative.parts, "PATH", "source escapes allowed root")
    return source, root, relative.as_posix()


class PrivateArtifactStore:
    """A private catalog; public readers address only registered handles/hashes."""

    def __init__(self, root: str | os.PathLike[str], *, create: bool = False):
        raw = Path(root).absolute()
        if create:
            raw.mkdir(parents=True, exist_ok=True)
            try:
                raw.chmod(0o700)
            except OSError:
                pass
        self.root = safe_path(raw)
        need(self.root.is_dir(), "ARTIFACT_STORE", "private artifact root does not exist")
        self.blobs = safe_path(self.root / "blobs")
        self.catalog_path = safe_path(self.root / "catalog.json")
        if create:
            self.blobs.mkdir(exist_ok=True)
        else:
            need(self.blobs.is_dir(), "ARTIFACT_STORE", "private artifact blob directory does not exist")
        if create and not self.catalog_path.exists():
            write_new(self.catalog_path, packed(_empty_catalog()) + b"\n")
        else:
            need(self.catalog_path.is_file(), "ARTIFACT_STORE", "private artifact catalog does not exist")
        self._load()

    def _load(self) -> dict[str, Any]:
        catalog = parse(self.catalog_path.read_bytes(), tagged=True)
        exact(catalog, {"schema", "entries"})
        need(catalog["schema"] == CATALOG_SCHEMA and isinstance(catalog["entries"], dict), "ARTIFACT_STORE", "invalid private artifact catalog")
        return catalog

    def _blob_path(self, digest: str) -> Path:
        digest = _hash(digest)
        directory = safe_path(self.blobs / digest[:2])
        directory.mkdir(exist_ok=True)
        return safe_path(directory / digest)

    def _publish_catalog(self, catalog: dict[str, Any]) -> None:
        temporary = safe_path(self.root / f".catalog-{uuid.uuid4().hex}.tmp")
        try:
            write_new(temporary, packed(catalog) + b"\n")
            os.replace(temporary, self.catalog_path)
        finally:
            if temporary.exists():
                temporary.unlink()

    def register_bytes(
        self,
        handle: str,
        content: bytes,
        *,
        kind: str,
        origin: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        handle = identity(handle)
        need(isinstance(content, bytes), "ARTIFACT", "artifact content must be bytes")
        need(kind in ENTRY_KINDS, "ARTIFACT", "unsupported artifact kind")
        need(origin is None or isinstance(origin, dict), "ARTIFACT", "artifact origin must be a mapping")
        digest = sha(content)
        blob = self._blob_path(digest)
        with writer_lock(self.root):
            if blob.exists():
                need(blob.is_file() and sha(blob.read_bytes()) == digest, "ARTIFACT_STORE", "content-addressed blob differs")
            else:
                temporary = safe_path(blob.parent / f".{digest}.{uuid.uuid4().hex}.tmp")
                try:
                    write_new(temporary, content)
                    try:
                        os.link(temporary, blob)
                    except FileExistsError:
                        pass
                finally:
                    if temporary.exists():
                        temporary.unlink()
                need(blob.exists() and sha(blob.read_bytes()) == digest, "ARTIFACT_STORE", "blob publication failed")
            catalog = self._load()
            entry = catalog["entries"].setdefault(handle, {"handle": handle, "kind": kind, "versions": []})
            need(entry["kind"] == kind, "ARTIFACT", "registered handle kind differs")
            existing = next((row for row in entry["versions"] if row["sha256"] == digest), None)
            descriptor = {
                "sha256": digest,
                "bytes": len(content),
                "blob": str(blob.relative_to(self.root).as_posix()),
                "origin": copy.deepcopy(origin or {}),
            }
            if existing is None:
                entry["versions"].append(descriptor)
            else:
                need(existing == descriptor, "ARTIFACT", "registered artifact version differs")
            entry["versions"].sort(key=lambda row: row["sha256"])
            entry["current_sha256"] = digest
            self._publish_catalog(catalog)
        return {"handle": handle, "kind": kind, "sha256": digest, "bytes": len(content)}

    def capture_file(
        self,
        handle: str,
        path: str | os.PathLike[str],
        allowed_root: str | os.PathLike[str],
        *,
        kind: str = "artifact",
    ) -> dict[str, Any]:
        source, _root, relative = _guarded_file(path, allowed_root)
        raw = source.read_bytes()
        return self.register_bytes(handle, raw, kind=kind, origin={"locator": relative})

    def descriptors(self) -> list[dict[str, Any]]:
        catalog = self._load()
        return [
            {
                "handle": handle,
                "kind": row["kind"],
                "current_sha256": row["current_sha256"],
                "versions": [{"sha256": item["sha256"], "bytes": item["bytes"]} for item in row["versions"]],
            }
            for handle, row in sorted(catalog["entries"].items())
        ]

    def read(
        self,
        handle: str,
        expected_sha256: str,
        *,
        offset: int = 0,
        limit: int = DEFAULT_READ_LIMIT,
    ) -> dict[str, Any]:
        handle = identity(handle)
        expected_sha256 = _hash(expected_sha256)
        need(type(offset) is int and offset >= 0, "RANGE", "offset must be nonnegative")
        need(type(limit) is int and 0 < limit <= MAX_READ_LIMIT, "RANGE", "invalid content read limit")
        entry = self._load()["entries"].get(handle)
        need(isinstance(entry, dict), "REFERENCE", "artifact handle is not registered")
        version = next((row for row in entry["versions"] if row["sha256"] == expected_sha256), None)
        need(isinstance(version, dict), "REFERENCE", "artifact hash is not registered for handle")
        blob = safe_path(self.root / version["blob"])
        try:
            blob.relative_to(self.root)
        except ValueError as exc:
            raise ValueError("artifact blob escapes private root") from exc
        raw = blob.read_bytes()
        need(len(raw) == version["bytes"] and sha(raw) == expected_sha256, "ARTIFACT_STORE", "registered blob bytes differ")
        part = raw[offset:offset + limit]
        try:
            text = part.decode("utf-8")
            encoding, content = "utf-8", text
        except UnicodeDecodeError:
            encoding, content = "base64", base64.b64encode(part).decode("ascii")
        return {
            "schema": "zap-content/1",
            "handle": handle,
            "kind": entry["kind"],
            "sha256": expected_sha256,
            "bytes": len(raw),
            "offset": offset,
            "returned_bytes": len(part),
            "complete": offset + len(part) >= len(raw),
            "next_offset": None if offset + len(part) >= len(raw) else offset + len(part),
            "encoding": encoding,
            "content": content,
        }


def capture_source_blob(
    artifacts: PrivateArtifactStore,
    path: str | os.PathLike[str],
    allowed_root: str | os.PathLike[str],
    *,
    source_id: str | None = None,
    source_kind: str = "file",
    applicability_scope: dict[str, Any] | None = None,
) -> dict[str, Any]:
    descriptor = capture_source(
        path, allowed_root, source_id=source_id, source_kind=source_kind,
        applicability_scope=applicability_scope,
    )
    source, _root, relative = _guarded_file(path, allowed_root)
    raw = source.read_bytes()
    need(sha(raw) == descriptor["content_sha256"] and len(raw) == descriptor["bytes"], "STALE", "source changed during capture")
    blob = artifacts.register_bytes(
        f"source:{descriptor['id']}", raw, kind="source",
        origin={"locator": relative, "source_kind": descriptor["source_kind"]},
    )
    return {"source": descriptor, "blob": blob}


def capture_native_facts_blob(
    artifacts: PrivateArtifactStore,
    path: str | os.PathLike[str],
    allowed_root: str | os.PathLike[str],
    *,
    source_id: str | None = None,
    applicability_scope: dict[str, Any] | None = None,
) -> dict[str, Any]:
    capture = capture_vibevm_facts(
        path, allowed_root, source_id=source_id,
        applicability_scope=applicability_scope,
    )
    descriptor = capture["source"]
    source, _root, relative = _guarded_file(path, allowed_root)
    raw = source.read_bytes()
    need(sha(raw) == descriptor["content_sha256"] and len(raw) == descriptor["bytes"], "STALE", "native fact source changed during capture")
    blob = artifacts.register_bytes(
        f"source:{descriptor['id']}", raw, kind="source",
        origin={"locator": relative, "source_kind": descriptor["source_kind"]},
    )
    return {"capture": capture, "blob": blob}


def read_registered_source(
    state: dict[str, Any],
    artifacts: PrivateArtifactStore,
    source_id: str,
    expected_sha256: str,
    *,
    offset: int = 0,
    limit: int = DEFAULT_READ_LIMIT,
) -> dict[str, Any]:
    source_id = identity(source_id)
    source = state.get("extensions", {}).get("knowledge", {}).get("sources", {}).get(source_id)
    need(isinstance(source, dict), "REFERENCE", "source handle is not registered in campaign state")
    versions: Iterable[str] = source.get("versions", [source.get("content_sha256")])
    need(expected_sha256 in versions, "REFERENCE", "source hash is not registered in campaign history")
    return artifacts.read(f"source:{source_id}", expected_sha256, offset=offset, limit=limit)
