/** Durable project lifecycle state transitions. @scope spec://org.vibevm.zap/lens/PROP-009#project-lifecycle */
import { createHash } from "node:crypto";
import { z } from "zod";
import { ActorIdSchema } from "../protocol/index.ts";
import {
  ProjectExecutionStateSchema,
  WorkspaceAccessContextSchema,
  WorkspaceCommandRequestSchema,
  WorkspaceCommandResponseSchema,
  type ProjectExecutionState,
  type ProjectId,
  type WorkContextId,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";
import {
  CoordinatorLaunchReceiptSchema,
  type CoordinatorLaunchReceipt,
  ProjectLifecycleSettlementSchema,
  type ProjectLifecycleSettlement,
} from "./types.ts";

type LifecycleRequest = Extract<
  WorkspaceCommandRequest,
  { operation: "project.pause.v1" | "project.stop.v1" | "project.continue.v1" }
>;
const ExecutionRowSchema = z.object({ public_json: z.string() });
const IdempotencyRowSchema = z.object({ request_digest: z.string() });

export function activateProjectExecution(
  state: WorkspaceState,
  raw: CoordinatorLaunchReceipt,
): WorkspaceResult<ProjectExecutionState> {
  const input = CoordinatorLaunchReceiptSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "coordinator execution receipt is malformed");
  const current = readProjectExecution(state, input.data.projectId, input.data.contextId);
  if (!current.ok) return current;
  if (
    current.value.state === "running" &&
    current.value.sessionId === input.data.sessionId &&
    current.value.processEpoch === input.data.processEpoch
  ) {
    return current;
  }
  const updated = ProjectExecutionStateSchema.parse({
    ...current.value,
    state: "running",
    sessionId: input.data.sessionId,
    processEpoch: input.data.processEpoch,
    pendingAction: null,
    revision: String(BigInt(current.value.revision) + 1n),
    updatedAt: input.data.updatedAt,
  });
  writeExecution(state, updated);
  state.appendHistory({
    projectId: updated.projectId,
    contextId: updated.contextId,
    kind: "project.execution.running",
    source: "host",
    actorId: ActorIdSchema.parse(input.data.actorId),
    occurrenceAt: updated.updatedAt,
    sourceEventId: `execution:${input.data.sessionId}:${input.data.processEpoch}`,
    sourceSequence: null,
    correlationId: input.data.sessionId,
    causationId: null,
    planProvenance: null,
    payload: { nativeThreadId: input.data.nativeThreadId, revision: updated.revision },
  });
  return { ok: true, value: updated };
}

export function initializeProjectExecution(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
  updatedAt: string,
): ProjectExecutionState {
  const execution = ProjectExecutionStateSchema.parse({
    projectId,
    contextId,
    state: "uninitialized",
    sessionId: null,
    processEpoch: null,
    pendingAction: null,
    lastAction: null,
    revision: "1",
    updatedAt,
  });
  state.database.run(
    `INSERT OR IGNORE INTO workspace_project_execution(project_id, context_id, state, revision, public_json)
     VALUES(?, ?, ?, ?, ?)`,
    [projectId, contextId, execution.state, 1n, state.json(execution)],
  );
  return execution;
}

export function readProjectExecution(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
): WorkspaceResult<ProjectExecutionState> {
  const row = state.database.get(
    "SELECT public_json FROM workspace_project_execution WHERE project_id = ? AND context_id = ?",
    ExecutionRowSchema,
    [projectId, contextId],
  );
  return row === null
    ? failure("not_found", "project execution state does not exist")
    : { ok: true, value: state.parse(row.public_json, ProjectExecutionStateSchema) };
}

export function requestProjectLifecycle(
  state: WorkspaceState,
  rawContext: WorkspaceAccessContext,
  rawRequest: LifecycleRequest,
): WorkspaceResult<{ execution: ProjectExecutionState; acquired: boolean }> {
  const context = WorkspaceAccessContextSchema.safeParse(rawContext);
  const parsed = WorkspaceCommandRequestSchema.safeParse(rawRequest);
  if (!context.success || !parsed.success || !isLifecycle(parsed.data)) {
    return failure("invalid_input", "project lifecycle request is malformed");
  }
  const request = parsed.data;
  if (!context.data.authorizedProjectIds.includes(request.projectId)) {
    return failure("forbidden", "project lifecycle request is outside project scope");
  }
  try {
    return state.database.transaction(() => requestOnce(state, context.data, request));
  } catch {
    return failure("storage_failure", "project lifecycle transaction failed");
  }
}

function requestOnce(
  state: WorkspaceState,
  context: WorkspaceAccessContext,
  request: LifecycleRequest,
): WorkspaceResult<{ execution: ProjectExecutionState; acquired: boolean }> {
  const digest = createHash("sha256").update(JSON.stringify(request)).digest("hex");
  const actorKey = context.actorId ?? `client:${context.clientId}`;
  const identity = [
    context.principalId,
    actorKey,
    request.projectId,
    request.contextId,
    request.clientRequestId,
  ];
  const prior = state.database.get(
    `SELECT request_digest FROM workspace_idempotency
     WHERE principal_id = ? AND actor_key = ? AND project_id = ? AND context_id = ?
       AND client_request_id = ?`,
    IdempotencyRowSchema,
    identity,
  );
  const current = readProjectExecution(state, request.projectId, request.contextId);
  if (!current.ok) return current;
  if (prior !== null) {
    return prior.request_digest === digest
      ? { ok: true, value: { execution: current.value, acquired: false } }
      : failure("idempotency_conflict", "client request identity was reused with new content");
  }
  if (current.value.revision !== request.expectedRevision) {
    return failure("stale_revision", "project execution revision changed");
  }
  if (current.value.sessionId !== request.sessionId) {
    return failure("conflict", "project lifecycle request targets a different coordinator");
  }
  const action = actionOf(request.operation);
  if (!allowedTransition(current.value.state, action, request.sessionId)) {
    return failure("conflict", `project state ${current.value.state} does not allow ${action}`);
  }
  const revision = BigInt(current.value.revision) + 1n;
  const updated = ProjectExecutionStateSchema.parse({
    ...current.value,
    state: transitionState(action),
    pendingAction: {
      action,
      clientRequestId: request.clientRequestId,
      previousState: current.value.state,
      observation: "requested",
      reasonMarkdown: request.reasonMarkdown,
    },
    lastAction: {
      action,
      clientRequestId: request.clientRequestId,
      observation: "requested",
    },
    revision: String(revision),
    updatedAt: state.now(),
  });
  writeExecution(state, updated);
  const response = WorkspaceCommandResponseSchema.parse({
    operation: request.operation,
    execution: updated,
  });
  state.database.run(
    `INSERT INTO workspace_idempotency(
       principal_id, actor_key, project_id, context_id, client_request_id,
       request_digest, result_json) VALUES(?, ?, ?, ?, ?, ?, ?)`,
    [...identity, digest, state.json(response)],
  );
  state.appendHistory({
    projectId: updated.projectId,
    contextId: updated.contextId,
    kind: `project.${action}.requested`,
    source: "lens",
    actorId: context.actorId,
    occurrenceAt: updated.updatedAt,
    sourceEventId: lifecycleEventId(request.clientRequestId, "requested"),
    sourceSequence: null,
    correlationId: request.clientRequestId,
    causationId: null,
    planProvenance: null,
    payload: { sessionId: request.sessionId, revision: updated.revision },
  });
  return { ok: true, value: { execution: updated, acquired: true } };
}

export function settleProjectLifecycle(
  state: WorkspaceState,
  raw: ProjectLifecycleSettlement,
): WorkspaceResult<ProjectExecutionState> {
  const input = ProjectLifecycleSettlementSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "lifecycle settlement is malformed");
  try {
    return state.database.transaction(() => settleOnce(state, input.data));
  } catch {
    return failure("storage_failure", "lifecycle settlement transaction failed");
  }
}

function settleOnce(
  state: WorkspaceState,
  input: ProjectLifecycleSettlement,
): WorkspaceResult<ProjectExecutionState> {
  const current = readProjectExecution(state, input.projectId, input.contextId);
  if (!current.ok) return current;
  const pending = current.value.pendingAction;
  if (pending === null) {
    if (
      input.action === "pause" &&
      input.observation === "requested" &&
      current.value.state === "paused" &&
      current.value.lastAction?.action === "pause"
    ) {
      const reopened = ProjectExecutionStateSchema.parse({
        ...current.value,
        state: "pausing",
        pendingAction: {
          action: "pause",
          clientRequestId: input.clientRequestId,
          previousState: "paused",
          observation: "requested",
          reasonMarkdown: "A new owned turn was observed after pause settlement.",
        },
        lastAction: {
          action: "pause",
          clientRequestId: input.clientRequestId,
          observation: "requested",
        },
        revision: String(BigInt(current.value.revision) + 1n),
        updatedAt: input.updatedAt,
      });
      writeExecution(state, reopened);
      return { ok: true, value: reopened };
    }
    return settledStateMatches(current.value, input.action, input.observation)
      ? current
      : failure("conflict", "project has no matching lifecycle action to settle");
  }
  if (
    pending.clientRequestId !== input.clientRequestId ||
    pending.action !== input.action ||
    current.value.sessionId !== input.sessionId
  ) {
    return failure("conflict", "lifecycle settlement does not match the pending action");
  }
  if (input.observation === "requested") return current;
  const revision = BigInt(current.value.revision) + 1n;
  const stable =
    input.observation === "settled" ? settledState(input.action) : pending.previousState;
  const pauseUnsettled = input.action === "pause" && input.observation !== "settled";
  const updated = ProjectExecutionStateSchema.parse({
    ...current.value,
    state:
      input.observation === "settled"
        ? stable
        : input.observation === "uncertain"
          ? "uncertain"
          : pauseUnsettled && current.value.sessionId !== null
            ? "pausing"
            : pending.previousState,
    processEpoch: input.processEpoch ?? current.value.processEpoch,
    pendingAction:
      pauseUnsettled || input.observation === "uncertain"
        ? { ...pending, observation: input.observation }
        : null,
    lastAction: {
      action: input.action,
      clientRequestId: input.clientRequestId,
      observation: input.observation,
    },
    revision: String(revision),
    updatedAt: input.updatedAt,
  });
  writeExecution(state, updated);
  updateClaim(state, updated, input.action, input.observation);
  state.appendHistory({
    projectId: updated.projectId,
    contextId: updated.contextId,
    kind: `project.${input.action}.${input.observation}`,
    source: "host",
    actorId: null,
    occurrenceAt: input.updatedAt,
    sourceEventId: lifecycleEventId(input.clientRequestId, input.observation),
    sourceSequence: null,
    correlationId: input.clientRequestId,
    causationId: null,
    planProvenance: null,
    payload: { state: updated.state, revision: updated.revision },
  });
  return { ok: true, value: updated };
}

function writeExecution(state: WorkspaceState, execution: ProjectExecutionState): void {
  state.database.run(
    `UPDATE workspace_project_execution SET state = ?, revision = ?, public_json = ?
     WHERE project_id = ? AND context_id = ?`,
    [
      execution.state,
      BigInt(execution.revision),
      state.json(execution),
      execution.projectId,
      execution.contextId,
    ],
  );
}

function updateClaim(
  state: WorkspaceState,
  execution: ProjectExecutionState,
  action: ProjectLifecycleSettlement["action"],
  observation: ProjectLifecycleSettlement["observation"],
): void {
  if (observation !== "settled" || (action !== "stop" && action !== "continue")) return;
  state.database.run(
    `UPDATE workspace_coordinator_claims SET state = ?, process_epoch = COALESCE(?, process_epoch),
       updated_at = ? WHERE project_id = ? AND context_id = ? AND session_id = ?`,
    [
      action === "stop" ? "stopped" : "running",
      execution.processEpoch,
      execution.updatedAt,
      execution.projectId,
      execution.contextId,
      execution.sessionId,
    ],
  );
}

function isLifecycle(request: WorkspaceCommandRequest): request is LifecycleRequest {
  return ["project.pause.v1", "project.stop.v1", "project.continue.v1"].includes(request.operation);
}
function actionOf(operation: LifecycleRequest["operation"]): "pause" | "stop" | "continue" {
  return operation === "project.pause.v1"
    ? "pause"
    : operation === "project.stop.v1"
      ? "stop"
      : "continue";
}
function transitionState(action: "pause" | "stop" | "continue") {
  return action === "pause" ? "pausing" : action === "stop" ? "stopping" : "continuing";
}
function settledState(action: "pause" | "stop" | "continue") {
  return action === "pause" ? "paused" : action === "stop" ? "stopped" : "running";
}
function allowedTransition(
  state: ProjectExecutionState["state"],
  action: string,
  sessionId: ProjectExecutionState["sessionId"],
): boolean {
  if (action === "pause")
    return state === "running" || (state === "uninitialized" && sessionId === null);
  if (action === "stop")
    return (
      ["running", "pausing", "paused", "uncertain"].includes(state) ||
      (state === "uninitialized" && sessionId === null)
    );
  return state === "paused" || state === "stopped";
}
function settledStateMatches(
  state: ProjectExecutionState,
  action: ProjectLifecycleSettlement["action"],
  observation: ProjectLifecycleSettlement["observation"],
): boolean {
  return observation === "settled" && state.state === settledState(action);
}
function lifecycleEventId(requestId: string, observation: string): string {
  return `lifecycle:${requestId}:${observation}`;
}
