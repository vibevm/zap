/**
 * Project-isolated Codex coordinator pause, stop and continuation.
 *
 * Pause interrupts observed turns; it never claims to freeze model computation.
 * @scope spec://org.vibevm.zap/lens/PROP-009#project-lifecycle
 */
import {
  CoordinatorLifecycleInputSchema,
  runtimeFailure,
  type AgentRuntimeResult,
  type CoordinatorLifecycleInput,
  type CoordinatorLifecycleReceipt,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
} from "../agent-runtime/index.ts";
import type { CodexEventRouter } from "./events.ts";
import { scopeOf } from "./helpers.ts";
import type { SessionState, WorkerState } from "./state.ts";

type OpenWorker = (
  ownerCoordinatorSessionId: string,
  profileId: string,
  cwd: string,
) => Promise<AgentRuntimeResult<WorkerState>>;
type ResumeSession = (
  input: CoordinatorResumeInput,
  worker: WorkerState,
  existing: SessionState,
) => Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>>;

export class CodexLifecycleController {
  readonly #sessions: ReadonlyMap<string, SessionState>;
  readonly #workers: ReadonlyMap<string, WorkerState>;
  readonly #events: CodexEventRouter;
  readonly #openWorker: OpenWorker;
  readonly #resume: ResumeSession;
  readonly #discardWorker: (ownerCoordinatorSessionId: string) => void;

  constructor(options: {
    sessions: ReadonlyMap<string, SessionState>;
    workers: ReadonlyMap<string, WorkerState>;
    events: CodexEventRouter;
    openWorker: OpenWorker;
    resume: ResumeSession;
    discardWorker: (ownerCoordinatorSessionId: string) => void;
  }) {
    this.#sessions = options.sessions;
    this.#workers = options.workers;
    this.#events = options.events;
    this.#openWorker = options.openWorker;
    this.#resume = options.resume;
    this.#discardWorker = options.discardWorker;
  }

  async pause(raw: CoordinatorLifecycleInput) {
    const context = this.#context(raw);
    if (!context.ok) return context;
    const { session, worker } = context.value;
    if (session.lifecycle === "stopped" || session.lifecycle === "stop_requested") {
      return {
        ok: true as const,
        value: receipt(session, "pause", "unsupported", worker?.epoch ?? null, []),
      };
    }
    if (session.lifecycle === "pause_requested") {
      return {
        ok: true as const,
        value: receipt(
          session,
          "pause",
          session.pauseObservation ?? "requested",
          worker?.epoch ?? null,
          [],
        ),
      };
    }
    if (worker === null) {
      return runtimeFailure(
        "transport_lost",
        "Owned Codex process is not observed",
        "after_reconcile",
      );
    }
    if (session.lifecycle === "paused") {
      return { ok: true as const, value: receipt(session, "pause", "settled", worker.epoch, []) };
    }
    const targets = activeTargets(session);
    if (targets.length === 0) {
      session.lifecycle = "paused";
      session.pauseObservation = "settled";
      session.descriptor = { ...session.descriptor, state: "paused" };
      this.#events.emit(session, worker, "session_paused", null, null, null, {
        observation: "settled",
      });
      return { ok: true as const, value: receipt(session, "pause", "settled", worker.epoch, []) };
    }

    session.lifecycle = "pause_requested";
    session.pauseObservation = "requested";
    session.descriptor = { ...session.descriptor, state: "pausing" };
    session.pauseTargets.clear();
    for (const target of targets) {
      session.pauseTargets.add(targetKey(target.nativeThreadId, target.nativeTurnId));
    }
    this.#events.emit(session, worker, "session_pause_requested", null, null, null, {
      targets,
    });
    const observed: CoordinatorLifecycleReceipt["targets"] = [];
    for (const target of targets) {
      const result = await worker.process.request("turn/interrupt", {
        threadId: target.nativeThreadId,
        turnId: target.nativeTurnId,
      });
      observed.push({
        ...target,
        observation: result.ok
          ? "requested"
          : result.error.kind === "rpc"
            ? "refused"
            : "uncertain",
      });
    }
    const observation = isPaused(session)
      ? "settled"
      : observed.some((target) => target.observation === "uncertain")
        ? "uncertain"
        : observed.some((target) => target.observation === "refused")
          ? "refused"
          : "requested";
    if (observation === "uncertain") {
      this.#events.emit(session, worker, "lifecycle_uncertain", null, null, null, {
        action: "pause",
      });
    }
    session.pauseObservation = observation;
    return {
      ok: true as const,
      value: receipt(session, "pause", observation, worker.epoch, observed),
    };
  }

  async stop(raw: CoordinatorLifecycleInput) {
    const input = CoordinatorLifecycleInputSchema.safeParse(raw);
    if (!input.success) return runtimeFailure("invalid_input", "Lifecycle input is invalid");
    const session = this.#sessions.get(input.data.coordinatorSessionId);
    if (session === undefined) return runtimeFailure("not_found", "Coordinator session is unknown");
    if (session.descriptor.processEpoch !== input.data.expectedProcessEpoch) {
      return runtimeFailure("stale_epoch", "Coordinator process epoch changed", "after_refresh");
    }
    if (session.lifecycle === "stopped") {
      return {
        ok: true as const,
        value: receipt(session, "stop", "settled", null, []),
      };
    }
    if (session.lifecycle === "stop_requested") {
      return {
        ok: true as const,
        value: receipt(
          session,
          "stop",
          session.stopObservation ?? "requested",
          this.#workers.get(session.descriptor.coordinatorSessionId)?.epoch ?? null,
          [],
        ),
      };
    }
    const worker = this.#workers.get(session.descriptor.coordinatorSessionId);
    if (worker === undefined) {
      return {
        ok: true as const,
        value: receipt(session, "stop", "uncertain", null, []),
      };
    }
    session.lifecycle = "stop_requested";
    session.stopObservation = "requested";
    session.descriptor = { ...session.descriptor, state: "stopping" };
    retireRequests(session);
    this.#events.emit(session, worker, "session_stop_requested", null, null, null, {});
    await worker.process.terminate();
    if (!isStopped(session)) {
      session.stopObservation = "uncertain";
      this.#events.emit(session, worker, "lifecycle_uncertain", null, null, null, {
        action: "stop",
      });
      return {
        ok: true as const,
        value: receipt(session, "stop", "uncertain", worker.epoch, []),
      };
    }
    return { ok: true as const, value: receipt(session, "stop", "settled", null, []) };
  }

  async continueSession(raw: CoordinatorLifecycleInput) {
    const input = CoordinatorLifecycleInputSchema.safeParse(raw);
    if (!input.success) return runtimeFailure("invalid_input", "Lifecycle input is invalid");
    const session = this.#sessions.get(input.data.coordinatorSessionId);
    if (session === undefined) return runtimeFailure("not_found", "Coordinator session is unknown");
    if (session.descriptor.processEpoch !== input.data.expectedProcessEpoch) {
      return runtimeFailure("stale_epoch", "Coordinator process epoch changed", "after_refresh");
    }
    const previousEpoch = session.descriptor.processEpoch;
    const current = this.#workers.get(session.descriptor.coordinatorSessionId);
    if (session.lifecycle === "stop_requested" && current !== undefined) {
      return {
        ok: true as const,
        value: receipt(session, "continue", "uncertain", current.epoch, []),
      };
    }
    if (current !== undefined) {
      session.pauseTargets.clear();
      session.lifecycle = "active";
      session.pauseObservation = null;
      session.stopObservation = null;
      session.descriptor = {
        ...session.descriptor,
        state:
          session.activeTurnId === null && session.childActiveTurns.size === 0
            ? "ready"
            : "running",
      };
      this.#events.emit(session, current, "session_continued", null, null, null, {
        resumedNativeThread: false,
      });
      return {
        ok: true as const,
        value: receipt(session, "continue", "settled", current.epoch, [], previousEpoch),
      };
    }

    const worker = await this.#openWorker(
      session.descriptor.coordinatorSessionId,
      session.descriptor.profileId,
      session.descriptor.cwd,
    );
    if (!worker.ok) return worker;
    const resumed = await this.#resume(
      {
        ...scopeOf(session.start),
        profileId: session.descriptor.profileId,
        cwd: session.descriptor.cwd,
        nativeThreadId: session.descriptor.nativeThreadRef.value,
        modelId: session.modelId,
        reasoningEffort: session.reasoningEffort,
        agentScope: session.start.agentScope,
        agentBinding: session.start.agentBinding,
      },
      worker.value,
      session,
    );
    if (!resumed.ok) {
      this.#discardWorker(session.descriptor.coordinatorSessionId);
      return resumed;
    }
    this.#events.emit(session, worker.value, "session_continued", null, null, null, {
      resumedNativeThread: true,
    });
    return {
      ok: true as const,
      value: receipt(session, "continue", "settled", worker.value.epoch, [], previousEpoch),
    };
  }

  #context(raw: CoordinatorLifecycleInput): AgentRuntimeResult<{
    session: SessionState;
    worker: WorkerState | null;
  }> {
    const input = CoordinatorLifecycleInputSchema.safeParse(raw);
    if (!input.success) return runtimeFailure("invalid_input", "Lifecycle input is invalid");
    const session = this.#sessions.get(input.data.coordinatorSessionId);
    if (session === undefined) return runtimeFailure("not_found", "Coordinator session is unknown");
    if (session.descriptor.processEpoch !== input.data.expectedProcessEpoch) {
      return runtimeFailure("stale_epoch", "Coordinator process epoch changed", "after_refresh");
    }
    const worker = this.#workers.get(session.descriptor.coordinatorSessionId) ?? null;
    return { ok: true, value: { session, worker } };
  }
}

function activeTargets(session: SessionState) {
  const targets: Array<{
    nativeThreadId: string;
    nativeTurnId: string;
    role: "coordinator" | "native_child";
  }> = [];
  if (session.activeTurnId !== null) {
    targets.push({
      nativeThreadId: session.descriptor.nativeThreadRef.value,
      nativeTurnId: session.activeTurnId,
      role: "coordinator",
    });
  }
  for (const [nativeThreadId, nativeTurnId] of session.childActiveTurns) {
    targets.push({ nativeThreadId, nativeTurnId, role: "native_child" });
  }
  return targets;
}

function receipt(
  session: SessionState,
  action: CoordinatorLifecycleReceipt["action"],
  observation: CoordinatorLifecycleReceipt["observation"],
  currentProcessEpoch: string | null,
  targets: CoordinatorLifecycleReceipt["targets"],
  previousProcessEpoch = session.descriptor.processEpoch,
): CoordinatorLifecycleReceipt {
  return {
    coordinatorSessionId: session.descriptor.coordinatorSessionId,
    action,
    observation,
    previousProcessEpoch,
    currentProcessEpoch,
    nativeThreadId: session.descriptor.nativeThreadRef.value,
    targets,
    message: lifecycleMessage(action, observation),
  };
}

function lifecycleMessage(
  action: CoordinatorLifecycleReceipt["action"],
  observation: CoordinatorLifecycleReceipt["observation"],
): string {
  return `${action} is ${observation}; project dispatch state remains owned by Zap Wayfinder`;
}

function targetKey(threadId: string, turnId: string): string {
  return `${threadId}\u0000${turnId}`;
}

function retireRequests(session: SessionState): void {
  for (const key of session.pending.keys()) session.retiredRequests.add(key);
  session.pending.clear();
  session.answeredRequests.clear();
}

function isStopped(session: SessionState): boolean {
  return session.lifecycle === "stopped";
}

function isPaused(session: SessionState): boolean {
  return session.lifecycle === "paused";
}
