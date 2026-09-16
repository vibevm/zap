/** Wayfinder lifecycle and chat integration proof. @scope spec://org.vibevm.zap/lens/PROP-009#verification */
import assert from "node:assert/strict";
import test from "node:test";
import {
  CoordinatorEventSchema,
  CoordinatorSessionDescriptorSchema,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorEvent,
  type CoordinatorHistory,
  type CoordinatorLifecycleInput,
  type CoordinatorLifecycleReceipt,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorTurnReceipt,
} from "../agent-runtime/index.ts";
import { ActorIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  ExecutionHostIdSchema,
  NativeRefSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
  type ProjectId,
  type WorkspaceAccessContext,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { createCoordinatorAdapterRegistry, createWorkspaceService } from "./index.ts";

const capabilities = {
  persistentThreads: true,
  turnStart: true,
  activeTurnSteer: true,
  turnInterrupt: true,
  historyRead: true,
  nativeChildObservation: true,
  nativeChildDirectInput: false,
  structuredUserInput: true,
  commandApproval: true,
  managedTerminal: false,
};

test("durable project controls isolate projects and queued chat resumes once", async () => {
  const storeResult = openWorkspaceStore({
    databasePath: ":memory:",
    clock: monotonicClock(),
    idFactory: sequentialIds(),
  });
  assert.equal(storeResult.ok, true);
  if (!storeResult.ok) return;
  const store = storeResult.value;
  const projectA = registration("a");
  const projectB = registration("b");
  assert.equal(store.registerProject(projectA).ok, true);
  assert.equal(store.registerProject(projectB).ok, true);
  const adapterA = new LifecycleAdapter("a");
  const adapterB = new LifecycleAdapter("b");
  const registry = createCoordinatorAdapterRegistry([
    { profileRef: "protected-profile.a", host: host("a", adapterA) },
    { profileRef: "protected-profile.b", host: host("b", adapterB) },
  ]);
  const service = createWorkspaceService({
    store,
    adapters: registry,
    clock: monotonicClock(),
    idFactory: sequentialIds(),
  });
  const client = service.bind({
    access: access([projectA.projectId, projectB.projectId]),
    allowedActions: [
      "read",
      "events",
      "subscribe",
      "session.start.v1",
      "chat.post.v1",
      "project.pause.v1",
      "project.stop.v1",
      "project.continue.v1",
    ],
  });
  assert.equal((await client.command(startRequest(projectA.projectId, "a"))).ok, true);
  assert.equal((await client.command(startRequest(projectB.projectId, "b"))).ok, true);
  const executionA = await execution(client, projectA.projectId, "a");
  const executionB = await execution(client, projectB.projectId, "b");
  assert.equal(executionA.state, "running");
  assert.equal(executionB.state, "running");
  assert.notEqual(executionA.sessionId, null);
  assert.notEqual(executionB.sessionId, null);
  if (executionA.sessionId === null || executionB.sessionId === null) return;

  adapterA.emit(event(executionA.sessionId, "1", "turn_started", "root.a", "turn.busy-a"));
  const chatARequest = chatRequest(projectA.projectId, "a", "request.chat-a", "Queued for A");
  const queuedA = await client.command(chatARequest);
  assert.equal(queuedA.ok, true);
  if (queuedA.ok && queuedA.value.operation === "chat.post.v1") {
    assert.equal(queuedA.value.message.deliveryState, "queued");
  }
  assert.equal(adapterA.sends, 0);

  const acceptedB = await client.command(
    chatRequest(projectB.projectId, "b", "request.chat-b", "Delivered to B"),
  );
  assert.equal(acceptedB.ok, true);
  if (acceptedB.ok && acceptedB.value.operation === "chat.post.v1") {
    assert.equal(acceptedB.value.message.deliveryState, "host_accepted");
  }
  assert.equal(adapterB.sends, 1);
  adapterB.failNextSend = true;
  const uncertainBRequest = chatRequest(
    projectB.projectId,
    "b",
    "request.chat-b-uncertain",
    "Uncertain B",
  );
  const uncertainB = await client.command(uncertainBRequest);
  assert.equal(uncertainB.ok, true);
  if (uncertainB.ok && uncertainB.value.operation === "chat.post.v1") {
    assert.equal(uncertainB.value.message.deliveryState, "uncertain");
  }
  await client.command(uncertainBRequest);
  assert.equal(adapterB.sends, 2, "uncertain dispatch is not blindly repeated");

  const pause = lifecycleRequest(
    "project.pause.v1",
    projectA.projectId,
    "a",
    executionA.sessionId,
    executionA.revision,
    "request.pause-a",
  );
  const pausing = await client.command(pause);
  assert.equal(pausing.ok, true);
  if (pausing.ok && pausing.value.operation === "project.pause.v1") {
    assert.equal(pausing.value.execution.state, "pausing");
  }
  assert.equal((await client.command(pause)).ok, true);
  assert.equal(adapterA.pauses, 1);
  adapterA.emit(event(executionA.sessionId, "1", "turn_completed", "root.a", "turn.busy-a"));
  adapterA.emit(event(executionA.sessionId, "1", "session_paused", null, null));
  const paused = await execution(client, projectA.projectId, "a");
  assert.equal(paused.state, "paused");
  assert.equal(adapterA.sends, 0);
  assert.equal((await execution(client, projectB.projectId, "b")).state, "running");

  const stop = lifecycleRequest(
    "project.stop.v1",
    projectA.projectId,
    "a",
    executionA.sessionId,
    paused.revision,
    "request.stop-a",
  );
  const stopped = await client.command(stop);
  assert.equal(stopped.ok, true);
  if (stopped.ok && stopped.value.operation === "project.stop.v1") {
    assert.equal(stopped.value.execution.state, "stopped");
  }
  await client.command(stop);
  assert.equal(adapterA.stops, 1);
  assert.equal(adapterB.stops, 0);

  const stoppedState = await execution(client, projectA.projectId, "a");
  const blockedStart = await client.command(
    WorkspaceCommandRequestSchema.parse({
      ...startRequest(projectA.projectId, "a"),
      clientRequestId: "request.start-a-while-stopped",
    }),
  );
  assert.equal(blockedStart.ok, false, "session.start cannot bypass stopped execution");
  assert.equal(adapterA.starts, 1);
  service.close();
  const restarted = createWorkspaceService({
    store,
    adapters: registry,
    clock: monotonicClock(),
    idFactory: sequentialIds(),
  });
  const resumedClient = restarted.bind({
    access: access([projectA.projectId, projectB.projectId]),
    allowedActions: [
      "read",
      "events",
      "subscribe",
      "session.start.v1",
      "chat.post.v1",
      "project.pause.v1",
      "project.stop.v1",
      "project.continue.v1",
    ],
  });
  adapterA.completeInsideSend = true;
  const continued = await resumedClient.command(
    lifecycleRequest(
      "project.continue.v1",
      projectA.projectId,
      "a",
      executionA.sessionId,
      stoppedState.revision,
      "request.continue-a",
    ),
  );
  assert.equal(continued.ok, true);
  await new Promise<void>((resolveWait) => setImmediate(resolveWait));
  assert.equal(adapterA.resumes, 1);
  assert.equal(adapterA.continues, 0, "restart continuation resumes rather than double-continuing");
  assert.equal(adapterA.starts, 1, "continue never repeats coordinator bootstrap");
  assert.equal(adapterA.sends, 1, "one retained message dispatches after continuation");
  assert.equal((await execution(resumedClient, projectA.projectId, "a")).state, "running");

  const nativeTurn = adapterA.lastTurnId;
  assert.notEqual(nativeTurn, null);
  if (nativeTurn === null) return;
  // Replay the completion already emitted synchronously inside send. The stable
  // native thread/turn/item identity must not append a second assistant reply.
  adapterA.emit({
    coordinatorSessionId: executionA.sessionId,
    processEpoch: "2",
    nativeThreadId: "root.a",
    nativeTurnId: nativeTurn,
    nativeItemId: "item.reply-a",
    kind: "item_completed",
    sourceEventId: "event.reply-a",
    data: { id: "item.reply-a", type: "agentMessage", status: "completed", text: "Reply A" },
  });
  const page = await resumedClient.read({
    operation: "chat.page.v1",
    projectId: projectA.projectId,
    contextId: projectA.context.contextId,
    conversationId: projectA.context.coordinatorConversationId,
    afterSequence: DecimalSchema.parse("0"),
    limit: 10,
  });
  assert.equal(page.ok, true);
  if (page.ok && page.value.operation === "chat.page.v1") {
    assert.deepEqual(
      page.value.page.messages.map((message) => [
        message.role,
        message.deliveryState,
        message.bodyMarkdown,
      ]),
      [
        ["user", "answered", "Queued for A"],
        ["assistant", "answered", "Reply A"],
      ],
    );
  }
  await resumedClient.command(chatARequest);
  assert.equal(adapterA.sends, 1, "exact chat retry does not start another model turn");
  restarted.close();
  store.close();
});

class LifecycleAdapter implements CoordinatorAdapter {
  readonly capabilities = capabilities;
  readonly lifecycleCapabilities = {
    pause: "interrupt_known_turns" as const,
    stop: "owned_process" as const,
    continue: "saved_thread_resume" as const,
    nativeChildren: "known_active_turns" as const,
  };
  readonly #name: string;
  #listener: ((event: CoordinatorEvent) => void) | null = null;
  #descriptor: CoordinatorSessionDescriptor | null = null;
  starts = 0;
  resumes = 0;
  pauses = 0;
  stops = 0;
  continues = 0;
  sends = 0;
  lastTurnId: string | null = null;
  failNextSend = false;
  completeInsideSend = false;

  constructor(name: string) {
    this.#name = name;
  }

  start(input: CoordinatorStartInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    this.starts += 1;
    const descriptor = this.#makeDescriptor(input, "1");
    this.#descriptor = descriptor;
    return Promise.resolve({ ok: true, value: descriptor });
  }
  resume(input: CoordinatorResumeInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    if (this.#descriptor === null) return Promise.resolve(unsupported());
    this.resumes += 1;
    const descriptor = CoordinatorSessionDescriptorSchema.parse({
      ...this.#descriptor,
      coordinatorSessionId: input.coordinatorSessionId,
      projectId: input.projectId,
      contextId: input.contextId,
      conversationId: input.conversationId,
      coordinatorActorId: input.coordinatorActorId,
      hostId: input.hostId,
      profileId: input.profileId,
      cwd: input.cwd,
      nativeThreadRef: NativeRefSchema.parse({
        namespace: "fake.thread",
        value: input.nativeThreadId,
        incarnation: "2",
      }),
      processEpoch: "2",
      state: "ready",
      bootstrap: "submitted",
    });
    this.#descriptor = descriptor;
    return Promise.resolve({ ok: true, value: descriptor });
  }
  send(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    this.sends += 1;
    if (this.failNextSend) {
      this.failNextSend = false;
      return Promise.resolve({
        ok: false,
        error: {
          code: "transport_lost",
          message: "Synthetic lost response",
          retry: "after_reconcile",
        },
      });
    }
    this.lastTurnId = `turn.${this.#name}.${this.sends}`;
    if (this.completeInsideSend && this.#descriptor !== null) {
      this.emit({
        coordinatorSessionId: this.#descriptor.coordinatorSessionId,
        processEpoch: this.#descriptor.processEpoch,
        nativeThreadId: `root.${this.#name}`,
        nativeTurnId: this.lastTurnId,
        nativeItemId: "item.reply-a",
        kind: "item_completed",
        sourceEventId: "event.reply-a.synchronous",
        data: {
          id: "item.reply-a",
          type: "agentMessage",
          status: "completed",
          text: "Reply A",
        },
      });
    }
    return Promise.resolve({
      ok: true,
      value: {
        coordinatorSessionId:
          this.#descriptor?.coordinatorSessionId ?? AgentSessionIdSchema.parse("session.missing"),
        nativeThreadId: `root.${this.#name}`,
        nativeTurnId: this.lastTurnId,
        observation: "host_accepted",
        processEpoch: this.#descriptor?.processEpoch ?? "1",
      },
    });
  }
  startTurn() {
    return this.send();
  }
  steer() {
    return this.send();
  }
  pause(input: CoordinatorLifecycleInput) {
    this.pauses += 1;
    return Promise.resolve(this.#lifecycle(input, "pause", "requested", "1"));
  }
  stop(input: CoordinatorLifecycleInput) {
    this.stops += 1;
    this.emit(event(input.coordinatorSessionId, "1", "session_stopped", null, null));
    return Promise.resolve(this.#lifecycle(input, "stop", "settled", null));
  }
  continueSession(input: CoordinatorLifecycleInput) {
    this.continues += 1;
    if (this.#descriptor !== null)
      this.#descriptor = { ...this.#descriptor, processEpoch: "2", state: "ready" };
    this.emit(event(input.coordinatorSessionId, "2", "session_continued", null, null));
    return Promise.resolve(this.#lifecycle(input, "continue", "settled", "2"));
  }
  readHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return Promise.resolve(history(sessionId));
  }
  readNativeChildHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return Promise.resolve(history(sessionId));
  }
  interrupt(): Promise<AgentRuntimeResult<void>> {
    return Promise.resolve(unsupported());
  }
  respondToRequest(): Promise<AgentRuntimeResult<void>> {
    return Promise.resolve(unsupported());
  }
  restart(): Promise<AgentRuntimeResult<readonly CoordinatorSessionDescriptor[]>> {
    return Promise.resolve({ ok: true, value: [] });
  }
  subscribe(listener: (event: CoordinatorEvent) => void): () => void {
    this.#listener = listener;
    return () => {
      this.#listener = null;
    };
  }
  close(): void {
    this.#listener = null;
  }
  emit(raw: unknown): void {
    this.#listener?.(CoordinatorEventSchema.parse(raw));
  }
  #makeDescriptor(input: CoordinatorStartInput, epoch: string) {
    return CoordinatorSessionDescriptorSchema.parse({
      coordinatorSessionId: input.coordinatorSessionId,
      projectId: input.projectId,
      contextId: input.contextId,
      conversationId: input.conversationId,
      coordinatorActorId: input.coordinatorActorId,
      hostId: input.hostId,
      profileId: input.profileId,
      cwd: input.cwd,
      productId: "fake",
      role: "coordinator",
      launchOrigin: "lens",
      interactionKind: "structured",
      state: "ready",
      nativeThreadRef: NativeRefSchema.parse({
        namespace: "fake.thread",
        value: `root.${this.#name}`,
        incarnation: epoch,
      }),
      nativeSessionId: `native.${this.#name}`,
      processEpoch: epoch,
      bootstrap: "submitted",
      instructionSources: ["synthetic-test"],
      capabilities,
    });
  }
  #lifecycle(
    input: CoordinatorLifecycleInput,
    action: CoordinatorLifecycleReceipt["action"],
    observation: CoordinatorLifecycleReceipt["observation"],
    currentProcessEpoch: string | null,
  ): AgentRuntimeResult<CoordinatorLifecycleReceipt> {
    return {
      ok: true,
      value: {
        coordinatorSessionId: input.coordinatorSessionId,
        action,
        observation,
        previousProcessEpoch: input.expectedProcessEpoch,
        currentProcessEpoch,
        nativeThreadId: `root.${this.#name}`,
        targets: [],
        message: "synthetic lifecycle receipt",
      },
    };
  }
}

function registration(name: string) {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: `registration.${name}`,
    projectId: `project.${name}`,
    displayName: `Project ${name}`,
    repositoryRootRefs: [`repository.${name}`],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: `context.${name}`,
      displayName: `Context ${name}`,
      workspaceRef: `workspace.${name}`,
      branchLabel: "main",
      revisionBinding: "revision.synthetic",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: `conversation.${name}`,
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex-default",
        label: "Synthetic",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: {
      cwd: `C:\\fixtures\\project-${name}`,
      launchProfileRef: `protected-profile.${name}`,
    },
  });
}

function host(name: string, adapter: LifecycleAdapter) {
  return {
    hostId: ExecutionHostIdSchema.parse(`host.${name}`),
    profileIds: [`protected-profile.${name}`],
    openCoordinator: async () => ({ ok: true as const, value: adapter }),
  };
}
function access(projects: readonly ProjectId[]): WorkspaceAccessContext {
  return {
    principalId: PrincipalIdSchema.parse("principal.owner"),
    actorId: ActorIdSchema.parse("actor.owner"),
    clientId: ClientIdSchema.parse("client.owner"),
    authorizedProjectIds: [...projects],
  };
}
function startRequest(projectId: ProjectId, name: string) {
  return WorkspaceCommandRequestSchema.parse({
    operation: "session.start.v1",
    clientRequestId: `request.start-${name}`,
    projectId,
    contextId: WorkContextIdSchema.parse(`context.${name}`),
    interactionKind: "structured",
    profileId: "profile.codex-default",
  });
}
function chatRequest(projectId: ProjectId, name: string, requestId: string, bodyMarkdown: string) {
  return WorkspaceCommandRequestSchema.parse({
    operation: "chat.post.v1",
    clientRequestId: requestId,
    projectId,
    contextId: `context.${name}`,
    conversationId: `conversation.${name}`,
    bodyMarkdown,
    artifactRefs: [],
    correlationId: null,
    causationMessageId: null,
  });
}
function lifecycleRequest(
  operation: "project.pause.v1" | "project.stop.v1" | "project.continue.v1",
  projectId: ProjectId,
  name: string,
  sessionId: string,
  expectedRevision: string,
  clientRequestId: string,
) {
  return WorkspaceCommandRequestSchema.parse({
    operation,
    clientRequestId,
    projectId,
    contextId: `context.${name}`,
    sessionId,
    expectedRevision,
    reasonMarkdown: `Synthetic ${operation}`,
  });
}
async function execution(client: WorkspaceClientPort, projectId: ProjectId, name: string) {
  const result = await client.read({
    operation: "project.execution.get.v1",
    projectId,
    contextId: WorkContextIdSchema.parse(`context.${name}`),
  });
  assert.equal(result.ok, true);
  if (!result.ok || result.value.operation !== "project.execution.get.v1")
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-009#verification: execution fixture read failed; fix surface: repair the synthetic project execution fixture",
    );
  return result.value.execution;
}
function event(
  sessionId: string,
  processEpoch: string,
  kind: CoordinatorEvent["kind"],
  nativeThreadId: string | null,
  nativeTurnId: string | null,
) {
  return CoordinatorEventSchema.parse({
    coordinatorSessionId: sessionId,
    processEpoch,
    nativeThreadId,
    nativeTurnId,
    nativeItemId: null,
    kind,
    sourceEventId: `event.${sessionId}.${kind}.${nativeTurnId ?? "none"}`,
    data: {},
  });
}
function history(sessionId: string): AgentRuntimeResult<CoordinatorHistory> {
  return {
    ok: true,
    value: {
      coordinatorSessionId: AgentSessionIdSchema.parse(sessionId),
      nativeThreadId: "root",
      processEpoch: "1",
      status: {},
      turns: [],
    },
  };
}
function unsupported<T>(): AgentRuntimeResult<T> {
  return {
    ok: false,
    error: { code: "unsupported", message: "synthetic unsupported", retry: "never" },
  };
}
function sequentialIds() {
  let value = 0;
  return (kind: string) => `${kind}.control-${++value}`;
}
function monotonicClock() {
  let value = 0;
  return () => new Date(1_789_470_000_000 + value++);
}
