/** Managed-worker wake delivery over the durable addressed queue. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import type { ProjectId, WorkContextId, WorkspaceResult } from "../workspace-model/index.ts";
import type { ManagedWakeNotice, WorkspaceStore } from "../workspace-store/index.ts";
import type { ManagedWakePort } from "./types.ts";

export async function dispatchManagedWake(input: {
  readonly store: WorkspaceStore;
  readonly managedWake: ManagedWakePort;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly actorId: ManagedWakeNotice["actorId"];
  readonly clock?: () => Date;
}): Promise<WorkspaceResult<ManagedWakeNotice | null>> {
  const execution = input.store.readProjectExecution(input.projectId, input.contextId);
  if (!execution.ok) return execution;
  if (execution.value.state !== "running" && execution.value.state !== "uninitialized")
    return { ok: true, value: null };
  const next = input.store.nextManagedWake(input.projectId, input.contextId, input.actorId);
  if (!next.ok || next.value === null) return next;
  const target = await input.managedWake.resolveRecipient({
    projectId: input.projectId,
    contextId: input.contextId,
    actorId: input.actorId,
  });
  if (
    target === null ||
    target.actorId !== next.value.actorId ||
    target.runId !== next.value.runId ||
    target.attemptId !== next.value.attemptId ||
    target.adapterSessionId !== next.value.adapterSessionId
  )
    return { ok: true, value: null };
  if (target.processEpoch === null || target.leaseId === null) return { ok: true, value: null };
  const claim = input.store.claimManagedWake({
    wakeId: next.value.wakeId,
    projectId: next.value.projectId,
    contextId: next.value.contextId,
    actorId: next.value.actorId,
    runId: next.value.runId,
    attemptId: next.value.attemptId,
    adapterSessionId: next.value.adapterSessionId,
    processEpoch: target.processEpoch,
    leaseId: target.leaseId,
  });
  if (!claim.ok) return claim;
  const observation = await input.managedWake.offerWake({
    notice: claim.value,
    leaseId: claim.value.leaseId,
  });
  if (observation === "busy") {
    const released = input.store.releaseManagedWake({
      wakeId: claim.value.wakeId,
      projectId: claim.value.projectId,
      contextId: claim.value.contextId,
      actorId: claim.value.actorId,
      runId: claim.value.runId,
      attemptId: claim.value.attemptId,
      adapterSessionId: claim.value.adapterSessionId,
      processEpoch: claim.value.processEpoch,
      leaseId: claim.value.leaseId,
    });
    return released.ok ? { ok: true, value: released.value } : released;
  }
  const settled = input.store.settleManagedWake({
    wakeId: claim.value.wakeId,
    projectId: claim.value.projectId,
    contextId: claim.value.contextId,
    actorId: claim.value.actorId,
    runId: claim.value.runId,
    attemptId: claim.value.attemptId,
    adapterSessionId: claim.value.adapterSessionId,
    processEpoch: claim.value.processEpoch,
    leaseId: claim.value.leaseId,
    observation:
      observation === "host_accepted"
        ? "host_accepted"
        : observation === "uncertain"
          ? "uncertain"
          : "failed",
    updatedAt: (input.clock ?? (() => new Date()))().toISOString(),
  });
  return settled;
}

export async function dispatchManagedProject(input: {
  readonly store: WorkspaceStore;
  readonly managedWake: ManagedWakePort;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly clock?: () => Date;
}): Promise<void> {
  const actors = input.store.queuedManagedWakeActors(input.projectId, input.contextId);
  if (!actors.ok) return;
  for (const actorId of actors.value) await dispatchManagedWake({ ...input, actorId });
}
