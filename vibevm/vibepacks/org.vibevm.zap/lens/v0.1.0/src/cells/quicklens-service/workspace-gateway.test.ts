/** Authenticated WorkspaceClientPort transport proof. */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";

import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import {
  HistoryCursorSchema,
  HistoryPageSchema,
  WorkspaceCommandRequestSchema,
  WorkspaceReadResponseSchema,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import { createQuicklensDemoDataSource } from "../quicklens-demo/index.ts";
import { createQuicklensGateway } from "./gateway.ts";

test("two paired workspace clients stay project-scoped and receive events", async (context) => {
  let assignment = 0;
  const source = createQuicklensDemoDataSource();
  const workspaceSource = () => {
    const projectId = assignment++ === 0 ? "project.lens" : "project.zap";
    return scopedWorkspacePort(projectId);
  };
  const entries = ["alpha", "beta"].map((namespace) => {
    const token = randomBytes(32).toString("base64url");
    const opened = createQuicklensGateway({
      source,
      namespace,
      pairingToken: token,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
      workspaceSource,
    });
    assert.equal(opened.ok, true);
    if (!opened.ok) assert.fail();
    return { gateway: opened.value, token };
  });
  context.after(async () => {
    await Promise.all(entries.map((entry) => entry.gateway.close()));
  });
  const clients: WorkspaceClientPort[] = [];
  for (const entry of entries) {
    const started = await entry.gateway.start({ host: "127.0.0.1", port: 0 });
    assert.equal(started.ok, true);
    if (!started.ok) assert.fail();
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: entry.token,
      origin: "http://quicklens.test",
    });
    assert.notEqual(client, null);
    if (client === null) assert.fail();
    clients.push(client);
  }

  const firstClient = clients[0];
  const secondClient = clients[1];
  assert.notEqual(firstClient, undefined);
  assert.notEqual(secondClient, undefined);
  if (firstClient === undefined || secondClient === undefined) return;
  const first = await firstClient.read({ operation: "project.list.v1" });
  const second = await secondClient.read({ operation: "project.list.v1" });
  assert.equal(first.ok, true);
  assert.equal(second.ok, true);
  if (!first.ok || !second.ok) return;
  const firstResponse = WorkspaceReadResponseSchema.parse(first.value);
  const secondResponse = WorkspaceReadResponseSchema.parse(second.value);
  assert.equal(firstResponse.operation, "project.list.v1");
  assert.equal(secondResponse.operation, "project.list.v1");
  if (
    firstResponse.operation !== "project.list.v1" ||
    secondResponse.operation !== "project.list.v1"
  )
    return;
  assert.deepEqual(
    firstResponse.projects.map((project) => project.projectId),
    ["project.lens"],
  );
  assert.deepEqual(
    secondResponse.projects.map((project) => project.projectId),
    ["project.zap"],
  );

  const command = await firstClient.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "session.start.v1",
      clientRequestId: "request.workspace.start.fixture",
      projectId: "project.lens",
      contextId: "context.lens.main",
      interactionKind: "structured",
      profileId: "profile.codex.native",
    }),
  );
  assert.equal(command.ok, true);

  const controller = new AbortController();
  const stream = firstClient.subscribe({
    cursor: HistoryCursorSchema.parse({
      scope: { kind: "all_authorized" },
      afterGlobalSequence: DecimalSchema.parse("0"),
    }),
    signal: controller.signal,
  });
  const firstEvent = await stream[Symbol.asyncIterator]().next();
  controller.abort();
  assert.equal(firstEvent.done, false);
  if (!firstEvent.done) assert.equal(firstEvent.value.ok, true);
});

function scopedWorkspacePort(projectId: "project.lens" | "project.zap"): WorkspaceClientPort {
  const base = createWorkspaceDemoPort();
  return {
    read: async (request) => {
      const result = await base.read(request);
      if (!result.ok) return result;
      const response = WorkspaceReadResponseSchema.parse(result.value);
      if (response.operation !== "project.list.v1") return result;
      const filtered = WorkspaceReadResponseSchema.parse({
        operation: "project.list.v1",
        projects: response.projects.filter((project) => project.projectId === projectId),
      });
      return { ok: true as const, value: filtered };
    },
    command: (request) => base.command(request),
    events: async (request) => {
      const result = await base.events(request);
      if (!result.ok) return result;
      return {
        ok: true,
        value: HistoryPageSchema.parse({
          ...result.value,
          events: result.value.events.filter((event) => event.projectId === projectId),
        }),
      };
    },
    subscribe: (request) => base.subscribe(request),
  };
}
