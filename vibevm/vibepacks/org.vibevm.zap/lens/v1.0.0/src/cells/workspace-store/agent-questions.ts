/** Durable broker-actor rich-question bindings. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";
import {
  AgentQuestionBindingSchema,
  type QuestionAnswerVersion,
  type QuestionGroup,
} from "../workspace-model/index.ts";
import { commandWorkspaceInTransaction } from "./commands.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";
import {
  AgentAnswerSettlementSchema,
  AgentQuestionRecordInputSchema,
  type AgentAnswerSettlement,
  type AgentQuestionRecordInput,
} from "./types.ts";

const PublicRowSchema = z.object({ public_json: z.string() });

export function recordAgentQuestion(state: WorkspaceState, raw: AgentQuestionRecordInput) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = AgentQuestionRecordInputSchema.safeParse(raw);
  if (
    !input.success ||
    input.data.access.actorId !== input.data.actor.actor.actorId ||
    input.data.actor.handle.actorId !== input.data.actor.actor.actorId
  ) {
    return failure("forbidden", "agent question binding is not broker-consistent");
  }
  try {
    return state.database.transaction(() => {
      const command = {
        operation: "question.create.v1" as const,
        clientRequestId: input.data.clientRequestId,
        projectId: input.data.projectId,
        contextId: input.data.contextId,
        conversationId: input.data.actor.actor.conversationId,
        draft: input.data.draft,
      };
      const created = commandWorkspaceInTransaction(state, input.data.access, command);
      if (!created.ok) return created;
      if (created.value.operation !== "question.create.v1")
        return failure("storage_failure", "agent question command returned another operation");
      const loaded = readAgentQuestionBinding(state, created.value.question.questionGroupId);
      if (!loaded.ok) return loaded;
      const existing = loaded.value;
      if (existing !== null)
        return {
          ok: true as const,
          value: { question: created.value.question, binding: existing },
        };
      const now = state.now();
      const binding = AgentQuestionBindingSchema.parse({
        questionGroupId: created.value.question.questionGroupId,
        projectId: input.data.projectId,
        contextId: input.data.contextId,
        workspaceId: input.data.actor.actor.workspaceId,
        conversationId: input.data.actor.actor.conversationId,
        actorId: input.data.actor.actor.actorId,
        parentActorId: input.data.actor.actor.parentActorId,
        actorHandle: input.data.actor.handle,
        state: "pending",
        answerVersionId: null,
        messageId: null,
        revision: "1",
        createdAt: now,
        updatedAt: now,
      });
      state.database.run(
        `INSERT INTO workspace_agent_question_bindings(
           question_group_id, project_id, context_id, actor_id, state, revision, public_json
         ) VALUES(?, ?, ?, ?, ?, ?, ?)`,
        [
          binding.questionGroupId,
          binding.projectId,
          binding.contextId,
          binding.actorId,
          binding.state,
          1n,
          state.json(binding),
        ],
      );
      return { ok: true as const, value: { question: created.value.question, binding } };
    });
  } catch {
    return failure("storage_failure", "agent question transaction failed");
  }
}

export function markAgentQuestionAnswered(
  state: WorkspaceState,
  question: QuestionGroup,
  answer: QuestionAnswerVersion,
  now: string,
): void {
  const result = readAgentQuestionBinding(state, question.questionGroupId);
  if (!result.ok || result.value === null || result.value.state !== "pending") return;
  const loaded = result.value;
  updateAgentBinding(state, {
    ...loaded,
    state: "answer_ready",
    answerVersionId: answer.answerVersionId,
    revision: nextRevision(loaded.revision),
    updatedAt: now,
  });
}

export function readAgentQuestionBinding(state: WorkspaceState, questionGroupId: string) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const row = state.database.get(
    "SELECT public_json FROM workspace_agent_question_bindings WHERE question_group_id = ?",
    PublicRowSchema,
    [questionGroupId],
  );
  return {
    ok: true as const,
    value: row === null ? null : state.parse(row.public_json, AgentQuestionBindingSchema),
  };
}

export function settleAgentAnswer(state: WorkspaceState, raw: AgentAnswerSettlement) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = AgentAnswerSettlementSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "agent answer settlement is malformed");
  try {
    return state.database.transaction(() => {
      const loaded = readAgentQuestionBinding(state, input.data.questionGroupId);
      if (!loaded.ok) return loaded;
      const binding = loaded.value;
      if (binding === null) return failure("not_found", "agent question binding does not exist");
      if (binding.revision !== input.data.expectedRevision || binding.state !== "answer_ready")
        return failure("stale_revision", "agent answer is no longer ready for delivery");
      const updated = AgentQuestionBindingSchema.parse({
        ...binding,
        state: input.data.observation,
        messageId: input.data.messageId,
        revision: nextRevision(binding.revision),
        updatedAt: input.data.updatedAt,
      });
      updateAgentBinding(state, updated);
      return { ok: true as const, value: updated };
    });
  } catch {
    return failure("storage_failure", "agent answer settlement failed");
  }
}

function updateAgentBinding(
  state: WorkspaceState,
  binding: z.infer<typeof AgentQuestionBindingSchema>,
): void {
  state.database.run(
    `UPDATE workspace_agent_question_bindings
     SET state = ?, revision = ?, public_json = ? WHERE question_group_id = ?`,
    [binding.state, BigInt(binding.revision), state.json(binding), binding.questionGroupId],
  );
}

function nextRevision(value: string) {
  return DecimalSchema.parse(String(BigInt(value) + 1n));
}
