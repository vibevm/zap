"""Authenticated localhost HTTP JSON and resumable SSE adapter."""
from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import json
import time
from typing import Any, Iterable
from urllib.parse import parse_qs, unquote, urlparse

from .backend import BackendApplication
from .common import Refusal, exact, need, packed, parse

MAX_BODY = 2 * 1024 * 1024


def _one(query: dict[str, list[str]], name: str, default: str | None = None) -> str | None:
    values = query.get(name)
    if not values:
        return default
    need(len(values) == 1, "QUERY", f"query parameter {name} must appear once")
    return values[0]


def _integer(query: dict[str, list[str]], name: str, default: int) -> int:
    raw = _one(query, name)
    if raw is None:
        return default
    try:
        return int(raw)
    except ValueError as exc:
        raise Refusal("QUERY", f"query parameter {name} must be an integer") from exc


def _csv(query: dict[str, list[str]], name: str) -> list[str]:
    raw = _one(query, name)
    return [] if not raw else [item for item in raw.split(",") if item]


def _status(exc: Refusal) -> int:
    if exc.code in {"CREDENTIAL"}:
        return 401
    if exc.code in {"AUTHORIZATION", "PRINCIPAL", "PRINCIPAL_SCOPE", "CREDENTIAL_SCOPE"}:
        return 403
    if exc.code in {"REFERENCE", "UNKNOWN_JOB"}:
        return 404
    if exc.code in {"STALE", "STALE_POLICY", "FOREIGN_BASE", "CURSOR_GAP", "PAGE_STALE", "IDEMPOTENCY", "PENDING_TAIL"}:
        return 409
    return 400


class ZapHTTPServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(
        self,
        address: tuple[str, int],
        application: BackendApplication,
        *,
        allowed_origins: Iterable[str] = (),
        max_body_bytes: int | None = MAX_BODY,
        max_follow_seconds: float = 30.0,
    ):
        self.application = application
        self.allowed_origins = frozenset(allowed_origins)
        need(max_body_bytes is None or type(max_body_bytes) is int and max_body_bytes > 0, "BIND", "max body bytes must be positive or null")
        need(isinstance(max_follow_seconds, (int, float)) and max_follow_seconds > 0, "BIND", "max follow seconds must be positive")
        self.max_body_bytes = max_body_bytes
        self.max_follow_seconds = float(max_follow_seconds)
        super().__init__(address, ZapRequestHandler)


class ZapRequestHandler(BaseHTTPRequestHandler):
    server: ZapHTTPServer
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: Any) -> None:
        # The embedding host owns logging; request headers may carry credentials.
        return

    def _origin(self) -> str | None:
        origin = self.headers.get("Origin")
        if origin is None:
            return None
        need(origin in self.server.allowed_origins, "ORIGIN", "request origin is not allowed")
        return origin

    def _headers(self, status: int, content_type: str, length: int | None = None, *, close: bool = False) -> None:
        origin = self._origin()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        if length is not None:
            self.send_header("Content-Length", str(length))
        if close:
            self.send_header("Connection", "close")
        if origin is not None:
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Vary", "Origin")
        self.end_headers()

    def _json(self, status: int, value: Any) -> None:
        raw = packed(value)
        self._headers(status, "application/json; charset=utf-8", len(raw))
        self.wfile.write(raw)

    def _error(self, exc: Exception) -> None:
        if isinstance(exc, Refusal):
            status, value = _status(exc), {"ok": False, "code": exc.code, "message": str(exc)}
        else:
            status, value = 500, {"ok": False, "code": "INTERNAL", "message": "internal backend error"}
        raw = packed(value)
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.send_header("Content-Length", str(len(raw)))
        origin = self.headers.get("Origin")
        if origin in self.server.allowed_origins:
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Vary", "Origin")
        self.end_headers()
        self.close_connection = True
        self.wfile.write(raw)

    def _credential(self) -> tuple[str, str]:
        credential_id = self.headers.get("X-ZAP-Credential-ID")
        authorization = self.headers.get("Authorization", "")
        need(isinstance(credential_id, str) and credential_id, "CREDENTIAL", "credential id header is required")
        need(authorization.startswith("Bearer ") and len(authorization) > 7, "CREDENTIAL", "bearer credential is required")
        return credential_id, authorization[7:]

    def _principal(self):
        credential_id, credential = self._credential()
        return self.server.application.authenticate(credential_id, credential)

    def _body(self) -> dict[str, Any]:
        try:
            length = int(self.headers.get("Content-Length", ""))
        except ValueError as exc:
            raise Refusal("BODY", "valid Content-Length is required") from exc
        need(length >= 0 and (self.server.max_body_bytes is None or length <= self.server.max_body_bytes), "BODY", "request body is too large")
        try:
            raw = self.rfile.read(length)
            raw.decode("utf-8")
            value = parse(raw)
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise Refusal("BODY", "request body must be UTF-8 JSON") from exc
        need(isinstance(value, dict), "BODY", "request body must be an object")
        return value

    def do_OPTIONS(self) -> None:
        try:
            origin = self._origin()
            need(origin is not None, "ORIGIN", "CORS preflight requires an allowed origin")
            self.send_response(204)
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "Authorization, X-ZAP-Credential-ID, Content-Type, Last-Event-ID")
            self.send_header("Access-Control-Max-Age", "600")
            self.send_header("Vary", "Origin")
            self.send_header("Content-Length", "0")
            self.end_headers()
        except Exception as exc:
            self._error(exc)

    def do_GET(self) -> None:
        try:
            self._origin()
            self._principal()
            parsed = urlparse(self.path)
            query = parse_qs(parsed.query, keep_blank_values=True)
            base = _one(query, "base_sha256")
            if parsed.path == "/v1/health":
                self._json(200, {"ok": True, "schema": "zap-health/1"})
            elif parsed.path == "/v1/capabilities":
                result = self.server.application.capabilities(requested_base=base)
                result["backend"]["active_http"] = {
                    "max_body_bytes": self.server.max_body_bytes,
                    "max_follow_seconds": self.server.max_follow_seconds,
                    "finite_tail": "/v1/stream",
                    "live_follow": "/v1/follow",
                }
                self._json(200, result)
            elif parsed.path == "/v1/snapshot":
                self._json(200, self.server.application.snapshot(requested_base=base))
            elif parsed.path == "/v1/events":
                self._json(200, self.server.application.events(after=_integer(query, "after", -1), requested_base=base))
            elif parsed.path == "/v1/stream":
                self._stream(query, base)
            elif parsed.path == "/v1/follow":
                self._follow(query, base)
            elif parsed.path == "/v1/graph/overview":
                self._json(200, self.server.application.overview(
                    limit=_integer(query, "limit", 100), cursor=_one(query, "cursor"),
                    states=_csv(query, "states"), work_types=_csv(query, "work_types"),
                    knowledge_states=_csv(query, "knowledge_states"), entity_kinds=_csv(query, "entity_kinds"),
                    requested_base=base,
                ))
            elif parsed.path == "/v1/graph/subgraph":
                node_id = _one(query, "id")
                need(node_id is not None, "QUERY", "subgraph id is required")
                self._json(200, self.server.application.subgraph(
                    node_id, depth=_integer(query, "depth", 1),
                    limit=_integer(query, "limit", 100), cursor=_one(query, "cursor"),
                    requested_base=base,
                ))
            elif parsed.path == "/v1/search":
                text = _one(query, "q")
                need(text is not None, "QUERY", "search q is required")
                self._json(200, self.server.application.search(
                    text, kinds=_csv(query, "kinds"), limit=_integer(query, "limit", 100),
                    cursor=_one(query, "cursor"), requested_base=base,
                ))
            elif parsed.path == "/v1/assessments":
                self._json(200, self.server.application.assessment_status(
                    _one(query, "id"), requested_base=base,
                ))
            elif parsed.path.startswith("/v1/entities/"):
                parts = [unquote(item) for item in parsed.path.split("/")[3:]]
                need(len(parts) == 2, "QUERY", "entity path requires kind and id")
                self._json(200, self.server.application.detail(parts[0], parts[1], requested_base=base))
            elif parsed.path.startswith("/v1/content/source/"):
                source_id = unquote(parsed.path[len("/v1/content/source/"):])
                digest = _one(query, "sha256")
                need(digest is not None, "QUERY", "expected source hash is required")
                self._json(200, self.server.application.source_content(
                    source_id, digest, offset=_integer(query, "offset", 0),
                    limit=_integer(query, "limit", 65536), requested_base=base,
                ))
            elif parsed.path.startswith("/v1/content/artifact/"):
                handle = unquote(parsed.path[len("/v1/content/artifact/"):])
                digest = _one(query, "sha256")
                need(digest is not None, "QUERY", "expected artifact hash is required")
                self._json(200, self.server.application.artifact_content(
                    handle, digest, offset=_integer(query, "offset", 0),
                    limit=_integer(query, "limit", 65536), requested_base=base,
                ))
            else:
                self._json(404, {"ok": False, "code": "NOT_FOUND", "message": "unknown backend route"})
        except Exception as exc:
            self._error(exc)

    def _stream(self, query: dict[str, list[str]], base: str | None) -> None:
        after_raw = self.headers.get("Last-Event-ID")
        try:
            after = int(after_raw) if after_raw is not None else _integer(query, "after", -1)
        except ValueError as exc:
            raise Refusal("CURSOR", "Last-Event-ID must be an integer cursor") from exc
        tail = self.server.application.events(after=after, requested_base=base)
        chunks = []
        for event in tail["events"]:
            chunks.append(f"id: {event['revision']}\nevent: zap\ndata: {packed(event).decode('ascii')}\n\n")
        chunks.append(f"id: {tail['cursor']}\nevent: cursor\ndata: {packed({'base_sha256': tail['base_sha256'], 'cursor': tail['cursor'], 'pending_tail': tail['pending_tail']}).decode('ascii')}\n\n")
        raw = "".join(chunks).encode("utf-8")
        self._headers(200, "text/event-stream; charset=utf-8", len(raw))
        self.wfile.write(raw)

    def _follow(self, query: dict[str, list[str]], base: str | None) -> None:
        after_raw = self.headers.get("Last-Event-ID")
        try:
            cursor = int(after_raw) if after_raw is not None else _integer(query, "after", -1)
        except ValueError as exc:
            raise Refusal("CURSOR", "Last-Event-ID must be an integer cursor") from exc
        wait_ms = _integer(query, "wait_ms", int(self.server.max_follow_seconds * 1000))
        need(0 <= wait_ms <= int(self.server.max_follow_seconds * 1000), "QUERY", "follow wait exceeds configured maximum")
        tail = self.server.application.events(after=cursor, requested_base=base)
        self._headers(200, "text/event-stream; charset=utf-8", close=True)
        deadline = time.monotonic() + wait_ms / 1000
        try:
            while True:
                for event in tail["events"]:
                    self.wfile.write(f"id: {event['revision']}\nevent: zap\ndata: {packed(event).decode('ascii')}\n\n".encode("utf-8"))
                cursor = tail["cursor"]
                if tail["events"]:
                    self.wfile.flush()
                if time.monotonic() >= deadline:
                    marker = {"base_sha256": tail["base_sha256"], "cursor": cursor, "pending_tail": tail["pending_tail"]}
                    self.wfile.write(f"id: {cursor}\nevent: cursor\ndata: {packed(marker).decode('ascii')}\n\n".encode("utf-8"))
                    self.wfile.flush()
                    break
                time.sleep(0.05)
                tail = self.server.application.events(after=cursor, requested_base=base)
        except Refusal as exc:
            error = {"ok": False, "code": exc.code, "message": str(exc), "cursor": cursor}
            try:
                self.wfile.write(f"id: {cursor}\nevent: error\ndata: {packed(error).decode('ascii')}\n\n".encode("utf-8"))
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
        except (BrokenPipeError, ConnectionResetError, OSError):
            pass
        self.close_connection = True

    def do_POST(self) -> None:
        try:
            self._origin()
            credential_id, credential = self._credential()
            principal = self.server.application.authenticate(credential_id, credential)
            parsed = urlparse(self.path)
            body = self._body()
            if parsed.path == "/v1/review-transition/materialize":
                exact(body, {"request"})
                result = self.server.application.materialize_review_transition(body["request"])
            else:
                need(principal.role in {"owner", "coordinator"}, "AUTHORIZATION", "reader credential cannot mutate campaign state")
            if parsed.path == "/v1/review-transition/materialize":
                pass
            elif parsed.path == "/v1/agent":
                exact(body, {"command"})
                result = self.server.application.submit_agent(body["command"])
            elif parsed.path == "/v1/control":
                exact(body, {"command"})
                result = self.server.application.submit_control(body["command"], credential_id, credential)
            elif parsed.path == "/v1/observation":
                exact(body, {"command"})
                result = self.server.application.submit_observation(body["command"], credential_id, credential)
            elif parsed.path == "/v1/action":
                exact(body, {"command", "action", "assessment"}, {"exception_id"})
                result = self.server.application.apply_action(
                    body["command"], body["action"], body["assessment"],
                    credential_id, credential, exception_id=body.get("exception_id"),
                )
            elif parsed.path == "/v1/tick":
                exact(body, set())
                result = self.server.application.tick(principal)
            else:
                self._json(404, {"ok": False, "code": "NOT_FOUND", "message": "unknown backend route"})
                return
            self._json(200, result)
        except Exception as exc:
            self._error(exc)


def _loopback(host: str) -> bool:
    if host.casefold() == "localhost":
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def create_server(
    application: BackendApplication,
    *,
    host: str = "127.0.0.1",
    port: int = 0,
    allow_nonlocal: bool = False,
    allowed_origins: Iterable[str] = (),
    max_body_bytes: int | None = MAX_BODY,
    max_follow_seconds: float = 30.0,
) -> ZapHTTPServer:
    need(_loopback(host) or allow_nonlocal, "BIND", "nonlocal binding requires explicit allow_nonlocal")
    need(type(port) is int and 0 <= port <= 65535, "BIND", "invalid server port")
    if max_body_bytes == 0:
        max_body_bytes = None
    return ZapHTTPServer(
        (host, port), application, allowed_origins=allowed_origins,
        max_body_bytes=max_body_bytes, max_follow_seconds=max_follow_seconds,
    )


def serve(
    application: BackendApplication,
    *,
    host: str = "127.0.0.1",
    port: int = 8765,
    allow_nonlocal: bool = False,
    allowed_origins: Iterable[str] = (),
    max_body_bytes: int | None = MAX_BODY,
    max_follow_seconds: float = 30.0,
) -> None:
    server = create_server(
        application, host=host, port=port, allow_nonlocal=allow_nonlocal,
        allowed_origins=allowed_origins, max_body_bytes=max_body_bytes,
        max_follow_seconds=max_follow_seconds,
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
