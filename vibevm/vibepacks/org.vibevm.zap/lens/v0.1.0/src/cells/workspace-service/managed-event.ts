/** Ordered managed lifecycle and wake event handling. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import type { LaunchActions } from "./launch.ts";
import { dispatchManagedWake } from "./managed-wake.ts";
import { settleManagedLifecycle } from "./managed-lifecycle.ts";
import type { ManagedWakeEvent, ManagedWakePort } from "./types.ts";

export async function handleManagedWakeEvent(
  actions: LaunchActions,
  managedWake: ManagedWakePort,
  event: ManagedWakeEvent,
): Promise<void> {
  const launch = [...actions.launches.values()].find(
    (candidate) =>
      candidate.projectId === event.projectId && candidate.contextId === event.contextId,
  );
  const settled = settleManagedLifecycle(actions, event, launch);
  if (!settled.ok) return;
  if (event.action === "continue" && settled.value?.state === "running") {
    if (launch !== undefined) await actions.dispatchQueued(launch);
    await actions.dispatchManaged(event.projectId, event.contextId);
    return;
  }
  if (!event.wakeEligible || event.state !== "idle") return;
  if (event.actorId !== null)
    await dispatchManagedWake({
      store: actions.store,
      managedWake,
      projectId: event.projectId,
      contextId: event.contextId,
      actorId: event.actorId,
      clock: actions.clock,
    });
  if (launch !== undefined) await actions.dispatchQueued(launch);
}
