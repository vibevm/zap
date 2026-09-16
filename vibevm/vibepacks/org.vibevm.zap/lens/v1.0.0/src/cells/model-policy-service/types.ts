/** Authenticated model-policy application feature seam. @scope spec://org.vibevm.zap/lens/PROP-008#policy-interface */
import { z } from "zod";
import {
  type ModelPolicyChange,
  type ModelPolicyStore,
  type ModelPolicyStoreAccess,
  type ModelPolicyStoreResult,
  type ModelPolicyVersion,
  type StoredModelPolicy,
  type StoredModelSelection,
} from "../model-policy-store/index.ts";
import {
  ModelSelectionRequestSchema,
  type ModelCapabilityProfile,
  type ModelPolicyResult as CoreModelPolicyResult,
  type ModelSelection,
  type ModelSelectionRequest,
  type TrustedModelContext,
} from "../model-policy/index.ts";
import {
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type AttemptId,
  type ProjectId,
  type RunId,
  type WorkContextId,
} from "../workspace-model/index.ts";

export const ModelPolicyGetRequestSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();
export type ModelPolicyGetRequest = z.infer<typeof ModelPolicyGetRequestSchema>;

export const ModelPolicyUpdateRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    clientRequestId: z.string().min(3).max(160),
    sourceEventId: z.string().min(1).max(512),
    expectedRevision: z.string().regex(/^(0|[1-9][0-9]*)$/),
    policy: z.unknown(),
  })
  .strict();
export type ModelPolicyUpdateRequest = z.infer<typeof ModelPolicyUpdateRequestSchema>;

export const ModelPolicyPreviewRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    request: ModelSelectionRequestSchema,
  })
  .strict();
export type ModelPolicyPreviewRequest = z.infer<typeof ModelPolicyPreviewRequestSchema>;

export const ModelPolicyHistoryRequestSchema = ModelPolicyGetRequestSchema;
export type ModelPolicyHistoryRequest = ModelPolicyGetRequest;

export const ModelSelectionGetRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
  })
  .strict();
export type ModelSelectionGetRequest = z.infer<typeof ModelSelectionGetRequestSchema>;

export const ModelPolicyGetResponseSchema = z
  .object({ feature: z.literal("model-policy.get.v1"), policy: z.unknown() })
  .strict();
export type ModelPolicyGetResponse = {
  readonly feature: "model-policy.get.v1";
  readonly policy: StoredModelPolicy;
};
export type ModelPolicyUpdateResponse = {
  readonly feature: "model-policy.update.v1";
  readonly policy: StoredModelPolicy;
  readonly change: ModelPolicyChange;
};
export type ModelPolicyPreviewResponse = {
  readonly feature: "model-policy.preview.v1";
  readonly result: CoreModelPolicyResult<ModelSelection>;
};
export type ModelPolicyHistoryResponse = {
  readonly feature: "model-policy.history.v1";
  readonly versions: readonly ModelPolicyVersion[];
  readonly changes: readonly ModelPolicyChange[];
};
export type ModelSelectionGetResponse = {
  readonly feature: "model-selection.get.v1";
  readonly selection: StoredModelSelection;
};

export interface TrustedModelContextProvider {
  resolve(input: {
    readonly access: ModelPolicyStoreAccess;
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly request: ModelSelectionRequest;
  }):
    | Promise<ModelPolicyStoreResult<TrustedModelContext>>
    | ModelPolicyStoreResult<TrustedModelContext>;
}

export interface ModelPolicyService {
  get(
    access: ModelPolicyStoreAccess,
    request: ModelPolicyGetRequest,
  ): ModelPolicyStoreResult<ModelPolicyGetResponse>;
  update(
    access: ModelPolicyStoreAccess,
    request: ModelPolicyUpdateRequest,
  ): ModelPolicyStoreResult<ModelPolicyUpdateResponse>;
  preview(
    access: ModelPolicyStoreAccess,
    request: ModelPolicyPreviewRequest,
  ): Promise<ModelPolicyStoreResult<ModelPolicyPreviewResponse>>;
  history(
    access: ModelPolicyStoreAccess,
    request: ModelPolicyHistoryRequest,
  ): ModelPolicyStoreResult<ModelPolicyHistoryResponse>;
  selection(
    access: ModelPolicyStoreAccess,
    request: ModelSelectionGetRequest,
  ): ModelPolicyStoreResult<ModelSelectionGetResponse>;
}

export interface ModelPolicyServiceOptions {
  readonly store: ModelPolicyStore;
  readonly trustedContext: TrustedModelContextProvider;
}

export type {
  ModelCapabilityProfile,
  ModelPolicyStoreAccess,
  ModelPolicyStoreResult,
  ProjectId,
  WorkContextId,
  RunId,
  AttemptId,
};
