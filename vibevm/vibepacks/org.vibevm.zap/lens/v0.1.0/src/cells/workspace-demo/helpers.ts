/** @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
/** Pure helpers for the synthetic workspace acceptance port. */
import type {
  HistoryEvent,
  WorkspaceClientPort,
  WorkspaceCommandRequest,
  WorkspaceResult,
} from "../workspace-model/index.ts";

export function eventInScope(
  event: HistoryEvent,
  scope: Parameters<WorkspaceClientPort["events"]>[0]["cursor"]["scope"],
): boolean {
  if (scope.kind === "all_authorized") return true;
  if (event.projectId !== scope.projectId) return false;
  if (scope.kind === "project") return true;
  if (event.contextId !== scope.contextId) return false;
  return scope.kind === "context" || event.actorId === scope.actorId;
}

export function pending(request: WorkspaceCommandRequest) {
  return {
    requestId: request.clientRequestId,
    state: "pending" as const,
    availability: { state: "available" } as const,
  };
}

export function ok<T>(value: T): WorkspaceResult<T> {
  return { ok: true, value };
}

export function fail(message: string): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code: "unsupported_operation",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#incremental-delivery: ${message}`,
    },
  };
}
