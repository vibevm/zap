/** Installed Codex error projection and safe diagnostics.
 * @scope spec://org.vibevm.zap/lens/PROP-010#provider-support
 * @scope spec://org.vibevm.zap/lens/PROP-012#events
 */
import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { z } from "zod";
import {
  CoordinatorLifecycleInputSchema,
  CoordinatorStartInputSchema,
  CoordinatorTurnInputSchema,
  type CoordinatorEvent,
} from "../agent-runtime/index.ts";
import type { JsonValue } from "../protocol/index.ts";
import { OwnedCoordinatorAgentBindingSchema } from "../workspace-service/index.ts";
import { createCodexCoordinatorAdapter } from "./index.ts";
import { codexProfileFixture } from "./test-support.ts";
import { CodexWireMessageSchema, type CodexWireMessage } from "./protocol.ts";
import type { CodexProcessFactory, CodexProcessResult, CodexRpcProcess } from "./process.ts";

const cwd = resolve("fixture-workspace");

test("retry errors stay active while terminal errors expose safe failure", async () => {
  const process = new ErrorProcess("process.errors");
  const created = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: factory(process),
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const events: CoordinatorEvent[] = [];
  created.value.subscribe((event) => events.push(event));
  assert.equal((await created.value.start(startInput())).ok, true);
  process.emit(errorNotification(true, "serverOverloaded", "retry upstream"));
  const retry = statusData(events);
  assert.equal(retry.status.type, "active");
  assert.equal(retry.willRetry, true);
  assert.equal(retry.terminal, false);
  const steered = await created.value.send(
    CoordinatorTurnInputSchema.parse({
      coordinatorSessionId: "session.root",
      clientMessageId: "message.retry-steer",
      text: "Retain the active turn",
    }),
  );
  assert.equal(steered.ok, true);
  assert.equal(process.requests.at(-1)?.method, "turn/steer");

  process.emit(
    errorNotification(
      false,
      { httpConnectionFailed: { httpStatusCode: 503 } },
      "terminal upstream",
    ),
  );
  const terminal = statusData(events);
  assert.equal(terminal.status.type, "systemError");
  assert.equal(terminal.willRetry, false);
  assert.equal(terminal.terminal, true);
  assert.deepEqual(terminal.error, {
    message: "terminal upstream",
    codexErrorInfo: { httpConnectionFailed: { httpStatusCode: 503 } },
  });
  process.emit({
    method: "turn/completed",
    params: {
      threadId: "thread-root",
      turn: {
        id: "turn-bootstrap",
        status: "failed",
        items: [],
        error: {
          message: "terminal upstream",
          codexErrorInfo: { httpConnectionFailed: { httpStatusCode: 503 } },
        },
      },
    },
  });
  process.emit(errorNotification(false, "other", "late old-turn diagnostic"));
  const stale = events.findLast((event) => event.kind === "host_event_unmapped");
  const staleData = z
    .object({ method: z.literal("error"), coverage: z.literal("stale_turn_diagnostic") })
    .loose()
    .parse(stale?.data);
  assert.equal(staleData.method, "error");
  const restarted = await created.value.send(
    CoordinatorTurnInputSchema.parse({
      coordinatorSessionId: "session.root",
      clientMessageId: "message.after-stale-error",
      text: "Start only after the terminal turn",
    }),
  );
  assert.equal(restarted.ok, true);
  assert.equal(process.requests.at(-1)?.method, "turn/start");
  created.value.close();
});

test("unexpected stderr is classified without disclosure and explicit stop omits it", async () => {
  const unexpected = new ErrorProcess("process.unexpected");
  const first = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: factory(unexpected),
  });
  assert.equal(first.ok, true);
  if (!first.ok) return;
  const events: CoordinatorEvent[] = [];
  first.value.subscribe((event) => events.push(event));
  assert.equal((await first.value.start(startInput())).ok, true);
  unexpected.emitExit(1, "401 unauthorized bearer SECRET-FIXTURE");
  const exited = events.find((event) => event.kind === "process_exited");
  const data = z
    .object({
      code: z.literal(1),
      diagnostic: z.object({
        category: z.literal("authentication"),
        digest: z.string().regex(/^[0-9a-f]{64}$/),
        bytes: z.number().int().positive(),
      }),
    })
    .parse(exited?.data);
  assert.equal(JSON.stringify(data).includes("SECRET-FIXTURE"), false);
  first.value.close();

  const stopped = new ErrorProcess("process.stopped", "proxy SECRET-FIXTURE");
  const second = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: factory(stopped),
  });
  assert.equal(second.ok, true);
  if (!second.ok) return;
  const stoppedEvents: CoordinatorEvent[] = [];
  second.value.subscribe((event) => stoppedEvents.push(event));
  const started = await second.value.start(startInput());
  assert.equal(started.ok, true);
  if (!started.ok || second.value.stop === undefined) return;
  assert.equal(
    (
      await second.value.stop(
        CoordinatorLifecycleInputSchema.parse({
          coordinatorSessionId: started.value.coordinatorSessionId,
          expectedProcessEpoch: started.value.processEpoch,
        }),
      )
    ).ok,
    true,
  );
  const stoppedEvent = stoppedEvents.find((event) => event.kind === "session_stopped");
  assert.equal(JSON.stringify(stoppedEvent?.data).includes("diagnostic"), false);
  assert.equal(JSON.stringify(stoppedEvent?.data).includes("SECRET-FIXTURE"), false);
  second.value.close();
});

function statusData(events: readonly CoordinatorEvent[]) {
  return z
    .object({
      status: z.object({ type: z.enum(["active", "systemError"]) }).loose(),
      error: z.object({ message: z.string(), codexErrorInfo: z.unknown() }),
      willRetry: z.boolean(),
      terminal: z.boolean(),
      threadId: z.literal("thread-root"),
      turnId: z.literal("turn-bootstrap"),
    })
    .parse(events.findLast((event) => event.kind === "session_status")?.data);
}

function errorNotification(
  willRetry: boolean,
  codexErrorInfo: JsonValue,
  message: string,
): JsonValue {
  return {
    method: "error",
    params: {
      error: { message, codexErrorInfo, additionalDetails: null, misalignment: null },
      willRetry,
      threadId: "thread-root",
      turnId: "turn-bootstrap",
    },
  };
}

function startInput() {
  return CoordinatorStartInputSchema.parse({
    coordinatorSessionId: "session.root",
    projectId: "project.one",
    contextId: "context.main",
    conversationId: "conversation.main",
    coordinatorActorId: "actor.root",
    hostId: "host.local",
    profileId: "profile.codex",
    cwd,
    bootstrapText: "FULL PROJECT BOOT",
    bootstrapBasis: "source-basis.fixture",
    agentScope: { workspaceId: "workspace.main", conversationId: "conversation.main" },
    agentBinding: OwnedCoordinatorAgentBindingSchema.parse({
      actorId: "actor.broker.main",
      adapterSessionId: "adapter.owned.fixture.0000001",
    }),
  });
}

function factory(process: ErrorProcess): CodexProcessFactory {
  return { start: () => Promise.resolve({ ok: true, value: process }) };
}

class ErrorProcess implements CodexRpcProcess {
  readonly epoch: string;
  readonly requests: { readonly method: string; readonly params: JsonValue }[] = [];
  readonly #diagnostic: string;
  readonly #messages = new Set<(message: CodexWireMessage) => void>();
  readonly #exits = new Set<(exit: { code: number | null; diagnostic: string }) => void>();

  constructor(epoch: string, diagnostic = "") {
    this.epoch = epoch;
    this.#diagnostic = diagnostic;
  }

  request(method: string, params: JsonValue): Promise<CodexProcessResult<JsonValue>> {
    this.requests.push({ method, params });
    if (method === "thread/start")
      return Promise.resolve({
        ok: true,
        value: { thread: thread(), cwd, instructionSources: [resolve("AGENTS.md")] },
      });
    if (method === "turn/start")
      return Promise.resolve({ ok: true, value: { turn: turn("turn-bootstrap") } });
    if (method === "turn/steer")
      return Promise.resolve({ ok: true, value: { turnId: "turn-bootstrap" } });
    return Promise.resolve({ ok: true, value: {} });
  }
  notify(): CodexProcessResult<void> {
    return { ok: true, value: undefined };
  }
  respond(): CodexProcessResult<void> {
    return { ok: true, value: undefined };
  }
  subscribe(listener: (message: CodexWireMessage) => void): () => void {
    this.#messages.add(listener);
    return () => this.#messages.delete(listener);
  }
  onExit(listener: (exit: { code: number | null; diagnostic: string }) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }
  terminate(): Promise<CodexProcessResult<{ code: number | null }>> {
    for (const listener of this.#exits) listener({ code: 0, diagnostic: this.#diagnostic });
    return Promise.resolve({ ok: true, value: { code: 0 } });
  }
  close(): void {
    this.#messages.clear();
    this.#exits.clear();
  }
  emit(value: unknown): void {
    const message = CodexWireMessageSchema.parse(value);
    for (const listener of this.#messages) listener(message);
  }
  emitExit(code: number | null, diagnostic: string): void {
    for (const listener of this.#exits) listener({ code, diagnostic });
  }
}

function turn(id: string): JsonValue {
  return { id, status: "inProgress", items: [], error: null };
}

function thread(): JsonValue {
  return {
    id: "thread-root",
    sessionId: "thread-root",
    cwd,
    status: { type: "idle" },
    turns: [],
    parentThreadId: null,
    canAcceptDirectInput: true,
    cliVersion: "0.152.1",
    ephemeral: false,
    modelProvider: "openai",
    preview: "fixture",
    createdAt: 1,
    updatedAt: 1,
  };
}
