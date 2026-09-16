/** Protected execution account bindings and server-only ports. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import { isAbsolute } from "node:path";
import { z } from "zod";
import {
  AgentProductSchema,
  ContextCapabilitySchema,
  ContextRequestSchema,
  ExecutionCatalogIdSchema,
  UsageObservationSchema,
  type AgentProduct,
  type AppliedContext,
  type ContextCapability,
  type ContextRequest,
  type UsageObservation,
} from "../execution-catalog/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";

const AbsolutePathSchema = z
  .string()
  .min(1)
  .max(32_768)
  .refine(isAbsolute, "protected account home must be absolute");

const BindingBaseSchema = z.object({
  bindingId: ExecutionCatalogIdSchema,
  hostId: ExecutionHostIdSchema,
  displayName: z.string().trim().min(1).max(160),
  enabled: z.boolean(),
  setupGuidance: z.string().min(1).max(4_000),
});

export const ProtectedExecutionBindingSchema = z.discriminatedUnion("kind", [
  BindingBaseSchema.extend({
    kind: z.literal("codex_home"),
    agentProduct: z.literal("codex"),
    homePath: AbsolutePathSchema,
  }).strict(),
  BindingBaseSchema.extend({
    kind: z.literal("claude_config_dir"),
    agentProduct: z.literal("claude_code"),
    homePath: AbsolutePathSchema,
  }).strict(),
  BindingBaseSchema.extend({
    kind: z.literal("zap_mock_fixture"),
    agentProduct: z.literal("zap_mock"),
  }).strict(),
  BindingBaseSchema.extend({
    kind: z.literal("environment_reference"),
    agentProduct: AgentProductSchema.exclude(["codex", "zap_mock"]),
    environmentRef: ExecutionCatalogIdSchema,
  }).strict(),
]);
export type ProtectedExecutionBinding = z.infer<typeof ProtectedExecutionBindingSchema>;

export interface ResolvedExecutionAccount {
  readonly bindingId: string;
  readonly hostId: z.infer<typeof ExecutionHostIdSchema>;
  readonly agentProduct: AgentProduct;
  readonly environment: Readonly<Record<string, string>>;
  readonly environmentRef: string | null;
  readonly setupGuidance: string;
}

export interface SafeExecutionBinding {
  readonly bindingId: string;
  readonly hostId: z.infer<typeof ExecutionHostIdSchema>;
  readonly displayName: string;
  readonly agentProduct: AgentProduct;
  readonly enabled: boolean;
  readonly setupGuidance: string;
}

export type ExecutionAccountResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code:
          | "invalid_input"
          | "not_found"
          | "disabled"
          | "host_mismatch"
          | "product_mismatch"
          | "unavailable"
          | "unsupported_override"
          | "protocol_error";
        readonly message: string;
      };
    };

export interface ExecutionAccountIsolationPort {
  list(): readonly SafeExecutionBinding[];
  resolve(input: {
    readonly bindingId: string;
    readonly hostId: string;
    readonly agentProduct: AgentProduct;
  }): Promise<ExecutionAccountResult<ResolvedExecutionAccount>>;
}

export interface ContextApplicationPort {
  apply(input: {
    readonly request: ContextRequest;
    readonly capability: ContextCapability;
    readonly inheritedTokens?: number | null;
  }): ExecutionAccountResult<AppliedContext>;
}

export interface ObservedExecutionAccount {
  readonly authMode: string;
  readonly planType: string | null;
  readonly identityDigest: string | null;
}

export interface ExecutionUsageRead {
  readonly bindingId: string;
  readonly account: ObservedExecutionAccount | null;
  readonly observations: readonly UsageObservation[];
}

export interface ExecutionUsagePort {
  read(input: {
    readonly connectionId: string;
    readonly bindingId: string;
    readonly bucketApplicability?: Readonly<Record<string, UsageObservation["applicability"]>>;
  }): Promise<ExecutionAccountResult<ExecutionUsageRead>>;
}

export function parseContextInput(input: {
  readonly request: ContextRequest;
  readonly capability: ContextCapability;
}) {
  const request = ContextRequestSchema.safeParse(input.request);
  const capability = ContextCapabilitySchema.safeParse(input.capability);
  return request.success && capability.success
    ? { ok: true as const, request: request.data, capability: capability.data }
    : { ok: false as const };
}

export function parseUsageObservation(input: unknown): UsageObservation {
  return UsageObservationSchema.parse(input);
}
