/** Native Codex request projection. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import type { z } from "zod";
import { PendingHostRequestSchema, type PendingHostRequest } from "../agent-runtime/index.ts";
import {
  CodexUserInputResponseSchema,
  UserInputRequestParamsSchema,
} from "../codex-coordinator/index.ts";
import { ClientRequestIdSchema, type ActorId } from "../protocol/index.ts";
import {
  NativeApprovalIdSchema,
  NativeApprovalRequestSchema,
  QuestionGroupDraftSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  type NativeHostRequestIdentity,
  type NativeInteractionRecord,
  type NativeQuestionMap,
  type ProjectId,
  type QuestionAnswerVersion,
  type QuestionGroupDraft,
  type WorkContextId,
} from "../workspace-model/index.ts";
import { interactionFailure, type InteractionResult } from "./types.ts";

export interface NativeProjectionScope {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly originActorId: ActorId;
}

export type NativeRequestProjection =
  | {
      readonly kind: "question";
      readonly clientRequestId: z.infer<typeof ClientRequestIdSchema>;
      readonly draft: QuestionGroupDraft;
      readonly questionMap: NativeQuestionMap;
      readonly identity: NativeHostRequestIdentity;
    }
  | {
      readonly kind: "approval";
      readonly approval: z.infer<typeof NativeApprovalRequestSchema>;
      readonly identity: NativeHostRequestIdentity;
    };

export function projectNativeRequest(
  raw: unknown,
  scope: NativeProjectionScope,
  id: (kind: string) => string,
  now: string,
): InteractionResult<NativeRequestProjection> {
  const pending = PendingHostRequestSchema.safeParse(raw);
  if (!pending.success) return interactionFailure("invalid_input", "native request is malformed");
  const identity = identityOf(pending.data);
  if (pending.data.kind !== "user_input") {
    const approval = NativeApprovalRequestSchema.safeParse({
      nativeApprovalId: idValue(NativeApprovalIdSchema, id("native-approval")),
      projectId: scope.projectId,
      contextId: scope.contextId,
      originActorId: scope.originActorId,
      coordinatorSessionId: pending.data.coordinatorSessionId,
      kind: pending.data.kind,
      title: approvalTitle(pending.data.kind),
      descriptionMarkdown: "Codex requested an explicit host approval.",
      details: {
        nativeThreadId: pending.data.nativeThreadId,
        nativeTurnId: pending.data.nativeTurnId,
        nativeItemId: pending.data.nativeItemId,
      },
      state: "pending",
      responseAvailability:
        pending.data.kind === "permission_approval"
          ? { enabled: false, reason: "This Codex permission response shape is not mapped safely." }
          : { enabled: true, reason: null },
      revision: "1",
      createdAt: now,
      updatedAt: now,
    });
    return approval.success
      ? { ok: true, value: { kind: "approval", approval: approval.data, identity } }
      : interactionFailure("invalid_input", "native approval projection is invalid");
  }
  return projectQuestions(pending.data, identity, id);
}

export function nativeQuestionResponse(
  record: NativeInteractionRecord,
  version: QuestionAnswerVersion,
): InteractionResult<z.infer<typeof CodexUserInputResponseSchema>> {
  if (
    record.kind !== "user_input" ||
    record.questionGroupId !== version.questionGroupId ||
    record.questionMap === null
  ) {
    return interactionFailure("conflict", "answer does not match the native question binding");
  }
  const submitted = new Map(
    version.submission.answers.map((entry) => [entry.questionItemId, entry.answer]),
  );
  const answers: Record<string, { answers: string[] }> = {};
  for (const mapping of record.questionMap) {
    const answer = submitted.get(mapping.questionItemId);
    if (answer === undefined) {
      return interactionFailure("conflict", "native question answer is incomplete");
    }
    if (answer.kind === "single_choice") {
      const option = mapping.options.find((item) => item.optionId === answer.optionId);
      if (option === undefined)
        return interactionFailure("conflict", "native answer selected a foreign option");
      answers[mapping.nativeQuestionId] = { answers: [option.answerText] };
    } else if (
      answer.kind === "short_text" ||
      answer.kind === "multiline_text" ||
      answer.kind === "custom"
    ) {
      answers[mapping.nativeQuestionId] = { answers: [answer.text] };
    } else if (answer.kind === "skipped") {
      answers[mapping.nativeQuestionId] = { answers: [] };
    } else {
      return interactionFailure(
        "unsupported_operation",
        "native multiple-choice input is unsupported",
      );
    }
  }
  const response = CodexUserInputResponseSchema.safeParse({ answers });
  return response.success
    ? { ok: true, value: response.data }
    : interactionFailure("invalid_input", "native user-input response is invalid");
}

function projectQuestions(
  pending: PendingHostRequest,
  identity: NativeHostRequestIdentity,
  id: (kind: string) => string,
): InteractionResult<NativeRequestProjection> {
  const body = UserInputRequestParamsSchema.safeParse(pending.body);
  if (!body.success)
    return interactionFailure("invalid_input", "Codex user-input body is malformed");
  if (body.data.questions.some((question) => question.isSecret === true)) {
    return interactionFailure(
      "unsupported_operation",
      "secret native input cannot be persisted as an ordinary rich question",
    );
  }
  if (
    new Set(body.data.questions.map((question) => question.id)).size !== body.data.questions.length
  ) {
    return interactionFailure("invalid_input", "native question identities are duplicated");
  }
  const nativeIds = new Set<string>();
  const questionMap: NativeQuestionMap = [];
  const items = body.data.questions.map((question) => {
    nativeIds.add(question.id);
    const questionItemId = idValue(QuestionItemIdSchema, id("question-item"));
    const options = (question.options ?? []).map((option) => {
      const optionId = idValue(QuestionOptionIdSchema, id("question-option"));
      return {
        optionId,
        label: option.label,
        description: option.description,
        previewMarkdown: null,
        artifactRefs: [],
        answerText: option.label,
      };
    });
    questionMap.push({
      questionItemId,
      nativeQuestionId: question.id,
      options: options.map(({ optionId, answerText }) => ({ optionId, answerText })),
    });
    return {
      questionItemId,
      header: question.header.slice(0, 80),
      promptMarkdown: question.question,
      contextMarkdown: null,
      artifactRefs: [],
      required: true,
      answerMode: options.length === 0 ? ("short_text" as const) : ("single_choice" as const),
      options: options.map((option) => ({
        optionId: option.optionId,
        label: option.label,
        description: option.description,
        previewMarkdown: option.previewMarkdown,
        artifactRefs: option.artifactRefs,
      })),
      customAnswer:
        question.isOther === true
          ? { allowed: true, label: "Other", multiline: false, maximumLength: 4_000 }
          : null,
      recommendation: null,
    };
  });
  const draft = QuestionGroupDraftSchema.safeParse({
    title: items.length === 1 ? items[0]?.header : "Codex needs your input",
    introductionMarkdown: "This question was requested by the active Codex session.",
    items,
    independentWorkAvailable: !body.data.isBlocking,
    deadlineAt: null,
  });
  const clientRequestId = ClientRequestIdSchema.safeParse(
    id(`native-question-${typeof pending.requestId}-${String(pending.requestId)}`),
  );
  if (!draft.success || !clientRequestId.success) {
    return interactionFailure("invalid_input", "native question projection identities are invalid");
  }
  return {
    ok: true,
    value: {
      kind: "question",
      clientRequestId: clientRequestId.data,
      draft: draft.data,
      questionMap,
      identity,
    },
  };
}

function identityOf(pending: PendingHostRequest): NativeHostRequestIdentity {
  return {
    coordinatorSessionId: pending.coordinatorSessionId,
    processEpoch: pending.processEpoch,
    requestId: pending.requestId,
    nativeThreadId: pending.nativeThreadId,
    nativeTurnId: pending.nativeTurnId,
    nativeItemId: pending.nativeItemId,
  };
}

function approvalTitle(kind: PendingHostRequest["kind"]): string {
  if (kind === "command_approval") return "Approve Codex command";
  if (kind === "file_approval") return "Approve Codex file change";
  return "Approve Codex permissions";
}

function idValue<S extends z.ZodType>(schema: S, value: string): z.output<S> {
  return schema.parse(value);
}
