/** Host-neutral managed-agent work contracts. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { z } from "zod";
import { ManagedProviderIdSchema, type ManagedProviderCapabilities } from "./provider-types.ts";
import type { ManagedAgentProfile } from "./providers.ts";
import { WorkPacketSchema } from "../agent-runtime/index.ts";
import { ModelSelectionSchema } from "../model-policy/index.ts";
import { ActorIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ArtifactRefIdSchema,
  AttemptIdSchema,
  ManagedWorkSelectionSchema,
  ProjectObjectReferenceSchema,
  RunIdSchema,
  TaskIdSchema,
  TerminalIdSchema,
  type WorkspaceAccessContext,
  type ProjectObjectReference,
} from "../workspace-model/index.ts";

export const ManagedWorkRequestSchema = z
  .object({
    clientRequestId: z.string().min(3).max(160),
    projectId: WorkPacketSchema.shape.projectId,
    contextId: WorkPacketSchema.shape.contextId,
    selection: ManagedWorkSelectionSchema.default({ mode: "project_policy" }),
    goal: WorkPacketSchema.shape.goal,
    expectedResult: WorkPacketSchema.shape.expectedResult,
    targetRefs: z.array(ProjectObjectReferenceSchema).max(256),
    contextRefs: WorkPacketSchema.shape.contextRefs,
    parentTaskId: TaskIdSchema.nullable(),
    parentRunId: RunIdSchema.nullable(),
    projectedParentActorId: ActorIdSchema.nullable(),
    sourceBasisRef: WorkPacketSchema.shape.sourceBasisRef,
    planRevision: WorkPacketSchema.shape.planRevision,
    depth: WorkPacketSchema.shape.depth,
    budgets: WorkPacketSchema.shape.budgets,
  })
  .strict()
  .superRefine((request, context) => {
    if (
      request.targetRefs.some(
        (target) =>
          target.projectId !== request.projectId || target.contextId !== request.contextId,
      )
    )
      context.addIssue({
        code: "custom",
        path: ["targetRefs"],
        message: "targets must share work scope",
      });
  });
export type ManagedWorkRequest = z.infer<typeof ManagedWorkRequestSchema>;

export const ManagedWorkClaimSchema = z
  .object({
    taskId: TaskIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    actorId: z.string().min(3).max(160),
    creatorActorId: z.string().min(3).max(160).nullable().default(null),
    parentActorId: z.string().min(3).max(160).nullable().default(null),
    adapterSessionId: z.string().min(3).max(160),
    sessionId: AgentSessionIdSchema,
    terminalId: TerminalIdSchema,
    controlLeaseId: z.string().min(3).max(160).nullable().default(null),
    controlEpoch: z
      .string()
      .regex(/^(0|[1-9][0-9]*)$/)
      .nullable()
      .default(null),
    provider: ManagedProviderIdSchema,
    profileId: z.string().min(3).max(160),
    packet: WorkPacketSchema,
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
    processExit: z
      .object({ code: z.number().int().nullable(), observedAt: z.iso.datetime() })
      .strict()
      .nullable(),
    report: z
      .object({
        summaryMarkdown: z.string().min(1).max(100_000),
        artifactRefs: z.array(ArtifactRefIdSchema).max(256),
        reportedAt: z.iso.datetime(),
      })
      .strict()
      .nullable(),
    review: z
      .object({
        disposition: z.enum(["accepted", "follow_up_required"]),
        commentMarkdown: z.string().max(100_000),
        reviewerActorId: z.string().min(3).max(160).nullable(),
        reviewedAt: z.iso.datetime(),
      })
      .strict()
      .nullable(),
    revision: z.string().regex(/^(0|[1-9][0-9]*)$/),
  })
  .strict();
export type ManagedWorkClaim = z.infer<typeof ManagedWorkClaimSchema>;

export interface WorkAttachmentPort {
  prepareBeforeWork(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: ManagedWorkClaim["attemptId"];
    readonly recipientActorId: ManagedWorkClaim["actorId"];
    readonly targets: readonly ProjectObjectReference[];
    readonly sourceBasisRef: string;
    readonly planRevision: string | null;
  }): Promise<
    ManagedWorkResult<{
      readonly state: "ready" | "waiting_for_target";
      readonly instructions: readonly {
        readonly attachmentId: string;
        readonly version: string;
        readonly bodyMarkdown: string;
      }[];
    }>
  >;
  acknowledge(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: ManagedWorkClaim["attemptId"];
    readonly attachmentId: string;
    readonly version: string;
  }): Promise<ManagedWorkResult<null>>;
}

export interface ManagedExecutionFencePort {
  canStart(access: WorkspaceAccessContext, claim: ManagedWorkClaim): ManagedWorkResult<null>;
}

export interface ManagedAgentBackend {
  readonly capabilities: readonly ManagedProviderCapabilities[];
  registerProfile(profile: ManagedAgentProfile): ManagedWorkResult<ManagedAgentProfile>;
  prepare(
    access: WorkspaceAccessContext,
    request: ManagedWorkRequest,
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  get(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
  ): ManagedWorkResult<ManagedWorkClaim>;
  list(
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
  ): ManagedWorkResult<readonly ManagedWorkClaim[]>;
  profiles(
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
  ): ManagedWorkResult<readonly ManagedAgentProfile[]>;
  start(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    expectedRevision: string,
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  interrupt(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    expectedRevision: string,
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  stop(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    expectedRevision: string,
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  reconcile(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  report(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    expectedRevision: string,
    report: {
      readonly summaryMarkdown: string;
      readonly artifactRefs: readonly z.infer<typeof ArtifactRefIdSchema>[];
    },
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  review(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    expectedRevision: string,
    review: {
      readonly disposition: "accepted" | "follow_up_required";
      readonly commentMarkdown: string;
    },
  ): Promise<ManagedWorkResult<ManagedWorkClaim>>;
  acknowledgeAttachment(
    access: WorkspaceAccessContext,
    runId: ManagedWorkClaim["runId"],
    attemptId: ManagedWorkClaim["attemptId"],
    attachmentId: string,
    version: string,
  ): Promise<ManagedWorkResult<null>>;
}

export type ManagedWorkResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_input" | "forbidden" | "conflict" | "unavailable" | "uncertain";
        readonly message: string;
      };
    };
