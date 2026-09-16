/** Ordinary product setup DTOs. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { z } from "zod";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

export const ProductProviderProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    provider: z.enum(["codex", "claude_code", "opencode", "qwen_code"]),
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
    registeredAt: z.iso.datetime(),
  })
  .strict();
export type ProductProject = z.infer<typeof ProductProjectSchema>;

export const ProductSetupSnapshotSchema = z
  .object({
    projects: z.array(ProductProjectSchema).max(256),
    providers: z.array(ProductProviderProfileSchema).max(64),
    projectLimit: z.number().int().min(1).max(256),
  })
  .strict();
export type ProductSetupSnapshot = z.infer<typeof ProductSetupSnapshotSchema>;

export const ProductProjectRegistrationRequestSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    directoryPath: z.string().min(1).max(32_000),
    displayName: z.string().min(1).max(256).nullable(),
    profileId: z.string().min(3).max(160),
  })
  .strict();
export type ProductProjectRegistrationRequest = z.infer<
  typeof ProductProjectRegistrationRequestSchema
>;

export const ProductSetupRequestSchema = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("product.setup.get.v1") }).strict(),
  ProductProjectRegistrationRequestSchema.extend({
    operation: z.literal("product.project.register.v1"),
  }).strict(),
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
]);
export type ProductSetupResponse = z.infer<typeof ProductSetupResponseSchema>;

export type ProductSetupResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_input" | "not_found" | "conflict" | "unavailable";
        readonly message: string;
      };
    };

export interface ProductSetupPort {
  request(request: ProductSetupRequest): Promise<ProductSetupResult<ProductSetupResponse>>;
}
