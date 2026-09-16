/** Trusted runtime model-policy configuration helper. @scope spec://org.vibevm.zap/lens/PROP-008#capability-evidence */
import { z } from "zod";
import { ModelCapabilityProfileSchema, TrustedModelContextSchema } from "../model-policy/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  type ModelPolicyStore,
  type ModelPolicyStoreAccess,
  type ModelPolicyStoreResult,
} from "../model-policy-store/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import {
  CoordinatorLaunchProfileSchema,
  type CoordinatorLaunchProfile,
  type CoordinatorRoutingProvider,
  type CoordinatorRoutingResult,
} from "./types.ts";

const ScopeSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();
export const CoordinatorCapabilityBindingSchema = z
  .object({ profileId: z.string().min(3).max(160), capability: ModelCapabilityProfileSchema })
  .strict();
export type CoordinatorCapabilityBinding = z.infer<typeof CoordinatorCapabilityBindingSchema>;
export const CoordinatorProfileBindingSchema = z
  .object({ scope: ScopeSchema, profile: CoordinatorLaunchProfileSchema })
  .strict();
export type CoordinatorProfileBinding = z.infer<typeof CoordinatorProfileBindingSchema>;
export const CoordinatorPolicyInitializationSchema = z
  .object({
    scope: ScopeSchema,
    policyId: z.string().min(3).max(160),
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
  })
  .strict();
export type CoordinatorPolicyInitialization = z.infer<typeof CoordinatorPolicyInitializationSchema>;
export const CoordinatorRoutingConfigSchema = z
  .object({
    profiles: z.array(CoordinatorProfileBindingSchema).max(256),
    capabilities: z.array(CoordinatorCapabilityBindingSchema).max(1_000),
    policies: z.array(CoordinatorPolicyInitializationSchema).max(256),
  })
  .strict();
export type CoordinatorRoutingConfig = z.infer<typeof CoordinatorRoutingConfigSchema>;

function failure(
  code: "invalid_input" | "forbidden" | "not_found" | "storage_failure",
  message: string,
): CoordinatorRoutingResult<never> {
  return { ok: false, error: { code, message } };
}

export function createConfiguredCoordinatorRoutingProvider(
  rawConfig: CoordinatorRoutingConfig,
): CoordinatorRoutingProvider {
  const config = CoordinatorRoutingConfigSchema.parse(rawConfig);
  return {
    trustedContext: ({ projectId, contextId }) => {
      const scope = config.profiles.filter(
        (binding) => binding.scope.projectId === projectId && binding.scope.contextId === contextId,
      );
      if (scope.length === 0)
        return failure(
          "not_found",
          "no trusted coordinator profile is configured for this context",
        );
      const allowed = new Set(scope.map((binding) => binding.profile.profileId));
      const capabilities = config.capabilities
        .filter((binding) => allowed.has(binding.profileId))
        .map((binding) => binding.capability);
      const trusted = TrustedModelContextSchema.safeParse({
        capabilities,
        allowedProfileIds: [...allowed],
        parentSelection: null,
        actualObservation: null,
      });
      return trusted.success
        ? { ok: true, value: trusted.data }
        : failure("invalid_input", "trusted coordinator capability configuration is invalid");
    },
    explicitProfile: ({ projectId, contextId, profileId, productId, productVersion }) => {
      const found = config.profiles.find(
        (binding) =>
          binding.scope.projectId === projectId &&
          binding.scope.contextId === contextId &&
          binding.profile.profileId === profileId &&
          binding.profile.productId === productId &&
          binding.profile.productVersion === productVersion,
      );
      return found === undefined
        ? failure("not_found", "explicit coordinator profile is not registered for this context")
        : { ok: true, value: found.profile };
    },
  };
}

export function initializeConfiguredCoordinatorPolicies(
  store: ModelPolicyStore,
  rawAccess: ModelPolicyStoreAccess,
  rawConfig: CoordinatorRoutingConfig,
): ModelPolicyStoreResult<readonly unknown[]> {
  const config = CoordinatorRoutingConfigSchema.safeParse(rawConfig);
  const access = ModelPolicyStoreAccessSchema.safeParse(rawAccess);
  if (!config.success || !access.success)
    return {
      ok: false,
      error: {
        code: "invalid_input",
        message: "trusted coordinator policy configuration is malformed",
      },
    };
  const results: unknown[] = [];
  for (const policy of config.data.policies) {
    const initialized = store.initializeDefaultPolicy(access.data, {
      projectId: policy.scope.projectId,
      contextId: policy.scope.contextId,
      policyId: policy.policyId,
      clientRequestId: policy.clientRequestId,
      sourceEventId: policy.sourceEventId,
    });
    if (!initialized.ok && initialized.error.code !== "conflict") return initialized;
    if (initialized.ok) results.push(initialized.value);
  }
  return { ok: true, value: results };
}

export type { CoordinatorLaunchProfile, ProjectId, WorkContextId };
