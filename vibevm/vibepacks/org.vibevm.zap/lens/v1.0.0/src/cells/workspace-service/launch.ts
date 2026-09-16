/** @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  RunIdSchema,
  type AgentDescriptor,
  type CoordinatorSession,
  type ProjectId,
  type WorkContextId,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { ActorIdSchema, type ActorId } from "../protocol/index.ts";
import type { ReasoningEffort } from "../model-policy/index.ts";
import {
  type CoordinatorAdapter,
  type CoordinatorScope,
  type CoordinatorSessionDescriptor,
  type CoordinatorEvent,
} from "../agent-runtime/index.ts";
import type { TrustedProjectLaunch, WorkspaceStore } from "../workspace-store/index.ts";
import type { CoordinatorAdapterRegistry } from "./types.ts";
import type { CoordinatorRoutingBridge } from "./types.ts";
import type { OwnedCoordinatorAgentBinding, OwnedCoordinatorAgentPort } from "./types.ts";
import { runtimeFailure, workspaceFailure } from "./errors.ts";
import {
  coordinatorActor,
  coordinatorInitialization,
  matchesScope,
  pending,
  provisionalLaunch,
  requestResult,
  restoreActors,
  session,
} from "./launch-model.ts";

export interface Launch {
  readonly key: string;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly profileId: string;
  readonly adapter: CoordinatorAdapter;
  readonly scope: CoordinatorScope;
  readonly coordinatorActorId: string;
  processEpoch: string;
  descriptor: CoordinatorSessionDescriptor;
  session: CoordinatorSession;
  started: boolean;
  readonly actors: Map<string, AgentDescriptor>;
  readonly pending: CoordinatorEvent[];
  readonly pendingChatReplies: CoordinatorEvent[];
  managedStopObservation: "not_requested" | "settled" | "uncertain";
  managedPauseObservation: "not_requested" | "settled" | "unsupported" | "uncertain";
  managedContinueObservation: "not_requested" | "settled" | "unsupported" | "uncertain";
  coordinatorPauseSettled: boolean;
  coordinatorStopSettled: boolean;
  coordinatorContinueSettled: boolean;
  readonly agentBinding: OwnedCoordinatorAgentBinding | null;
}

export interface LaunchActions {
  readonly store: WorkspaceStore;
  readonly adapters: CoordinatorAdapterRegistry;
  readonly clock: () => Date;
  readonly idFactory: (kind: string) => string;
  readonly launches: Map<string, Launch>;
  readonly starting: Map<string, Promise<WorkspaceResult<WorkspaceCommandResponse>>>;
  readonly startRequests: Map<
    string,
    { readonly digest: string; readonly result: WorkspaceResult<WorkspaceCommandResponse> }
  >;
  readonly ensureSubscription: (adapter: CoordinatorAdapter) => void;
  readonly drainPending: (launch: Launch) => void;
  readonly drainChatReplies: (launch: Launch) => void;
  readonly dispatchQueued: (launch: Launch) => Promise<void>;
  readonly dispatchManaged: (projectId: ProjectId, contextId: WorkContextId) => Promise<void>;
  readonly coordinatorRouting?: CoordinatorRoutingBridge | undefined;
  readonly stopManagedProject: (
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ) => Promise<"settled" | "uncertain">;
  readonly continueManagedProject: (
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ) => Promise<"settled" | "unsupported" | "uncertain">;
  readonly inspectManagedPause: (
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ) => Promise<"settled" | "unsupported" | "uncertain">;
  readonly ownedCoordinatorAgents?: OwnedCoordinatorAgentPort | undefined;
}

export async function startWorkspace(
  actions: LaunchActions,
  access: WorkspaceAccessContext,
  request: Extract<WorkspaceCommandRequest, { operation: "session.start.v1" }>,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const key = `${request.projectId}/${request.contextId}`;
  const scopeCheck = validateStartScope(actions.store, access, request);
  if (!scopeCheck.ok) return scopeCheck;
  const identity = `${access.principalId}/${access.actorId ?? `client:${access.clientId}`}/${request.projectId}/${request.contextId}/${request.clientRequestId}`;
  const digest = JSON.stringify(request);
  const previous = actions.startRequests.get(identity);
  if (previous !== undefined)
    return previous.digest === digest
      ? previous.result
      : workspaceFailure(
          "idempotency_conflict",
          "client request identity was reused with new content",
        );
  const current = actions.starting.get(key);
  if (current !== undefined) return requestResult(await current, request.clientRequestId);
  const work = startOnce(actions, access, request, key);
  actions.starting.set(key, work);
  try {
    const result = await work;
    if (result.ok) actions.startRequests.set(identity, { digest, result });
    return result;
  } finally {
    actions.starting.delete(key);
  }
}

export async function restoreWorkspaceLaunch(
  actions: LaunchActions,
  access: WorkspaceAccessContext,
  input: {
    projectId: ProjectId;
    contextId: WorkContextId;
    sessionId: ReturnType<typeof AgentSessionIdSchema.parse>;
  },
): Promise<WorkspaceResult<Launch>> {
  const claim = actions.store.readCoordinatorClaim(input.projectId, input.contextId);
  if (!claim.ok) return claim;
  const persisted = claim.value;
  if (
    persisted === null ||
    persisted.sessionId !== input.sessionId ||
    persisted.actorId === null ||
    persisted.nativeThreadId === null
  ) {
    return workspaceFailure("unavailable", "saved coordinator identity is incomplete");
  }
  const detail = actions.store.read(access, {
    operation: "project.get.v1",
    projectId: input.projectId,
  });
  if (!detail.ok) return detail;
  if (detail.value.operation !== "project.get.v1") {
    return workspaceFailure("storage_failure", "project detail projection changed");
  }
  const context = detail.value.detail.contexts.find(
    (candidate) => candidate.contextId === input.contextId,
  );
  if (context === undefined) return workspaceFailure("not_found", "work context does not exist");
  const protectedLaunch = actions.store.resolveProjectLaunch(input.projectId, input.contextId);
  if (!protectedLaunch.ok) return protectedLaunch;
  let selectedProfileId = persisted.profileId;
  let selectedModelId: string | null = null;
  let selectedEffort: ReasoningEffort | null = null;
  if (actions.coordinatorRouting !== undefined) {
    const routed = await actions.coordinatorRouting.resume(access, {
      projectId: input.projectId,
      contextId: input.contextId,
      sessionId: input.sessionId,
      runId: RunIdSchema.parse(input.sessionId),
      attemptId: AttemptIdSchema.parse(`${input.sessionId}.attempt`),
      explicitProfileId: persisted.profileId,
    });
    if (!routed.ok) return workspaceFailure("unavailable", routed.error.message);
    selectedProfileId = routed.value.parameters.profileId;
    selectedModelId = routed.value.parameters.modelId;
    selectedEffort = routed.value.parameters.effectiveEffort;
  }
  const adapter = await actions.adapters.resolve(
    actions.coordinatorRouting === undefined
      ? protectedLaunch.value.launchProfileRef
      : selectedProfileId,
  );
  if (!adapter.ok) return runtimeFailure(adapter.error.code, adapter.error.message);
  const scope: CoordinatorScope = {
    coordinatorSessionId: input.sessionId,
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: context.coordinatorConversationId,
    coordinatorActorId: ActorIdSchema.parse(persisted.actorId),
    hostId: adapter.value.hostId,
  };
  const agentBinding = await bindOwnedCoordinator(
    actions,
    input.projectId,
    input.contextId,
    input.sessionId,
    scope.coordinatorActorId,
    protectedLaunch.value.agentScope,
  );
  if (!agentBinding.ok) return agentBinding;
  const launch = provisionalLaunch(
    `${input.projectId}/${input.contextId}`,
    scope,
    selectedProfileId,
    adapter.value.adapter,
    actions.clock,
    agentBinding.value,
  );
  const actors = restoreActors(actions.store, access, input.projectId, input.contextId, launch);
  if (!actors.ok) return actors;
  actions.ensureSubscription(adapter.value.adapter);
  actions.launches.set(input.sessionId, launch);
  const resumed = await adapter.value.adapter.resume({
    ...scope,
    profileId: selectedProfileId,
    cwd: protectedLaunch.value.cwd,
    nativeThreadId: persisted.nativeThreadId,
    agentScope: protectedLaunch.value.agentScope,
    agentBinding: agentBinding.value,
    ...(selectedModelId === null ? {} : { modelId: selectedModelId }),
    ...(selectedEffort === null ? {} : { reasoningEffort: selectedEffort }),
  });
  if (!resumed.ok || !matchesScope(resumed.value, scope)) {
    actions.launches.delete(input.sessionId);
    return !resumed.ok
      ? runtimeFailure(resumed.error.code, resumed.error.message)
      : workspaceFailure("unavailable", "resumed coordinator returned an out-of-scope session");
  }
  launch.descriptor = resumed.value;
  launch.processEpoch = resumed.value.processEpoch;
  launch.session = session(resumed.value, "structured", actions.clock);
  launch.started = true;
  const savedSession = actions.store.upsertCoordinatorSession(launch.session);
  if (!savedSession.ok) return savedSession;
  const savedActor = actions.store.upsertAgent(
    launch.actors.get("__coordinator__") ?? coordinatorActor(launch),
  );
  if (!savedActor.ok) return savedActor;
  actions.drainPending(launch);
  return { ok: true, value: launch };
}

function validateStartScope(
  store: WorkspaceStore,
  access: WorkspaceAccessContext,
  request: Extract<WorkspaceCommandRequest, { operation: "session.start.v1" }>,
): WorkspaceResult<null> {
  const detail = store.read(access, { operation: "project.get.v1", projectId: request.projectId });
  if (!detail.ok) return detail;
  if (detail.value.operation !== "project.get.v1")
    return workspaceFailure("storage_failure", "project detail projection changed");
  if (detail.value.detail.contexts.every((context) => context.contextId !== request.contextId))
    return workspaceFailure("not_found", "requested work context does not exist");
  const option = detail.value.detail.coordinatorLaunchOptions.find(
    (candidate) =>
      candidate.profileId === request.profileId &&
      candidate.interactionKind === request.interactionKind,
  );
  if (option === undefined)
    return workspaceFailure(
      "forbidden",
      "profile and interaction mode are not registered for this project",
    );
  if (request.interactionKind !== "structured")
    return workspaceFailure(
      "unsupported_operation",
      "the registered coordinator adapter only supports structured sessions",
    );
  return option.availability.state === "available"
    ? { ok: true, value: null }
    : workspaceFailure("unavailable", option.availability.reason);
}

async function startOnce(
  actions: LaunchActions,
  access: WorkspaceAccessContext,
  request: Extract<WorkspaceCommandRequest, { operation: "session.start.v1" }>,
  key: string,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const detailResult = actions.store.read(access, {
    operation: "project.get.v1",
    projectId: request.projectId,
  });
  if (!detailResult.ok) return detailResult;
  if (detailResult.value.operation !== "project.get.v1")
    return workspaceFailure("storage_failure", "project detail projection changed");
  const detail = detailResult.value.detail;
  const execution = actions.store.readProjectExecution(request.projectId, request.contextId);
  if (!execution.ok) return execution;
  if (execution.value.state !== "uninitialized" && execution.value.state !== "running") {
    return workspaceFailure(
      "conflict",
      "paused, stopped or uncertain project execution requires explicit Continue or reconciliation",
    );
  }
  const startAvailability = detail.project.actions["startCoordinator"];
  if (startAvailability?.state === "unavailable")
    return workspaceFailure("forbidden", startAvailability.reason);
  const context = detail.contexts.find((candidate) => candidate.contextId === request.contextId);
  if (context === undefined)
    return workspaceFailure("not_found", "requested work context does not exist");
  const option = detail.coordinatorLaunchOptions.find(
    (candidate) =>
      candidate.profileId === request.profileId &&
      candidate.interactionKind === request.interactionKind,
  );
  if (option === undefined)
    return workspaceFailure(
      "forbidden",
      "profile and interaction mode are not registered for this project",
    );
  if (request.interactionKind !== "structured")
    return workspaceFailure(
      "unsupported_operation",
      "the registered coordinator adapter only supports structured sessions",
    );
  if (option.availability.state !== "available")
    return workspaceFailure("unavailable", option.availability.reason);
  if (
    detail.coordinator !== null &&
    actions.launches.has(detail.coordinator.sessionId) &&
    ["starting", "bootstrapping", "ready", "running", "waiting_for_user"].includes(
      detail.coordinator.state,
    )
  )
    return pending(request);
  const launch = actions.store.resolveProjectLaunch(request.projectId, request.contextId);
  if (!launch.ok) return launch;
  const claim = actions.store.claimCoordinatorLaunch({
    projectId: request.projectId,
    contextId: request.contextId,
    claimId: request.clientRequestId,
    principalId: access.principalId,
    actorKey: access.actorId ?? `client:${access.clientId}`,
    clientRequestId: request.clientRequestId,
    profileId: request.profileId,
    interactionKind: "structured",
    updatedAt: actions.clock().toISOString(),
  });
  if (!claim.ok) return claim;
  const resuming = !claim.value.acquired && claim.value.claim.state === "running";
  if (!claim.value.acquired && !resuming)
    return workspaceFailure(
      "unavailable",
      "coordinator launch needs reconciliation before another host start",
    );
  const durableClaimId = claim.value.claim.claimId;
  if (
    resuming &&
    (claim.value.claim.sessionId === null ||
      claim.value.claim.actorId === null ||
      claim.value.claim.nativeThreadId === null)
  )
    return workspaceFailure("unavailable", "running coordinator claim has no native receipt");
  let sessionId: ReturnType<typeof AgentSessionIdSchema.parse>;
  let actorId: ReturnType<typeof ActorIdSchema.parse>;
  let nativeThreadId: string | null = null;
  if (resuming) {
    const persisted = claim.value.claim;
    if (
      persisted.sessionId === null ||
      persisted.actorId === null ||
      persisted.nativeThreadId === null
    )
      return workspaceFailure("unavailable", "running coordinator claim has no native receipt");
    sessionId = AgentSessionIdSchema.parse(persisted.sessionId);
    actorId = ActorIdSchema.parse(persisted.actorId);
    nativeThreadId = persisted.nativeThreadId;
  } else {
    sessionId = AgentSessionIdSchema.parse(actions.idFactory("session"));
    actorId = ActorIdSchema.parse(actions.idFactory("actor"));
  }
  const runId = RunIdSchema.parse(sessionId);
  const attemptId = AttemptIdSchema.parse(`${sessionId}.attempt`);
  let selectedProfileId = request.profileId;
  let adapterProfileRef = launch.value.launchProfileRef;
  let selectedModelId: string | null = null;
  let selectedEffort: ReasoningEffort | null = null;
  if (actions.coordinatorRouting !== undefined) {
    const routed = resuming
      ? await actions.coordinatorRouting.resume(access, {
          projectId: request.projectId,
          contextId: request.contextId,
          sessionId,
          runId,
          attemptId,
          explicitProfileId: request.profileId,
        })
      : await actions.coordinatorRouting.resolve(access, {
          projectId: request.projectId,
          contextId: request.contextId,
          sessionId,
          runId,
          attemptId,
          clientRequestId: request.clientRequestId,
          sourceEventId: `session.start:${request.clientRequestId}`,
          explicitProfileId: request.profileId,
        });
    if (!routed.ok) {
      actions.store.markCoordinatorClaimState(
        request.projectId,
        request.contextId,
        durableClaimId,
        "failed",
        actions.clock().toISOString(),
      );
      return workspaceFailure("unavailable", routed.error.message);
    }
    selectedProfileId = routed.value.parameters.profileId;
    adapterProfileRef = routed.value.parameters.profileId;
    selectedModelId = routed.value.parameters.modelId;
    selectedEffort = routed.value.parameters.effectiveEffort;
  }
  const adapter = await actions.adapters.resolve(adapterProfileRef);
  if (!adapter.ok) return runtimeFailure(adapter.error.code, adapter.error.message);
  const scope: CoordinatorScope = {
    coordinatorSessionId: sessionId,
    projectId: request.projectId,
    contextId: request.contextId,
    conversationId: context.coordinatorConversationId,
    coordinatorActorId: actorId,
    hostId: adapter.value.hostId,
  };
  const agentBinding = await bindOwnedCoordinator(
    actions,
    request.projectId,
    request.contextId,
    sessionId,
    scope.coordinatorActorId,
    launch.value.agentScope,
  );
  if (!agentBinding.ok) return agentBinding;
  const opened = {
    ...scope,
    profileId: selectedProfileId,
    cwd: launch.value.cwd,
    bootstrapText: coordinatorInitialization(detail.project.displayName, context.workspaceRef),
    bootstrapBasis: "lens.workspace-service.v1",
    agentScope: launch.value.agentScope,
    agentBinding: agentBinding.value,
    ...(selectedModelId === null ? {} : { modelId: selectedModelId }),
    ...(selectedEffort === null ? {} : { reasoningEffort: selectedEffort }),
  };
  actions.ensureSubscription(adapter.value.adapter);
  const provisional = provisionalLaunch(
    key,
    scope,
    selectedProfileId,
    adapter.value.adapter,
    actions.clock,
    agentBinding.value,
  );
  if (resuming) {
    const restored = restoreActors(
      actions.store,
      access,
      request.projectId,
      request.contextId,
      provisional,
    );
    if (!restored.ok) return restored;
  }
  actions.launches.set(sessionId, provisional);
  let started;
  if (resuming) {
    if (nativeThreadId === null)
      return workspaceFailure("unavailable", "running coordinator claim has no native receipt");
    started = await adapter.value.adapter.resume({
      ...scope,
      profileId: selectedProfileId,
      cwd: launch.value.cwd,
      nativeThreadId,
      agentScope: launch.value.agentScope,
      agentBinding: agentBinding.value,
      ...(selectedModelId === null ? {} : { modelId: selectedModelId }),
      ...(selectedEffort === null ? {} : { reasoningEffort: selectedEffort }),
    });
  } else started = await adapter.value.adapter.start(opened);
  if (!started.ok) {
    actions.launches.delete(sessionId);
    actions.store.markCoordinatorClaimState(
      request.projectId,
      request.contextId,
      durableClaimId,
      "needs_reconcile",
      actions.clock().toISOString(),
    );
    return runtimeFailure(started.error.code, started.error.message);
  }
  if (!matchesScope(started.value, scope)) {
    actions.launches.delete(sessionId);
    actions.store.markCoordinatorClaimState(
      request.projectId,
      request.contextId,
      durableClaimId,
      "needs_reconcile",
      actions.clock().toISOString(),
    );
    return workspaceFailure("unavailable", "coordinator host returned an out-of-scope session");
  }
  provisional.descriptor = started.value;
  provisional.processEpoch = started.value.processEpoch;
  provisional.session = session(started.value, request.interactionKind, actions.clock);
  provisional.started = true;
  const receipt = actions.store.recordCoordinatorReceipt({
    projectId: request.projectId,
    contextId: request.contextId,
    claimId: durableClaimId,
    sessionId: started.value.coordinatorSessionId,
    actorId: started.value.coordinatorActorId,
    nativeThreadId: started.value.nativeThreadRef.value,
    processEpoch: started.value.processEpoch,
    updatedAt: actions.clock().toISOString(),
  });
  if (!receipt.ok) return receipt;
  const savedSession = actions.store.upsertCoordinatorSession(provisional.session);
  if (!savedSession.ok) return savedSession;
  const savedActor = actions.store.upsertAgent(
    provisional.actors.get("__coordinator__") ?? coordinatorActor(provisional),
  );
  if (!savedActor.ok) return savedActor;
  actions.drainPending(provisional);
  return pending(request);
}

async function bindOwnedCoordinator(
  actions: LaunchActions,
  projectId: ProjectId,
  contextId: WorkContextId,
  coordinatorSessionId: ReturnType<typeof AgentSessionIdSchema.parse>,
  coordinatorActorId: ActorId,
  agentScope: TrustedProjectLaunch["agentScope"],
) {
  if (actions.ownedCoordinatorAgents === undefined || agentScope === null)
    return { ok: true as const, value: null };
  return actions.ownedCoordinatorAgents.bind({
    projectId,
    contextId,
    coordinatorSessionId,
    coordinatorActorId,
    workspaceId: agentScope.workspaceId,
    conversationId: agentScope.conversationId,
  });
}
