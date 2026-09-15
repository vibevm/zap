/** @scope spec://org.vibevm.zap/lens/PROP-005#incremental-delivery */
/** Shared typed fixtures for WorkspaceService integration tests. */
import {
  CoordinatorEventSchema,
  CoordinatorSessionDescriptorSchema,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorCapabilities,
  type CoordinatorEvent,
  type CoordinatorHistory,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorTurnReceipt,
} from "../agent-runtime/index.ts";
import { ActorIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  NativeRefSchema,
  ProjectIdSchema,
  WorkspaceCommandRequestSchema,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";

const capabilities: CoordinatorCapabilities = {
  persistentThreads: true,
  turnStart: true,
  activeTurnSteer: false,
  turnInterrupt: true,
  historyRead: true,
  nativeChildObservation: true,
  nativeChildDirectInput: false,
  structuredUserInput: true,
  commandApproval: false,
  managedTerminal: false,
};

export class FakeAdapter implements CoordinatorAdapter {
  readonly capabilities = capabilities;
  starts = 0;
  startInputs: CoordinatorStartInput[] = [];
  resumes = 0;
  onStart: (() => void) | null = null;
  #lastDescriptor: CoordinatorSessionDescriptor | null = null;
  #listener: ((event: CoordinatorEvent) => void) | null = null;

  start(input: CoordinatorStartInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    this.starts += 1;
    this.startInputs.push(input);
    this.onStart?.();
    const descriptor = CoordinatorSessionDescriptorSchema.parse({
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
        value: `root.${input.projectId}`,
        incarnation: "1",
      }),
      nativeSessionId: `native.${input.projectId}`,
      processEpoch: "1",
      bootstrap: "submitted",
      instructionSources: ["test"],
      capabilities,
    });
    this.#lastDescriptor = descriptor;
    return Promise.resolve({ ok: true, value: descriptor });
  }

  resume(input: CoordinatorResumeInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    this.resumes += 1;
    if (this.#lastDescriptor === null) return Promise.resolve(unsupported());
    const descriptor = CoordinatorSessionDescriptorSchema.parse({
      ...this.#lastDescriptor,
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
        incarnation: "1",
      }),
      state: "ready",
      processEpoch: "2",
    });
    this.#lastDescriptor = descriptor;
    return Promise.resolve({ ok: true, value: descriptor });
  }

  readHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return Promise.resolve({
      ok: true,
      value: {
        coordinatorSessionId: AgentSessionIdSchema.parse(sessionId),
        nativeThreadId: "root",
        processEpoch: "1",
        status: {},
        turns: [],
      },
    });
  }

  readNativeChildHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return this.readHistory(sessionId);
  }

  startTurn(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(unsupported());
  }
  steer(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(unsupported());
  }
  send(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(unsupported());
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
  emit(event: unknown): void {
    this.#listener?.(CoordinatorEventSchema.parse(event));
  }
}

export function requestWithId(request: unknown, clientRequestId: string) {
  return WorkspaceCommandRequestSchema.parse({
    ...WorkspaceCommandRequestSchema.parse(request),
    clientRequestId,
  });
}

export function registration(name: string) {
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
      revisionBinding: "revision.test",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: `conversation.${name}`,
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex-default",
        label: "Fake",
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

export function access(
  name: string,
  projects: readonly string[],
  client: string,
): WorkspaceAccessContext {
  return {
    principalId: PrincipalIdSchema.parse(`principal.${name}`),
    actorId: ActorIdSchema.parse(`actor.${name}`),
    clientId: ClientIdSchema.parse(client),
    authorizedProjectIds: projects.map((project) => ProjectIdSchema.parse(project)),
  };
}

function unsupported<T>(): AgentRuntimeResult<T> {
  return {
    ok: false,
    error: {
      code: "unsupported",
      message: "test adapter operation is unsupported",
      retry: "never",
    },
  };
}
