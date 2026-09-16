/** Coordinator and child state projection. @scope spec://org.vibevm.zap/lens/PROP-009#project-lifecycle */
import { z } from "zod";
import type { CoordinatorEvent } from "../agent-runtime/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import type { AgentDescriptor } from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { childStateFromEvent } from "./child-state.ts";
import { HostStatusSchema } from "./event-model.ts";
import { stateForAgent, type LaunchState } from "./helpers.ts";
import type { Launch } from "./launch.ts";

export function updateWorkspaceState(input: {
  readonly launch: Launch;
  readonly event: CoordinatorEvent;
  readonly actor: AgentDescriptor | undefined;
  readonly store: WorkspaceStore;
  readonly now: () => Date;
}): void {
  const { launch, event, actor } = input;
  const isRoot =
    event.nativeThreadId === launch.descriptor.nativeThreadRef.value ||
    event.nativeThreadId === null;
  let state: LaunchState | null = null;
  if (event.kind === "process_exited") state = "failed";
  else if (event.kind === "session_pause_requested") state = "pausing";
  else if (event.kind === "session_paused") state = "paused";
  else if (event.kind === "session_stop_requested") state = "stopping";
  else if (event.kind === "session_stopped") state = "stopped";
  else if (event.kind === "session_continued") state = "ready";
  else if (event.kind === "turn_started" && isRoot) state = "running";
  else if (event.kind === "turn_completed" && isRoot) {
    state = rootTurnCompletionState(event.data);
  } else if (event.kind === "session_started" || event.kind === "session_resumed") state = "ready";
  else if (event.kind === "session_status" && isRoot) {
    state = rootSessionStatusState(event.data);
  }
  if (state !== null && isRoot) {
    launch.descriptor = {
      ...launch.descriptor,
      state: state === "starting" ? "bootstrapping" : state,
    };
    launch.session = {
      ...launch.session,
      state,
      revision: DecimalSchema.parse(String(BigInt(launch.session.revision) + 1n)),
      updatedAt: input.now().toISOString(),
    };
    input.store.upsertCoordinatorSession(launch.session);
    const root = launch.actors.get("__coordinator__");
    if (root !== undefined) {
      const updated = {
        ...root,
        state: stateForAgent(state),
        revision: launch.session.revision,
      } satisfies AgentDescriptor;
      launch.actors.set("__coordinator__", updated);
      launch.actors.set(launch.descriptor.nativeThreadRef.value, updated);
      input.store.upsertAgent(updated);
    }
  } else if (actor !== undefined) {
    const next = childStateFromEvent(event);
    if (next !== null) {
      const updated = {
        ...actor,
        state: next,
        revision: DecimalSchema.parse(String(BigInt(actor.revision) + 1n)),
      } satisfies AgentDescriptor;
      launch.actors.set(event.nativeThreadId ?? "", updated);
      input.store.upsertAgent(updated);
    }
  }
}

export function rootTurnCompletionState(data: unknown): "ready" | "failed" {
  const completed = z
    .looseObject({ status: z.enum(["completed", "interrupted", "failed"]) })
    .safeParse(data);
  return completed.success && completed.data.status === "failed" ? "failed" : "ready";
}

export function rootSessionStatusState(data: unknown): LaunchState | null {
  const status = z.looseObject({ status: z.unknown() }).safeParse(data);
  if (!status.success) return null;
  const parsed = HostStatusSchema.safeParse(status.data.status);
  if (!parsed.success) return null;
  if (typeof parsed.data === "string") return parsed.data;
  if (parsed.data.type === "systemError") return "failed";
  if (parsed.data.type === "active")
    return parsed.data.activeFlags?.includes("waitingOnApproval") ? "waiting_for_user" : "running";
  return parsed.data.type === "closed" ? "stopped" : "ready";
}
