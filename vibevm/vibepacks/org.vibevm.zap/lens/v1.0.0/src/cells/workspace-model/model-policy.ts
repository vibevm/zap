/** Public model-policy wire schemas for Zap Wayfinder. @scope spec://org.vibevm.zap/lens/PROP-008#policy-interface */
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
} from "../protocol/index.ts";
import {
  ModelPolicyErrorSchema,
  ModelPolicySchema,
  ModelSelectionRequestSchema,
  ModelSelectionSchema,
} from "../model-policy/index.ts";
export { ModelSelectionSchema } from "../model-policy/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
} from "./ids.ts";

export const StoredModelPolicyViewSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    policy: ModelPolicySchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export const ModelPolicyVersionViewSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    revision: DecimalSchema,
    policy: ModelPolicySchema,
    sourceEventId: z.string().min(1).max(512),
    actorId: ActorIdSchema.nullable(),
    changedAt: z.iso.datetime(),
  })
  .strict();
export const ModelPolicyChangeViewSchema = z
  .object({
    changeId: z.string().min(3).max(160),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sourceEventId: z.string().min(1).max(512),
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    fromRevision: DecimalSchema.nullable(),
    toRevision: DecimalSchema,
    changedAt: z.iso.datetime(),
  })
  .strict();
export const StoredModelSelectionViewSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    selection: ModelSelectionSchema,
    storedAt: z.iso.datetime(),
    sourceEventId: z.string().min(1).max(512),
    actorId: ActorIdSchema.nullable(),
  })
  .strict();
export const ModelSelectionPreviewResultSchema = z.union([
  z.object({ ok: z.literal(true), value: ModelSelectionSchema }).strict(),
  z.object({ ok: z.literal(false), error: ModelPolicyErrorSchema }).strict(),
]);

export const ModelPolicyGetInputSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();
export const ModelPolicyPreviewInputSchema = ModelPolicyGetInputSchema.extend({
  request: ModelSelectionRequestSchema,
}).strict();
export const ModelPolicyHistoryInputSchema = ModelPolicyGetInputSchema;
export const ModelSelectionGetInputSchema = ModelPolicyGetInputSchema.extend({
  runId: RunIdSchema,
  attemptId: AttemptIdSchema,
}).strict();
export const ModelPolicyUpdateInputSchema = ModelPolicyGetInputSchema.extend({
  clientRequestId: ClientRequestIdSchema,
  sourceEventId: z.string().min(1).max(512),
  expectedRevision: DecimalSchema,
  policy: ModelPolicySchema,
}).strict();

export type StoredModelPolicyView = z.infer<typeof StoredModelPolicyViewSchema>;
export type ModelPolicyVersionView = z.infer<typeof ModelPolicyVersionViewSchema>;
export type ModelPolicyChangeView = z.infer<typeof ModelPolicyChangeViewSchema>;
export type StoredModelSelectionView = z.infer<typeof StoredModelSelectionViewSchema>;
