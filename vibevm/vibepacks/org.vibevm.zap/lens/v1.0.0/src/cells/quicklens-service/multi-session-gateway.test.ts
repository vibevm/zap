import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";

import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import { createQuicklensDemoDataSource } from "../quicklens-demo/index.ts";
import { createQuicklensGateway } from "./gateway.ts";

test("multi-session Wayfinder gateway issues single-use tickets for two clients and reconnect", async (context) => {
  const opened = createQuicklensGateway({
    source: createQuicklensDemoDataSource(),
    namespace: "multiway",
    pairingToken: randomBytes(32).toString("base64url"),
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: ["http://quicklens.test"],
    multiSession: true,
    workspaceSource: () => createWorkspaceDemoPort(),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  context.after(() => opened.value.close());
  const started = await opened.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const firstTicket = opened.value.issuePairingTicket?.();
  const secondTicket = opened.value.issuePairingTicket?.();
  assert.equal(firstTicket?.ok, true);
  assert.equal(secondTicket?.ok, true);
  if (!firstTicket?.ok || !secondTicket?.ok) return;
  const baseUrl = `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`;
  const first = createWorkspaceHttpClient({
    baseUrl,
    pairingToken: firstTicket.value.ticket,
    origin: "http://quicklens.test",
  });
  const second = createWorkspaceHttpClient({
    baseUrl,
    pairingToken: secondTicket.value.ticket,
    origin: "http://quicklens.test",
  });
  assert.notEqual(first, null);
  assert.notEqual(second, null);
  if (first === null || second === null) return;
  const firstRead = await first.read({ operation: "project.list.v1" });
  const secondRead = await second.read({ operation: "project.list.v1" });
  assert.equal(firstRead.ok, true);
  assert.equal(secondRead.ok, true);
  const reconnectTicket = opened.value.issuePairingTicket?.();
  assert.equal(reconnectTicket?.ok, true);
  if (!reconnectTicket?.ok) return;
  const reconnect = createWorkspaceHttpClient({
    baseUrl,
    pairingToken: reconnectTicket.value.ticket,
    origin: "http://quicklens.test",
  });
  assert.notEqual(reconnect, null);
  if (reconnect !== null)
    assert.equal((await reconnect.read({ operation: "project.list.v1" })).ok, true);
  const reused = createWorkspaceHttpClient({
    baseUrl,
    pairingToken: firstTicket.value.ticket,
    origin: "http://quicklens.test",
  });
  assert.notEqual(reused, null);
  if (reused !== null)
    assert.equal((await reused.read({ operation: "project.list.v1" })).ok, false);
});
