/** Rich cross-client question contracts. @scope spec://org.vibevm.zap/lens/PROP-005#rich-questions */
import { z } from "zod";
import {
  ActorIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  JsonValueSchema,
  MessageIdSchema,
} from "../protocol/index.ts";
import {
  AnswerVersionIdSchema,
  ArtifactRefIdSchema,
  ProjectIdSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  WorkContextIdSchema,
} from "./ids.ts";

export const ArtifactReferenceSchema = z
  .object({
    artifactRefId: ArtifactRefIdSchema,
    kind: z.enum(["file", "image", "document", "url", "project_object"]),
    label: z.string().min(1).max(256),
    mediaType: z.string().min(1).max(160).nullable(),
  })
  .strict();
export type ArtifactReference = z.infer<typeof ArtifactReferenceSchema>;

export const QuestionOptionSchema = z
  .object({
    optionId: QuestionOptionIdSchema,
    label: z.string().min(1).max(160),
    description: z.string().min(1).max(2_000),
    previewMarkdown: z.string().max(8_000).nullable(),
    artifactRefs: z.array(ArtifactReferenceSchema).max(16),
  })
  .strict();
export type QuestionOption = z.infer<typeof QuestionOptionSchema>;

const CustomAnswerSchema = z
  .object({
    allowed: z.boolean(),
    label: z.string().min(1).max(80),
    multiline: z.boolean(),
    maximumLength: z.number().int().min(1).max(32_000),
  })
  .strict();

export const QuestionItemSchema = z
  .object({
    questionItemId: QuestionItemIdSchema,
    header: z.string().min(1).max(80),
    promptMarkdown: z.string().min(1).max(16_000),
    contextMarkdown: z.string().max(16_000).nullable(),
    artifactRefs: z.array(ArtifactReferenceSchema).max(32),
    required: z.boolean(),
    answerMode: z.enum(["single_choice", "multiple_choice", "short_text", "multiline_text"]),
    options: z.array(QuestionOptionSchema).max(100),
    customAnswer: CustomAnswerSchema.nullable(),
    recommendation: z
      .object({
        optionIds: z.array(QuestionOptionIdSchema).max(100),
        explanationMarkdown: z.string().min(1).max(8_000),
      })
      .strict()
      .nullable(),
  })
  .strict()
  .superRefine((item, context) => {
    const choice = item.answerMode === "single_choice" || item.answerMode === "multiple_choice";
    if (choice !== item.options.length > 0) {
      context.addIssue({
        code: "custom",
        message: "choice modes require options; text modes forbid them",
      });
    }
    if (!choice && item.recommendation !== null) {
      context.addIssue({ code: "custom", message: "text questions cannot recommend options" });
    }
  });
export type QuestionItem = z.infer<typeof QuestionItemSchema>;

export const QuestionGroupDraftSchema = z
  .object({
    title: z.string().min(1).max(256),
    introductionMarkdown: z.string().max(16_000),
    items: z.array(QuestionItemSchema).min(1).max(32),
    independentWorkAvailable: z.boolean(),
    deadlineAt: z.iso.datetime().nullable(),
  })
  .strict();
export type QuestionGroupDraft = z.infer<typeof QuestionGroupDraftSchema>;

export const QuestionGroupStateSchema = z.enum(["open", "answered", "cancelled", "expired"]);
export const QuestionGroupSchema = z
  .object({
    questionGroupId: QuestionGroupIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    conversationId: ConversationIdSchema,
    originActorId: ActorIdSchema,
    messageId: MessageIdSchema,
    title: z.string().min(1).max(256),
    introductionMarkdown: z.string().max(16_000),
    items: z.array(QuestionItemSchema).min(1).max(32),
    independentWorkAvailable: z.boolean(),
    state: QuestionGroupStateSchema,
    revision: DecimalSchema,
    deadlineAt: z.iso.datetime().nullable(),
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type QuestionGroup = z.infer<typeof QuestionGroupSchema>;

export const QuestionAnswerSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("single_choice"), optionId: QuestionOptionIdSchema }).strict(),
  z
    .object({
      kind: z.literal("multiple_choice"),
      optionIds: z.array(QuestionOptionIdSchema).min(1),
    })
    .strict(),
  z.object({ kind: z.literal("short_text"), text: z.string().min(1).max(4_000) }).strict(),
  z.object({ kind: z.literal("multiline_text"), text: z.string().min(1).max(32_000) }).strict(),
  z.object({ kind: z.literal("custom"), text: z.string().min(1).max(32_000) }).strict(),
  z.object({ kind: z.literal("skipped") }).strict(),
]);
export type QuestionAnswer = z.infer<typeof QuestionAnswerSchema>;

export const QuestionSubmissionSchema = z
  .object({
    answers: z
      .array(
        z.object({ questionItemId: QuestionItemIdSchema, answer: QuestionAnswerSchema }).strict(),
      )
      .min(1)
      .max(32),
    noteMarkdown: z.string().max(8_000).nullable(),
  })
  .strict();
export type QuestionSubmission = z.infer<typeof QuestionSubmissionSchema>;

export const QuestionAnswerVersionSchema = z
  .object({
    answerVersionId: AnswerVersionIdSchema,
    questionGroupId: QuestionGroupIdSchema,
    revision: DecimalSchema,
    previousVersionId: AnswerVersionIdSchema.nullable(),
    submission: QuestionSubmissionSchema,
    responderActorId: ActorIdSchema.nullable(),
    responderPrincipalId: z.string().min(3).max(160),
    amendmentReasonMarkdown: z.string().max(8_000).nullable(),
    createdAt: z.iso.datetime(),
    metadata: JsonValueSchema,
  })
  .strict();
export type QuestionAnswerVersion = z.infer<typeof QuestionAnswerVersionSchema>;

export const QuestionDraftSchema = z
  .object({
    questionGroupId: QuestionGroupIdSchema,
    expectedRevision: DecimalSchema,
    submission: QuestionSubmissionSchema,
    savedAt: z.iso.datetime(),
  })
  .strict();
export type QuestionDraft = z.infer<typeof QuestionDraftSchema>;
