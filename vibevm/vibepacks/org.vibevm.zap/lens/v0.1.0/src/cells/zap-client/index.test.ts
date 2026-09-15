/** @verifies spec://org.vibevm.zap/lens/PROP-002#plan-control */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { z } from "zod";
import {
  byteArray,
  encodeCanonicalJson,
  parseCanonicalJson,
  parseWireJson,
  wireU32,
  wireU64,
} from "./codec.ts";
import { diagnostic } from "./diagnostics.ts";
import { canonicalQueryInput, createZapClient, protectedCommandDigest } from "./client.ts";
import {
  EventCursorSchema,
  ProtectedSubmissionSchema,
  PreparedCompositeSuccessorWireSchema,
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
} from "./schemas.ts";
import type {
  EventCursor,
  AdvanceChangeAdmissionRequest,
  PrepareBundleRequest,
  PrepareComparisonRequest,
  PrepareProjectedRecordRequest,
  ProtectedSubmission,
  ZapExchangeRequest,
  ZapExchangeResponse,
  ZapHttpExchange,
} from "./types.ts";
const utf8 = new TextEncoder();
const digest = ZapDigestSchema.parse("a".repeat(64));
const store =
  '{"store_id":"store.fixture","campaign_id":"campaign.fixture","base_id":"base.fixture","store_epoch":"zap/2","codec_epoch":2,"reducer_epoch":1}';
const MAX_U64 = "18446744073709551615";
// Rust-serde response fixtures transcribed from the named zap_api public shapes.
// They are deterministic protocol fixtures, not claims of a live backend run.
const ACTIVE_ITEM =
  '{"active_outcome":{"outcome_id":"outcome.fixture","record_revision":18446744073709551615,"state":"present"},"adopted_milestone_plan":{"state":"absent"},"current_strategy":{"record_revision":9007199254740993,"state":"present","strategic_revision_id":"strategy.fixture"},"snapshot":{"base_id":"base.fixture","campaign_id":"campaign.fixture","revision":18446744073709551615,"store_id":"store.fixture"}}';
test("protected command digest matches the captured Rust arbitrary-precision frame preimage", () => {
  assert.equal(
    protectedCommandDigest(rustCommand("rust-plan-authoring-command.json")),
    "7eb139e4a0b08f78010c595fc7efada0564891677e30eed53b02247bda3b9524",
  );
  assert.equal(
    protectedCommandDigest(rustCommand("rust-large-header-integer.json")),
    "e75c87276625441d5c17afc6d2b3e6a49cd06c353cc6bdc0179507660ea2591e",
  );
  assert.equal(
    protectedCommandDigest(rustCommand("rust-payload-large-integer.json")),
    "78196573ae9e3bebe833e61a9537a11b7d3545d83a5f7dc1425de92348681e0b",
  );
});
test("Rust-refused float and negative-zero command payload fixtures are not digestible", () => {
  assert.throws(() => protectedCommandDigest(rustCommand("rust-payload-float-refused.json")));
  assert.throws(() =>
    protectedCommandDigest(rustCommand("rust-payload-negative-zero-refused.json")),
  );
});
class ScriptedExchange implements ZapHttpExchange {
  readonly requests: ZapExchangeRequest[] = [];
  readonly #steps: (ZapExchangeResponse | Error)[];
  constructor(...steps: (ZapExchangeResponse | Error)[]) {
    this.#steps = steps;
  }
  async request(input: ZapExchangeRequest): Promise<ZapExchangeResponse> {
    this.requests.push(input);
    const step = this.#steps.shift();
    if (step === undefined) throw diagnostic("unexpected exchange request in fixture");
    if (step instanceof Error) throw step;
    return step;
  }
}
test("query emits exact codec-2 bytes and preserves integers beyond 2^53", async () => {
  const item = '{"count":9007199254740993}';
  const exchange = new ScriptedExchange(queryPage(item, "9007199254740993"));
  const fixture = client(exchange);
  const result = await fixture.query(
    ZapIdSchema.parse("zap.fixture.query"),
    { z: 9_007_199_254_740_993n, a: "first" },
    z.object({ count: U64WireSchema }).strict(),
  );
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.revision, "9007199254740993");
  assert.equal(result.value.items[0]?.count, "9007199254740993");
  const sent = exchange.requests[0];
  assert.ok(sent);
  assert.equal(sent.method, "POST");
  assert.equal(sent.url.href, "http://127.0.0.1:42001/v1/query");
  assert.equal(sent.headers["X-ZAP-Credential-ID"], fixtureCredential.id);
  assert.equal(sent.headers["Authorization"], `Bearer ${fixtureCredential.bearer}`);
  assert.equal(sent.headers["Content-Type"], "application/json");
  const canonical = canonicalQueryInput({ z: 9_007_199_254_740_993n, a: "first" });
  const expected = utf8.encode(
    `{"input":{"canonical_json":${JSON.stringify(canonical.canonical_json)},"codec":2},"kind":"query","query_id":"zap.fixture.query"}`,
  );
  assert.deepEqual(sent.body, expected);
});
test("canonical codec follows Rust lexical keys and supported numeric forms", () => {
  assert.equal(
    new TextDecoder().decode(encodeCanonicalJson({ "2": "two", "10": "ten" })),
    '{"10":"ten","2":"two"}',
  );
  assert.equal(
    new TextDecoder().decode(
      encodeCanonicalJson({ "\u{10000}": "supplementary", "\ue000": "bmp" }),
    ),
    '{"":"bmp","𐀀":"supplementary"}',
  );
  assert.equal(
    new TextDecoder().decode(encodeCanonicalJson({ float: 1.5, negative_zero: -0 })),
    '{"float":1.5,"negative_zero":-0.0}',
  );
  assert.doesNotThrow(() => parseCanonicalJson(utf8.encode('{"10":"ten","2":"two"}')));
  assert.throws(() => parseCanonicalJson(utf8.encode('{"2":"two","10":"ten"}')));
  assert.throws(() => encodeCanonicalJson({ overflow: 18_446_744_073_709_551_616n }));
  assert.throws(() => encodeCanonicalJson({ unsafe: 9_007_199_254_740_992 }));
  const sparse = new Array<string>(2);
  sparse[1] = "hole";
  assert.throws(() => encodeCanonicalJson(sparse));
  assert.throws(() => encodeCanonicalJson("\ud800"));
});
test("active context enforces a single coherent page identity", async () => {
  const staleItem = ACTIVE_ITEM.replace(
    '"revision":18446744073709551615',
    '"revision":18446744073709551614',
  );
  const exchange = new ScriptedExchange(
    queryPage(ACTIVE_ITEM, MAX_U64),
    queryPage(staleItem, MAX_U64),
  );
  const fixture = client(exchange);
  const current = await fixture.activeContext();
  assert.equal(current.ok, true);
  if (current.ok) {
    assert.equal(current.value.items[0]?.snapshot.revision, MAX_U64);
    assert.equal(
      current.value.items[0]?.current_strategy.state === "present"
        ? current.value.items[0].current_strategy.record_revision
        : undefined,
      "9007199254740993",
    );
  }
  const stale = await fixture.activeContext();
  assert.equal(stale.ok, false);
  if (!stale.ok) assert.equal(stale.error.kind, "stale_context");
});
test("capabilities, snapshot and preparation routes decode their Rust response kinds", async () => {
  const bundleValue = `{"store":${store},"observed_revision":9007199254740993,"request":{},"preflight":{},"affected_scopes":[]}`;
  const exchange = new ScriptedExchange(
    response(
      200,
      '{"kind":"capabilities","value":{"schema":"zap-machine-capabilities/1","read_operations":["query"],"command_operations":["command"],"query_ids":["zap.planning.active-context.v1"],"unavailable_operations":[],"max_page_items":4096,"local_bind_default":true}}',
    ),
    response(
      200,
      `{"kind":"snapshot","value":{"store":${store},"revision":${MAX_U64},"head_event_digest":"${digest}","event_count":9007199254740993,"record_count":2,"index_count":3,"projection_digest":"${digest}","physical_schema_version":2,"physical_schema":"v2","physical_projection_algorithm":"v2_tables","physical_projection_digest":"${digest}","logical_row_digest":"${digest}","derived_index_catalog":{"version":2,"query_epoch":1,"covered_revision":${MAX_U64},"families":["zap.planning.current-strategy.v1"],"algorithms":[{"family":"zap.planning.current-strategy.v1","fingerprint":"${digest}"}]}}}`,
    ),
    response(200, `{"kind":"prepared_effect_bundle","value":${bundleValue}}`),
    response(
      200,
      `{"kind":"prepared_effect_comparison","value":{"store":${store},"observed_revision":9007199254740993,"alternatives":[],"basis_request":{},"relevant_basis":"${digest}","affected_scopes":[]}}`,
    ),
    response(
      200,
      `{"kind":"projected_record","value":{"store":${store},"observed_revision":9007199254740993,"family":"zap.planning.strategy","key":[34,120,34],"canonical_value":null,"preparation":${bundleValue}}}`,
    ),
  );
  const fixture = client(exchange);
  const capabilities = await fixture.capabilities();
  assert.equal(capabilities.ok && capabilities.value.max_page_items, 4096);
  const snapshot = await fixture.snapshot();
  assert.equal(snapshot.ok && snapshot.value.revision, MAX_U64);
  assert.equal(snapshot.ok && snapshot.value.event_count, "9007199254740993");
  const bundle = prepareBundleRequest();
  assert.equal((await fixture.prepareBundle(bundle)).ok, true);
  const comparison: PrepareComparisonRequest = {
    at: bundle.at,
    actor: null,
    draft: {
      assessment_id: ZapIdSchema.parse("assessment.fixture"),
      alternatives: [bundle.draft],
      policy: "required",
      capacity: "not_applicable",
      closure: "known_graph",
    },
  };
  assert.equal((await fixture.prepareComparison(comparison)).ok, true);
  const projected: PrepareProjectedRecordRequest = {
    ...bundle,
    record: { family: ZapIdSchema.parse("zap.planning.strategy"), key: [34, 120, 34] },
  };
  assert.equal((await fixture.prepareProjectedRecord(projected)).ok, true);
  assert.deepEqual(
    exchange.requests.slice(2).map((request) => request.url.pathname),
    ["/v1/prepare/bundle", "/v1/prepare/comparison", "/v1/prepare/projected-record"],
  );
  const preparedBody = exchange.requests[2]?.body;
  assert.ok(preparedBody);
  assert.match(new TextDecoder().decode(preparedBody), /"revision":9007199254740993/);
});
test("malformed, foreign, duplicate and unsafe numeric inputs refuse", async () => {
  const exchange = new ScriptedExchange(
    response(200, '{"kind":"snapshot","value":{}}'),
    response(200, '{"kind":"query","kind":"query","value":{}}'),
    response(200, "{invalid"),
  );
  const fixture = client(exchange);
  const unsafe = await fixture.query(
    ZapIdSchema.parse("zap.fixture.query"),
    { unsafe: 9_007_199_254_740_992 },
    z.unknown(),
  );
  assert.equal(unsafe.ok, false);
  assert.equal(exchange.requests.length, 0);
  for (const expected of ["foreign_response", "malformed_response", "malformed_response"]) {
    const result = await fixture.query(ZapIdSchema.parse("zap.fixture.query"), {}, z.unknown());
    assert.equal(result.ok, false);
    if (!result.ok) assert.equal(result.error.kind, expected);
  }
});

test("typed HTTP refusal preserves exact structured error detail", async () => {
  const exchange = new ScriptedExchange(
    response(
      409,
      '{"code":"stale_revision","requirement":"spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY","message":"violates REQ spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY: stale fixture; fix surface: command","fix":"command","detail":{"kind":"stale_revision","expected":9007199254740993,"actual":9007199254740994}}',
    ),
  );
  const result = await client(exchange).snapshot();
  assert.equal(result.ok, false);
  if (!result.ok && result.error.kind === "http_refusal") {
    assert.equal(result.error.refusal.detail.kind, "stale_revision");
    if (result.error.refusal.detail.kind === "stale_revision") {
      assert.equal(result.error.refusal.detail.expected, "9007199254740993");
      assert.equal(result.error.refusal.detail.actual, "9007199254740994");
    }
  } else {
    assert.fail("expected typed HTTP refusal");
  }
});

test("event and SSE continuations reject foreign snapshot context", async () => {
  const requested = cursor("store.fixture");
  const foreignStore = store.replace("store.fixture", "store.foreign");
  const page = `{"kind":"events","value":{"store":${foreignStore},"revision":7,"events":[],"resume":{"store":${foreignStore},"revision":7,"next_sequence":8},"next":null}}`;
  const sse = `event: cursor\ndata: {"store":${foreignStore},"revision":7,"next_sequence":8}\n\n`;
  const exchange = new ScriptedExchange(
    response(200, page),
    response(200, sse, "text/event-stream"),
  );
  const fixture = client(exchange);
  const events = await fixture.events(requested, 10);
  assert.equal(events.ok, false);
  if (!events.ok) assert.equal(events.error.kind, "stale_context");
  const streamed = await fixture.streamEvents(requested, 10);
  assert.equal(streamed.ok, false);
  if (!streamed.ok) assert.equal(streamed.error.kind, "stale_context");
});

test("aborted and malformed mutation outcomes are uncertain and never retried", async () => {
  const exchange = new ScriptedExchange(
    diagnostic("synthetic aborted exchange"),
    response(200, '{"kind":"snapshot","value":{}}'),
  );
  const fixture = client(exchange);
  const submission = protectedSubmission();
  for (const expectedCalls of [1, 2]) {
    const result = await fixture.submit("command", submission);
    assert.equal(result.ok, false);
    if (!result.ok) {
      assert.equal(result.error.kind, "uncertain_submission");
      if (result.error.kind === "uncertain_submission") {
        assert.equal(result.error.command_id, submission.reconciliation.command_id);
        assert.equal(result.error.reconcile_required, true);
      }
    }
    assert.equal(exchange.requests.length, expectedCalls);
  }
});

test("protected submit rejects a reconciliation digest that does not match its frame", async () => {
  const exchange = new ScriptedExchange();
  const fixture = client(exchange);
  const original = protectedSubmission();
  const changed: ProtectedSubmission = {
    ...original,
    reconciliation: {
      ...original.reconciliation,
      command_digest: ZapDigestSchema.parse("b".repeat(64)),
    },
  };
  const result = await fixture.submit("control", changed);
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.error.kind, "configuration");
  assert.equal(exchange.requests.length, 0);
});

test("authoritative unknown mutation returns once and reconciles by exact identity", async () => {
  const submission = protectedSubmission();
  const unknown = `{"kind":"command","value":{"status":"unknown","command_id":"${submission.reconciliation.command_id}","command_digest":"${submission.reconciliation.command_digest}"}}`;
  const notCommitted = `{"kind":"command","value":{"status":"not_committed","command_id":"${submission.reconciliation.command_id}"}}`;
  const exchange = new ScriptedExchange(response(200, unknown), response(200, notCommitted));
  const fixture = client(exchange);
  const result = await fixture.submit("agent", submission);
  assert.equal(result.ok && result.value.status, "unknown");
  assert.equal(exchange.requests.length, 1);
  const reconciled = await fixture.reconcile(submission.reconciliation);
  assert.equal(reconciled.ok && reconciled.value.status, "not_committed");
  assert.equal(exchange.requests.length, 2);
  assert.equal(exchange.requests[1]?.url.pathname, "/v1/reconcile");
});

test("all four protected channels use their exact backend routes", async () => {
  const submission = protectedSubmission();
  const unknown = `{"kind":"command","value":{"status":"unknown","command_id":"${submission.reconciliation.command_id}","command_digest":"${submission.reconciliation.command_digest}"}}`;
  const exchange = new ScriptedExchange(...Array.from({ length: 4 }, () => response(200, unknown)));
  const fixture = client(exchange);
  for (const channel of ["command", "control", "observation", "agent"] as const) {
    const result = await fixture.submit(channel, submission);
    assert.equal(result.ok && result.value.status, "unknown");
  }
  assert.deepEqual(
    exchange.requests.map((request) => request.url.pathname),
    ["/v1/command", "/v1/control", "/v1/observation", "/v1/agent"],
  );
});

test("change admission sends corrected exact wire and transport loss is retry-exact uncertain", async () => {
  const request = changeAdmissionRequest();
  const responseBody = `{"kind":"change_admission","value":{"status":"owner_decision_required","operation_id":"${request.operation_id}","assessment_id":"${request.assessment_id}","alternative_id":"${request.alternative_id}","observed_revision":8,"hold_id":"hold.fixture","assessment_digest":"${request.assessment_digest}","adjudication":{},"decision":{"assessment_digest":"${request.assessment_digest}","forecast_id":null,"forecast_digest":null,"policy_id":"policy.fixture","policy_revision":1,"recommended_alternative_id":"${request.alternative_id}","effect_fingerprints":["${digest}"],"effect_preflight_digests":["${digest}"],"decision_revision":1}}}`;
  const exchange = new ScriptedExchange(response(200, responseBody), diagnostic("lost response"));
  const fixture = client(exchange);
  const held = await fixture.advanceChangeAdmission(request);
  assert.equal(held.ok && held.value.status, "owner_decision_required");
  const body = new TextDecoder().decode(exchange.requests[0]?.body);
  assert.match(body, /"kind":"advance_change_admission"/);
  assert.match(body, /"source_assessment_digest"/);
  const uncertainResult = await fixture.advanceChangeAdmission(request);
  assert.equal(uncertainResult.ok, false);
  if (!uncertainResult.ok) {
    assert.equal(uncertainResult.error.kind, "uncertain_operation");
  }
  assert.equal(exchange.requests.length, 2);
});

test("composite successor preserves canonical replay bytes across its protected record route", async () => {
  const draft = prepareBundleRequest().draft;
  const plan = canonicalQueryInput({});
  const identity = {
    store_id: ZapIdSchema.parse("store.fixture"),
    campaign_id: ZapIdSchema.parse("campaign.fixture"),
    base_id: ZapIdSchema.parse("base.fixture"),
    store_epoch: "zap/2",
    codec_epoch: 2,
    reducer_epoch: 1,
  };
  const reconciliation = {
    command_id: ZapIdSchema.parse("command.composite"),
    command_digest: digest,
  };
  const prepared = {
    request: {
      operation_id: ZapIdSchema.parse("operation.composite"),
      store: identity,
      expected_revision: 7,
      precursors: draft,
      plan_intent: plan,
    },
    request_digest: digest,
    plan,
    precursor_preparation: {
      store: identity,
      observed_revision: 7,
      request: {},
      preflight: {},
      affected_scopes: [],
    },
    reconciliation,
  };
  const wireCheck = PreparedCompositeSuccessorWireSchema.safeParse(
    parseWireJson(encodeCanonicalJson(prepared)),
  );
  assert.equal(wireCheck.success, true, wireCheck.success ? "" : wireCheck.error.message);
  const exchange = new ScriptedExchange(
    response(200, JSON.stringify({ kind: "prepared_composite_successor", value: prepared })),
    response(
      200,
      JSON.stringify({
        kind: "recorded_composite_successor",
        value: {
          prepared,
          submission: { status: "not_committed", command_id: reconciliation.command_id },
        },
      }),
    ),
  );
  const fixture = client(exchange);
  const preparedResult = await fixture.prepareCompositeSuccessor({
    ...prepared.request,
    store: identity,
    expected_revision: 7n,
  });
  assert.equal(
    preparedResult.ok,
    true,
    preparedResult.ok ? "" : JSON.stringify(preparedResult.error),
  );
  if (!preparedResult.ok) return;
  const recorded = await fixture.recordCompositeSuccessor(preparedResult.value);
  assert.equal(recorded.ok && recorded.value.submission.status, "not_committed");
  assert.deepEqual(
    exchange.requests.map((request) => request.url.pathname),
    ["/v1/prepare/composite-successor", "/v1/composite-successor"],
  );
  const body = exchange.requests[1]?.body;
  assert.ok(body);
  const text = new TextDecoder().decode(body);
  assert.match(text, /"expected_revision":7/);
  assert.doesNotMatch(text, /"replay"/);
});

function client(exchange: ZapHttpExchange) {
  const created = createZapClient({
    endpoint: new URL("http://127.0.0.1:42001/"),
    credential: fixtureCredential,
    exchange,
  });
  if (!created.ok) throw diagnostic("fixture client configuration failed");
  return created.value;
}

const fixtureCredential = {
  id: ZapIdSchema.parse("credential.fixture.reader"),
  bearer: randomBytes(32).toString("base64url"),
};

function response(status: number, body: string, contentType = "application/json") {
  return { status, headers: { "content-type": contentType }, body: utf8.encode(body) };
}

function queryPage(item: string, revision: string) {
  const bytes = [...utf8.encode(item)];
  return response(
    200,
    `{"kind":"query","value":{"store":${store},"revision":${revision},"query_epoch":1,"items":[${JSON.stringify(bytes)}],"completeness":"complete"}}`,
  );
}

function cursor(storeId: string): EventCursor {
  const raw = `{"store":${store.replace("store.fixture", storeId)},"revision":7,"next_sequence":4}`;
  return EventCursorSchema.parse(parseWireJson(utf8.encode(raw)));
}

function protectedSubmission(): ProtectedSubmission {
  const commandId = ZapIdSchema.parse("command.fixture.plan");
  const command: ProtectedSubmission["command"] = {
    frame: {
      header: {
        protocol: 1,
        store_id: ZapIdSchema.parse("store.fixture"),
        campaign_id: ZapIdSchema.parse("campaign.fixture"),
        base_id: ZapIdSchema.parse("base.fixture"),
        command_id: commandId,
        event_id: ZapIdSchema.parse("event.fixture.plan"),
        expected_revision: 7n,
        kind: ZapIdSchema.parse("milestone.plan-adopted"),
        causes: [],
        basis: { kind: "exact", digest },
      },
      reason: { summary: "Apply prepared plan", evidence: [], decision: null, change: null },
      payload: canonicalQueryInput({ schema: "milestone-plan-adopted/1" }),
    },
  };
  return {
    command,
    reconciliation: {
      command_id: commandId,
      command_digest: ZapDigestSchema.parse(protectedCommandDigest(command)),
    },
  };
}

function prepareBundleRequest(): PrepareBundleRequest {
  return {
    at: { kind: "revision", revision: 9_007_199_254_740_993n },
    actor: null,
    draft: {
      alternative_id: ZapIdSchema.parse("alternative.fixture"),
      committed_prefix: [],
      effects: [],
      no_op_basis: { purpose: "completion", roots: [] },
    },
  };
}

function changeAdmissionRequest(): AdvanceChangeAdmissionRequest {
  const alternative = ZapIdSchema.parse("alternative.fixture");
  const product = protectedSubmission().command;
  return {
    operation_id: ZapIdSchema.parse("operation.fixture"),
    store: {
      store_id: ZapIdSchema.parse("store.fixture"),
      campaign_id: ZapIdSchema.parse("campaign.fixture"),
      base_id: ZapIdSchema.parse("base.fixture"),
      store_epoch: "zap/2",
      codec_epoch: 2,
      reducer_epoch: 1,
    },
    expected_revision: 7n,
    action: ZapIdSchema.parse("plan.lower"),
    assessment_id: ZapIdSchema.parse("assessment.fixture"),
    alternative_id: alternative,
    source_assessment_digest: digest,
    assessment_digest: digest,
    relevant_basis: digest,
    comparison: {
      at: { kind: "current" },
      actor: null,
      draft: {
        assessment_id: ZapIdSchema.parse("assessment.fixture"),
        alternatives: [
          {
            alternative_id: alternative,
            committed_prefix: [],
            effects: [
              {
                effect_id: ZapIdSchema.parse("effect.fixture"),
                index: 0,
                kind: ZapIdSchema.parse("milestone.plan-adopted"),
                payload: product.frame.payload,
                predecessors: [],
                product_event_id: product.frame.header.event_id,
              },
            ],
            no_op_basis: null,
          },
        ],
        policy: "required",
        capacity: "not_applicable",
        closure: "known_graph",
      },
    },
    product,
    decision_id: null,
    exception_id: null,
  };
}

function rustCommand(name: string) {
  const raw = parseWireJson(readFileSync(new URL(`./fixtures/${name}`, import.meta.url)));
  const root = z
    .looseObject({
      prepared: z.looseObject({ command: z.unknown() }).optional(),
      frame: z.unknown().optional(),
    })
    .parse(raw);
  const candidate = root.prepared?.command ?? root;
  const command = z
    .looseObject({
      frame: z.looseObject({
        header: z.looseObject({ protocol: z.unknown(), expected_revision: z.unknown() }),
        payload: z.looseObject({ codec: z.unknown(), canonical_json: z.unknown() }),
        reason: z.unknown(),
      }),
    })
    .parse(candidate);
  const frame = command.frame;
  return ProtectedSubmissionSchema.shape.command.parse({
    frame: {
      header: {
        ...frame.header,
        protocol: wireU32(frame.header.protocol),
        expected_revision: BigInt(wireU64(frame.header.expected_revision)),
      },
      payload: {
        codec: wireU32(frame.payload.codec),
        canonical_json: [...byteArray(frame.payload.canonical_json)],
      },
      reason: frame.reason,
    },
  });
}
