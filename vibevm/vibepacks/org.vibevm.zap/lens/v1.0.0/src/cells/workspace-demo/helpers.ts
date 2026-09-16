/** @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
/** Pure helpers for the synthetic workspace acceptance port. */
import {
  HistoryEventSchema,
  type ProjectId,
  type WorkContextId,
  type HistoryEvent,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
  type WorkspaceResult,
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

export function demoEvent(
  historyEventId: string,
  projectId: ProjectId,
  contextId: WorkContextId,
  sequence: string,
  kind: string,
  source: "lens" | "host" | "zap",
  actorId: string | null,
  occurredAt: string,
) {
  return HistoryEventSchema.parse({
    historyEventId,
    projectId,
    contextId,
    globalSequence: sequence,
    projectSequence: sequence,
    sourceSequence: sequence,
    kind,
    source,
    actorId,
    occurrenceAt: occurredAt,
    ingestedAt: occurredAt,
    sourceEventId: `source.${historyEventId}`,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  });
}

export function createDemoEvent(input: {
  readonly alphaProjectId: ProjectId;
  readonly alphaContextId: WorkContextId;
  readonly betaContextId: WorkContextId;
  readonly occurredAt: string;
}) {
  return (
    id: string,
    projectId: ProjectId,
    sequence: string,
    kind: string,
    source: "lens" | "host" | "zap",
    actorId: string | null,
  ) =>
    demoEvent(
      id,
      projectId,
      projectId === input.alphaProjectId ? input.alphaContextId : input.betaContextId,
      sequence,
      kind,
      source,
      actorId,
      input.occurredAt,
    );
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
