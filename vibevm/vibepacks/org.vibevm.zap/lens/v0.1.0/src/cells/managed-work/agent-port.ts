/** Managed worker MCP/HTTP contracts. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { z } from "zod";
import {
  ClientRequestIdSchema,
  DecimalSchema,
  type JsonValue,
  type Result,
} from "../protocol/index.ts";
import {
  ArtifactRefIdSchema,
  AttemptIdSchema,
  ManagedWorkSelectionSchema,
  ProjectObjectReferenceSchema,
  RunIdSchema,
} from "../workspace-model/index.ts";
import type { AdapterSessionId } from "../transport/index.ts";

export const ManagedAgentCreateInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    selection: ManagedWorkSelectionSchema.default({ mode: "project_policy" }),
    goal: z.string().min(1).max(1_000_000),
    expectedResult: z.string().min(1).max(100_000),
    targetRefs: z.array(ProjectObjectReferenceSchema).max(256),
    contextRefs: z.array(ArtifactRefIdSchema).max(1_000).default([]),
    sourceBasisRef: z.string().min(1).max(512).optional(),
    planRevision: DecimalSchema.nullable().default(null),
    budgets: z
      .object({
        maximumTurns: z.number().int().min(1).max(100_000),
        wallTimeMs: z.number().int().min(1_000).max(86_400_000),
      })
      .strict()
      .default({ maximumTurns: 64, wallTimeMs: 3_600_000 }),
  })
  .strict();
export const ManagedAgentStartInputSchema = z
  .object({ runId: RunIdSchema, expectedRevision: DecimalSchema })
  .strict();
export const ManagedAgentReadInputSchema = z.object({ runId: RunIdSchema }).strict();
export const ManagedAgentReportInputSchema = ManagedAgentStartInputSchema.extend({
  summaryMarkdown: z.string().min(1).max(100_000),
  artifactRefs: z.array(ArtifactRefIdSchema).max(256),
}).strict();
export const ManagedAgentAttachmentAckInputSchema = z
  .object({
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    attachmentId: z.string().min(3).max(160),
    version: DecimalSchema,
  })
  .strict();

export interface ManagedWorkAgentPort {
  profiles(session: AdapterSessionId): Promise<Result<JsonValue>>;
  create(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  start(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  read(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  report(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  acknowledgeAttachment(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
}

export const NativeWorkBeforeInputSchema = z
  .object({
    attemptId: AttemptIdSchema,
    targetRefs: z.array(ProjectObjectReferenceSchema).max(256),
    sourceBasisRef: z.string().min(1).max(512),
    planRevision: DecimalSchema.nullable(),
  })
  .strict();
export const NativeWorkReadInputSchema = z.object({ attemptId: AttemptIdSchema }).strict();
export const NativeWorkAttachmentAckInputSchema = NativeWorkReadInputSchema.extend({
  attachmentId: z.string().min(3).max(160),
  version: DecimalSchema,
}).strict();

export interface NativeWorkAgentPort {
  beforeWork(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  read(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
  acknowledgeAttachment(session: AdapterSessionId, input: unknown): Promise<Result<JsonValue>>;
}
