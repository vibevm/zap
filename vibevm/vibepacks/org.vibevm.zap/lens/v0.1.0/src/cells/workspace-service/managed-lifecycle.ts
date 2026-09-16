/** Managed lifecycle settlement from authoritative control events. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import type { ProjectExecutionState, WorkspaceResult } from "../workspace-model/index.ts";
import type { ManagedWakeEvent } from "./types.ts";
import { updateLaunch } from "./control.ts";
import type { Launch, LaunchActions } from "./launch.ts";

export function settleManagedLifecycle(
  actions: LaunchActions,
  event: ManagedWakeEvent,
  launch: Launch | undefined,
): WorkspaceResult<ProjectExecutionState | null> {
  if (event.action === "wake") return { ok: true, value: null };
  observeManagedSide(launch, event);
  const current = actions.store.readProjectExecution(event.projectId, event.contextId);
  if (!current.ok) return current;
  if (current.value.pendingAction?.action !== event.action || !managedSettled(event))
    return { ok: true, value: null };
  if (
    launch !== undefined &&
    current.value.sessionId !== null &&
    !coordinatorSettled(launch, event)
  )
    return { ok: true, value: null };
  const settled = actions.store.settleProjectLifecycle({
    projectId: event.projectId,
    contextId: event.contextId,
    sessionId: current.value.sessionId,
    clientRequestId: current.value.pendingAction.clientRequestId,
    action: event.action,
    observation: "settled",
    processEpoch: event.processEpoch ?? current.value.processEpoch,
    updatedAt: actions.clock().toISOString(),
  });
  if (settled.ok && launch !== undefined) {
    launch.descriptor = {
      ...launch.descriptor,
      ...(event.processEpoch === null ? {} : { processEpoch: event.processEpoch }),
      state: event.action === "pause" ? "paused" : event.action === "stop" ? "stopped" : "ready",
    };
    if (event.processEpoch !== null) launch.processEpoch = event.processEpoch;
    updateLaunch(actions, launch);
  }
  return settled;
}

function observeManagedSide(launch: Launch | undefined, event: ManagedWakeEvent): void {
  if (launch === undefined) return;
  if (event.action === "pause")
    launch.managedPauseObservation = managedSettled(event) ? "settled" : "uncertain";
  else if (event.action === "stop")
    launch.managedStopObservation = managedSettled(event) ? "settled" : "uncertain";
  else if (event.action === "continue")
    launch.managedContinueObservation = managedSettled(event) ? "settled" : "uncertain";
}

function managedSettled(event: ManagedWakeEvent): boolean {
  return (
    (event.action === "pause" && event.state === "paused") ||
    (event.action === "stop" && event.state === "stopped") ||
    (event.action === "continue" && event.state === "idle")
  );
}

function coordinatorSettled(launch: Launch, event: ManagedWakeEvent): boolean {
  return event.action === "pause"
    ? launch.coordinatorPauseSettled
    : event.action === "stop"
      ? launch.coordinatorStopSettled
      : launch.coordinatorContinueSettled;
}
