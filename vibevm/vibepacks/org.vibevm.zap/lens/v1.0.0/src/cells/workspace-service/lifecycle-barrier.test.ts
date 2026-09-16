/** Coordinator and managed-worker continuation barrier. @scope spec://org.vibevm.zap/lens/PROP-012#continuation */
import assert from "node:assert/strict";
import test from "node:test";
import type {
  AgentRuntimeResult,
  CoordinatorLifecycleInput,
  CoordinatorLifecycleReceipt,
  CoordinatorTurnReceipt,
} from "../agent-runtime/index.ts";
import { ActorIdSchema, ClientRequestIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { createCoordinatorAdapterRegistry, createWorkspaceService } from "./index.ts";
import { FakeAdapter, access, registration } from "./index.test-support.ts";
import type { ManagedWakeEvent, ManagedWakePort, ManagedWakeTarget } from "./types.ts";

test("Continue waits for coordinator and managed worker before dispatching queued input", async () => {
  const opened = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const project = registration("barrier");
  assert.equal(opened.value.registerProject(project).ok, true);
  const adapter = new BarrierAdapter();
  const wake = new BarrierWake();
  const host = {
    hostId: ExecutionHostIdSchema.parse("host.barrier"),
    profileIds: ["protected-profile.barrier"],
    openCoordinator: () => Promise.resolve({ ok: true as const, value: adapter }),
  };
  const service = createWorkspaceService({
    store: opened.value,
    adapters: createCoordinatorAdapterRegistry([{ profileRef: "protected-profile.barrier", host }]),
    managedWake: wake,
  });
  const client = service.bind({
    access: access("barrier", [project.projectId], "barrier-client"),
    allowedActions: [
      "read",
      "session.start.v1",
      "chat.post.v1",
      "project.pause.v1",
      "project.continue.v1",
    ],
  });
  const started = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "session.start.v1",
      clientRequestId: "request.barrier.start",
      projectId: project.projectId,
      contextId: project.context.contextId,
      interactionKind: "structured",
      profileId: "profile.codex-default",
    }),
  );
  assert.equal(started.ok, true);
  const running = await execution(client);
  assert.notEqual(running.sessionId, null);
  if (running.sessionId === null) return;
  const paused = await client.command(
    lifecycle("project.pause.v1", running.sessionId, running.revision, "request.barrier.pause"),
  );
  assert.equal(paused.ok, true);
  if (!paused.ok || paused.value.operation !== "project.pause.v1") return;
  assert.equal(paused.value.execution.state, "paused");
  const queued = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "chat.post.v1",
      clientRequestId: "request.barrier.chat",
      projectId: project.projectId,
      contextId: project.context.contextId,
      conversationId: project.context.coordinatorConversationId,
      bodyMarkdown: "Dispatch only after both continuation sides settle.",
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    }),
  );
  assert.equal(queued.ok, true);
  const beforeContinue = await execution(client);
  const continuing = await client.command(
    lifecycle(
      "project.continue.v1",
      running.sessionId,
      beforeContinue.revision,
      "request.barrier.continue",
    ),
  );
  assert.equal(continuing.ok, true);
  assert.equal(adapter.sends, 0);
  assert.equal((await execution(client)).state, "uncertain");
  wake.settleContinue();
  await until(() => adapter.sends === 1);
  assert.equal((await execution(client)).state, "running");
  assert.equal(wake.continues, 1);
  service.close();
  opened.value.close();
});

class BarrierAdapter extends FakeAdapter {
  readonly lifecycleCapabilities = {
    pause: "interrupt_known_turns" as const,
    stop: "owned_process" as const,
    continue: "saved_thread_resume" as const,
    nativeChildren: "unsupported" as const,
  };
  sends = 0;
  pause(input: CoordinatorLifecycleInput) {
    this.emit(lifecycleEvent(input.coordinatorSessionId, "1", "session_paused"));
    return Promise.resolve(receipt(input, "pause", null));
  }
  continueSession(input: CoordinatorLifecycleInput) {
    this.emit(lifecycleEvent(input.coordinatorSessionId, "2", "session_continued"));
    return Promise.resolve(receipt(input, "continue", "2"));
  }
  override send(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    this.sends += 1;
    return Promise.resolve({
      ok: true,
      value: {
        coordinatorSessionId: AgentSessionIdSchema.parse("session.service-1"),
        nativeThreadId: "root.project.barrier",
        nativeTurnId: `turn.barrier.${this.sends}`,
        observation: "host_accepted",
        processEpoch: "2",
      },
    });
  }
}

class BarrierWake implements ManagedWakePort {
  continues = 0;
  #listener: ((event: ManagedWakeEvent) => void) | null = null;
  pauseOwned() {
    return Promise.resolve("settled" as const);
  }
  stopOwned() {
    return Promise.resolve("settled" as const);
  }
  continueOwned() {
    this.continues += 1;
    return Promise.resolve("uncertain" as const);
  }
  resolveRecipient(): Promise<ManagedWakeTarget | null> {
    return Promise.resolve(null);
  }
  offerWake() {
    return Promise.resolve("refused" as const);
  }
  subscribe(listener: (event: ManagedWakeEvent) => void) {
    this.#listener = listener;
    return () => {
      this.#listener = null;
    };
  }
  settleContinue() {
    this.#listener?.({
      projectId: ProjectIdSchema.parse("project.barrier"),
      contextId: WorkContextIdSchema.parse("context.barrier"),
      actorId: ActorIdSchema.parse("actor.barrier.worker"),
      state: "idle",
      wakeEligible: false,
      action: "continue",
      processEpoch: "process.barrier.worker.2",
      leaseId: "lease.barrier.worker.2",
    });
  }
}

function lifecycle(
  operation: "project.pause.v1" | "project.continue.v1",
  sessionId: string,
  expectedRevision: string,
  clientRequestId: string,
) {
  return WorkspaceCommandRequestSchema.parse({
    operation,
    clientRequestId: ClientRequestIdSchema.parse(clientRequestId),
    projectId: ProjectIdSchema.parse("project.barrier"),
    contextId: WorkContextIdSchema.parse("context.barrier"),
    sessionId,
    expectedRevision,
    reasonMarkdown: operation,
  });
}

function lifecycleEvent(sessionId: string, processEpoch: string, kind: string) {
  return {
    coordinatorSessionId: sessionId,
    processEpoch,
    nativeThreadId: "root.project.barrier",
    nativeTurnId: null,
    nativeItemId: null,
    kind,
    sourceEventId: `event.barrier.${kind}.${processEpoch}`,
    data: {},
  };
}

function receipt(
  input: CoordinatorLifecycleInput,
  action: "pause" | "continue",
  currentProcessEpoch: string | null,
): AgentRuntimeResult<CoordinatorLifecycleReceipt> {
  return {
    ok: true,
    value: {
      coordinatorSessionId: input.coordinatorSessionId,
      action,
      observation: "settled",
      previousProcessEpoch: input.expectedProcessEpoch,
      currentProcessEpoch,
      nativeThreadId: "root.project.barrier",
      targets: [],
      message: "synthetic barrier",
    },
  };
}

async function execution(client: ReturnType<ReturnType<typeof createWorkspaceService>["bind"]>) {
  const value = await client.read({
    operation: "project.execution.get.v1",
    projectId: ProjectIdSchema.parse("project.barrier"),
    contextId: WorkContextIdSchema.parse("context.barrier"),
  });
  if (!value.ok || value.value.operation !== "project.execution.get.v1") throw new Error();
  return value.value.execution;
}

async function until(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setImmediate(resolve));
  }
  throw new Error();
}
