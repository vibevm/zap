/**
 * Fake-process Codex coordinator contract tests.
 * @scope spec://org.vibevm.zap/lens/PROP-005#incremental-delivery
 * @scope spec://org.vibevm.zap/lens/PROP-009#verification
 */
import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { z } from "zod";
import {
  CoordinatorLifecycleInputSchema,
  CoordinatorStartInputSchema,
  CoordinatorTurnInputSchema,
  HostRequestAnswerSchema,
  type CoordinatorEvent,
} from "../agent-runtime/index.ts";
import { zapPreauthorizedToolNames, type JsonValue } from "../protocol/index.ts";
import { AgentSessionIdSchema } from "../workspace-model/index.ts";
import { OwnedCoordinatorAgentBindingSchema } from "../workspace-service/index.ts";
import { createCodexCoordinatorAdapter } from "./index.ts";
import { codexCollabFixture, codexProfileFixture } from "./test-support.ts";
import { CodexWireMessageSchema, type CodexRpcId, type CodexWireMessage } from "./protocol.ts";
import {
  CodexProcessProfileSchema,
  type CodexProcessFactory,
  type CodexProcessProfile,
  type CodexProcessResult,
  type CodexRpcProcess,
} from "./process.ts";

const cwd = resolve("fixture-workspace");
const sessionId = AgentSessionIdSchema.parse("session.root");
const relationshipData = z
  .object({ relationship: z.enum(["parent", "message"]), toNativeThreadId: z.string() })
  .loose();

test("starts one persistent coordinator with exact profile and explicit coordinator bootstrap", async () => {
  const process = new FakeProcess("process-one");
  const factory = new SequenceFactory([process]);
  const adapter = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: factory,
  });
  assert.equal(adapter.ok, true);
  if (!adapter.ok) return;
  const started = await adapter.value.start(startInput());
  assert.equal(started.ok, true);
  if (!started.ok) return;
  assert.equal(started.value.productId, "codex");
  assert.equal(started.value.interactionKind, "structured");
  assert.equal(started.value.capabilities.managedTerminal, false);
  assert.equal(started.value.nativeThreadRef.value, "thread-root");
  assert.deepEqual(Object.keys(startInput().agentBinding ?? {}).sort(), [
    "actorId",
    "adapterSessionId",
  ]);
  const receivedProfile = CodexProcessProfileSchema.parse(factory.receivedProfiles[0]);
  assert.deepEqual(Object.keys(receivedProfile).sort(), ["executablePath", "requestTimeoutMs"]);
  const threadStart = process.requests.find((request) => request.method === "thread/start");
  assert.deepEqual(threadStart?.params, {
    model: "gpt-test",
    config: {
      model_reasoning_effort: "low",
      mcp_servers: {
        codlens: {
          command: resolve("codlens.exe"),
          args: ["mcp", "serve"],
          disabled_tools: ["codlens_connect"],
          default_tools_approval_mode: "prompt",
          tools: Object.fromEntries(
            zapPreauthorizedToolNames(false).map((tool) => [tool, { approval_mode: "approve" }]),
          ),
          env: {
            CODLENS_URL: "http://127.0.0.1:32191",
            CODLENS_CREDENTIAL_FILE: resolve("fixture-credentials.json"),
            CODLENS_WORKSPACE_ID: "workspace.main",
            CODLENS_CONVERSATION_ID: "conversation.main",
            CODLENS_ADAPTER_SESSION_ID: "adapter.owned.fixture.0000001",
          },
        },
      },
    },
    allowProviderModelFallback: false,
    cwd,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "quicklens-test",
    ephemeral: false,
    historyMode: "legacy",
  });
  const turns = process.requests.filter((request) => request.method === "turn/start");
  assert.equal(turns.length, 1);
  assert.match(JSON.stringify(turns[0]?.params), /long-lived project coordinator/);
  assert.match(JSON.stringify(turns[0]?.params), /FULL PROJECT BOOT/);
  assert.match(JSON.stringify(turns[0]?.params), /ZapAskUserQuestion/);
  assert.equal(
    z
      .object({ effort: z.literal("low") })
      .passthrough()
      .parse(turns[0]?.params).effort,
    "low",
  );
  const duplicate = await adapter.value.start(startInput());
  assert.equal(duplicate.ok, false);
  if (!duplicate.ok) assert.equal(duplicate.error.code, "already_exists");
  adapter.value.close();
});

test("keeps child turns out of coordinator control state and distinguishes spawn from messages", async () => {
  const process = new FakeProcess("process-network");
  const created = adapterWith(process);
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const events: CoordinatorEvent[] = [];
  created.value.subscribe((event) => events.push(event));
  const started = await created.value.start(startInput());
  assert.equal(started.ok, true);
  process.emit({
    method: "item/started",
    emittedAtMs: 1_789_470_000_000,
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      startedAtMs: 1,
      item: codexCollabFixture("collab-spawn", "spawnAgent", ["thread-child"]),
    },
  });
  process.emit({
    method: "turn/started",
    params: { threadId: "thread-child", turn: turn("turn-child", "inProgress") },
  });
  process.emit({
    method: "turn/completed",
    params: { threadId: "thread-child", turn: turn("turn-child", "completed") },
  });
  process.emit({
    method: "item/completed",
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      completedAtMs: 2,
      item: codexCollabFixture("collab-message", "sendMessage", ["thread-peer"]),
    },
  });

  const sent = await created.value.send(
    CoordinatorTurnInputSchema.parse({
      coordinatorSessionId: "session.root",
      clientMessageId: "message.user",
      text: "Continue root work",
    }),
  );
  assert.equal(sent.ok, true);
  assert.equal(process.requests.at(-1)?.method, "turn/steer");
  const steerParams = z
    .object({ expectedTurnId: z.string() })
    .loose()
    .parse(process.requests.at(-1)?.params);
  assert.equal(steerParams.expectedTurnId, "turn-bootstrap");

  const child = events.find((event) => event.kind === "native_child_observed");
  const message = events.find((event) => event.kind === "native_message_observed");
  const childData = relationshipData.parse(child?.data);
  const messageData = relationshipData.parse(message?.data);
  assert.equal(childData.relationship, "parent");
  assert.equal(childData.toNativeThreadId, "thread-child");
  assert.equal(messageData.relationship, "message");
  assert.equal(messageData.toNativeThreadId, "thread-peer");

  const unknownPeer = await created.value.readNativeChildHistory(sessionId, "thread-peer");
  assert.equal(unknownPeer.ok, false, "message recipient is not promoted to a child");
  created.value.close();
});

test("fences stale host answers across restart and exposes bounded child history", async () => {
  const first = new FakeProcess("process-old");
  const second = new FakeProcess("process-new");
  const factory = new SequenceFactory([first, second]);
  const created = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: factory,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const events: CoordinatorEvent[] = [];
  created.value.subscribe((event) => events.push(event));
  const started = await created.value.start(startInput());
  assert.equal(started.ok, true);
  if (!started.ok) return;

  first.emit({
    id: "request-one",
    method: "item/tool/requestUserInput",
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      itemId: "item-question",
      isBlocking: true,
      questions: [{ id: "choice", header: "Choice", question: "Pick one", options: null }],
    },
  });
  const pending = events.find((event) => event.kind === "host_request_pending");
  assert.notEqual(pending, undefined);

  const hostAnswer = HostRequestAnswerSchema.parse({
    coordinatorSessionId: "session.root",
    requestId: "request-one",
    processEpoch: pending?.processEpoch ?? "missing",
    answer: { answers: { choice: { answers: ["A"] } } },
  });
  const answered = await created.value.respondToRequest(hostAnswer);
  assert.equal(answered.ok, true);
  assert.equal(first.responses.length, 1);
  const duplicateAnswer = await created.value.respondToRequest(hostAnswer);
  assert.equal(duplicateAnswer.ok, false);
  if (!duplicateAnswer.ok) assert.equal(duplicateAnswer.error.code, "already_exists");

  first.emit({
    id: "request-two",
    method: "item/tool/requestUserInput",
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      itemId: "item-question-two",
      isBlocking: true,
      questions: [{ id: "choice", header: "Choice", question: "Pick one", options: null }],
    },
  });
  const pendingTwo = events.findLast((event) => event.kind === "host_request_pending");

  const restarted = await created.value.restart("profile.codex");
  assert.equal(restarted.ok, true);
  const stale = await created.value.respondToRequest(
    HostRequestAnswerSchema.parse({
      coordinatorSessionId: "session.root",
      requestId: "request-two",
      processEpoch: pendingTwo?.processEpoch ?? "missing",
      answer: { answers: { choice: { answers: ["A"] } } },
    }),
  );
  assert.equal(stale.ok, false);
  if (!stale.ok) assert.equal(stale.error.code, "stale_epoch");

  second.emit({
    method: "item/started",
    params: {
      threadId: "thread-root",
      turnId: "turn-after-resume",
      startedAtMs: 3,
      item: codexCollabFixture("collab-spawn-two", "spawnAgent", ["thread-child"]),
    },
  });
  second.historyThread = thread(
    "thread-child",
    "idle",
    [
      turn("turn-history", "completed", [
        { id: "reasoning-one", type: "reasoning", status: "completed", content: ["hidden"] },
        { id: "message-one", type: "agentMessage", text: "Public child result" },
      ]),
    ],
    "thread-root",
  );
  const history = await created.value.readNativeChildHistory(sessionId, "thread-child");
  assert.equal(history.ok, true);
  if (history.ok) {
    assert.equal(JSON.stringify(history.value.turns).includes("Public child result"), true);
    assert.equal(JSON.stringify(history.value.turns).includes("hidden"), false);
    assert.equal(JSON.stringify(history.value.turns).includes("redacted"), true);
  }
  created.value.close();
});

test("pause and stop are project-owned while continue resumes without another bootstrap", async () => {
  const processA = new FakeProcess("process-a");
  const processB = new FakeProcess("process-b");
  const resumedA = new FakeProcess("process-a-resumed");
  const created = createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: new SequenceFactory([processA, processB, resumedA]),
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const events: CoordinatorEvent[] = [];
  created.value.subscribe((event) => events.push(event));
  const startedA = await created.value.start(startInput());
  const startedB = await created.value.start(startInput("session.other", "project.other"));
  assert.equal(startedA.ok, true);
  assert.equal(startedB.ok, true);
  if (!startedA.ok || !startedB.ok) return;

  processA.emit({
    method: "item/started",
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      startedAtMs: 4,
      item: codexCollabFixture("collab-pause-child", "spawnAgent", ["thread-child-a"]),
    },
  });
  processA.emit({
    method: "turn/started",
    params: { threadId: "thread-child-a", turn: turn("turn-child-a", "inProgress") },
  });
  const paused = await created.value.pause?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: startedA.value.processEpoch,
    }),
  );
  assert.equal(paused?.ok, true);
  if (paused?.ok) {
    assert.equal(paused.value.observation, "requested");
    assert.equal(paused.value.targets.length, 2);
  }
  assert.equal(
    processA.requests.filter((request) => request.method === "turn/interrupt").length,
    2,
  );
  assert.equal(
    processB.requests.filter((request) => request.method === "turn/interrupt").length,
    0,
  );
  const repeatedPause = await created.value.pause?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: startedA.value.processEpoch,
    }),
  );
  assert.equal(repeatedPause?.ok, true);
  assert.equal(
    processA.requests.filter((request) => request.method === "turn/interrupt").length,
    2,
    "repeated pause does not issue duplicate interrupts",
  );

  processA.emit({
    method: "turn/completed",
    params: { threadId: "thread-root", turn: turn("turn-bootstrap", "interrupted") },
  });
  processA.emit({
    method: "turn/completed",
    params: { threadId: "thread-child-a", turn: turn("turn-child-a", "interrupted") },
  });
  assert.equal(
    events.some((event) => event.kind === "session_paused"),
    true,
  );
  const settledCount = events.filter((event) => event.kind === "session_paused").length;
  processA.emit({
    method: "turn/started",
    params: { threadId: "thread-child-a", turn: turn("turn-child-late", "inProgress") },
  });
  await Promise.resolve();
  assert.equal(
    processA.requests.filter((request) => request.method === "turn/interrupt").length,
    3,
  );
  assert.equal(events.filter((event) => event.kind === "session_paused").length, settledCount);
  processA.emit({
    method: "turn/completed",
    params: { threadId: "thread-child-a", turn: turn("turn-child-late", "interrupted") },
  });
  assert.equal(events.filter((event) => event.kind === "session_paused").length, settledCount + 1);

  const stopped = await created.value.stop?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: startedA.value.processEpoch,
    }),
  );
  assert.equal(stopped?.ok, true);
  if (stopped?.ok) assert.equal(stopped.value.observation, "settled");
  assert.equal(processA.terminated, true);
  assert.equal(processB.terminated, false);
  const repeatedStop = await created.value.stop?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: startedA.value.processEpoch,
    }),
  );
  assert.equal(repeatedStop?.ok, true);
  assert.equal(processA.terminateCalls, 1, "repeated stop does not terminate twice");

  processB.emit({
    method: "item/agentMessage/delta",
    params: {
      threadId: "thread-root",
      turnId: "turn-bootstrap",
      itemId: "message-b",
      delta: "Project B remains visible",
    },
  });
  assert.equal(
    events.some(
      (event) => event.coordinatorSessionId === "session.other" && event.kind === "message_delta",
    ),
    true,
  );

  const continued = await created.value.continueSession?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: startedA.value.processEpoch,
    }),
  );
  assert.equal(continued?.ok, true);
  assert.equal(
    resumedA.requests.some((request) => request.method === "thread/resume"),
    true,
  );
  assert.equal(
    resumedA.requests.some((request) => request.method === "turn/start"),
    false,
  );
  assert.equal(processB.terminated, false);
  created.value.close();
});

test("stop remains uncertain until the owned process exit is observed", async () => {
  const process = new FakeProcess("process-uncertain", "uncertain");
  const created = adapterWith(process);
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const started = await created.value.start(startInput());
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const stopped = await created.value.stop?.(
    CoordinatorLifecycleInputSchema.parse({
      coordinatorSessionId: "session.root",
      expectedProcessEpoch: started.value.processEpoch,
    }),
  );
  assert.equal(stopped?.ok, true);
  if (stopped?.ok) assert.equal(stopped.value.observation, "uncertain");
  created.value.close();
});

function adapterWith(process: FakeProcess) {
  return createCodexCoordinatorAdapter({
    profiles: [codexProfileFixture()],
    processFactory: new SequenceFactory([process]),
  });
}

function startInput(coordinatorSessionId = "session.root", projectId = "project.one") {
  const other = coordinatorSessionId !== "session.root";
  return CoordinatorStartInputSchema.parse({
    coordinatorSessionId,
    projectId,
    contextId: other ? "context.other" : "context.main",
    conversationId: other ? "conversation.other" : "conversation.main",
    coordinatorActorId: other ? "actor.other" : "actor.root",
    hostId: "host.local",
    profileId: "profile.codex",
    cwd,
    bootstrapText: "FULL PROJECT BOOT",
    bootstrapBasis: "source-basis.fixture",
    agentScope: {
      workspaceId: other ? "workspace.other" : "workspace.main",
      conversationId: other ? "conversation.other" : "conversation.main",
    },
    agentBinding: OwnedCoordinatorAgentBindingSchema.parse({
      actorId: other ? "actor.broker.other" : "actor.broker.main",
      adapterSessionId: other ? "adapter.owned.other.00000001" : "adapter.owned.fixture.0000001",
    }),
  });
}

function turn(id: string, status: string, items: JsonValue[] = []): JsonValue {
  return { id, status, items, error: null };
}

function thread(
  id = "thread-root",
  status: "idle" | "active" = "idle",
  turns: JsonValue[] = [],
  parentThreadId: string | null = null,
): JsonValue {
  return {
    id,
    sessionId: parentThreadId === null ? id : "thread-root",
    cwd,
    status: status === "active" ? { type: "active", activeFlags: [] } : { type: "idle" },
    turns,
    parentThreadId,
    canAcceptDirectInput: parentThreadId === null ? true : false,
    cliVersion: "0.152.1",
    ephemeral: false,
    modelProvider: "openai",
    preview: "fixture",
    createdAt: 1,
    updatedAt: 1,
  };
}

class SequenceFactory implements CodexProcessFactory {
  readonly #processes: FakeProcess[];
  readonly receivedProfiles: CodexProcessProfile[] = [];

  constructor(processes: FakeProcess[]) {
    this.#processes = [...processes];
  }

  start(profile: CodexProcessProfile): Promise<CodexProcessResult<CodexRpcProcess>> {
    this.receivedProfiles.push(profile);
    const process = this.#processes.shift();
    return Promise.resolve(
      process === undefined
        ? { ok: false, error: { kind: "spawn_failed", message: "No fake process" } }
        : { ok: true, value: process },
    );
  }
}

class FakeProcess implements CodexRpcProcess {
  readonly requests: { method: string; params: JsonValue }[] = [];
  readonly responses: { id: CodexRpcId; result: JsonValue }[] = [];
  readonly epoch: string;
  readonly #termination: "confirmed" | "uncertain";
  historyThread: JsonValue = thread();
  terminated = false;
  terminateCalls = 0;
  readonly #messages = new Set<(message: CodexWireMessage) => void>();
  readonly #exits = new Set<(exit: { code: number | null; diagnostic: string }) => void>();

  constructor(epoch: string, termination: "confirmed" | "uncertain" = "confirmed") {
    this.epoch = epoch;
    this.#termination = termination;
  }

  request(method: string, params: JsonValue): Promise<CodexProcessResult<JsonValue>> {
    this.requests.push({ method, params });
    if (method === "thread/start" || method === "thread/resume") {
      return Promise.resolve({
        ok: true,
        value: { thread: thread(), cwd, instructionSources: [resolve("AGENTS.md")] },
      });
    }
    if (method === "thread/read") {
      return Promise.resolve({ ok: true, value: { thread: this.historyThread } });
    }
    if (method === "turn/start") {
      return Promise.resolve({ ok: true, value: { turn: turn("turn-bootstrap", "inProgress") } });
    }
    if (method === "turn/steer") {
      return Promise.resolve({ ok: true, value: { turnId: "turn-bootstrap" } });
    }
    return Promise.resolve({ ok: true, value: {} });
  }

  notify(): CodexProcessResult<void> {
    return { ok: true, value: undefined };
  }

  respond(id: CodexRpcId, result: JsonValue): CodexProcessResult<void> {
    this.responses.push({ id, result });
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
    this.terminateCalls += 1;
    this.terminated = true;
    if (this.#termination === "uncertain") {
      return Promise.resolve({
        ok: false,
        error: { kind: "timeout", message: "Synthetic exit was not observed" },
      });
    }
    for (const listener of this.#exits) listener({ code: 0, diagnostic: "" });
    return Promise.resolve({ ok: true, value: { code: 0 } });
  }

  emit(value: unknown): void {
    const message = CodexWireMessageSchema.parse(value);
    for (const listener of this.#messages) listener(message);
  }

  close(): void {
    this.#messages.clear();
    this.#exits.clear();
  }
}
