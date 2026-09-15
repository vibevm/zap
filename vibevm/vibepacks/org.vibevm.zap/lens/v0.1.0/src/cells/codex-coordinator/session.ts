/** Codex session state construction and serialization. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import type { CoordinatorResumeInput, CoordinatorStartInput } from "../agent-runtime/index.ts";
import { activeTurn, descriptor } from "./helpers.ts";
import type { CodexThread } from "./protocol.ts";
import type { SessionState, WorkerState } from "./state.ts";

export function makeSession(
  input: CoordinatorStartInput | CoordinatorResumeInput,
  worker: WorkerState,
  thread: CodexThread,
  bootstrap: "submitted" | "not_observed",
): SessionState {
  return {
    descriptor: descriptor(input, worker, { thread, instructionSources: [] }, bootstrap),
    start: input,
    modelId: input.modelId ?? worker.profile.model,
    reasoningEffort:
      input.reasoningEffort === undefined ? (worker.profile.effort ?? null) : input.reasoningEffort,
    activeTurnId: activeTurn(thread),
    childThreads: new Map(),
    childActiveTurns: new Map(),
    pauseTargets: new Set(),
    lifecycle: "active",
    pauseObservation: null,
    stopObservation: null,
    pending: new Map(),
    answeredRequests: new Set(),
    retiredRequests: new Set(),
    serial: Promise.resolve(),
  };
}

export async function exclusive<T>(session: SessionState, run: () => Promise<T>): Promise<T> {
  const prior = session.serial;
  let release: () => void = () => undefined;
  session.serial = new Promise<void>((resolveSerial) => {
    release = resolveSerial;
  });
  await prior;
  try {
    return await run();
  } finally {
    release();
  }
}
