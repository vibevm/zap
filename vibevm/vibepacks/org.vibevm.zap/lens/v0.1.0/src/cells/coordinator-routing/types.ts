/** Trusted coordinator model-routing seam. @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import { z } from "zod";
import { ClientRequestIdSchema, DecimalSchema, type ClientRequestId } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type AgentSessionId,
  type AttemptId,
  type ProjectId,
  type RunId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import {
  EffortRequestSchema,
  ModelSelectionRequestSchema,
  ModelTierSchema,
  ReasoningEffortSchema,
  type ModelSelection,
  type ModelSelectionRequest,
  type TrustedModelContext,
} from "../model-policy/index.ts";
import type {
  ModelPolicyStore,
  ModelPolicyStoreAccess,
  ModelPolicyStoreResult,
} from "../model-policy-store/index.ts";

export const CoordinatorRoutingRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sessionId: AgentSessionIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
    policyEnabled: z.boolean(),
    purpose: ModelSelectionRequestSchema.shape.purpose,
    taskClass: ModelSelectionRequestSchema.shape.taskClass,
    role: z.literal("coordinator"),
    executionMode: z.literal("native"),
    invocationScope: z.literal("coordinator"),
    productId: z.string().min(3).max(160),
    productVersion: z.string().min(1).max(160),
    selectionRef: z.string().min(3).max(160),
    override: z
      .object({
        overrideRef: z.string().min(3).max(160),
        tier: ModelTierSchema,
        effort: EffortRequestSchema,
        reason: z.string().min(1).max(2_000),
      })
      .strict()
      .nullable(),
    explicitProfileId: z.string().min(3).max(160).nullable(),
  })
  .strict();
export type CoordinatorRoutingRequest = z.infer<typeof CoordinatorRoutingRequestSchema>;

export const CoordinatorLaunchProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    productId: z.string().min(3).max(160),
    productVersion: z.string().min(1).max(160),
    providerId: z.string().min(3).max(160),
    modelId: z.string().min(1).max(256),
    effort: ReasoningEffortSchema.nullable(),
  })
  .strict();
export type CoordinatorLaunchProfile = z.infer<typeof CoordinatorLaunchProfileSchema>;

export const CoordinatorLaunchParametersSchema = z
  .object({
    selectionRef: z.string().min(3).max(160).nullable(),
    policyId: z.string().min(3).max(160).nullable(),
    policyRevision: DecimalSchema.nullable(),
    profileId: z.string().min(3).max(160),
    productId: z.string().min(3).max(160),
    productVersion: z.string().min(1).max(160),
    providerId: z.string().min(3).max(160),
    modelId: z.string().min(1).max(256),
    requestedEffort: EffortRequestSchema.nullable(),
    effectiveEffort: ReasoningEffortSchema.nullable(),
  })
  .strict();
export type CoordinatorLaunchParameters = z.infer<typeof CoordinatorLaunchParametersSchema>;

export const CoordinatorRoutingErrorSchema = z
  .object({
    code: z.enum([
      "invalid_input",
      "unauthorized",
      "forbidden",
      "not_found",
      "policy_refused",
      "conflict",
      "storage_failure",
      "unsupported",
    ]),
    message: z.string().min(1).max(4_000),
  })
  .strict();
export type CoordinatorRoutingError = z.infer<typeof CoordinatorRoutingErrorSchema>;
export type CoordinatorRoutingResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: CoordinatorRoutingError };

export interface CoordinatorRoutingProvider {
  trustedContext(input: {
    readonly access: ModelPolicyStoreAccess;
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly request: ModelSelectionRequest;
  }):
    | Promise<CoordinatorRoutingResult<TrustedModelContext>>
    | CoordinatorRoutingResult<TrustedModelContext>;
  explicitProfile(input: {
    readonly access: ModelPolicyStoreAccess;
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly profileId: string;
    readonly productId: string;
    readonly productVersion: string;
  }):
    | Promise<CoordinatorRoutingResult<CoordinatorLaunchProfile>>
    | CoordinatorRoutingResult<CoordinatorLaunchProfile>;
}

export interface CoordinatorRoutingOptions {
  readonly store: ModelPolicyStore;
  readonly provider: CoordinatorRoutingProvider;
}
export interface CoordinatorRoutingResultValue {
  readonly selection: ModelSelection | null;
  readonly parameters: CoordinatorLaunchParameters;
  readonly pinned: boolean;
}
export interface CoordinatorLaunchAdapter<T> {
  launch(parameters: CoordinatorLaunchParameters): Promise<CoordinatorRoutingResult<T>>;
}

export type {
  AgentSessionId,
  AttemptId,
  ClientRequestId,
  ProjectId,
  RunId,
  WorkContextId,
  ModelPolicyStoreResult,
};
