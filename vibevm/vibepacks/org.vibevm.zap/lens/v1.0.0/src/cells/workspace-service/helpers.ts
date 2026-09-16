/** @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import type { AgentDescriptor, CoordinatorSession } from "../workspace-model/index.ts";

export type LaunchState =
  | "starting"
  | "bootstrapping"
  | "ready"
  | "running"
  | "waiting_for_user"
  | "stopped"
  | "failed"
  | "paused"
  | "pausing"
  | "stopping";

export function stateForAgent(state: LaunchState): AgentDescriptor["state"] {
  if (state === "bootstrapping" || state === "starting") return "starting";
  if (state === "ready" || state === "running") return "active";
  if (state === "paused" || state === "pausing") return "waiting_for_user";
  return state === "stopping" ? "stopped" : state;
}

export function stateForSession(state: LaunchState): CoordinatorSession["state"] {
  if (state === "paused" || state === "pausing") return "ready";
  return state === "stopping" ? "stopped" : state;
}

export function actionAvailable(available: boolean) {
  return available
    ? { state: "available" as const }
    : {
        state: "unavailable" as const,
        code: "unsupported" as const,
        reason: "Host capability is unavailable",
      };
}
