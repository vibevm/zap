/** Wayfinder lifecycle orchestration over durable state and adapters. @scope spec://org.vibevm.zap/lens/PROP-009#project-lifecycle */
import { DecimalSchema } from "../protocol/index.ts";
import {
  AgentDescriptorSchema,
  CoordinatorSessionSchema,
  type ProjectExecutionState,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { CoordinatorEvent } from "../agent-runtime/index.ts";
import { restoreWorkspaceLaunch, type Launch, type LaunchActions } from "./launch.ts";
import { stateForAgent, stateForSession } from "./helpers.ts";
import { workspaceFailure } from "./errors.ts";

type LifecycleRequest = Extract<
  WorkspaceCommandRequest,
  { operation: "project.pause.v1" | "project.stop.v1" | "project.continue.v1" }
>;

export async function controlProject(
  actions: LaunchActions,
  access: WorkspaceAccessContext,
  request: LifecycleRequest,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const requested = actions.store.requestProjectLifecycle(access, request);
  if (!requested.ok) return requested;
  if (!requested.value.acquired) return response(request.operation, requested.value.execution);
  const managedStopObservation =
    request.operation === "project.stop.v1"
      ? await actions.stopManagedProject(access, request.projectId, request.contextId)
      : null;
  const managedPauseObservation =
    request.operation === "project.pause.v1"
      ? actions.inspectManagedPause(access, request.projectId, request.contextId)
      : null;
  let launch = actions.launches.get(request.sessionId);
  if (
    launch === undefined ||
    launch.projectId !== request.projectId ||
    launch.contextId !== request.contextId
  ) {
    if (request.operation === "project.continue.v1") {
      const restored = await restoreWorkspaceLaunch(actions, access, {
        projectId: request.projectId,
        contextId: request.contextId,
        sessionId: request.sessionId,
      });
      if (restored.ok) {
        launch = restored.value;
        const continued = settle(
          actions,
          request,
          requested.value.execution,
          "settled",
          launch.processEpoch,
        );
        if (continued.ok) {
          actions.dispatchQueued(launch);
          return response(request.operation, continued.value);
        }
        return continued;
      }
    }
    const unavailable = settle(
      actions,
      request,
      requested.value.execution,
      "uncertain",
      requested.value.execution.processEpoch,
    );
    return unavailable.ok
      ? response(request.operation, unavailable.value)
      : workspaceFailure("unavailable", "coordinator lifecycle requires session reconciliation");
  }
  const expectedEpoch = requested.value.execution.processEpoch;
  if (expectedEpoch === null || expectedEpoch !== launch.processEpoch) {
    return workspaceFailure(
      "conflict",
      "coordinator process epoch changed before lifecycle control",
    );
  }
  const method = adapterMethod(launch, request.operation);
  if (method === null) {
    const unsupported = settle(
      actions,
      request,
      requested.value.execution,
      "unsupported",
      expectedEpoch,
    );
    return unsupported.ok ? response(request.operation, unsupported.value) : unsupported;
  }

  if (request.operation === "project.continue.v1") launch.started = false;
  if (managedStopObservation !== null) launch.managedStopObservation = managedStopObservation;
  if (managedPauseObservation !== null) launch.managedPauseObservation = managedPauseObservation;
  const result = await method({
    coordinatorSessionId: request.sessionId,
    expectedProcessEpoch: expectedEpoch,
  });
  let observation = result.ok ? result.value.observation : runtimeObservation(result.error.code);
  if (
    request.operation === "project.stop.v1" &&
    launch.managedStopObservation !== "settled" &&
    observation === "settled"
  ) {
    observation = "uncertain";
  }
  if (
    request.operation === "project.pause.v1" &&
    launch.managedPauseObservation !== "settled" &&
    observation === "settled"
  ) {
    observation = launch.managedPauseObservation === "unsupported" ? "unsupported" : "uncertain";
  }
  const processEpoch = result.ok ? result.value.currentProcessEpoch : expectedEpoch;
  if (result.ok && result.value.currentProcessEpoch !== null) {
    launch.processEpoch = result.value.currentProcessEpoch;
    launch.descriptor = {
      ...launch.descriptor,
      processEpoch: result.value.currentProcessEpoch,
      state: descriptorState(request.operation, observation),
    };
  } else {
    launch.descriptor = {
      ...launch.descriptor,
      state: descriptorState(request.operation, observation),
    };
  }
  launch.started = true;
  updateLaunch(actions, launch);
  if (request.operation === "project.continue.v1") actions.drainPending(launch);
  const settled = settle(actions, request, requested.value.execution, observation, processEpoch);
  if (settled.ok) return response(request.operation, settled.value);
  const current = actions.store.readProjectExecution(request.projectId, request.contextId);
  return current.ok ? response(request.operation, current.value) : settled;
}

export function settleLifecycleEvent(
  actions: LaunchActions,
  launch: Launch,
  event: CoordinatorEvent,
): WorkspaceResult<ProjectExecutionState | null> {
  const mapped = eventSettlement(event);
  if (mapped === null) return { ok: true, value: null };
  const effective =
    mapped.action === "stop" &&
    mapped.observation === "settled" &&
    launch.managedStopObservation !== "settled"
      ? { ...mapped, observation: "uncertain" as const }
      : mapped.action === "pause" &&
          mapped.observation === "settled" &&
          launch.managedPauseObservation !== "settled"
        ? { ...mapped, observation: "uncertain" as const }
        : mapped;
  const current = actions.store.readProjectExecution(launch.projectId, launch.contextId);
  if (!current.ok) return current;
  const pending = current.value.pendingAction;
  const requestId =
    pending?.clientRequestId ??
    (mapped.action === "pause" && mapped.observation === "requested"
      ? current.value.lastAction?.clientRequestId
      : undefined);
  if (
    requestId === undefined ||
    (pending !== null && pending.action !== effective.action) ||
    (pending === null && current.value.lastAction?.action !== effective.action)
  ) {
    return { ok: true, value: current.value };
  }
  const settled = actions.store.settleProjectLifecycle({
    projectId: launch.projectId,
    contextId: launch.contextId,
    sessionId: launch.scope.coordinatorSessionId,
    clientRequestId: requestId,
    action: effective.action,
    observation: effective.observation,
    processEpoch: event.processEpoch,
    updatedAt: actions.clock().toISOString(),
  });
  if (settled.ok) {
    launch.processEpoch = event.processEpoch;
    launch.descriptor = {
      ...launch.descriptor,
      processEpoch: event.processEpoch,
      state:
        effective.action === "pause"
          ? effective.observation === "settled"
            ? "paused"
            : "pausing"
          : effective.action === "stop"
            ? effective.observation === "settled"
              ? "stopped"
              : "stopping"
            : effective.observation === "settled"
              ? "ready"
              : "stopped",
    };
    updateLaunch(actions, launch);
  }
  return settled;
}

function settle(
  actions: LaunchActions,
  request: LifecycleRequest,
  execution: ProjectExecutionState,
  observation: "requested" | "settled" | "unsupported" | "refused" | "uncertain",
  processEpoch: string | null,
) {
  const pending = execution.pendingAction;
  return pending === null
    ? workspaceFailure("conflict", "durable lifecycle action is missing")
    : actions.store.settleProjectLifecycle({
        projectId: request.projectId,
        contextId: request.contextId,
        sessionId: request.sessionId,
        clientRequestId: pending.clientRequestId,
        action: pending.action,
        observation,
        processEpoch,
        updatedAt: actions.clock().toISOString(),
      });
}

function adapterMethod(launch: Launch, operation: LifecycleRequest["operation"]) {
  if (operation === "project.pause.v1") return launch.adapter.pause?.bind(launch.adapter) ?? null;
  if (operation === "project.stop.v1") return launch.adapter.stop?.bind(launch.adapter) ?? null;
  return launch.adapter.continueSession?.bind(launch.adapter) ?? null;
}

function runtimeObservation(code: string): "unsupported" | "refused" | "uncertain" {
  if (code === "unsupported") return "unsupported";
  if (code === "host_refused" || code === "policy_denied") return "refused";
  return "uncertain";
}

function descriptorState(
  operation: LifecycleRequest["operation"],
  observation: "requested" | "settled" | "unsupported" | "refused" | "uncertain",
): Launch["descriptor"]["state"] {
  if (operation === "project.pause.v1") return observation === "settled" ? "paused" : "pausing";
  if (operation === "project.stop.v1") return observation === "settled" ? "stopped" : "stopping";
  return observation === "settled" ? "ready" : "stopped";
}

function eventSettlement(event: CoordinatorEvent): {
  action: "pause" | "stop" | "continue";
  observation: "requested" | "settled" | "uncertain";
} | null {
  if (event.kind === "session_pause_requested") {
    return { action: "pause", observation: "requested" };
  }
  if (event.kind === "session_paused") return { action: "pause", observation: "settled" };
  if (event.kind === "session_stopped") return { action: "stop", observation: "settled" };
  if (event.kind === "session_continued") return { action: "continue", observation: "settled" };
  if (event.kind !== "lifecycle_uncertain") return null;
  const action =
    typeof event.data === "object" && event.data !== null && !Array.isArray(event.data)
      ? event.data["action"]
      : null;
  return action === "pause" || action === "stop" || action === "continue"
    ? { action, observation: "uncertain" }
    : null;
}

function updateLaunch(actions: LaunchActions, launch: Launch): void {
  const now = actions.clock().toISOString();
  launch.session = CoordinatorSessionSchema.parse({
    ...launch.session,
    state: stateForSession(launch.descriptor.state),
    revision: DecimalSchema.parse(String(BigInt(launch.session.revision) + 1n)),
    updatedAt: now,
  });
  actions.store.upsertCoordinatorSession(launch.session);
  const coordinator = launch.actors.get("__coordinator__");
  if (coordinator === undefined) return;
  const updated = AgentDescriptorSchema.parse({
    ...coordinator,
    state: stateForAgent(launch.descriptor.state),
    revision: DecimalSchema.parse(String(BigInt(coordinator.revision) + 1n)),
  });
  launch.actors.set("__coordinator__", updated);
  launch.actors.set(launch.descriptor.nativeThreadRef.value, updated);
  actions.store.upsertAgent(updated);
}

function response(
  operation: LifecycleRequest["operation"],
  execution: ProjectExecutionState,
): WorkspaceResult<WorkspaceCommandResponse> {
  return { ok: true, value: { operation, execution } };
}
