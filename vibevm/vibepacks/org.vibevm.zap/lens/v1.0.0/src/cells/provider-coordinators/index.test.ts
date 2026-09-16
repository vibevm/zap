import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  createProviderCoordinatorAdapter,
  createProviderCoordinatorHost,
  type ProviderCoordinatorProfile,
  type ProviderCoordinatorTransport,
} from "./index.ts";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema, DecimalSchema } from "../protocol/index.ts";
import { CoordinatorEventSchema } from "../agent-runtime/index.ts";
import { ProviderProjectionSimulationSchema } from "../mock-simulation/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import {
  createCoordinatorAdapterRegistry,
  createWorkspaceService,
} from "../workspace-service/index.ts";
import { access, registration } from "../workspace-service/index.test-support.ts";

const projectionScenario = ProviderProjectionSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./running-projection.simulation.json", import.meta.url), "utf8"),
  ),
);

test("provider host opens an independent adapter for each coordinator scope", async () => {
  let opened = 0;
  const host = createProviderCoordinatorHost({
    hostId: "host.opencode.independent",
    profile: {
      profileId: "profile.opencode.independent",
      provider: "opencode",
      executablePath: "C:/fixture/opencode.exe",
      cwd: "C:/fixture",
      modelId: "fixture-model",
      effort: null,
      endpoint: "http://127.0.0.1:4096",
    },
    transportFactory: {
      open: () => {
        opened += 1;
        return Promise.resolve({ ok: true, value: fixtureTransport("opencode") });
      },
    },
  });
  const first = await host.openCoordinator("profile.opencode.independent");
  const second = await host.openCoordinator("profile.opencode.independent");
  assert.equal(first.ok, true);
  assert.equal(second.ok, true);
  assert.equal(opened, 2);
  if (first.ok && second.ok) assert.notEqual(first.value, second.value);
});

test("addressed provider acceptance projects running, queues while busy, then returns ready", async () => {
  assert.equal(
    process.env["ZAP_MOCK_SIMULATION_ID"] ?? projectionScenario.scenarioId,
    projectionScenario.scenarioId,
  );
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? projectionScenario.seed;
  const opened = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const project = registration("provider-public");
  assert.equal(opened.value.registerProject(project).ok, true);
  const transport = fixtureTransport(projectionScenario.inputs.provider);
  const host = createProviderCoordinatorHost({
    hostId: "host.provider-public",
    profile: {
      profileId: project.protected.launchProfileRef,
      provider: projectionScenario.inputs.provider,
      executablePath: "C:/fixture/qwen.exe",
      cwd: "C:/fixtures/project-provider-public",
      modelId: "fixture-model",
      effort: "low",
      endpoint: null,
    },
    transportFactory: {
      open: () => Promise.resolve({ ok: true, value: transport }),
    },
  });
  const service = createWorkspaceService({
    store: opened.value,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: project.protected.launchProfileRef, host },
    ]),
  });
  const client = service.bind({
    access: access("provider-public", [project.projectId], "client.provider-public"),
    allowedActions: ["read", "events", "session.start.v1", "chat.post.v1", "project.stop.v1"],
  });
  const started = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "session.start.v1",
      clientRequestId: "request.provider-public.start",
      projectId: project.projectId,
      contextId: project.context.contextId,
      interactionKind: "structured",
      profileId: "profile.codex-default",
    }),
  );
  assert.equal(started.ok, true);
  const repeatedCompletion = {
    kind: "turn_completed",
    data: { status: "completed", subtype: "success" },
  };
  transport.emit(repeatedCompletion);
  await new Promise((resolve) => setImmediate(resolve));
  const first = await post(
    projectionScenario.inputs.firstClientRequestId,
    projectionScenario.inputs.firstMessage,
  );
  assert.equal(first.ok, true);
  await until(() => transport.sentClientMessageIds.length === 1);
  assert.equal(transport.sentNativeTurnIds[0], projectionScenario.expected.acceptedNativeTurnId);
  assert.equal(await state(), projectionScenario.expected.firstState);
  if (!first.ok || first.value.operation !== "chat.post.v1") return;
  const history = await client.events({
    cursor: {
      scope: {
        kind: "context",
        projectId: project.projectId,
        contextId: project.context.contextId,
      },
      afterGlobalSequence: DecimalSchema.parse("0"),
    },
    limit: 100,
  });
  assert.equal(history.ok, true);
  if (history.ok) {
    const accepted = history.value.events.find((event) => event.kind === "host.turn_started");
    assert.equal(
      accepted?.correlationId === first.value.message.messageId &&
        transport.sentClientMessageIds[0] === first.value.message.messageId,
      projectionScenario.expected.exactTransportCorrelation,
    );
  }
  const second = await post(
    projectionScenario.inputs.secondClientRequestId,
    projectionScenario.inputs.secondMessage,
  );
  assert.equal(second.ok, true);
  assert.equal(
    transport.sentClientMessageIds.length,
    projectionScenario.expected.sendCountWhileBusy,
  );
  transport.emit({
    type: "stream_event",
    parent_tool_use_id: null,
    event: { type: "message_start" },
  });
  transport.emit({
    type: "stream_event",
    parent_tool_use_id: null,
    event: { type: "message_stop" },
  });
  assert.equal(
    (await state()) === "running",
    projectionScenario.expected.intermediateMessageBoundaryKeepsRunning,
  );
  transport.emit({
    kind: "item_completed",
    nativeItemId: "item.progress.first",
    data: {
      type: "agentMessage",
      phase: "commentary",
      text: projectionScenario.inputs.intermediateText,
    },
  });
  transport.emit({
    kind: "item_completed",
    nativeItemId: "item.final.first",
    data: {
      type: "agentMessage",
      phase: "final_answer",
      text: projectionScenario.inputs.firstReply,
    },
  });
  transport.emit(repeatedCompletion);
  await until(() => transport.sentClientMessageIds.length === 2);
  assert.equal(await state(), projectionScenario.expected.stateAfterQueuedDispatch);
  transport.emit({
    kind: "item_completed",
    nativeItemId: "item.final.second",
    data: {
      type: "agentMessage",
      phase: "final_answer",
      text: projectionScenario.inputs.secondReply,
    },
  });
  transport.emit(repeatedCompletion);
  await untilAsync(async () => (await state()) === projectionScenario.expected.finalState);
  const page = await client.read({
    operation: "chat.page.v1",
    projectId: project.projectId,
    contextId: project.context.contextId,
    conversationId: project.context.coordinatorConversationId,
    afterSequence: DecimalSchema.parse("0"),
    limit: 20,
  });
  assert.equal(page.ok, true);
  if (page.ok && page.value.operation === "chat.page.v1")
    assert.deepEqual(
      page.value.page.messages
        .filter((message) => message.role === "assistant")
        .map((message) => message.bodyMarkdown),
      projectionScenario.expected.assistantReplies,
    );
  const finalHistory = await client.events({
    cursor: {
      scope: {
        kind: "context",
        projectId: project.projectId,
        contextId: project.context.contextId,
      },
      afterGlobalSequence: DecimalSchema.parse("0"),
    },
    limit: 100,
  });
  assert.equal(finalHistory.ok, true);
  if (finalHistory.ok) {
    const completions = finalHistory.value.events.filter(
      (event) => event.kind === "host.turn_completed",
    );
    assert.equal(
      new Set(completions.map((event) => event.sourceEventId)).size === completions.length &&
        completions.length === 3,
      projectionScenario.expected.repeatedCompletionsRetained,
    );
  }
  const execution = await client.read({
    operation: "project.execution.get.v1",
    projectId: project.projectId,
    contextId: project.context.contextId,
  });
  assert.equal(execution.ok, true);
  if (!execution.ok || execution.value.operation !== "project.execution.get.v1") return;
  const stopped = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "project.stop.v1",
      clientRequestId: "request.provider-public.stop",
      projectId: project.projectId,
      contextId: project.context.contextId,
      sessionId: execution.value.execution.sessionId,
      expectedRevision: execution.value.execution.revision,
      reasonMarkdown: "Observe exact owned raw process exit.",
    }),
  );
  assert.equal(stopped.ok, true);
  if (stopped.ok && stopped.value.operation === "project.stop.v1")
    assert.equal(stopped.value.execution.state, "stopped");
  service.close();
  opened.value.close();
  process.stdout.write(
    `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
      scenarioId: projectionScenario.scenarioId,
      seed,
      passed: true,
      inputPosition: 7,
      zeroLlmInference: projectionScenario.expected.zeroLlmInference,
    })}\n`,
  );

  function post(clientRequestId: string, bodyMarkdown: string) {
    return client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "chat.post.v1",
        clientRequestId,
        projectId: project.projectId,
        contextId: project.context.contextId,
        conversationId: project.context.coordinatorConversationId,
        bodyMarkdown,
        artifactRefs: [],
        correlationId: null,
        causationMessageId: null,
      }),
    );
  }
  async function state() {
    const result = await client.read({
      operation: "session.list.v1",
      projectId: project.projectId,
      contextId: project.context.contextId,
    });
    assert.equal(result.ok, true);
    if (!result.ok || result.value.operation !== "session.list.v1") throw new Error();
    return result.value.sessions[0]?.state;
  }
});

for (const provider of ["claude_code", "opencode", "qwen_code"] as const) {
  test(`${provider} fixture adapter preserves host IDs and honest capabilities`, async () => {
    const transport = fixtureTransport(provider);
    const profile: ProviderCoordinatorProfile = {
      profileId: `profile.${provider}.fixture`,
      provider,
      executablePath: `C:/fixture/${provider}.exe`,
      cwd: "C:/fixture",
      modelId: "fixture-model",
      effort: "low",
      endpoint: provider === "opencode" ? "http://127.0.0.1:4096" : null,
    };
    const created = createProviderCoordinatorAdapter({
      profile,
      hostId: `host.${provider}.fixture`,
      transport,
    });
    assert.equal(created.ok, true);
    if (!created.ok) return;
    const events: unknown[] = [];
    const scope = {
      coordinatorSessionId: AgentSessionIdSchema.parse(`session.${provider}.fixture`),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse(`host.${provider}.fixture`),
    };
    const started = await created.value.start({
      ...scope,
      profileId: profile.profileId,
      cwd: profile.cwd,
      bootstrapText: "",
      bootstrapBasis: "fixture",
    });
    assert.equal(started.ok, true);
    if (!started.ok) return;
    created.value.subscribe((event) => events.push(event));
    const sent = await created.value.send({
      coordinatorSessionId: scope.coordinatorSessionId,
      text: "fixture turn",
      clientMessageId: "message.fixture",
    });
    assert.equal(sent.ok, true);
    const accepted = CoordinatorEventSchema.parse(events[0]);
    assert.equal(accepted.kind, "turn_started");
    assert.equal(accepted.nativeTurnId, null);
    assert.equal(accepted.transportCorrelation?.clientMessageId, "message.fixture");
    assert.equal(accepted.transportCorrelation?.processEpoch, started.value.processEpoch);
    transport.emit(
      provider === "claude_code"
        ? {
            type: "stream_event",
            message_id: "turn.fixture",
            event: {
              type: "content_block_delta",
              index: "item.fixture",
              delta: { text: "fixture output" },
            },
          }
        : {
            type: "text",
            turn_id: "turn.fixture",
            item_id: "item.fixture",
            text: "fixture output",
          },
    );
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(events.length, 2);
    transport.emit(
      provider === "opencode"
        ? { type: "turn.completed", turn_id: "turn.fixture" }
        : { type: "result", turn_id: "turn.fixture" },
    );
    await new Promise((resolve) => setImmediate(resolve));
    const completed = CoordinatorEventSchema.parse(events[2]);
    assert.equal(completed.kind, "turn_completed");
    assert.equal(completed.transportCorrelation?.clientMessageId, "message.fixture");
    const next = await created.value.send({
      coordinatorSessionId: scope.coordinatorSessionId,
      text: "next fixture turn",
      clientMessageId: "message.fixture.next",
    });
    assert.equal(next.ok, true);
    assert.equal(created.value.capabilities.nativeChildObservation, false);
    assert.equal(created.value.capabilities.commandApproval, false);
    if (provider === "opencode") {
      const stopped = await created.value.stop?.({
        coordinatorSessionId: scope.coordinatorSessionId,
        expectedProcessEpoch: started.value.processEpoch,
      });
      assert.equal(stopped?.ok && stopped.value.observation, "settled");
      const continued = await created.value.continueSession?.({
        coordinatorSessionId: scope.coordinatorSessionId,
        expectedProcessEpoch: started.value.processEpoch,
      });
      assert.equal(continued?.ok && continued.value.observation, "settled");
      assert.equal(continued?.ok && continued.value.previousProcessEpoch, "1");
      assert.equal(continued?.ok && continued.value.currentProcessEpoch, "2");
      assert.equal(continued?.ok && continued.value.nativeThreadId, "opencode.thread.fixture");
    } else if (provider === "qwen_code") {
      transport.emit({
        type: "process_diagnostic",
        channel: "stderr",
        category: "network",
        digest: "a".repeat(64),
        bytes: 42,
        truncated: false,
      });
      await new Promise((resolve) => setImmediate(resolve));
      const diagnostic = CoordinatorEventSchema.parse(events.at(-1));
      assert.equal(diagnostic.kind, "host_event_unmapped");
      assert.deepEqual(diagnostic.data, {
        provider: "qwen_code",
        eventType: "process_diagnostic",
        channel: "stderr",
        category: "network",
        digest: "a".repeat(64),
        bytes: 42,
        truncated: false,
      });
      transport.emit({ type: "process_exited", exitCode: 17 });
      await new Promise((resolve) => setImmediate(resolve));
      const exited = CoordinatorEventSchema.parse(events.at(-1));
      assert.equal(exited.kind, "process_exited");
      assert.deepEqual(exited.data, { exitCode: 17 });
    }
    created.value.close();
  });
}

function fixtureTransport(
  provider: "claude_code" | "opencode" | "qwen_code",
): ProviderCoordinatorTransport & {
  readonly sentClientMessageIds: readonly string[];
  readonly sentNativeTurnIds: readonly (string | null)[];
  emit(raw: unknown): void;
} {
  let listener: ((raw: unknown) => void) | undefined;
  let epoch = 1;
  const sentClientMessageIds: string[] = [];
  const sentNativeTurnIds: Array<string | null> = [];
  const session = () => ({
    coordinatorSessionId: AgentSessionIdSchema.parse(`session.${provider}.fixture`),
    nativeSessionId: `${provider}.session.fixture`,
    nativeThreadId: `${provider}.thread.fixture`,
    processEpoch: String(epoch),
    modelId: "fixture-model",
    effort: "low" as const,
  });
  return {
    sentClientMessageIds,
    sentNativeTurnIds,
    async start() {
      return { ok: true, value: session() };
    },
    async resume() {
      epoch += 1;
      return { ok: true, value: session() };
    },
    async history() {
      return {
        ok: true,
        value: {
          coordinatorSessionId: AgentSessionIdSchema.parse("session.fixture"),
          nativeThreadId: session().nativeThreadId,
          processEpoch: String(epoch),
          status: {},
          turns: [],
        },
      };
    },
    async send(input) {
      sentClientMessageIds.push(input.clientMessageId);
      sentNativeTurnIds.push(null);
      return {
        ok: true,
        value: {
          nativeTurnId: null,
          transportCorrelation: {
            provenance: "transport_correlation",
            clientMessageId: input.clientMessageId,
            processEpoch: input.session.processEpoch,
          },
        },
      };
    },
    async interrupt() {
      return { ok: true, value: null };
    },
    async respond() {
      return { ok: true, value: null };
    },
    async pause() {
      return { ok: true, value: { observation: "requested" as const } };
    },
    async stop() {
      listener?.({ type: "process_exited", exitCode: 0 });
      return { ok: true, value: null };
    },
    subscribe(next) {
      listener = next;
      return () => {
        listener = undefined;
      };
    },
    emit(raw) {
      listener?.(raw);
    },
    close() {
      listener = undefined;
    },
  };
}

async function until(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setImmediate(resolve));
  }
  throw new Error(
    reqMessage(
      "provider fixture did not reach the expected synchronous state",
      "inspect accepted-turn dispatch and the queued message",
    ),
  );
}

async function untilAsync(predicate: () => Promise<boolean>): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await predicate()) return;
    await new Promise((resolve) => setImmediate(resolve));
  }
  throw new Error(
    reqMessage(
      "provider fixture did not project the expected state",
      "inspect the provider completion event and session projection",
    ),
  );
}

function reqMessage(why: string, fix: string): string {
  return `violates REQ spec://org.vibevm.zap/lens/PROP-012#events: ${why}; fix surface: ${fix}`;
}
