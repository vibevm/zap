/** Durable workspace chat/question commands. @scope spec://org.vibevm.zap/lens/PROP-005#simultaneous-clients */
import { createHash } from "node:crypto";
import { z } from "zod";
import { DecimalSchema, MessageIdSchema } from "../protocol/index.ts";
import {
  AnswerVersionIdSchema,
  ChatMessageSchema,
  QuestionAnswerVersionSchema,
  QuestionGroupIdSchema,
  QuestionGroupSchema,
  WorkspaceCommandRequestSchema,
  WorkspaceCommandResponseSchema,
  type QuestionAnswer,
  type QuestionGroup,
  type QuestionSubmission,
  type WorkspaceCommandContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { failure } from "./errors.ts";
import { markNativeQuestionAnswered, respondNativeApprovalInTransaction } from "./interactions.ts";
import { markAgentQuestionAnswered } from "./agent-questions.ts";
import type { WorkspaceState } from "./state.ts";

const IdempotencyRowSchema = z.object({ request_digest: z.string(), result_json: z.string() });
const JsonRowSchema = z.object({ public_json: z.string(), revision: z.bigint() });
const AnswerRowSchema = z.object({ public_json: z.string() });

export function commandWorkspace(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  rawRequest: WorkspaceCommandRequest,
): WorkspaceResult<WorkspaceCommandResponse> {
  const parsed = WorkspaceCommandRequestSchema.safeParse(rawRequest);
  if (!parsed.success) return failure("invalid_input", "workspace command is malformed");
  const request = parsed.data;
  if (!context.authorizedProjectIds.includes(request.projectId)) {
    return failure("forbidden", "workspace command is outside project scope");
  }
  if (!contextExists(state, request.projectId, request.contextId)) {
    return failure("not_found", "workspace command context does not exist");
  }
  try {
    return state.database.transaction(() => commandWorkspaceInTransaction(state, context, request));
  } catch {
    return failure("storage_failure", "workspace command transaction failed");
  }
}

/** Internal same-cell seam for atomic trusted extensions that add records to a command. */
export function commandWorkspaceInTransaction(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: WorkspaceCommandRequest,
): WorkspaceResult<WorkspaceCommandResponse> {
  return idempotent(state, context, request);
}

function idempotent(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: WorkspaceCommandRequest,
): WorkspaceResult<WorkspaceCommandResponse> {
  const digest = createHash("sha256").update(JSON.stringify(request)).digest("hex");
  const actorKey = context.actorId ?? `client:${context.clientId}`;
  const parameters = [
    context.principalId,
    actorKey,
    request.projectId,
    request.contextId,
    request.clientRequestId,
  ];
  const existing = state.database.get(
    `SELECT request_digest, result_json FROM workspace_idempotency
     WHERE principal_id = ? AND actor_key = ? AND project_id = ? AND context_id = ?
       AND client_request_id = ?`,
    IdempotencyRowSchema,
    parameters,
  );
  if (existing !== null) {
    return existing.request_digest === digest
      ? {
          ok: true,
          value: WorkspaceCommandResponseSchema.parse(JSON.parse(existing.result_json)),
        }
      : failure("idempotency_conflict", "client request identity was reused with new content");
  }
  const result = execute(state, context, request);
  if (!result.ok) return result;
  state.database.run(
    `INSERT INTO workspace_idempotency(
       principal_id, actor_key, project_id, context_id, client_request_id,
       request_digest, result_json
     ) VALUES(?, ?, ?, ?, ?, ?, ?)`,
    [...parameters, digest, state.json(result.value)],
  );
  return result;
}

function execute(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: WorkspaceCommandRequest,
): WorkspaceResult<WorkspaceCommandResponse> {
  switch (request.operation) {
    case "chat.post.v1":
      return postChat(state, context, request);
    case "question.create.v1":
      return createQuestion(state, context, request);
    case "question.answer.v1":
      return answerQuestion(state, context, request, null);
    case "question.amend.v1":
      return answerQuestion(state, context, request, request.amendmentReasonMarkdown);
    case "question.cancel.v1":
      return cancelQuestion(state, context, request);
    case "native-approval.respond.v1":
      return respondNativeApprovalInTransaction(state, context, request);
    case "session.start.v1":
    case "session.interrupt.v1":
    case "project.pause.v1":
    case "project.stop.v1":
    case "project.continue.v1":
    case "model-policy.update.v1":
    case "terminal.acquire.v1":
    case "terminal.start.v1":
    case "terminal.release.v1":
    case "terminal.input.v1":
    case "terminal.resize.v1":
    case "terminal.interrupt.v1":
    case "terminal.stop.v1":
    case "plan.intent.v1":
    case "plan.preview.v1":
    case "plan.apply.v1":
    case "plan.reconcile.v1":
    case "plan.decide.v1":
      return failure(
        "unsupported_operation",
        "execution actions require the shared application runtime and are not store commits",
      );
  }
}

function postChat(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: Extract<WorkspaceCommandRequest, { operation: "chat.post.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  const sequence = state.nextConversation(
    request.projectId,
    request.contextId,
    request.conversationId,
  );
  const now = state.now();
  const message = ChatMessageSchema.parse({
    messageId: MessageIdSchema.parse(state.id("message")),
    projectId: request.projectId,
    contextId: request.contextId,
    conversationId: request.conversationId,
    senderActorId: context.actorId,
    role: "user",
    bodyMarkdown: request.bodyMarkdown,
    artifactRefs: request.artifactRefs,
    correlationId: request.correlationId,
    causationMessageId: request.causationMessageId,
    deliveryState: "persisted",
    revision: DecimalSchema.parse(String(sequence)),
    createdAt: now,
    updatedAt: now,
  });
  state.database.run(
    `INSERT INTO workspace_chat(
       message_id, project_id, context_id, conversation_id, sequence, public_json
     ) VALUES(?, ?, ?, ?, ?, ?)`,
    [
      message.messageId,
      message.projectId,
      message.contextId,
      message.conversationId,
      sequence,
      state.json(message),
    ],
  );
  state.appendHistory({
    projectId: request.projectId,
    contextId: request.contextId,
    kind: "chat.message.persisted",
    source: "lens",
    actorId: context.actorId,
    occurrenceAt: now,
    sourceEventId: commandEventId(context, request.clientRequestId),
    sourceSequence: null,
    correlationId: request.correlationId,
    causationId: null,
    planProvenance: null,
    payload: { messageId: message.messageId, deliveryState: message.deliveryState },
  });
  return { ok: true, value: { operation: request.operation, message } };
}

function createQuestion(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: Extract<WorkspaceCommandRequest, { operation: "question.create.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  if (context.actorId === null) return failure("forbidden", "question origin requires an actor");
  const duplicate = duplicateQuestionIdentity(request.draft.items);
  if (duplicate !== null) return failure("invalid_input", duplicate);
  const now = state.now();
  const question = QuestionGroupSchema.parse({
    questionGroupId: QuestionGroupIdSchema.parse(state.id("question-group")),
    projectId: request.projectId,
    contextId: request.contextId,
    conversationId: request.conversationId,
    originActorId: context.actorId,
    messageId: MessageIdSchema.parse(state.id("message")),
    title: request.draft.title,
    introductionMarkdown: request.draft.introductionMarkdown,
    items: request.draft.items,
    independentWorkAvailable: request.draft.independentWorkAvailable,
    state: "open",
    revision: DecimalSchema.parse("1"),
    deadlineAt: request.draft.deadlineAt,
    createdAt: now,
    updatedAt: now,
  });
  state.database.run(
    `INSERT INTO workspace_questions(
       question_group_id, project_id, context_id, state, revision, public_json
     ) VALUES(?, ?, ?, ?, ?, ?)`,
    [
      question.questionGroupId,
      question.projectId,
      question.contextId,
      question.state,
      1n,
      state.json(question),
    ],
  );
  appendQuestionHistory(state, context, request, question, "question.created");
  return { ok: true, value: { operation: request.operation, question } };
}

function answerQuestion(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: Extract<
    WorkspaceCommandRequest,
    { operation: "question.answer.v1" | "question.amend.v1" }
  >,
  amendmentReasonMarkdown: string | null,
): WorkspaceResult<WorkspaceCommandResponse> {
  const row = questionRow(state, request.projectId, request.contextId, request.questionGroupId);
  if (row === null) return failure("not_found", "question group does not exist");
  const question = state.parse(row.public_json, QuestionGroupSchema);
  if (String(row.revision) !== request.expectedRevision) {
    return failure("stale_revision", "question revision changed");
  }
  const isAmend = request.operation === "question.amend.v1";
  if (question.deadlineAt !== null && Date.parse(question.deadlineAt) <= state.clock().getTime()) {
    const expiredRevision = row.revision + 1n;
    const expired = QuestionGroupSchema.parse({
      ...question,
      state: "expired",
      revision: DecimalSchema.parse(String(expiredRevision)),
      updatedAt: state.now(),
    });
    state.database.run(
      "UPDATE workspace_questions SET state = ?, revision = ?, public_json = ? WHERE question_group_id = ?",
      [expired.state, expiredRevision, state.json(expired), expired.questionGroupId],
    );
    appendQuestionHistory(state, context, request, expired, "question.expired");
    return failure("conflict", "question deadline has expired");
  }
  if ((!isAmend && question.state !== "open") || (isAmend && question.state !== "answered")) {
    return failure("conflict", "question state does not accept this answer operation");
  }
  const invalid = validateSubmission(question, request.submission);
  if (invalid !== null) return failure("invalid_input", invalid);
  const nextRevision = row.revision + 1n;
  const previous = state.database.get(
    `SELECT public_json FROM workspace_question_answers
     WHERE question_group_id = ? ORDER BY revision DESC LIMIT 1`,
    AnswerRowSchema,
    [question.questionGroupId],
  );
  const now = state.now();
  const answerVersion = QuestionAnswerVersionSchema.parse({
    answerVersionId: AnswerVersionIdSchema.parse(state.id("answer-version")),
    questionGroupId: question.questionGroupId,
    revision: DecimalSchema.parse(String(nextRevision)),
    previousVersionId:
      previous === null
        ? null
        : state.parse(previous.public_json, QuestionAnswerVersionSchema).answerVersionId,
    submission: request.submission,
    responderActorId: context.actorId,
    responderPrincipalId: context.principalId,
    amendmentReasonMarkdown,
    createdAt: now,
    metadata: {},
  });
  const updated = QuestionGroupSchema.parse({
    ...question,
    state: "answered",
    revision: DecimalSchema.parse(String(nextRevision)),
    updatedAt: now,
  });
  state.database.run(
    `INSERT INTO workspace_question_answers(question_group_id, revision, answer_version_id, public_json)
     VALUES(?, ?, ?, ?)`,
    [
      question.questionGroupId,
      nextRevision,
      answerVersion.answerVersionId,
      state.json(answerVersion),
    ],
  );
  markNativeQuestionAnswered(state, updated, answerVersion, now);
  markAgentQuestionAnswered(state, updated, answerVersion, now);
  state.database.run(
    `UPDATE workspace_questions SET state = ?, revision = ?, public_json = ?
     WHERE question_group_id = ? AND revision = ?`,
    [updated.state, nextRevision, state.json(updated), updated.questionGroupId, row.revision],
  );
  appendQuestionHistory(
    state,
    context,
    request,
    updated,
    isAmend ? "question.amended" : "question.answered",
  );
  return {
    ok: true,
    value: { operation: request.operation, question: updated, answerVersion },
  };
}

function cancelQuestion(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: Extract<WorkspaceCommandRequest, { operation: "question.cancel.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  const row = questionRow(state, request.projectId, request.contextId, request.questionGroupId);
  if (row === null) return failure("not_found", "question group does not exist");
  const question = state.parse(row.public_json, QuestionGroupSchema);
  if (String(row.revision) !== request.expectedRevision || question.state !== "open") {
    return failure("stale_revision", "question is no longer open at the expected revision");
  }
  const nextRevision = row.revision + 1n;
  const updated = QuestionGroupSchema.parse({
    ...question,
    state: "cancelled",
    revision: DecimalSchema.parse(String(nextRevision)),
    updatedAt: state.now(),
  });
  state.database.run(
    "UPDATE workspace_questions SET state = ?, revision = ?, public_json = ? WHERE question_group_id = ?",
    [updated.state, nextRevision, state.json(updated), updated.questionGroupId],
  );
  appendQuestionHistory(state, context, request, updated, "question.cancelled");
  return { ok: true, value: { operation: request.operation, question: updated } };
}

function validateSubmission(
  question: QuestionGroup,
  submission: QuestionSubmission,
): string | null {
  const answers = new Map(submission.answers.map((entry) => [entry.questionItemId, entry.answer]));
  if (answers.size !== submission.answers.length)
    return "question answer identities must be unique";
  for (const entry of submission.answers) {
    if (!question.items.some((item) => item.questionItemId === entry.questionItemId)) {
      return "question submission contains a foreign item";
    }
  }
  for (const item of question.items) {
    const answer = answers.get(item.questionItemId);
    if (answer === undefined) {
      if (item.required) return "required question item is unanswered";
      continue;
    }
    const invalid = validateAnswer(item, answer);
    if (invalid !== null) return invalid;
  }
  return null;
}

function validateAnswer(
  item: QuestionGroup["items"][number],
  answer: QuestionAnswer,
): string | null {
  if (answer.kind === "skipped")
    return item.required ? "required question item cannot be skipped" : null;
  if (answer.kind === "custom") {
    if (item.customAnswer?.allowed !== true) return "custom answer is disabled";
    return answer.text.length <= item.customAnswer.maximumLength
      ? null
      : "custom answer exceeds its declared maximum length";
  }
  if (answer.kind !== item.answerMode) return "question answer kind does not match its item";
  if (answer.kind === "single_choice") {
    return item.options.some((option) => option.optionId === answer.optionId)
      ? null
      : "selected option does not exist";
  }
  if (answer.kind === "multiple_choice") {
    const unique = new Set(answer.optionIds);
    return unique.size === answer.optionIds.length &&
      answer.optionIds.every((id) => item.options.some((option) => option.optionId === id))
      ? null
      : "multiple-choice answer contains duplicate or foreign options";
  }
  return null;
}

function duplicateQuestionIdentity(items: QuestionGroup["items"]): string | null {
  const itemIds = new Set(items.map((item) => item.questionItemId));
  if (itemIds.size !== items.length) return "question item identities must be unique";
  return items.some(
    (item) => new Set(item.options.map((option) => option.optionId)).size !== item.options.length,
  )
    ? "question option identities must be unique within an item"
    : null;
}

function appendQuestionHistory(
  state: WorkspaceState,
  context: WorkspaceCommandContext,
  request: Extract<
    WorkspaceCommandRequest,
    {
      operation:
        | "question.create.v1"
        | "question.answer.v1"
        | "question.amend.v1"
        | "question.cancel.v1";
    }
  >,
  question: QuestionGroup,
  kind: string,
): void {
  state.appendHistory({
    projectId: request.projectId,
    contextId: request.contextId,
    kind,
    source: "lens",
    actorId: context.actorId,
    occurrenceAt: state.now(),
    sourceEventId: commandEventId(context, request.clientRequestId),
    sourceSequence: null,
    correlationId: question.questionGroupId,
    causationId: null,
    planProvenance: null,
    payload: { questionGroupId: question.questionGroupId, revision: question.revision },
  });
}

function commandEventId(context: WorkspaceCommandContext, requestId: string): string {
  const actorKey = context.actorId ?? `client:${context.clientId}`;
  const digest = createHash("sha256")
    .update(JSON.stringify([context.principalId, actorKey, requestId]))
    .digest("hex");
  return `command:${digest}`;
}

function questionRow(state: WorkspaceState, projectId: string, contextId: string, groupId: string) {
  return state.database.get(
    `SELECT public_json, revision FROM workspace_questions
     WHERE project_id = ? AND context_id = ? AND question_group_id = ?`,
    JsonRowSchema,
    [projectId, contextId, groupId],
  );
}

function contextExists(state: WorkspaceState, projectId: string, contextId: string): boolean {
  return (
    state.database.get(
      "SELECT COUNT(*) AS revision, '{}' AS public_json FROM workspace_contexts WHERE project_id = ? AND context_id = ?",
      JsonRowSchema,
      [projectId, contextId],
    )?.revision === 1n
  );
}
