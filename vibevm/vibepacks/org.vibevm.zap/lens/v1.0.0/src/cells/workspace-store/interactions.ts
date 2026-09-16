/** Durable native interaction bindings. @scope spec://org.vibevm.zap/lens/PROP-005#rich-questions */
import { createHash } from "node:crypto";
import { z } from "zod";
import { DecimalSchema, type ConversationId, type WorkspaceId } from "../protocol/index.ts";
import {
  NativeApprovalRequestSchema,
  NativeInteractionRecordSchema,
  QuestionGroupSchema,
  type NativeApprovalRequest,
  type NativeInteractionRecord,
  type QuestionAnswerVersion,
  type QuestionGroup,
  type WorkspaceResult,
  type WorkspaceCommandContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
} from "../workspace-model/index.ts";
import { commandWorkspaceInTransaction } from "./commands.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";
import {
  AgentScopeResolutionSchema,
  NativeApprovalRecordInputSchema,
  NativeQuestionRecordInputSchema,
  NativeResponsePreparationSchema,
  NativeResponseSettlementSchema,
  type NativeApprovalRecordInput,
  type NativeQuestionRecordInput,
  type NativeResponsePreparation,
  type NativeResponseSettlement,
} from "./types.ts";

const StoredRowSchema = z.object({ public_json: z.string(), request_digest: z.string() });
const PublicRowSchema = z.object({ public_json: z.string() });
const ScopeRowSchema = z.object({ project_id: z.string(), context_id: z.string() });

export function resolveAgentScope(
  state: WorkspaceState,
  workspaceId: WorkspaceId,
  conversationId: ConversationId,
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const row = state.database.get(
    `SELECT project_id, context_id FROM workspace_agent_scopes
     WHERE workspace_id = ? AND conversation_id = ?`,
    ScopeRowSchema,
    [workspaceId, conversationId],
  );
  return row === null
    ? failure("not_found", "broker scope is not registered to a project context")
    : {
        ok: true as const,
        value: AgentScopeResolutionSchema.parse({
          projectId: row.project_id,
          contextId: row.context_id,
        }),
      };
}

export function recordNativeQuestion(
  state: WorkspaceState,
  raw: NativeQuestionRecordInput,
): WorkspaceResult<{ question: QuestionGroup; interaction: NativeInteractionRecord }> {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = NativeQuestionRecordInputSchema.safeParse(raw);
  if (!input.success || input.data.access.actorId === null) {
    return failure("invalid_input", "native question binding is malformed");
  }
  const digest = requestDigest(input.data);
  try {
    return state.database.transaction(() => {
      const existing = interactionByIdentity(state, input.data.identity);
      if (existing !== null) {
        if (existing.request_digest !== digest)
          return failure("idempotency_conflict", "native request identity changed content");
        const record = state.parse(existing.public_json, NativeInteractionRecordSchema);
        const question =
          record.questionGroupId === null ? null : questionById(state, record.questionGroupId);
        return question === null
          ? failure("storage_failure", "native question binding lost its question group")
          : { ok: true as const, value: { question, interaction: record } };
      }
      const command = {
        operation: "question.create.v1" as const,
        clientRequestId: input.data.clientRequestId,
        projectId: input.data.projectId,
        contextId: input.data.contextId,
        conversationId: input.data.conversationId,
        draft: input.data.draft,
      };
      const created = commandWorkspaceInTransaction(state, input.data.access, command);
      if (!created.ok) return created;
      if (created.value.operation !== "question.create.v1")
        return failure("storage_failure", "native question command returned another operation");
      const now = state.now();
      const record = NativeInteractionRecordSchema.parse({
        projectId: input.data.projectId,
        contextId: input.data.contextId,
        originActorId: input.data.access.actorId,
        identity: input.data.identity,
        kind: "user_input",
        state: "pending",
        questionGroupId: created.value.question.questionGroupId,
        answerVersionId: null,
        questionMap: input.data.questionMap,
        approval: null,
        response: null,
        revision: "1",
        createdAt: now,
        updatedAt: now,
      });
      insertInteraction(state, record, digest);
      return {
        ok: true as const,
        value: { question: created.value.question, interaction: record },
      };
    });
  } catch {
    return failure("storage_failure", "native question transaction failed");
  }
}

export function recordNativeApproval(
  state: WorkspaceState,
  raw: NativeApprovalRecordInput,
): WorkspaceResult<NativeApprovalRequest> {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = NativeApprovalRecordInputSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "native approval binding is malformed");
  const digest = requestDigest(input.data);
  try {
    return state.database.transaction(() => {
      const existing = interactionByIdentity(state, input.data.identity);
      if (existing !== null) {
        if (existing.request_digest !== digest)
          return failure("idempotency_conflict", "native request identity changed content");
        const record = state.parse(existing.public_json, NativeInteractionRecordSchema);
        return record.approval === null
          ? failure("storage_failure", "native approval binding lost its projection")
          : { ok: true as const, value: record.approval };
      }
      const now = state.now();
      const record = NativeInteractionRecordSchema.parse({
        projectId: input.data.approval.projectId,
        contextId: input.data.approval.contextId,
        originActorId: input.data.approval.originActorId,
        identity: input.data.identity,
        kind: input.data.approval.kind,
        state: "pending",
        questionGroupId: null,
        answerVersionId: null,
        questionMap: null,
        approval: input.data.approval,
        response: null,
        revision: "1",
        createdAt: now,
        updatedAt: now,
      });
      insertInteraction(state, record, digest);
      appendInteractionHistory(state, record, "native-approval.pending");
      return { ok: true as const, value: input.data.approval };
    });
  } catch {
    return failure("storage_failure", "native approval transaction failed");
  }
}

export function markNativeQuestionAnswered(
  state: WorkspaceState,
  question: QuestionGroup,
  answer: QuestionAnswerVersion,
  now: string,
): void {
  const row = state.database.get(
    "SELECT public_json FROM workspace_native_interactions WHERE question_group_id = ?",
    PublicRowSchema,
    [question.questionGroupId],
  );
  if (row === null) return;
  const current = state.parse(row.public_json, NativeInteractionRecordSchema);
  if (current.state !== "pending") return;
  const updated = NativeInteractionRecordSchema.parse({
    ...current,
    state: "answer_ready",
    answerVersionId: answer.answerVersionId,
    revision: nextRevision(current.revision),
    updatedAt: now,
  });
  updateInteraction(state, updated);
}

export function respondNativeApprovalInTransaction(
  state: WorkspaceState,
  access: WorkspaceCommandContext,
  request: Extract<WorkspaceCommandRequest, { operation: "native-approval.respond.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  if (!access.authorizedProjectIds.includes(request.projectId))
    return failure("forbidden", "native approval response is outside scope");
  const row = state.database.get(
    "SELECT public_json FROM workspace_native_interactions WHERE native_approval_id = ?",
    PublicRowSchema,
    [request.nativeApprovalId],
  );
  if (row === null) return failure("not_found", "native approval does not exist");
  const record = state.parse(row.public_json, NativeInteractionRecordSchema);
  if (record.projectId !== request.projectId || record.contextId !== request.contextId)
    return failure("forbidden", "native approval belongs to another project context");
  if (record.approval === null)
    return failure("storage_failure", "native approval projection is missing");
  if (record.revision !== request.expectedRevision || record.state !== "pending")
    return failure("stale_revision", "native approval is no longer pending");
  if (record.kind === "permission_approval")
    return failure("unsupported_operation", "permission approval response is not mapped safely");
  const now = state.now();
  const approval = NativeApprovalRequestSchema.parse({
    ...record.approval,
    state: "answer_ready",
    revision: nextRevision(record.revision),
    updatedAt: now,
  });
  const updated = NativeInteractionRecordSchema.parse({
    ...record,
    approval,
    state: "answer_ready",
    response: request.response,
    revision: approval.revision,
    updatedAt: now,
  });
  updateInteraction(state, updated);
  appendInteractionHistory(state, updated, "native-approval.answer-ready");
  return {
    ok: true as const,
    value: { operation: "native-approval.respond.v1" as const, approval },
  };
}

export function readNativeInteractionForQuestion(state: WorkspaceState, questionGroupId: string) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const row = state.database.get(
    "SELECT public_json FROM workspace_native_interactions WHERE question_group_id = ?",
    PublicRowSchema,
    [questionGroupId],
  );
  return {
    ok: true as const,
    value: row === null ? null : state.parse(row.public_json, NativeInteractionRecordSchema),
  };
}

export function prepareNativeResponse(state: WorkspaceState, raw: NativeResponsePreparation) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = NativeResponsePreparationSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "native response preparation is malformed");
  try {
    return state.database.transaction(() => {
      const row = interactionByPublicId(
        state,
        input.data.questionGroupId,
        input.data.nativeApprovalId,
      );
      if (row === null) return failure("not_found", "native interaction does not exist");
      const record = state.parse(row.public_json, NativeInteractionRecordSchema);
      if (record.projectId !== input.data.projectId || record.contextId !== input.data.contextId)
        return failure("forbidden", "native interaction belongs to another project context");
      if (record.identity.processEpoch !== input.data.currentProcessEpoch)
        return failure("stale_revision", "native interaction belongs to an earlier process epoch");
      if (record.revision !== input.data.expectedRevision || record.state !== "answer_ready")
        return failure("stale_revision", "native interaction is not ready at this revision");
      const updated = NativeInteractionRecordSchema.parse({
        ...record,
        state: "response_inflight",
        response: input.data.response,
        revision: nextRevision(record.revision),
        updatedAt: input.data.updatedAt,
        approval:
          record.approval === null
            ? null
            : {
                ...record.approval,
                state: "response_inflight",
                revision: nextRevision(record.revision),
                updatedAt: input.data.updatedAt,
              },
      });
      updateInteraction(state, updated);
      return { ok: true as const, value: updated };
    });
  } catch {
    return failure("storage_failure", "native response preparation failed");
  }
}

export function settleNativeResponse(state: WorkspaceState, raw: NativeResponseSettlement) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = NativeResponseSettlementSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "native response settlement is malformed");
  try {
    return state.database.transaction(() => {
      const row = interactionByIdentity(state, {
        coordinatorSessionId: input.data.coordinatorSessionId,
        processEpoch: input.data.processEpoch,
        requestId: input.data.requestId,
      });
      if (row === null) return failure("not_found", "native interaction does not exist");
      const record = state.parse(row.public_json, NativeInteractionRecordSchema);
      if (record.identity.processEpoch !== input.data.processEpoch)
        return failure("stale_revision", "native interaction process epoch changed");
      if (record.state === input.data.observation) return { ok: true as const, value: record };
      const allowed =
        (input.data.observation === "host_accepted" && record.state === "response_inflight") ||
        (input.data.observation === "resolved" &&
          (record.state === "pending" ||
            record.state === "answer_ready" ||
            record.state === "response_inflight" ||
            record.state === "host_accepted" ||
            record.state === "uncertain")) ||
        ((input.data.observation === "refused" || input.data.observation === "uncertain") &&
          record.state === "response_inflight") ||
        (input.data.observation === "stale" &&
          (record.state === "pending" || record.state === "answer_ready"));
      if (!allowed)
        return failure("conflict", "native interaction state does not accept this observation");
      const updated = NativeInteractionRecordSchema.parse({
        ...record,
        state: input.data.observation,
        revision: nextRevision(record.revision),
        updatedAt: input.data.updatedAt,
        approval:
          record.approval === null
            ? null
            : {
                ...record.approval,
                state: input.data.observation,
                revision: nextRevision(record.revision),
                updatedAt: input.data.updatedAt,
              },
      });
      updateInteraction(state, updated);
      if (input.data.observation === "resolved") closeResolvedQuestion(state, updated);
      appendInteractionHistory(state, updated, `native-interaction.${input.data.observation}`);
      return { ok: true as const, value: updated };
    });
  } catch {
    return failure("storage_failure", "native response settlement failed");
  }
}

export function pendingNativeResponses(
  state: WorkspaceState,
  projectId: string,
  contextId: string,
  limit: number,
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  if (!Number.isInteger(limit) || limit < 1 || limit > 256)
    return failure("invalid_input", "native response page limit is invalid");
  const rows = state.database.all(
    `SELECT public_json FROM workspace_native_interactions
     WHERE project_id = ? AND context_id = ? AND state = 'answer_ready'
     ORDER BY updated_at LIMIT ?`,
    PublicRowSchema,
    [projectId, contextId, limit],
  );
  return {
    ok: true as const,
    value: rows.map((row) => state.parse(row.public_json, NativeInteractionRecordSchema)),
  };
}

function insertInteraction(
  state: WorkspaceState,
  record: NativeInteractionRecord,
  digest: string,
): void {
  const id = record.identity.requestId;
  state.database.run(
    `INSERT INTO workspace_native_interactions(
       coordinator_session_id, process_epoch, request_id_type, request_id_value,
       project_id, context_id, kind, state, revision, question_group_id,
       native_approval_id, request_digest, updated_at, public_json
     ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      record.identity.coordinatorSessionId,
      record.identity.processEpoch,
      typeof id,
      String(id),
      record.projectId,
      record.contextId,
      record.kind,
      record.state,
      BigInt(record.revision),
      record.questionGroupId,
      record.approval?.nativeApprovalId ?? null,
      digest,
      record.updatedAt,
      state.json(record),
    ],
  );
}

function updateInteraction(state: WorkspaceState, record: NativeInteractionRecord): void {
  const id = record.identity.requestId;
  state.database.run(
    `UPDATE workspace_native_interactions SET state = ?, revision = ?, updated_at = ?, public_json = ?
     WHERE coordinator_session_id = ? AND process_epoch = ? AND request_id_type = ? AND request_id_value = ?`,
    [
      record.state,
      BigInt(record.revision),
      record.updatedAt,
      state.json(record),
      record.identity.coordinatorSessionId,
      record.identity.processEpoch,
      typeof id,
      String(id),
    ],
  );
}

function interactionByIdentity(
  state: WorkspaceState,
  identity: Pick<
    NativeInteractionRecord["identity"],
    "coordinatorSessionId" | "processEpoch" | "requestId"
  >,
) {
  return state.database.get(
    `SELECT public_json, request_digest FROM workspace_native_interactions
     WHERE coordinator_session_id = ? AND process_epoch = ? AND request_id_type = ? AND request_id_value = ?`,
    StoredRowSchema,
    [
      identity.coordinatorSessionId,
      identity.processEpoch,
      typeof identity.requestId,
      String(identity.requestId),
    ],
  );
}

function interactionByPublicId(
  state: WorkspaceState,
  questionGroupId: string | null,
  approvalId: string | null,
) {
  return state.database.get(
    `SELECT public_json FROM workspace_native_interactions
     WHERE (? IS NOT NULL AND question_group_id = ?) OR (? IS NOT NULL AND native_approval_id = ?)`,
    PublicRowSchema,
    [questionGroupId, questionGroupId, approvalId, approvalId],
  );
}

function questionById(state: WorkspaceState, id: string): QuestionGroup | null {
  const row = state.database.get(
    "SELECT public_json FROM workspace_questions WHERE question_group_id = ?",
    PublicRowSchema,
    [id],
  );
  return row === null ? null : state.parse(row.public_json, QuestionGroupSchema);
}

function closeResolvedQuestion(state: WorkspaceState, record: NativeInteractionRecord): void {
  if (record.questionGroupId === null) return;
  const question = questionById(state, record.questionGroupId);
  if (question === null || question.state !== "open") return;
  const revision = nextRevision(question.revision);
  const updated = QuestionGroupSchema.parse({
    ...question,
    state: "cancelled",
    revision,
    updatedAt: record.updatedAt,
  });
  state.database.run(
    `UPDATE workspace_questions SET state = ?, revision = ?, public_json = ?
     WHERE question_group_id = ? AND revision = ?`,
    [
      updated.state,
      BigInt(revision),
      state.json(updated),
      updated.questionGroupId,
      BigInt(question.revision),
    ],
  );
}

function requestDigest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function nextRevision(value: string): z.infer<typeof DecimalSchema> {
  return DecimalSchema.parse(String(BigInt(value) + 1n));
}

function appendInteractionHistory(
  state: WorkspaceState,
  record: NativeInteractionRecord,
  kind: string,
): void {
  state.appendHistory({
    projectId: record.projectId,
    contextId: record.contextId,
    kind,
    source: "host",
    actorId: record.originActorId,
    occurrenceAt: record.updatedAt,
    sourceEventId: `native:${record.identity.processEpoch}:${typeof record.identity.requestId}:${String(record.identity.requestId)}:${record.revision}`,
    sourceSequence: null,
    correlationId: record.questionGroupId ?? record.approval?.nativeApprovalId ?? null,
    causationId: null,
    planProvenance: null,
    payload: { state: record.state, kind: record.kind },
  });
}
