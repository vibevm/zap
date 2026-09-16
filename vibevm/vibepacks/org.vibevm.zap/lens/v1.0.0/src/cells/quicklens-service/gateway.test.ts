/** @verifies spec://org.vibevm.zap/lens/PROP-002#shells */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";
import {
  ExactDecimalSchema,
  QuicklensRefSchema,
  QuicklensSnapshotSchema,
  type QuicklensDataSource,
} from "../quicklens-model/index.ts";
import { createQuicklensGateway } from "./gateway.ts";

test("one-time pairing creates an HttpOnly scoped session for named operations", async (context) => {
  const token = randomBytes(32).toString("base64url");
  const opened = createQuicklensGateway({
    source: fixtureSource(),
    namespace: "alpha",
    pairingToken: token,
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: ["http://quicklens.test"],
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  context.after(() => opened.value.close());
  const started = await opened.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const base = `http://127.0.0.1:${String(started.value.port)}`;
  const api = `${base}${started.value.basePath}/v1`;
  const origin = "http://quicklens.test";

  const denied = await fetch(`${api}/read`, {
    method: "POST",
    headers: { Origin: origin, "Content-Type": "application/json" },
    body: "{}",
  });
  assert.equal(denied.status, 401);

  const paired = await fetch(`${api}/pair`, {
    method: "POST",
    headers: { Origin: origin, Authorization: `Bearer ${token}` },
  });
  assert.equal(paired.status, 200);
  const pairBody = await paired.text();
  assert.equal(pairBody.includes(token), false);
  const cookie = paired.headers.get("set-cookie");
  assert.ok(cookie);
  assert.match(cookie, /HttpOnly/i);
  assert.match(cookie, /SameSite=Strict/i);
  assert.match(cookie, /Path=\/quicklens\/alpha/i);
  assert.equal(cookie.includes(token), false);

  const repeated = await fetch(`${api}/pair`, {
    method: "POST",
    headers: { Origin: origin, Authorization: `Bearer ${token}` },
  });
  assert.equal(repeated.status, 401);

  const read = await fetch(`${api}/read`, {
    method: "POST",
    headers: { Origin: origin, Cookie: cookie, "Content-Type": "application/json" },
    body: "{}",
  });
  assert.equal(read.status, 200);
  assert.equal(
    read.headers.get("content-security-policy"),
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
  );
  assert.equal(read.headers.get("x-frame-options"), "DENY");
  const body = await read.text();
  assert.match(body, /"sourceMode":"live"/);
  assert.equal(body.includes(token), false);

  const foreign = await fetch(`${api}/read`, {
    method: "POST",
    headers: { Origin: "http://foreign.test", Cookie: cookie, "Content-Type": "application/json" },
    body: "{}",
  });
  assert.equal(foreign.status, 403);
});

test("two localhost gateways keep distinct scoped cookies and invalidation cursors", async (context) => {
  const first = invalidationFixture();
  const second = invalidationFixture();
  const gateways = await Promise.all(
    [
      { namespace: "first", token: randomBytes(32).toString("base64url"), fixture: first },
      { namespace: "second", token: randomBytes(32).toString("base64url"), fixture: second },
    ].map(async (entry) => {
      const opened = createQuicklensGateway({
        source: entry.fixture.source,
        namespace: entry.namespace,
        pairingToken: entry.token,
        allowedHosts: ["127.0.0.1"],
        allowedOrigins: ["http://quicklens.test"],
      });
      assert.equal(opened.ok, true);
      if (!opened.ok) return null;
      context.after(() => opened.value.close());
      const started = await opened.value.start({ host: "127.0.0.1", port: 0 });
      assert.equal(started.ok, true);
      if (!started.ok) return null;
      const api = `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}/v1`;
      const paired = await fetch(`${api}/pair`, {
        method: "POST",
        headers: { Origin: "http://quicklens.test", Authorization: `Bearer ${entry.token}` },
      });
      return { api, cookie: paired.headers.get("set-cookie") ?? "", fixture: entry.fixture };
    }),
  );
  const active = gateways.filter((value) => value !== null);
  assert.equal(active.length, 2);
  const cookie = active.map((value) => value.cookie.split(";", 1)[0]).join("; ");
  assert.notEqual(active[0]?.cookie.split("=", 1)[0], active[1]?.cookie.split("=", 1)[0]);
  for (const gateway of active) {
    const read = await fetch(`${gateway.api}/read`, {
      method: "POST",
      headers: {
        Origin: "http://quicklens.test",
        Cookie: cookie,
        "Content-Type": "application/json",
      },
      body: "{}",
    });
    assert.equal(read.status, 200);
  }
  first.emit("questions");
  const firstGateway = active[0];
  assert.ok(firstGateway);
  const events = await fetch(`${firstGateway.api}/invalidations`, {
    method: "POST",
    headers: {
      Origin: "http://quicklens.test",
      Cookie: cookie,
      "Content-Type": "application/json",
    },
    body: '{"after":0}',
  });
  assert.match(await events.text(), /"reason":"questions"/);
  const reset = await fetch(`${firstGateway.api}/invalidations`, {
    method: "POST",
    headers: {
      Origin: "http://quicklens.test",
      Cookie: cookie,
      "Content-Type": "application/json",
    },
    body: '{"after":999}',
  });
  assert.match(await reset.text(), /"reason":"reconnect"/);
});

function fixtureSource(): QuicklensDataSource {
  const basis = {
    storeRef: QuicklensRefSchema.parse("store.fixture"),
    baseRef: QuicklensRefSchema.parse("base.fixture"),
    revision: ExactDecimalSchema.parse("1"),
    sourceBasisRef: QuicklensRefSchema.parse(`source.${"a".repeat(64)}`),
  };
  const snapshot = QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Gateway fixture",
    phase: "ready",
    phaseDetail: null,
    capturedAt: "2026-09-15T00:00:00.000Z",
    revision: "1",
    objects: [],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: null,
  });
  const operation = {
    operationRef: QuicklensRefSchema.parse("operation.fixture"),
    previewRef: null,
    preview: null,
    state: "queued" as const,
    message: "Queued fixture",
    nextBasis: basis,
  };
  return {
    read: async () => ({ ok: true, value: snapshot }),
    answerQuestion: async (input) => ({
      ok: true,
      value: {
        ref: input.questionRef,
        addressedActorLabel: "Fixture",
        prompt: "Fixture?",
        state: "answered",
        revision: input.expectedRevision,
        answerMode: "free_text",
        choices: [],
        answer: input.answer,
        amendmentCount: ExactDecimalSchema.parse("0"),
      },
    }),
    proposePlanIntent: async () => ({ ok: true, value: operation }),
    previewPlan: async () => ({ ok: true, value: operation }),
    applyPlan: async () => ({ ok: true, value: operation }),
    reconcilePlan: async () => ({ ok: true, value: operation }),
    decidePlan: async () => ({ ok: true, value: operation }),
  };
}

function invalidationFixture() {
  let listener: ((reason: "events" | "questions" | "plan" | "reconnect") => void) | undefined;
  return {
    source: {
      ...fixtureSource(),
      subscribe: (next: typeof listener) => {
        listener = next;
        return () => {
          listener = undefined;
        };
      },
    } satisfies QuicklensDataSource,
    emit: (reason: "events" | "questions" | "plan" | "reconnect") => listener?.(reason),
  };
}
