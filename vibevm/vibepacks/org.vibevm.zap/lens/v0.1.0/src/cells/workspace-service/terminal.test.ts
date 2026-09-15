/** @scope spec://org.vibevm.zap/lens/PROP-006#network-and-control */
import assert from "node:assert/strict";
import test from "node:test";
import type { WorkspaceManagedTerminalPort } from "./types.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  AgentSessionIdSchema,
  ProjectIdSchema,
  TerminalIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
  RunIdSchema,
} from "../workspace-model/index.ts";
import { commandTerminal, readTerminal } from "./terminal.ts";

test("workspace terminal bridge maps bounded output and lease identities", async () => {
  const access = WorkspaceAccessContextSchema.parse({
    principalId: "principal.terminal",
    actorId: null,
    clientId: ClientIdSchema.parse("client.terminal"),
    authorizedProjectIds: [ProjectIdSchema.parse("project.terminal")],
  });
  const terminalId = TerminalIdSchema.parse("terminal.fixture");
  const service = fixtureService();
  const launched = await commandTerminal(service, access, {
    operation: "terminal.start.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.terminal.start"),
    projectId: ProjectIdSchema.parse("project.terminal"),
    contextId: WorkContextIdSchema.parse("context.terminal"),
    profileId: "profile.terminal",
    terminalId,
    sessionId: AgentSessionIdSchema.parse("session.terminal"),
    runId: RunIdSchema.parse("run.terminal"),
  });
  assert.ok(launched.ok && launched.value.operation === "terminal.start.v1");
  const page = readTerminal(service, access, {
    operation: "terminal.output.page.v1",
    projectId: ProjectIdSchema.parse("project.terminal"),
    contextId: WorkContextIdSchema.parse("context.terminal"),
    terminalId,
    afterSequence: DecimalSchema.parse("0"),
    limit: 10,
  });
  assert.ok(page.ok && page.value.operation === "terminal.output.page.v1");
  if (page.ok && page.value.operation === "terminal.output.page.v1") {
    assert.equal(page.value.page.events[0]?.sequence, "1");
  }
  const acquired = await commandTerminal(service, access, {
    operation: "terminal.acquire.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.terminal.acquire"),
    projectId: ProjectIdSchema.parse("project.terminal"),
    contextId: WorkContextIdSchema.parse("context.terminal"),
    terminalId,
    expectedControlEpoch: DecimalSchema.parse("1"),
    takeover: false,
  });
  assert.ok(acquired.ok && acquired.value.operation === "terminal.acquire.v1");
  if (acquired.ok && acquired.value.operation === "terminal.acquire.v1") {
    assert.equal(acquired.value.lease.controlEpoch, "1");
  }
  const oversized = readTerminal(service, access, {
    operation: "terminal.output.page.v1",
    projectId: ProjectIdSchema.parse("project.terminal"),
    contextId: WorkContextIdSchema.parse("context.terminal"),
    terminalId,
    afterSequence: DecimalSchema.parse("9007199254740992"),
    limit: 10,
  });
  assert.equal(oversized.ok, false);
});

function fixtureService(): WorkspaceManagedTerminalPort {
  const unsupported = () => ({
    ok: false as const,
    error: { code: "unsupported" as const, message: "fixture" },
  });
  return {
    start: () => Promise.resolve(unsupported()),
    startRegistered: (_access, request) =>
      Promise.resolve({
        ok: true,
        value: {
          terminalId: request.terminalId,
          projectId: request.projectId,
          contextId: request.contextId,
          sessionId: request.sessionId,
          runId: request.runId,
          processId: 104,
          state: "running",
          controlEpoch: 1,
          lease: null,
          nextSequence: 1,
          output: [],
        },
      }),
    snapshot: unsupported,
    list: () => ({ ok: true, value: [] }),
    read: (_access, request) => ({
      ok: true,
      value: {
        terminalId: request.terminalId,
        events: [
          {
            terminalId: request.terminalId,
            projectId: request.projectId,
            contextId: request.contextId,
            controlEpoch: 1,
            sequence: 1,
            data: "ready",
            occurredAt: "2026-09-15T00:00:00.000Z",
          },
        ],
        afterSequence: request.afterSequence,
        nextSequence: 1,
        gap: null,
      },
    }),
    acquire: (_access, request) => ({
      ok: true,
      value: {
        terminalId: request.terminalId,
        controlEpoch: 1,
        lease: {
          terminalId: request.terminalId,
          leaseId: "lease.fixture",
          clientId: "client.terminal",
          controlEpoch: 1,
          acquiredAt: "2026-09-15T00:00:00.000Z",
        },
      },
    }),
    release: unsupported,
    input: unsupported,
    resize: unsupported,
    interrupt: unsupported,
    stop: unsupported,
    stopProject: () => Promise.resolve({ ok: true, value: [] }),
    close: () => undefined,
  };
}
