/** Electron IPC clone boundary proof. @scope spec://org.vibevm.zap/lens/PROP-005#shared-code */
import assert from "node:assert/strict";
import test from "node:test";
import { DecimalSchema } from "../../cells/protocol/index.ts";
import { createWorkspaceIpcClient, type WorkspaceIpcBridge } from "./workspace-ipc.ts";

test("renderer subscription keeps AbortSignal and async iteration outside contextBridge", async () => {
  const event = {
    historyEventId: "history.ipc",
    projectId: "project.ipc",
    contextId: "context.ipc",
    globalSequence: "1",
    projectSequence: "1",
    sourceSequence: "1",
    kind: "project.registered",
    source: "lens",
    actorId: null,
    occurrenceAt: "2026-09-15T00:00:00.000Z",
    ingestedAt: "2026-09-15T00:00:00.000Z",
    sourceEventId: "source.ipc",
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  };
  const requests: unknown[] = [];
  const bridge: WorkspaceIpcBridge = {
    read: () => Promise.resolve({ ok: false, error: unavailable() }),
    command: () => Promise.resolve({ ok: false, error: unavailable() }),
    events: (request) => {
      requests.push(request);
      return Promise.resolve({
        ok: true,
        value: {
          events: requests.length === 1 ? [event] : [],
          resume: { scope: { kind: "all_authorized" }, afterGlobalSequence: "1" },
          next: null,
          coverage: { state: "complete" },
        },
      });
    },
  };
  const client = createWorkspaceIpcClient(bridge, 1);
  const controller = new AbortController();
  const iterator = client.subscribe({
    cursor: {
      scope: { kind: "all_authorized" },
      afterGlobalSequence: DecimalSchema.parse("0"),
    },
    signal: controller.signal,
  });
  const first = await iterator[Symbol.asyncIterator]().next();
  assert.equal(first.done, false);
  assert.deepEqual(requests[0], {
    cursor: { scope: { kind: "all_authorized" }, afterGlobalSequence: "0" },
    limit: 256,
  });
  assert.equal(Object.hasOwn(requests[0] ?? {}, "signal"), false);
  controller.abort();
});

function unavailable() {
  return {
    code: "unavailable",
    message:
      "violates REQ spec://org.vibevm.zap/lens/PROP-005#transport: synthetic IPC unavailable",
  };
}
