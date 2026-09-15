/** Browser-safe native interaction projections. @scope spec://org.vibevm.zap/lens/PROP-005#rich-questions */
import { z } from "zod";
import {
  ActorHandleSchema,
  ActorIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  JsonValueSchema,
  MessageIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import { AgentSessionIdSchema, ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";
import {
  AnswerVersionIdSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
} from "./ids.ts";

export const NativeApprovalIdSchema = z
  .string()
  .min(3)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/)
  .brand<"NativeApprovalId">();
export type NativeApprovalId = z.infer<typeof NativeApprovalIdSchema>;

export const NativeApprovalKindSchema = z.enum([
  "command_approval",
  "file_approval",
  "permission_approval",
]);
export type NativeApprovalKind = z.infer<typeof NativeApprovalKindSchema>;

export const NativeInteractionStateSchema = z.enum([
  "pending",
  "answer_ready",
  "response_inflight",
  "host_accepted",
  "resolved",
  "refused",
  "uncertain",
  "stale",
]);
export type NativeInteractionState = z.infer<typeof NativeInteractionStateSchema>;

export const NativeApprovalRequestSchema = z
  .object({
    nativeApprovalId: NativeApprovalIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    coordinatorSessionId: AgentSessionIdSchema,
    originActorId: ActorIdSchema,
    kind: NativeApprovalKindSchema,
    title: z.string().min(1).max(256),
    descriptionMarkdown: z.string().max(32_000),
    details: JsonValueSchema,
    state: NativeInteractionStateSchema,
    responseAvailability: z
      .object({ enabled: z.boolean(), reason: z.string().min(1).max(2_000).nullable() })
      .strict(),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type NativeApprovalRequest = z.infer<typeof NativeApprovalRequestSchema>;

export const NativeApprovalDecisionSchema = z
  .object({ decision: z.enum(["accept", "acceptForSession", "decline", "cancel"]) })
  .strict();
export type NativeApprovalDecision = z.infer<typeof NativeApprovalDecisionSchema>;

export const NativeHostRequestIdentitySchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    processEpoch: z.string().min(1).max(160),
    requestId: z.union([z.string().min(1).max(512), z.number().int()]),
    nativeThreadId: z.string().min(1).max(512),
    nativeTurnId: z.string().min(1).max(512),
    nativeItemId: z.string().min(1).max(512),
  })
  .strict();
export type NativeHostRequestIdentity = z.infer<typeof NativeHostRequestIdentitySchema>;

export const NativeQuestionMapSchema = z
  .array(
    z
      .object({
        questionItemId: QuestionItemIdSchema,
        nativeQuestionId: z.string().min(1).max(256),
        options: z
          .array(
            z
              .object({ optionId: QuestionOptionIdSchema, answerText: z.string().max(10_000) })
              .strict(),
          )
          .max(1_000),
      })
      .strict(),
  )
  .min(1)
  .max(100);
export type NativeQuestionMap = z.infer<typeof NativeQuestionMapSchema>;

export const NativeInteractionRecordSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    originActorId: ActorIdSchema,
    identity: NativeHostRequestIdentitySchema,
    kind: z.enum(["user_input", "command_approval", "file_approval", "permission_approval"]),
    state: NativeInteractionStateSchema,
    questionGroupId: QuestionGroupIdSchema.nullable(),
    answerVersionId: AnswerVersionIdSchema.nullable(),
    questionMap: NativeQuestionMapSchema.nullable(),
    approval: NativeApprovalRequestSchema.nullable(),
    response: JsonValueSchema.nullable(),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type NativeInteractionRecord = z.infer<typeof NativeInteractionRecordSchema>;

export const AgentQuestionBindingSchema = z
  .object({
    questionGroupId: QuestionGroupIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    actorId: ActorIdSchema,
    parentActorId: ActorIdSchema.nullable().default(null),
    actorHandle: ActorHandleSchema,
    state: z.enum(["pending", "answer_ready", "persisted", "refused", "uncertain"]),
    answerVersionId: AnswerVersionIdSchema.nullable(),
    messageId: MessageIdSchema.nullable(),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type AgentQuestionBinding = z.infer<typeof AgentQuestionBindingSchema>;
