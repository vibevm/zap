/** Coordinator launch projections. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import {
  AgentDescriptorSchema,
  CoordinatorSessionSchema,
  NativeRefSchema,
  type AgentDescriptor,
  type CoordinatorSession,
  type ProjectId,
  type WorkContextId,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import {
  CoordinatorSessionDescriptorSchema,
  type CoordinatorAdapter,
  type CoordinatorScope,
  type CoordinatorSessionDescriptor,
} from "../agent-runtime/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { workspaceFailure } from "./errors.ts";
import { actionAvailable, stateForAgent, stateForSession } from "./helpers.ts";
import type { Launch } from "./launch.ts";

export function coordinatorInitialization(projectName: string, workspaceRef: string): string {
  return [
    `Coordinator initialization for ${projectName}. Work context: ${workspaceRef}.`,
    "Identify yourself as the long-lived project coordinator.",
    "Read AGENTS.md and its complete applicable project boot and local context before work.",
    "After initialization, wait for the actual user goal or queued message.",
    "Do not execute a discovered legacy plan merely because startup found it.",
  ].join("\n");
}

export function pending(
  request: Extract<WorkspaceCommandRequest, { operation: "session.start.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  return {
    ok: true,
    value: {
      operation: request.operation,
      action: {
        requestId: request.clientRequestId,
        state: "pending",
        availability: { state: "available" },
      },
    },
  };
}

export function requestResult(
  result: WorkspaceResult<WorkspaceCommandResponse>,
  requestId: WorkspaceCommandRequest["clientRequestId"],
): WorkspaceResult<WorkspaceCommandResponse> {
  return !result.ok || result.value.operation !== "session.start.v1"
    ? result
    : { ok: true, value: { ...result.value, action: { ...result.value.action, requestId } } };
}

export function matchesScope(
  descriptor: CoordinatorSessionDescriptor,
  scope: CoordinatorScope,
): boolean {
  return (
    descriptor.coordinatorSessionId === scope.coordinatorSessionId &&
    descriptor.projectId === scope.projectId &&
    descriptor.contextId === scope.contextId &&
    descriptor.conversationId === scope.conversationId &&
    descriptor.coordinatorActorId === scope.coordinatorActorId &&
    descriptor.hostId === scope.hostId
  );
}

export function provisionalLaunch(
  key: string,
  scope: CoordinatorScope,
  profileId: string,
  adapter: CoordinatorAdapter,
  clock: () => Date,
  agentBinding: Launch["agentBinding"],
): Launch {
  const descriptor = CoordinatorSessionDescriptorSchema.parse({
    ...scope,
    profileId,
    productId: "host",
    role: "coordinator",
    launchOrigin: "lens",
    interactionKind: "structured",
    state: "bootstrapping",
    nativeThreadRef: NativeRefSchema.parse({
      namespace: "native.thread",
      value: `pending.${scope.coordinatorSessionId}`,
      incarnation: "1",
    }),
    nativeSessionId: `pending.${scope.coordinatorSessionId}`,
    cwd: "pending",
    processEpoch: "1",
    bootstrap: "not_observed",
    instructionSources: ["lens.workspace-service.v1"],
    capabilities: adapter.capabilities,
  });
  return {
    key,
    projectId: scope.projectId,
    contextId: scope.contextId,
    profileId,
    adapter,
    scope,
    coordinatorActorId: scope.coordinatorActorId,
    processEpoch: "1",
    descriptor,
    session: session(descriptor, "structured", clock),
    started: false,
    actors: new Map(),
    pending: [],
    pendingChatReplies: [],
    managedStopObservation: "not_requested",
    managedPauseObservation: "not_requested",
    agentBinding,
  };
}

export function session(
  descriptor: CoordinatorSessionDescriptor,
  interactionKind: CoordinatorSession["interactionKind"],
  clock: () => Date,
): CoordinatorSession {
  const now = clock().toISOString();
  return CoordinatorSessionSchema.parse({
    sessionId: descriptor.coordinatorSessionId,
    projectId: descriptor.projectId,
    contextId: descriptor.contextId,
    conversationId: descriptor.conversationId,
    coordinatorActorId: descriptor.coordinatorActorId,
    role: "coordinator",
    launchOrigin: "lens",
    interactionKind,
    hostId: descriptor.hostId,
    nativeRef: descriptor.nativeThreadRef,
    terminal: { state: "unavailable", reason: "native_session" },
    state: stateForSession(descriptor.state),
    actions: {
      interrupt: actionAvailable(descriptor.capabilities.turnInterrupt),
      startTurn: actionAvailable(descriptor.capabilities.turnStart),
    },
    bootstrapBasis: descriptor.bootstrap === "submitted" ? "lens.workspace-service.v1" : null,
    revision: DecimalSchema.parse("1"),
    createdAt: now,
    updatedAt: now,
  });
}

export function coordinatorActor(launch: Launch): AgentDescriptor {
  const actor = AgentDescriptorSchema.parse({
    actorId: launch.coordinatorActorId,
    sessionId: launch.scope.coordinatorSessionId,
    projectId: launch.projectId,
    contextId: launch.contextId,
    role: "coordinator",
    parentActorId: null,
    displayName: "Coordinator",
    executionMode: "native",
    hostId: launch.scope.hostId,
    nativeRef: launch.descriptor.nativeThreadRef,
    state: stateForAgent(launch.descriptor.state),
    revision: DecimalSchema.parse("1"),
  });
  launch.actors.set(launch.descriptor.nativeThreadRef.value, actor);
  launch.actors.set("__coordinator__", actor);
  return actor;
}

export function restoreActors(
  store: WorkspaceStore,
  access: WorkspaceAccessContext,
  projectId: ProjectId,
  contextId: WorkContextId,
  launch: Launch,
): WorkspaceResult<null> {
  const listed = store.read(access, { operation: "agent.list.v1", projectId, contextId });
  if (!listed.ok) return listed;
  if (listed.value.operation !== "agent.list.v1")
    return workspaceFailure("storage_failure", "agent projection changed during resume");
  for (const actor of listed.value.agents) {
    if (actor.nativeRef !== null) launch.actors.set(actor.nativeRef.value, actor);
    if (actor.actorId === launch.coordinatorActorId) launch.actors.set("__coordinator__", actor);
  }
  return { ok: true, value: null };
}
