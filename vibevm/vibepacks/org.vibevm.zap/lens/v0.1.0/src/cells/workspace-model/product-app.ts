/** Ordinary product setup DTOs. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { z } from "zod";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";
import { ProjectPlanIdSchema, RepositoryWorktreeIdSchema } from "../repository-model/index.ts";
import {
  ExecutionBindingChoiceSchema,
  ExecutionCatalogPreferencesSchema,
  ExecutionCatalogSnapshotSchema,
  ExecutionConfigurationRecordSchema,
  ExecutionConnectionRecordSchema,
  ExecutionModelReferenceViewSchema,
} from "../execution-catalog/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import { ExecutionCatalogChangeViewSchema } from "./execution-catalog.ts";

export const ProductPlanContextSchema = z
  .object({
    planId: ProjectPlanIdSchema,
    contextId: WorkContextIdSchema,
    displayName: z.string().min(1).max(256),
    profileId: z.string().min(3).max(160),
    rootWorktreeId: RepositoryWorktreeIdSchema,
    registeredAt: z.iso.datetime(),
  })
  .strict();
export type ProductPlanContext = z.infer<typeof ProductPlanContextSchema>;

export const ProductProviderProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    provider: z.enum(["codex", "claude_code", "opencode", "qwen_code", "zap_mock"]),
    displayName: z.string().min(1).max(256),
    modelId: z.string().min(1).max(256),
    effort: z
      .enum(["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"])
      .nullable()
      .default(null),
    interactionKind: z.enum(["structured", "native_harness", "owned_terminal"]),
    installed: z.boolean(),
    configured: z.boolean(),
    authenticated: z.enum(["observed", "not_observed", "unavailable"]),
    launchable: z.boolean(),
    evidence: z.array(z.string().min(1).max(512)).max(32),
  })
  .strict();
export type ProductProviderProfile = z.infer<typeof ProductProviderProfileSchema>;

export const ProductProjectSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    displayName: z.string().min(1).max(256),
    directoryPath: z.string().min(1).max(32_000),
    profileId: z.string().min(3).max(160),
    executionConfigurationId: z.string().min(3).max(160).nullable().default(null),
    registeredAt: z.iso.datetime(),
    planContexts: z.array(ProductPlanContextSchema).max(256).default([]),
  })
  .strict();
export type ProductProject = z.infer<typeof ProductProjectSchema>;

export const ProductSetupSnapshotSchema = z
  .object({
    projects: z.array(ProductProjectSchema).max(256),
    providers: z.array(ProductProviderProfileSchema).max(64),
    executionConfigurations: z.array(ExecutionConfigurationRecordSchema).max(5_000).default([]),
    projectLimit: z.number().int().min(1).max(256),
  })
  .strict();
export type ProductSetupSnapshot = z.infer<typeof ProductSetupSnapshotSchema>;

export const ProductProjectRegistrationRequestSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    directoryPath: z.string().min(1).max(32_000),
    displayName: z.string().min(1).max(256).nullable(),
    profileId: z.string().min(3).max(160).optional(),
    executionConfigurationId: z.string().min(3).max(160).optional(),
  })
  .strict()
  .superRefine((request, context) => {
    if ((request.profileId === undefined) === (request.executionConfigurationId === undefined))
      context.addIssue({
        code: "custom",
        message: "choose exactly one legacy profile or execution configuration",
      });
  });
export type ProductProjectRegistrationRequest = z.infer<
  typeof ProductProjectRegistrationRequestSchema
>;

export const ProductSetupRequestSchema = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("product.setup.get.v1") }).strict(),
  ProductProjectRegistrationRequestSchema.safeExtend({
    operation: z.literal("product.project.register.v1"),
  }).strict(),
  z.object({ operation: z.literal("product.execution-catalog.get.v1") }).strict(),
  catalogMutation({
    operation: z.literal("product.execution-catalog.connection.create.v1"),
    bindingId: ExecutionBindingChoiceSchema.shape.bindingId,
    displayName: z.string().trim().min(1).max(160).nullable(),
  }),
  catalogMutation({
    operation: z.literal("product.execution-catalog.connection.upsert.v1"),
    connection: ExecutionConnectionRecordSchema,
  }),
  catalogMutation({
    operation: z.literal("product.execution-catalog.configuration.create.v1"),
    connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
    referenceId: ExecutionModelReferenceViewSchema.shape.referenceId,
    displayName: z.string().trim().min(1).max(200).nullable(),
  }),
  catalogMutation({
    operation: z.literal("product.execution-catalog.configuration.upsert.v1"),
    configuration: ExecutionConfigurationRecordSchema,
  }),
  catalogMutation({
    operation: z.literal("product.execution-catalog.preferences.update.v1"),
    preferences: ExecutionCatalogPreferencesSchema,
  }),
  catalogMutation({
    operation: z.literal("product.execution-catalog.usage.refresh.v1"),
    connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
  }),
]);
export type ProductSetupRequest = z.infer<typeof ProductSetupRequestSchema>;

export const ProductSetupResponseSchema = z.discriminatedUnion("operation", [
  z
    .object({ operation: z.literal("product.setup.get.v1"), snapshot: ProductSetupSnapshotSchema })
    .strict(),
  z
    .object({
      operation: z.literal("product.project.register.v1"),
      project: ProductProjectSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("product.execution-catalog.get.v1"),
      administrator: z.boolean(),
      snapshot: ExecutionCatalogSnapshotSchema,
      availableBindings: z.array(ExecutionBindingChoiceSchema),
      modelReferences: z.array(ExecutionModelReferenceViewSchema),
    })
    .strict(),
  catalogMutationResponse("product.execution-catalog.connection.create.v1"),
  catalogMutationResponse("product.execution-catalog.connection.upsert.v1"),
  catalogMutationResponse("product.execution-catalog.configuration.create.v1"),
  catalogMutationResponse("product.execution-catalog.configuration.upsert.v1"),
  catalogMutationResponse("product.execution-catalog.preferences.update.v1"),
  catalogMutationResponse("product.execution-catalog.usage.refresh.v1"),
]);
export type ProductSetupResponse = z.infer<typeof ProductSetupResponseSchema>;

export type ProductSetupResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code:
          | "invalid_input"
          | "not_found"
          | "conflict"
          | "forbidden"
          | "stale_revision"
          | "idempotency_conflict"
          | "unavailable";
        readonly message: string;
      };
    };

export interface ProductSetupPort {
  request(
    request: ProductSetupRequest,
    authorization?: ProductSetupAuthorization,
  ): Promise<ProductSetupResult<ProductSetupResponse>>;
}

export interface ProductSetupAuthorization {
  readonly catalogAdministrator: boolean;
}

function catalogMutation<T extends z.ZodRawShape>(shape: T) {
  return z
    .object({
      clientRequestId: ClientRequestIdSchema,
      sourceEventId: z.string().min(1).max(512),
      expectedCatalogRevision: DecimalSchema,
      expectedPreferencesRevision: DecimalSchema,
      ...shape,
    })
    .strict();
}

function catalogMutationResponse<
  T extends
    | "product.execution-catalog.connection.create.v1"
    | "product.execution-catalog.connection.upsert.v1"
    | "product.execution-catalog.configuration.create.v1"
    | "product.execution-catalog.configuration.upsert.v1"
    | "product.execution-catalog.preferences.update.v1"
    | "product.execution-catalog.usage.refresh.v1",
>(operation: T) {
  return z
    .object({
      operation: z.literal(operation),
      snapshot: ExecutionCatalogSnapshotSchema,
      change: ExecutionCatalogChangeViewSchema,
    })
    .strict();
}
