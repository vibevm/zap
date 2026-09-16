/** Browser-safe managed-work commands. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { z } from "zod";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import { ProjectObjectReferenceSchema } from "./object-reference.ts";
import { ProjectIdSchema, RunIdSchema, TaskIdSchema, WorkContextIdSchema } from "./ids.ts";
import { ModelSelectionSchema } from "./model-policy.ts";

const Scope = z.object({
  clientRequestId: ClientRequestIdSchema,
  projectId: ProjectIdSchema,
  contextId: WorkContextIdSchema,
});
export const ManagedWorkSelectionSchema = z.discriminatedUnion("mode", [
  z.object({ mode: z.literal("project_policy") }).strict(),
  z
    .object({
      mode: z.literal("profile_override"),
      profileId: z.string().min(3).max(160),
      reasonMarkdown: z.string().min(1).max(2_000),
    })
    .strict(),
]);
export const ManagedWorkCreateCommandSchema = Scope.extend({
  operation: z.literal("managed-work.create.v1"),
  selection: ManagedWorkSelectionSchema.default({ mode: "project_policy" }),
  goal: z.string().min(1).max(1_000_000),
  expectedResult: z.string().min(1).max(100_000),
  targetRefs: z.array(ProjectObjectReferenceSchema).max(256),
  contextRefs: z.array(z.string().min(3).max(512)).max(1_000).optional(),
  parentTaskId: TaskIdSchema.nullable().optional(),
  parentRunId: RunIdSchema.nullable().optional(),
  sourceBasisRef: z.string().min(1).max(512).optional(),
  planRevision: DecimalSchema.nullable().optional(),
  depth: z.number().int().min(0).max(100).optional(),
  budgets: z
    .object({
      maximumTurns: z.number().int().min(1).max(100_000),
      wallTimeMs: z.number().int().min(1_000).max(86_400_000),
    })
    .strict()
    .optional(),
}).strict();
const Existing = Scope.extend({ runId: RunIdSchema, expectedRevision: DecimalSchema });
export const ManagedWorkStartCommandSchema = Existing.extend({
  operation: z.literal("managed-work.start.v1"),
}).strict();
export const ManagedWorkStopCommandSchema = Existing.extend({
  operation: z.literal("managed-work.stop.v1"),
}).strict();
export const ManagedWorkInterruptCommandSchema = Existing.extend({
  operation: z.literal("managed-work.interrupt.v1"),
}).strict();
export const ManagedWorkReportCommandSchema = Existing.extend({
  operation: z.literal("managed-work.report.v1"),
  summaryMarkdown: z.string().min(1).max(100_000),
  artifactRefs: z.array(z.string().min(3).max(512)).max(256),
}).strict();
export const ManagedWorkReviewCommandSchema = Existing.extend({
  operation: z.literal("managed-work.review.v1"),
  disposition: z.enum(["accepted", "follow_up_required"]),
  commentMarkdown: z.string().max(100_000),
}).strict();
export const ManagedWorkCommandSchemas = [
  ManagedWorkCreateCommandSchema,
  ManagedWorkStartCommandSchema,
  ManagedWorkStopCommandSchema,
  ManagedWorkInterruptCommandSchema,
  ManagedWorkReportCommandSchema,
  ManagedWorkReviewCommandSchema,
] as const;

const ManagedWorkProcessExitSchema = z
  .object({ code: z.number().int().nullable(), observedAt: z.iso.datetime() })
  .strict();
const ManagedWorkReportSchema = z
  .object({
    summaryMarkdown: z.string().min(1).max(100_000),
    artifactRefs: z.array(z.string().min(3).max(512)).max(256),
    reportedAt: z.iso.datetime(),
  })
  .strict();
const ManagedWorkReviewSchema = z
  .object({
    disposition: z.enum(["accepted", "follow_up_required"]),
    commentMarkdown: z.string().max(100_000),
    reviewerActorId: z.string().min(3).max(160).nullable(),
    reviewedAt: z.iso.datetime(),
  })
  .strict();
export const ManagedWorkViewSchema = z
  .object({
    taskId: TaskIdSchema,
    runId: RunIdSchema,
    attemptId: z.string().min(3).max(160),
    actorId: z.string().min(3).max(160),
    adapterSessionId: z.string().min(3).max(160),
    sessionId: z.string().min(3).max(160),
    terminalId: z.string().min(3).max(160),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    provider: z.enum(["codex", "claude_code", "opencode", "qwen_code"]),
    profileId: z.string().min(3).max(160),
    goal: z.string().min(1).max(1_000_000),
    expectedResult: z.string().min(1).max(100_000),
    targetRefs: z.array(ProjectObjectReferenceSchema).max(256),
    modelSelection: ModelSelectionSchema,
    state: z.enum([
      "prepared",
      "launching",
      "running",
      "waiting_for_user",
      "reported",
      "accepted",
      "follow_up_required",
      "stopping",
      "stopped",
      "failed",
      "uncertain",
    ]),
    processExit: ManagedWorkProcessExitSchema.nullable(),
    report: ManagedWorkReportSchema.nullable(),
    review: ManagedWorkReviewSchema.nullable(),
    revision: DecimalSchema,
  })
  .strict();
export type ManagedWorkView = z.infer<typeof ManagedWorkViewSchema>;

export const ManagedWorkReadRequestSchemas = [
  z
    .object({
      operation: z.literal("managed-work.profile.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("managed-work.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      runId: RunIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("managed-work.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
] as const;
export const ManagedWorkReadResponseSchemas = [
  z
    .object({
      operation: z.literal("managed-work.profile.list.v1"),
      profiles: z.array(
        z
          .object({
            profileId: z.string().min(3).max(160),
            tier: z.enum(["ultra", "big", "medium", "small"]).nullable(),
            projectId: ProjectIdSchema,
            contextId: WorkContextIdSchema,
            provider: z.enum(["codex", "claude_code", "opencode", "qwen_code"]),
            modelId: z.string().min(1).max(256),
            effort: z.string().min(1).max(64).nullable(),
            installed: z.boolean(),
            launchable: z.boolean(),
            interactiveTerminal: z.enum(["supported", "conditional", "unsupported"]),
            authenticated: z.enum(["observed", "not_observed", "unavailable"]),
            evidence: z.array(z.string().min(1).max(512)).max(32),
          })
          .strict(),
      ),
    })
    .strict(),
  z.object({ operation: z.literal("managed-work.get.v1"), work: ManagedWorkViewSchema }).strict(),
  z
    .object({ operation: z.literal("managed-work.list.v1"), works: z.array(ManagedWorkViewSchema) })
    .strict(),
] as const;

export const ManagedWorkCommandResponseSchemas = [
  z
    .object({ operation: z.literal("managed-work.create.v1"), work: ManagedWorkViewSchema })
    .strict(),
  z.object({ operation: z.literal("managed-work.start.v1"), work: ManagedWorkViewSchema }).strict(),
  z.object({ operation: z.literal("managed-work.stop.v1"), work: ManagedWorkViewSchema }).strict(),
  z
    .object({ operation: z.literal("managed-work.interrupt.v1"), work: ManagedWorkViewSchema })
    .strict(),
  z
    .object({ operation: z.literal("managed-work.report.v1"), work: ManagedWorkViewSchema })
    .strict(),
  z
    .object({ operation: z.literal("managed-work.review.v1"), work: ManagedWorkViewSchema })
    .strict(),
] as const;
