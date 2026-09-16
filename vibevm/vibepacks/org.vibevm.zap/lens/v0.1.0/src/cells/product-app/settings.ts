/** Protected user-local product settings. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { readFile } from "node:fs/promises";
import { z } from "zod";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import { ReasoningEffortSchema } from "../model-policy/index.ts";
import { ModelTierSchema } from "../model-policy/index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";
import { isAbsolute } from "node:path";

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
    managedWorkers: z.array(ManagedWorkerTemplateSchema).max(64).default([]),
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
