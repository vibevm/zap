/** Validated Wayfinder runtime configuration. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { readFile } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import { z } from "zod";
import { CodexCoordinatorProfileSchema } from "../codex-coordinator/index.ts";
import { CoordinatorRoutingConfigSchema } from "../coordinator-routing/index.ts";
import { ManagedAgentProfileSchema } from "../managed-work/index.ts";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import { ProductProviderProfileSchema } from "../workspace-model/index.ts";
import { ManagedWorkerTemplateSchema } from "../product-app/index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { WorkspacePlanningRuntimeConfigSchema } from "../workspace-planning/index.ts";
import { WayfinderAgentGatewayConfigSchema } from "./agent.ts";
import { ManagedRuntimeConfigSchema } from "./managed.ts";
import { RuntimeRepositoryWorkspaceConfigSchema } from "./repository-runtime.ts";
import { invalidConfig } from "./runtime-result.ts";
import type { WayfinderResult } from "./types.ts";
import { WayfinderWebConfigSchema } from "./web.ts";

const AbsolutePathSchema = z.string().min(1).refine(isAbsolute, "path must be absolute");
const GatewaySchema = z
  .object({
    host: z.string().min(1).max(255),
    port: z.number().int().min(0).max(65_535),
    namespace: z.string().regex(/^[A-Za-z][A-Za-z0-9_-]{2,63}$/),
    pairingToken: z.string().min(24).max(512),
    allowedHosts: z.array(z.string().min(1)).min(1).max(16),
    allowedOrigins: z.array(z.string().min(1)).min(1).max(16),
  })
  .strict();

export const WayfinderRuntimeConfigSchema = z
  .object({
    version: z.literal(1),
    state: z
      .object({
        databasePath: AbsolutePathSchema,
        modelPolicyDatabasePath: AbsolutePathSchema.optional(),
        annotationsDatabasePath: AbsolutePathSchema.optional(),
        productRegistryPath: AbsolutePathSchema.optional(),
      })
      .strict(),
    gateway: GatewaySchema,
    agentGateway: WayfinderAgentGatewayConfigSchema.optional(),
    managedTerminals: ManagedRuntimeConfigSchema.optional(),
    managedAgents: z.array(ManagedAgentProfileSchema).max(256).default([]),
    planning: WorkspacePlanningRuntimeConfigSchema.optional(),
    profiles: z.array(CodexCoordinatorProfileSchema).max(32).default([]),
    providerCoordinatorProfiles: z.array(ProviderCoordinatorProfileSchema).max(32).default([]),
    productProviders: z.array(ProductProviderProfileSchema).max(64).default([]),
    managedWorkerProfiles: z.array(ManagedWorkerTemplateSchema).max(64).default([]),
    repositoryWorkspaces: RuntimeRepositoryWorkspaceConfigSchema.optional(),
    proxy: ProxyPolicySchema.default({ mode: "inherit" }),
    projects: z.array(TrustedProjectRegistrationSchema).max(256).default([]),
    modelPolicies: z
      .array(
        z
          .object({
            projectId: z.string().min(3).max(160),
            contextId: z.string().min(3).max(160),
            policyId: z.string().min(3).max(160),
          })
          .strict(),
      )
      .max(256)
      .default([]),
    routing: CoordinatorRoutingConfigSchema.optional(),
    web: WayfinderWebConfigSchema.optional(),
  })
  .strict()
  .superRefine((config, context) => {
    const profileIds = new Set([
      ...config.profiles.map((profile) => profile.profileId),
      ...config.providerCoordinatorProfiles.map((profile) => profile.profileId),
    ]);
    if (profileIds.size !== config.profiles.length + config.providerCoordinatorProfiles.length)
      context.addIssue({
        code: "custom",
        path: ["providerCoordinatorProfiles"],
        message: "coordinator profile IDs must be unique across providers",
      });
    for (const [index, profile] of config.productProviders.entries()) {
      if (profile.configured && profile.launchable && !profileIds.has(profile.profileId))
        context.addIssue({
          code: "custom",
          path: ["productProviders", index, "profileId"],
          message: "product provider must name a protected coordinator profile",
        });
    }
    for (const [index, profile] of config.profiles.entries()) {
      if (profile.lensMcp === undefined) continue;
      const endpoint = new URL(profile.lensMcp.brokerUrl);
      const endpointHost = endpoint.hostname === "[::1]" ? "::1" : endpoint.hostname;
      const configuredPort = Number(endpoint.port || "80");
      if (
        config.agentGateway === undefined ||
        config.agentGateway.port === 0 ||
        endpointHost !== config.agentGateway.host ||
        configuredPort !== config.agentGateway.port
      )
        context.addIssue({
          code: "custom",
          path: ["profiles", index, "lensMcp", "brokerUrl"],
          message: "Lens MCP URL must match the fixed configured agent gateway",
        });
    }
  });
export type WayfinderRuntimeConfig = z.infer<typeof WayfinderRuntimeConfigSchema>;

export async function loadWayfinderConfig(
  path: string,
): Promise<WayfinderResult<WayfinderRuntimeConfig>> {
  try {
    const raw: unknown = JSON.parse(await readFile(resolve(path), "utf8"));
    const parsed = WayfinderRuntimeConfigSchema.safeParse(raw);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : invalidConfig("config schema is invalid");
  } catch {
    return invalidConfig("config file could not be read");
  }
}
