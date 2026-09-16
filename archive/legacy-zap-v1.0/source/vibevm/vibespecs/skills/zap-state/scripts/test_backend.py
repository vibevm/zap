from __future__ import annotations

import json
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

from test_service import make_store
from test_runtime_support import RuntimeFixture
from zaplib.artifacts import PrivateArtifactStore, capture_source_blob
from zaplib.backend import BackendApplication
from zaplib.backend_http import create_server
from zaplib.backend_queries import entity_detail
from zaplib.backend_sanitize import sanitize_public_value
from zaplib.cli_trust import bootstrap_trust, load_trust, read_credential
from zaplib.common import Refusal
from zaplib.engine import Engine, build_engine
from zaplib.domain import domain_state
from zaplib.runtime import AutomaticCoordinator, runtime_state
from zaplib.storage import import_mup


class BackendTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        parent = self.root / "fixture"
        parent.mkdir()
        self.store = make_store(parent)
        state = build_engine(self.store).load()[0]
        bootstrap = bootstrap_trust(self.store, state, self.root / "trust")
        trust, self.principals = load_trust(bootstrap["trust_config"], state, store=self.store)
        self.tokens = {
            role: read_credential(path)
            for role, path in bootstrap["credential_files"].items()
        }
        self.ids = {role: f"{role}-credential" for role in self.tokens}
        self.engine = Engine(self.store, trust)
        self.artifacts = PrivateArtifactStore(self.root / "artifacts", create=True)
        self.application = BackendApplication(self.engine, self.artifacts)
        self.server = create_server(
            self.application, allowed_origins=["http://viewer.local"],
        )
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        host, port = self.server.server_address
        self.base = f"http://{host}:{port}"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.temp.cleanup()

    def request(self, path, *, role=None, method="GET", body=None, raw_body=None, headers=None):
        request_headers = dict(headers or {})
        if role is not None:
            request_headers["X-ZAP-Credential-ID"] = self.ids[role]
            request_headers["Authorization"] = f"Bearer {self.tokens[role]}"
        data = raw_body if raw_body is not None else None if body is None else json.dumps(body).encode("utf-8")
        if data is not None:
            request_headers["Content-Type"] = "application/json"
        request = Request(self.base + path, data=data, headers=request_headers, method=method)
        try:
            with urlopen(request, timeout=3) as response:
                return response.status, dict(response.headers), response.read()
        except HTTPError as exc:
            return exc.code, dict(exc.headers), exc.read()

    def test_auth_cors_overview_detail_and_foreign_base(self):
        status, _headers, body = self.request("/v1/graph/overview")
        self.assertEqual(status, 401)
        status, headers, _body = self.request(
            "/v1/stream", method="OPTIONS", headers={"Origin": "http://viewer.local"},
        )
        self.assertEqual(status, 204)
        self.assertIn("Last-Event-ID", headers["Access-Control-Allow-Headers"])
        status, headers, body = self.request(
            "/v1/graph/overview?limit=1",
            role="reader", headers={"Origin": "http://viewer.local"},
        )
        self.assertEqual(status, 200)
        self.assertEqual(headers["Access-Control-Allow-Origin"], "http://viewer.local")
        overview = json.loads(body)
        status, _headers, capabilities_body = self.request("/v1/capabilities", role="reader")
        self.assertEqual(status, 200)
        capabilities = json.loads(capabilities_body)
        self.assertIn("job", capabilities["backend"]["entity_kinds"])
        self.assertEqual(capabilities["backend"]["commands"]["tick"]["route"], "exact_configured_host_principal")
        self.assertEqual(capabilities["backend"]["active_http"]["max_body_bytes"], 2 * 1024 * 1024)
        self.assertEqual(capabilities["backend"]["active_http"]["live_follow"], "/v1/follow")
        self.assertEqual(overview["counts"]["nodes"], 2)
        self.assertFalse(overview["page"]["complete"])
        self.assertTrue(overview["data_state"]["knowledge_unknown_is_not_unloaded"])
        status, _headers, body = self.request("/v1/entities/node/work", role="reader")
        self.assertEqual(status, 200)
        detail = json.loads(body)["detail"]
        self.assertEqual(detail["criteria"], ["works"])
        self.assertIn("steps", detail)
        self.assertIn(detail["availability"]["provenance"], {"base_import", "event_history"})
        self.assertFalse(detail["availability"]["stale"])
        status, _headers, subgraph_body = self.request("/v1/graph/subgraph?id=work", role="reader")
        self.assertEqual(status, 200)
        edge_id = json.loads(subgraph_body)["edges"][0]["id"]
        status, _headers, body = self.request(f"/v1/entities/edge/{quote(edge_id, safe='')}", role="reader")
        self.assertEqual(status, 200)
        edge_detail = json.loads(body)["detail"]
        self.assertEqual(edge_detail["entity"]["relation"], "contains")
        self.assertEqual(edge_detail["availability"]["provenance"], "derived")
        for event_id, kind, payload in (
            ("region-http", "knowledge.region-recorded", {"id": "unknown-area", "question": "What remains unknown?", "node_refs": ["work"]}),
            ("decision-http", "decision.recorded", {
                "id": "decision-http", "question": "Which route?",
                "alternatives": [{"id": "a", "description": "A"}, {"id": "b", "description": "B"}],
                "chosen": "a", "rationale": "Captured choice", "authority_ref": "unverified:data",
                "consequences": ["Inspect it"], "node_refs": ["work"], "evidence_refs": [],
            }),
        ):
            state = self.engine.load()[0]
            command = {"event_id": event_id, "base_revision": state["revision"], "kind": kind,
                       "reason": {"summary": "collect canvas detail"}, "payload": payload}
            status, _headers, _body = self.request("/v1/agent", role="coordinator", method="POST", body={"command": command})
            self.assertEqual(status, 200)
        for kind, entity_id in (("region", "unknown-area"), ("decision", "decision-http")):
            status, _headers, body = self.request(f"/v1/entities/{kind}/{entity_id}", role="reader")
            self.assertEqual(status, 200)
            collected = json.loads(body)["detail"]
            self.assertEqual(collected["availability"]["provenance"], "event_history")
            self.assertEqual(collected["availability"]["history"], "available")
        status, _headers, body = self.request(
            "/v1/graph/overview?entity_kinds=region,decision", role="reader",
        )
        self.assertEqual(status, 200)
        entity_overview = json.loads(body)
        self.assertEqual({row["entity_kind"] for row in entity_overview["page"]["items"]}, {"region", "decision"})
        self.assertTrue(entity_overview["data_state"]["filtered"])
        status, _headers, body = self.request(
            "/v1/snapshot?base_sha256=" + "0" * 64, role="reader",
        )
        self.assertEqual(status, 409)
        self.assertEqual(json.loads(body)["code"], "FOREIGN_BASE")
        status, _headers, _body = self.request(
            "/v1/health", role="reader", headers={"Origin": "https://evil.invalid"},
        )
        self.assertEqual(status, 400)
        revision = self.engine.load()[0]["revision"]
        command = {
            "event_id": "evil-origin-write", "base_revision": revision,
            "kind": "node.classified", "reason": {"summary": "must not run"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"},
        }
        status, _headers, _body = self.request(
            "/v1/agent", role="coordinator", method="POST",
            headers={"Origin": "https://evil.invalid"}, body={"command": command},
        )
        self.assertEqual(status, 400)
        self.assertEqual(self.engine.load()[0]["revision"], revision)

    def test_reader_cannot_mutate_and_page_cursor_stales_after_agent_data(self):
        status, _headers, body = self.request("/v1/graph/overview?limit=1", role="reader")
        cursor = json.loads(body)["page"]["next_cursor"]
        state = self.engine.load()[0]
        command = {
            "event_id": "classify-http", "base_revision": state["revision"],
            "kind": "node.classified", "reason": {"summary": "classify"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "functional"},
        }
        status, _headers, _body = self.request(
            "/v1/agent", role="reader", method="POST", body={"command": command},
        )
        self.assertEqual(status, 403)
        status, _headers, body = self.request(
            "/v1/agent", role="coordinator", method="POST", body={"command": command},
        )
        self.assertEqual(status, 200)
        status, _headers, body = self.request(
            f"/v1/graph/overview?limit=1&cursor={cursor}", role="reader",
        )
        self.assertEqual(status, 409)
        self.assertEqual(json.loads(body)["code"], "PAGE_STALE")

    def test_sse_reconnect_uses_committed_cursor(self):
        state = self.engine.load()[0]
        command = {
            "event_id": "classify-sse", "base_revision": state["revision"],
            "kind": "node.classified", "reason": {"summary": "classify"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"},
        }
        self.request("/v1/agent", role="coordinator", method="POST", body={"command": command})
        status, headers, body = self.request(
            "/v1/stream", role="reader", headers={"Last-Event-ID": "0"},
        )
        self.assertEqual(status, 200)
        self.assertTrue(headers["Content-Type"].startswith("text/event-stream"))
        text = body.decode("utf-8")
        self.assertIn("id: 1", text)
        self.assertIn("event: zap", text)
        status, _headers, body = self.request(
            "/v1/stream", role="reader", headers={"Last-Event-ID": "1"},
        )
        self.assertEqual(status, 200)
        self.assertNotIn("event: zap", body.decode("utf-8"))
        self.assertIn("event: cursor", body.decode("utf-8"))

    def test_snapshot_then_tail_loses_no_event_and_unknown_request_fields_refuse(self):
        status, _headers, body = self.request("/v1/snapshot", role="reader")
        snapshot = json.loads(body)
        state = self.engine.load()[0]
        command = {
            "event_id": "snapshot-tail-event", "base_revision": state["revision"],
            "kind": "node.classified", "reason": {"summary": "classify"},
            "payload": {"node_id": "work", "work_type": "verification", "maturity": "functional"},
        }
        self.request("/v1/agent", role="coordinator", method="POST", body={"command": command})
        status, _headers, body = self.request(
            f"/v1/events?after={snapshot['cursor']}&base_sha256={snapshot['base_sha256']}",
            role="reader",
        )
        tail = json.loads(body)
        self.assertEqual([row["event_id"] for row in tail["events"]], ["snapshot-tail-event"])
        status, _headers, body = self.request(
            "/v1/agent", role="coordinator", method="POST",
            body={"command": command, "unknown": True},
        )
        self.assertEqual(status, 400)
        self.assertEqual(json.loads(body)["code"], "FIELDS")
        revision = self.engine.load()[0]["revision"]
        for raw, code in (
            (b'{"command":{},"command":{}}', "DUPLICATE"),
            (b'{"command":NaN}', "ENCODING"),
            (b'{"command":Infinity}', "ENCODING"),
        ):
            status, _headers, body = self.request(
                "/v1/agent", role="coordinator", method="POST", raw_body=raw,
            )
            self.assertEqual(status, 400)
            self.assertEqual(json.loads(body)["code"], code)
            self.assertEqual(self.engine.load()[0]["revision"], revision)

    def test_live_follow_delivers_a_later_event_and_reconnects_by_cursor(self):
        received = {}

        def follow():
            received["response"] = self.request(
                "/v1/follow?after=0&wait_ms=1200", role="reader",
            )

        thread = threading.Thread(target=follow)
        thread.start()
        threading.Event().wait(0.15)
        state = self.engine.load()[0]
        command = {
            "event_id": "follow-event", "base_revision": state["revision"],
            "kind": "node.classified", "reason": {"summary": "follow"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"},
        }
        status, _headers, _body = self.request(
            "/v1/agent", role="coordinator", method="POST", body={"command": command},
        )
        self.assertEqual(status, 200)
        thread.join(timeout=3)
        self.assertFalse(thread.is_alive())
        status, headers, body = received["response"]
        self.assertEqual(status, 200)
        self.assertTrue(headers["Content-Type"].startswith("text/event-stream"))
        self.assertIn("follow-event", body.decode("utf-8"))
        status, _headers, body = self.request(
            "/v1/stream", role="reader", headers={"Last-Event-ID": "1"},
        )
        self.assertEqual(status, 200)
        self.assertNotIn("event: zap", body.decode("utf-8"))
        status, _headers, body = self.request(
            "/v1/follow?after=0&wait_ms=0&base_sha256=" + "0" * 64, role="reader",
        )
        self.assertEqual(status, 409)
        self.assertEqual(json.loads(body)["code"], "FOREIGN_BASE")

    def test_snapshot_wire_encodes_toml_date_and_reserved_mapping(self):
        parent = self.root / "tagged-fixture"
        parent.mkdir()
        make_store(parent)
        plan = parent / "plan.toml"
        plan.write_text(
            plan.read_text(encoding="utf-8").replace(
                'current_node = "work"\n',
                'current_node = "work"\ncaptured_date = 2026-09-13\nreserved = { "$zap_type" = "owner-literal", value = "kept" }\n',
            ),
            encoding="utf-8",
        )
        store = parent / "tagged-store"
        import_mup(plan, parent / "tasks", store)
        state = build_engine(store).load()[0]
        bootstrap = bootstrap_trust(store, state, self.root / "tagged-trust")
        trust, _principals = load_trust(bootstrap["trust_config"], state, store=store)
        server = create_server(BackendApplication(Engine(store, trust)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            host, port = server.server_address
            request = Request(
                f"http://{host}:{port}/v1/snapshot",
                headers={
                    "X-ZAP-Credential-ID": "reader-credential",
                    "Authorization": f"Bearer {read_credential(bootstrap['credential_files']['reader'])}",
                },
            )
            with urlopen(request, timeout=3) as response:
                snapshot = json.loads(response.read())
            plan_state = snapshot["state"]["plan"]
            self.assertEqual(plan_state["captured_date"], {"$zap_type": "date", "value": "2026-09-13"})
            self.assertEqual(plan_state["reserved"]["$zap_type"], "mapping")
            self.assertIn(["$zap_type", "owner-literal"], plan_state["reserved"]["value"])
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)

    def test_authorized_reader_sees_pending_tail_boundary_but_mutation_refuses(self):
        with (self.store / "events.jsonl").open("ab") as stream:
            stream.write(b'{"partial"')
        status, _headers, body = self.request("/v1/snapshot", role="reader")
        self.assertEqual(status, 200)
        snapshot = json.loads(body)
        self.assertIsNotNone(snapshot["pending_tail"])
        self.assertFalse(snapshot["mutation_admitted"])
        command = {
            "event_id": "blocked-tail", "base_revision": snapshot["revision"],
            "kind": "node.classified", "reason": {"summary": "blocked"},
            "payload": {"node_id": "work", "work_type": "change", "maturity": "prototype"},
        }
        status, _headers, body = self.request(
            "/v1/agent", role="coordinator", method="POST", body={"command": command},
        )
        self.assertEqual(status, 409)
        self.assertEqual(json.loads(body)["code"], "PENDING_TAIL")

    def test_nonlocal_bind_requires_explicit_switch(self):
        with self.assertRaisesRegex(Refusal, "nonlocal"):
            create_server(self.application, host="0.0.0.0")
        unlimited = create_server(self.application, max_body_bytes=0, max_follow_seconds=5)
        try:
            self.assertIsNone(unlimited.max_body_bytes)
            self.assertEqual(unlimited.max_follow_seconds, 5)
        finally:
            unlimited.server_close()

    def test_nested_worker_packets_and_semantic_responses_are_redacted(self):
        public = sanitize_public_value({
            "request": {"jobs": [{"packet": "private task body", "packet_sha256": "a" * 64}]},
            "response": {"provider": "raw body"}, "response_sha256": "b" * 64,
        })
        self.assertEqual(public["request"]["jobs"][0]["packet"], {"redacted": True, "sha256": "a" * 64})
        self.assertEqual(public["response"], {"redacted": True, "sha256": "b" * 64})
        self.assertNotIn("private task body", repr(public))
        self.assertNotIn("raw body", repr(public))
        decision = sanitize_public_value({
            "response": {
                "schema": "zap-coordinator-response/1", "request_id": "request-1",
                "request_kind": "review", "state_revision": 3, "request_sha256": "c" * 64,
                "disposition": "commands", "rationale": "Apply the verified correction",
                "selection": None, "commands": [{
                    "kind": "domain.work-accepted", "reason": "Evidence covers the work",
                    "payload": {"work_id": "work", "evidence_ids": ["E"], "private_prompt": "hide me"},
                }],
            },
            "response_sha256": "d" * 64,
        })["response"]
        self.assertEqual(decision["rationale"], "Apply the verified correction")
        self.assertEqual(decision["commands"][0]["affected_ids"]["work_id"], "work")
        self.assertEqual(decision["commands"][0]["provenance"]["evidence_ids"], ["E"])
        self.assertNotIn("hide me", repr(decision))

    def test_runtime_job_is_a_clickable_entity_with_collected_history(self):
        state = self.engine.load()[0]
        runtime = runtime_state(state)
        runtime["jobs"]["job-canvas"] = {
            "job_id": "job-canvas", "work_id": "work", "attempt_id": "attempt-canvas",
            "state": "unknown_effect", "descriptor_sha256": "c" * 64,
        }
        state.setdefault("extensions", {})["runtime"] = runtime
        events = [{
            "event_id": "job-canvas-event", "revision": 1,
            "kind": "runtime.job-status-observed", "payload": {"job_id": "job-canvas"},
        }]
        result = entity_detail(state, events, "job", "job-canvas")
        self.assertEqual(result["detail"]["entity"]["state"], "unknown_effect")
        self.assertEqual(result["detail"]["availability"]["provenance"], "event_history")
        self.assertEqual(result["detail"]["history"][0]["event_id"], "job-canvas-event")

    def test_tick_credential_must_match_the_configured_host_principal(self):
        class Coordinator:
            def tick(self):
                return {"ok": True, "tick": 1}

        host = self.principals["coordinator-principal"]
        application = BackendApplication(Engine(self.store, self.engine.trust, host), coordinator=Coordinator())
        with self.assertRaisesRegex(Refusal, "configured runtime host"):
            application.tick(self.principals["owner-principal"])
        self.assertEqual(application.tick(host), {"ok": True, "tick": 1})

    def test_reader_can_inspect_assessment_request_and_value_provenance(self):
        class Provider:
            def status(self, request_id=None):
                self.request_id = request_id
                return {"schema": "zap-runtime/assessment-status/1", "requests": [{
                    "request_id": "assessment-1", "basis_sha256": "a" * 64,
                    "action": {"action_id": "action-1", "payload_sha256": "b" * 64},
                    "policy": {"policy_id": "policy-1", "policy_revision": 1},
                    "source_bindings": [{"source_id": "S", "expected_sha256": "c" * 64}],
                    "transport": {"state": "succeeded", "receipt_sha256": "d" * 64},
                    "result": {"outcome": "observed", "values": {"stop": False}},
                }]}

        provider = Provider()
        coordinator = SimpleNamespace(config=SimpleNamespace(assessment_provider=provider))
        server = create_server(BackendApplication(self.engine, coordinator=coordinator))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            host, port = server.server_address
            request = Request(
                f"http://{host}:{port}/v1/assessments?id=assessment-1",
                headers={"X-ZAP-Credential-ID": self.ids["reader"],
                         "Authorization": f"Bearer {self.tokens['reader']}"},
            )
            with urlopen(request, timeout=3) as response:
                body = json.loads(response.read())
            self.assertEqual(provider.request_id, "assessment-1")
            self.assertEqual(body["requests"][0]["result"]["values"], {"stop": False})
            self.assertEqual(body["requests"][0]["source_bindings"][0]["source_id"], "S")
            self.assertNotIn("credential", json.dumps(body).casefold())
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)

    def test_reader_can_materialize_sparse_review_without_appending(self):
        fixture_root = self.root / "sparse-http"
        fixture_root.mkdir()
        fixture = RuntimeFixture(fixture_root)
        fixture._data("domain.outcome-proposed", {
            "schema": "zap-domain/outcome-proposed/1", "outcome_id": "OUTCOME-2", "revision": 2,
            "previous_outcome_id": "OUTCOME", "intent_id": "INTENT", "summary": "Refined verified tasks",
            "benefits": ["proof"], "guarantees": ["checked"], "tradeoffs": [], "obligations": [],
        }, "Propose HTTP sparse target")
        state = fixture.state()
        bootstrap = bootstrap_trust(fixture.store, state, fixture_root / "trust")
        trust, _principals = load_trust(bootstrap["trust_config"], state, store=fixture.store)
        server = create_server(BackendApplication(Engine(fixture.store, trust)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        request_body = {
            "schema": "zap-domain/sparse-review-transition/1", "intent_id": None,
            "outcome_id": "OUTCOME-2", "changed_dispositions": [], "ownership_changes": [],
            "work_changes": [], "preserved_evidence_ids": [], "preserved_stage_acceptance_ids": [],
            "preserved_work_acceptance_ids": [], "preserved_integration_acceptance_ids": [],
            "job_reconciliation": [], "tradeoffs": [], "preserved_benefits": ["proof"],
        }
        try:
            host, port = server.server_address
            request = Request(
                f"http://{host}:{port}/v1/review-transition/materialize",
                data=json.dumps({"request": request_body}).encode("utf-8"), method="POST",
                headers={
                    "Content-Type": "application/json", "X-ZAP-Credential-ID": "reader-credential",
                    "Authorization": f"Bearer {read_credential(bootstrap['credential_files']['reader'])}",
                },
            )
            with urlopen(request, timeout=3) as response:
                result = json.loads(response.read())
            active = {key for key, row in domain_state(state)["obligations"].items() if row["status"] == "active"}
            self.assertEqual({row["obligation_id"] for row in result["transition"]["obligation_dispositions"]}, active)
            self.assertEqual(fixture.state()["revision"], state["revision"])
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)

    def test_http_tail_snapshot_and_detail_preserve_public_selection_reason(self):
        fixture_root = self.root / "semantic-http"
        fixture_root.mkdir()
        fixture = RuntimeFixture(fixture_root)
        coordinator = AutomaticCoordinator(
            fixture.store, fixture.handlers, fixture.service, fixture.transport,
            fixture.semantic, fixture.config,
        )
        coordinator.tick()
        coordinator.tick()
        state = fixture.state()
        bootstrap = bootstrap_trust(fixture.store, state, fixture_root / "trust")
        trust, _principals = load_trust(bootstrap["trust_config"], state, store=fixture.store)
        server = create_server(BackendApplication(Engine(fixture.store, trust)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            host, port = server.server_address
            headers = {
                "X-ZAP-Credential-ID": "reader-credential",
                "Authorization": f"Bearer {read_credential(bootstrap['credential_files']['reader'])}",
            }
            bodies = {}
            for name, path in (("tail", "/v1/events?after=0"), ("snapshot", "/v1/snapshot"), ("detail", "/v1/entities/node/T")):
                with urlopen(Request(f"http://{host}:{port}{path}", headers=headers), timeout=3) as response:
                    bodies[name] = json.loads(response.read())
            semantic = next(row for row in bodies["tail"]["events"] if row["kind"] == "runtime.semantic-result-recorded")
            decision = semantic["payload"]["response"]
            self.assertEqual(decision["disposition"], "select")
            self.assertEqual(decision["selection"], {"work_id": "T"})
            self.assertEqual(decision["rationale"], "Fixture semantic decision")
            detail_text = json.dumps(bodies["detail"])
            self.assertIn("Fixture semantic decision", detail_text)
            self.assertIn("Atomically claim selected work T", detail_text)
            public = json.dumps(bodies)
            self.assertNotIn("zap-worker-packet/1", public)
            self.assertNotIn("private_prompt", public)
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)

    def test_source_content_uses_registered_captured_blob_not_arbitrary_live_path(self):
        allowed = self.root / "sources"
        allowed.mkdir()
        source = allowed / "note.txt"
        source.write_text("captured bytes", encoding="utf-8")
        captured = capture_source_blob(
            self.artifacts, source, allowed, source_id="source-http",
        )
        state = self.engine.load()[0]
        forged = {**captured["source"], "id": "forged-http"}
        forged_command = {
            "event_id": "forged-capture-http", "base_revision": state["revision"],
            "kind": "knowledge.source-recorded", "reason": {"summary": "forged capture"},
            "payload": {"source": forged},
        }
        status, _headers, body = self.request(
            "/v1/observation", role="coordinator", method="POST", body={"command": forged_command},
        )
        self.assertEqual(status, 400)
        self.assertEqual(json.loads(body)["code"], "OBSERVATION")
        self.assertNotIn("forged-http", self.engine.load()[0].get("extensions", {}).get("knowledge", {}).get("sources", {}))
        command = {
            "event_id": "capture-http", "base_revision": state["revision"],
            "kind": "knowledge.source-recorded", "reason": {"summary": "capture"},
            "payload": {"source": captured["source"]},
        }
        self.engine.service.submit_observation(
            command, credential_id=self.ids["coordinator"],
            credential=self.tokens["coordinator"],
        )
        source.write_text("changed live bytes", encoding="utf-8")
        digest = captured["source"]["content_sha256"]
        status, _headers, body = self.request(
            f"/v1/content/source/source-http?sha256={digest}", role="reader",
        )
        self.assertEqual(status, 200)
        content = json.loads(body)
        self.assertEqual(content["content"], "captured bytes")
        self.assertEqual(content["cursor"], content["revision"])
        self.assertEqual(content["base_sha256"], self.engine.load()[0]["base_sha256"])
        status, _headers, events = self.request("/v1/events?after=0", role="reader")
        self.assertEqual(status, 200)
        self.assertNotIn(str(allowed), events.decode("utf-8"))
        status, _headers, body = self.request(
            f"/v1/content/artifact/not-registered?sha256={digest}", role="reader",
        )
        self.assertEqual(status, 404)


if __name__ == "__main__":
    unittest.main()
