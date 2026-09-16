/** Protected user-local product settings. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { readFile } from "node:fs/promises";
import { z } from "zod";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import { ReasoningEffortSchema } from "../model-policy/index.ts";
import { ModelTierSchema } from "../model-policy/index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";
import { isAbsolute } from "node:path";
import { AgentProductSchema, ExecutionCatalogIdSchema } from "../execution-catalog/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";

const ProductBindingBaseSchema = z.object({
  bindingId: ExecutionCatalogIdSchema,
  hostId: ExecutionHostIdSchema,
  displayName: z.string().trim().min(1).max(160),
  enabled: z.boolean(),
  setupGuidance: z.string().min(1).max(4_000),
});
export const ProductExecutionBindingSchema = z.discriminatedUnion("kind", [
  ProductBindingBaseSchema.extend({
    kind: z.literal("codex_home"),
    agentProduct: z.literal("codex"),
    homePath: z.string().min(1).max(32_768).refine(isAbsolute),
  }).strict(),
  ProductBindingBaseSchema.extend({
    kind: z.literal("claude_config_dir"),
    agentProduct: z.literal("claude_code"),
    homePath: z.string().min(1).max(32_768).refine(isAbsolute),
  }).strict(),
  ProductBindingBaseSchema.extend({
    kind: z.literal("environment_reference"),
    agentProduct: AgentProductSchema.exclude(["codex", "zap_mock"]),
    environmentRef: ExecutionCatalogIdSchema,
  }).strict(),
  ProductBindingBaseSchema.extend({
    kind: z.literal("zap_mock_fixture"),
    agentProduct: z.literal("zap_mock"),
  }).strict(),
]);
export type ProductExecutionBinding = z.infer<typeof ProductExecutionBindingSchema>;

export const ManagedWorkerTemplateSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    sourceProfileId: z.string().min(3).max(160),
    tier: ModelTierSchema.nullable(),
    modelId: z.string().min(1).max(256),
    effort: ReasoningEffortSchema.nullable(),
  })
  .strict();
export type ManagedWorkerTemplate = z.infer<typeof ManagedWorkerTemplateSchema>;

export const ProductLocalSettingsSchema = z
  .object({
    version: z.literal(1),
    uiPort: z.number().int().min(1).max(65_535).default(4174),
    proxy: ProxyPolicySchema.default({ mode: "inherit" }),
    coordinatorDefaults: z
      .object({
        modelId: z.string().min(1).max(256).default("gpt-5.6-luna"),
        effort: ReasoningEffortSchema.default("low"),
        proxy: ProxyPolicySchema.optional(),
      })
      .strict()
      .optional(),
    providerCoordinators: z.array(ProviderCoordinatorProfileSchema).max(32).default([]),
    environmentFiles: z
      .record(
        z.string().min(3).max(160),
        z.string().min(1).max(32_768).refine(isAbsolute, "environment file must be absolute"),
      )
      .default({}),
    executionBindings: z.array(ProductExecutionBindingSchema).max(64).default([]),
    managedWorkers: z.array(ManagedWorkerTemplateSchema).max(64).default([]),
    repositoryMergeIdentity: z
      .object({ name: z.string().min(1).max(200), email: z.email().max(320) })
      .strict()
      .nullable()
      .default(null),
  })
  .strict()
  .superRefine((settings, context) => {
    const tiers = settings.managedWorkers
      .map((profile) => profile.tier)
      .filter((tier) => tier !== null);
    if (new Set(tiers).size !== tiers.length)
      context.addIssue({
        code: "custom",
        path: ["managedWorkers"],
        message: "managed worker policy tiers must be unique",
      });
    const sources = new Set(settings.providerCoordinators.map((profile) => profile.profileId));
    if (settings.coordinatorDefaults !== undefined) sources.add("profile.codex.local");
    settings.managedWorkers.forEach((profile, index) => {
      if (!sources.has(profile.sourceProfileId))
        context.addIssue({
          code: "custom",
          path: ["managedWorkers", index, "sourceProfileId"],
          message: "managed worker source profile is not configured",
        });
    });
  });
export type ProductLocalSettings = z.infer<typeof ProductLocalSettingsSchema>;

export async function loadProductLocalSettings(
  path: string,
): Promise<
  | { readonly ok: true; readonly value: ProductLocalSettings }
  | { readonly ok: false; readonly message: string }
> {
  try {
    const raw: unknown = JSON.parse(await readFile(path, "utf8"));
    const parsed = ProductLocalSettingsSchema.safeParse(raw);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : { ok: false, message: "Zap local settings are invalid" };
  } catch (error) {
    if (missing(error))
      return { ok: true, value: ProductLocalSettingsSchema.parse({ version: 1 }) };
    return { ok: false, message: "Zap local settings could not be read" };
  }
}

function missing(error: unknown): boolean {
  return typeof error === "object" && error !== null && Reflect.get(error, "code") === "ENOENT";
}
