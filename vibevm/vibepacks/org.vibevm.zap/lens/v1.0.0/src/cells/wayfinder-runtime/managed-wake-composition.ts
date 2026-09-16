/** Wayfinder composition for owned managed wake/lifecycle control. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import { ActorIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  RunIdSchema,
  TerminalIdSchema,
  WorkspaceAccessContextSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import type {
  ManagedAgentBackend,
  ManagedSessionControlEvent,
  ManagedWorkClaim,
} from "../managed-work/index.ts";
import type {
  ManagedWakeEvent,
  ManagedWakePort,
  ManagedWakeTarget,
} from "../workspace-service/index.ts";

type LifecycleAction = "pause" | "stop" | "continue";
type Scope = { readonly projectId: ProjectId; readonly contextId: WorkContextId };

export function createWayfinderManagedWake(input: {
  readonly backend: ManagedAgentBackend;
  readonly projects: readonly Scope[] | (() => readonly Scope[]);
}): ManagedWakePort {
  const listeners = new Set<(event: ManagedWakeEvent) => void>();
  const pending = new Map<string, LifecycleAction>();
  const settlementWaiters = new Map<string, Set<() => void>>();
  let unsubscribeControl: (() => void) | null = null;
  let eventQueue = Promise.resolve();
  const scopes = () => (typeof input.projects === "function" ? input.projects() : input.projects);
  const accessFor = (projectId: ProjectId) =>
    WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.wayfinder.managed"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.wayfinder.managed"),
      authorizedProjectIds: [projectId],
    });
  const list = (projectId: ProjectId, contextId: WorkContextId) =>
    input.backend.list(accessFor(projectId), projectId, contextId);

  const port: ManagedWakePort = {
    async pauseOwned(access, projectId, contextId) {
      const claims = input.backend.list(access, projectId, contextId);
      if (!claims.ok) return "uncertain";
      const key = scopeKey(projectId, contextId);
      pending.set(key, "pause");
      let requested = false;
      let unsupported = false;
      let awaitingReadiness = false;
      for (const claim of claims.value) {
        if (!active(claim.state)) continue;
        const inspected = inspect(claim);
        if (
          inspected === null ||
          inspected.readiness === "permission_required" ||
          inspected.readiness === "unknown" ||
          claim.controlLeaseId === null ||
          inspected.automationControlEpoch === null
        )
          return "uncertain";
        if (inspected.readiness === "stopped") continue;
        awaitingReadiness ||= inspected.readiness === "starting";
        const controlTarget = target(claim);
        if (controlTarget === null) return "uncertain";
        const interrupted = await input.backend.control.interrupt({
          ...controlTarget,
          reason: "project_pause",
        });
        if (!interrupted.ok) return "uncertain";
        requested ||= interrupted.value.observation === "requested";
        unsupported ||= interrupted.value.observation === "unsupported";
      }
      const aggregate = aggregateState(projectId, contextId, "pause");
      if (aggregate !== null) {
        pending.delete(key);
        return "settled";
      }
      if (unsupported) {
        pending.delete(key);
        return "unsupported";
      }
      if (
        requested &&
        awaitingReadiness &&
        (await waitForAggregate(projectId, contextId, "pause")) !== null
      ) {
        pending.delete(key);
        return "settled";
      }
      return requested ? "requested" : "uncertain";
    },
    async stopOwned(access, projectId, contextId) {
      const claims = input.backend.list(access, projectId, contextId);
      if (!claims.ok) return "uncertain";
      const key = scopeKey(projectId, contextId);
      pending.set(key, "stop");
      for (const claim of claims.value) {
        if (!active(claim.state)) continue;
        const stopped = await input.backend.stop(access, claim.runId, claim.revision);
        if (!stopped.ok) return "uncertain";
      }
      if (aggregateState(projectId, contextId, "stop") !== null) {
        pending.delete(key);
        return "settled";
      }
      if ((await waitForAggregate(projectId, contextId, "stop")) !== null) {
        pending.delete(key);
        return "settled";
      }
      return "uncertain";
    },
    async continueOwned(access, projectId, contextId) {
      const claims = input.backend.list(access, projectId, contextId);
      if (!claims.ok) return "uncertain";
      const key = scopeKey(projectId, contextId);
      pending.set(key, "continue");
      for (const claim of claims.value) {
        if (["paused", "stopped"].includes(claim.state)) {
          const continued = await input.backend.continueRun(access, claim.runId, claim.revision);
          if (!continued.ok)
            return continued.error.code === "unavailable" ? "unsupported" : "uncertain";
          continue;
        }
        if (!active(claim.state)) continue;
        const controlTarget = target(claim);
        if (controlTarget === null) return "uncertain";
        const continued = input.backend.control.continueSession(controlTarget);
        if (!continued.ok)
          return continued.error.code === "unavailable" ? "unsupported" : "uncertain";
      }
      if (aggregateState(projectId, contextId, "continue") !== null) {
        pending.delete(key);
        return "settled";
      }
      if ((await waitForAggregate(projectId, contextId, "continue")) !== null) {
        pending.delete(key);
        return "settled";
      }
      return "uncertain";
    },
    async resolveRecipient({ projectId, contextId, actorId }) {
      await Promise.resolve();
      const claims = list(projectId, contextId);
      if (!claims.ok) return null;
      const claim = claims.value.find((item) => item.actorId === actorId);
      if (claim === undefined) return null;
      return {
        actorId,
        runId: claim.runId,
        attemptId: claim.attemptId,
        adapterSessionId: claim.adapterSessionId,
        processEpoch: claim.managedControl?.processEpoch ?? null,
        leaseId: claim.controlLeaseId,
      } satisfies ManagedWakeTarget;
    },
    async offerWake({ notice, leaseId }) {
      const loaded = input.backend.get(
        accessFor(notice.projectId),
        RunIdSchema.parse(notice.runId),
      );
      if (
        !loaded.ok ||
        loaded.value.packet.projectId !== notice.projectId ||
        loaded.value.packet.contextId !== notice.contextId ||
        loaded.value.actorId !== notice.actorId ||
        loaded.value.runId !== notice.runId ||
        loaded.value.attemptId !== notice.attemptId ||
        loaded.value.adapterSessionId !== notice.adapterSessionId ||
        notice.processEpoch !== loaded.value.managedControl?.processEpoch ||
        notice.leaseId !== leaseId ||
        loaded.value.controlLeaseId !== leaseId ||
        loaded.value.managedControl.pauseRequested
      )
        return "refused";
      const controlTarget = target(loaded.value);
      if (controlTarget === null) return "refused";
      const offered = await input.backend.control.offer({
        ...controlTarget,
        expectedAutomationControlEpoch: loaded.value.managedControl.automationControlEpoch,
        deliveryId: notice.wakeId,
        bodyMarkdown: notice.bodyMarkdown,
      });
      if (!offered.ok) return offered.error.code === "uncertain" ? "uncertain" : "refused";
      return offered.value.observation === "host_accepted" ||
        offered.value.observation === "provider_queued"
        ? "host_accepted"
        : offered.value.observation === "busy"
          ? "busy"
          : offered.value.observation === "uncertain"
            ? "uncertain"
            : "refused";
    },
    subscribe(listener) {
      listeners.add(listener);
      ensureControlSubscription();
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) {
          unsubscribeControl?.();
          unsubscribeControl = null;
        }
      };
    },
  };
  return port;

  function ensureControlSubscription(): void {
    if (unsubscribeControl !== null) return;
    unsubscribeControl = input.backend.control.subscribe((event) => {
      eventQueue = eventQueue.then(() => emit(event)).catch(() => undefined);
    });
  }

  async function emit(event: ManagedSessionControlEvent): Promise<void> {
    const runId = RunIdSchema.safeParse(event.runId);
    if (!runId.success) return;
    for (const scope of scopes()) {
      const loaded = input.backend.get(accessFor(scope.projectId), runId.data);
      if (!loaded.ok || loaded.value.packet.contextId !== scope.contextId) continue;
      const action = pending.get(scopeKey(scope.projectId, scope.contextId));
      const aggregate =
        action === undefined ? null : aggregateState(scope.projectId, scope.contextId, action);
      if (aggregate !== null) pending.delete(scopeKey(scope.projectId, scope.contextId));
      const state =
        aggregate?.state ??
        (action === "pause"
          ? "pausing"
          : event.kind === "session_exited"
            ? "stopped"
            : event.kind === "turn_settled" || event.kind === "session_ready"
              ? "idle"
              : "busy");
      const projected: ManagedWakeEvent = {
        projectId: scope.projectId,
        contextId: scope.contextId,
        actorId: ActorIdSchema.parse(event.actorId),
        state,
        wakeEligible: action === undefined && state === "idle",
        action: aggregate?.action ?? action ?? "wake",
        processEpoch: event.processEpoch,
        leaseId: loaded.value.controlLeaseId,
      };
      for (const listener of listeners) listener(projected);
      for (const waiter of settlementWaiters.get(scopeKey(scope.projectId, scope.contextId)) ?? [])
        waiter();
      return;
    }
    await Promise.resolve();
  }

  function aggregateState(
    projectId: ProjectId,
    contextId: WorkContextId,
    action: LifecycleAction,
  ): { readonly action: LifecycleAction; readonly state: ManagedWakeEvent["state"] } | null {
    const claims = list(projectId, contextId);
    if (!claims.ok) return null;
    for (const claim of claims.value) {
      if (!relevant(claim, action)) continue;
      const observed = inspect(claim);
      if (observed === null) return null;
      if (action === "pause") {
        if (
          !observed.pauseRequested ||
          (observed.readiness !== "idle" &&
            observed.readiness !== "stopped" &&
            !(observed.readiness === "starting" && observed.providerSessionId !== null))
        )
          return null;
      } else if (action === "stop") {
        if (observed.readiness !== "stopped") return null;
      } else if (
        observed.pauseRequested ||
        (observed.readiness !== "idle" &&
          observed.readiness !== "busy" &&
          !(observed.readiness === "starting" && observed.providerSessionId !== null)) ||
        observed.automationControlEpoch === null ||
        claim.controlLeaseId === null
      )
        return null;
    }
    return {
      action,
      state: action === "pause" ? "paused" : action === "stop" ? "stopped" : "idle",
    };
  }

  function inspect(claim: ManagedWorkClaim) {
    const controlTarget = target(claim);
    if (controlTarget === null) return null;
    const inspected = input.backend.control.inspect(controlTarget);
    return inspected.ok ? inspected.value : null;
  }

  function waitForAggregate(
    projectId: ProjectId,
    contextId: WorkContextId,
    action: LifecycleAction,
  ): Promise<ReturnType<typeof aggregateState>> {
    const current = aggregateState(projectId, contextId, action);
    if (current !== null) return Promise.resolve(current);
    const key = scopeKey(projectId, contextId);
    return new Promise((resolve) => {
      const waiters = settlementWaiters.get(key) ?? new Set<() => void>();
      const finish = () => {
        const settled = aggregateState(projectId, contextId, action);
        if (settled === null) return;
        clearTimeout(timeout);
        waiters.delete(finish);
        if (waiters.size === 0) settlementWaiters.delete(key);
        resolve(settled);
      };
      const timeout = setTimeout(() => {
        waiters.delete(finish);
        if (waiters.size === 0) settlementWaiters.delete(key);
        resolve(null);
      }, 5_000);
      waiters.add(finish);
      settlementWaiters.set(key, waiters);
    });
  }
}

function target(claim: ManagedWorkClaim) {
  const epoch = claim.managedControl?.processEpoch;
  if (epoch === undefined) return null;
  return {
    runId: RunIdSchema.parse(claim.runId),
    actorId: ActorIdSchema.parse(claim.actorId),
    sessionId: AgentSessionIdSchema.parse(claim.sessionId),
    terminalId: TerminalIdSchema.parse(claim.terminalId),
    expectedProcessEpoch: epoch,
  };
}

function relevant(claim: ManagedWorkClaim, action: LifecycleAction): boolean {
  if (action === "continue")
    return active(claim.state) || claim.state === "paused" || claim.state === "stopped";
  return active(claim.state);
}

function active(state: string): boolean {
  return ["launching", "running", "waiting_for_user", "stopping", "uncertain", "paused"].includes(
    state,
  );
}

function scopeKey(projectId: ProjectId, contextId: WorkContextId): string {
  return `${projectId}/${contextId}`;
}
