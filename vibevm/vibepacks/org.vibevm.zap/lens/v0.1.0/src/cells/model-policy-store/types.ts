/** Durable policy and future-attempt selection seam for Zap Wayfinder. @scope spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle */
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
  type ActorId,
  type ClientRequestId,
  type PrincipalId,
} from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type AttemptId,
  type ProjectId,
  type RunId,
  type WorkContextId,
  type ClientId,
} from "../workspace-model/index.ts";
import {
  ModelSelectionSchema,
  ModelPolicySchema,
  type ModelCapabilityProfile,
  type ModelPolicyResult,
  type ModelSelection,
  type ModelSelectionRequest,
  type TrustedModelContext,
} from "../model-policy/index.ts";

export const ModelPolicyStoreErrorSchema = z
  .object({
    code: z.enum([
      "invalid_input",
      "unauthorized",
      "forbidden",
      "not_found",
      "conflict",
      "stale_revision",
      "idempotency_conflict",
      "storage_failure",
      "closed",
    ]),
    message: z.string().min(1).max(4_000),
  })
  .strict();
export type ModelPolicyStoreError = z.infer<typeof ModelPolicyStoreErrorSchema>;
export type ModelPolicyStoreResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: ModelPolicyStoreError };

export const ModelPolicyStoreAccessSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    authorizedProjectIds: z.array(ProjectIdSchema).min(1).max(256),
  })
  .strict();
export type ModelPolicyStoreAccess = z.infer<typeof ModelPolicyStoreAccessSchema>;

export const StoredModelPolicySchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    policy: ModelPolicySchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type StoredModelPolicy = z.infer<typeof StoredModelPolicySchema>;

export const ModelPolicyVersionSchema = z
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
export type ModelPolicyVersion = z.infer<typeof ModelPolicyVersionSchema>;

export const ModelPolicyChangeSchema = z
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
export type ModelPolicyChange = z.infer<typeof ModelPolicyChangeSchema>;

const PolicyWriteBaseSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
  })
  .strict();
export const InitializeDefaultPolicyRequestSchema = PolicyWriteBaseSchema.extend({
  policyId: z.string().min(3).max(160),
}).strict();
export type InitializeDefaultPolicyRequest = z.infer<typeof InitializeDefaultPolicyRequestSchema>;

export const UpdateModelPolicyRequestSchema = PolicyWriteBaseSchema.extend({
  expectedRevision: DecimalSchema,
  policy: ModelPolicySchema,
}).strict();
export type UpdateModelPolicyRequest = z.infer<typeof UpdateModelPolicyRequestSchema>;

export const StoredModelSelectionSchema = z
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
export type StoredModelSelection = z.infer<typeof StoredModelSelectionSchema>;

export const StoreModelSelectionRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
    selection: ModelSelectionSchema,
  })
  .strict();
export type StoreModelSelectionRequest = z.infer<typeof StoreModelSelectionRequestSchema>;

export const ResolveModelPreviewRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    request: z.custom<ModelSelectionRequest>(),
    trustedContext: z.custom<TrustedModelContext>(),
  })
  .strict();
export type ResolveModelPreviewRequest = z.infer<typeof ResolveModelPreviewRequestSchema>;

export interface OpenModelPolicyStoreOptions {
  readonly databasePath: string;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export interface ModelPolicyStore {
  initializeDefaultPolicy(
    access: ModelPolicyStoreAccess,
    request: InitializeDefaultPolicyRequest,
  ): ModelPolicyStoreResult<StoredModelPolicy>;
  updatePolicy(
    access: ModelPolicyStoreAccess,
    request: UpdateModelPolicyRequest,
  ): ModelPolicyStoreResult<StoredModelPolicy>;
  readPolicy(
    access: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<StoredModelPolicy>;
  listPolicyVersions(
    access: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<readonly ModelPolicyVersion[]>;
  listChanges(
    access: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<readonly ModelPolicyChange[]>;
  resolvePreview(
    access: ModelPolicyStoreAccess,
    request: ResolveModelPreviewRequest,
  ): ModelPolicyStoreResult<ModelPolicyResult<ModelSelection>>;
  storeSelection(
    access: ModelPolicyStoreAccess,
    request: StoreModelSelectionRequest,
  ): ModelPolicyStoreResult<StoredModelSelection>;
  readSelection(
    access: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
    runId: RunId,
    attemptId: AttemptId,
  ): ModelPolicyStoreResult<StoredModelSelection>;
  close(): ModelPolicyStoreResult<null>;
}

export type {
  ActorId,
  ClientId,
  ClientRequestId,
  ModelCapabilityProfile,
  PrincipalId,
  ProjectId,
  RunId,
  AttemptId,
  WorkContextId,
};
