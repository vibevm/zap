/** Registered managed-agent provider launch profiles. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { isAbsolute } from "node:path";
import { readFileSync } from "node:fs";
import { z } from "zod";
import { ManagedProviderCapabilitiesSchema, ManagedProviderIdSchema } from "./provider-types.ts";
import { ModelTierSchema, type ModelSelection } from "../model-policy/index.ts";
import { ProxyPolicySchema, resolveProxyEnvironment } from "../proxy-policy/index.ts";
import { ZAP_MCP_SERVER_NAME, zapPreauthorizedToolNames } from "../protocol/index.ts";
import { qwenZapMcpPermissionArguments } from "../provider-coordinators/index.ts";

const AbsolutePathSchema = z.string().min(1).max(32_000).refine(isAbsolute);
export const ManagedAgentProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    tier: ModelTierSchema.nullable().default(null),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    provider: ManagedProviderIdSchema,
    executablePath: AbsolutePathSchema,
    argumentPrefix: z.array(z.string().max(32_000)).max(32).default([]),
    cwd: AbsolutePathSchema,
    modelId: z.string().min(1).max(256),
    effort: z.string().min(1).max(64).nullable(),
    effortSupported: z.boolean().default(false),
    environmentRef: z.string().min(3).max(512).nullable(),
    mcpConfigPath: AbsolutePathSchema,
    mcpCommandPath: AbsolutePathSchema.optional(),
    mcpArgs: z.array(z.string().max(16_384)).max(64).optional(),
    proxy: ProxyPolicySchema.optional(),
    capabilities: ManagedProviderCapabilitiesSchema,
  })
  .strict()
  .superRefine((profile, context) => {
    if (profile.capabilities.provider !== profile.provider)
      context.addIssue({
        code: "custom",
        message: "capability evidence must name the launch provider",
      });
    if (
      (profile.provider === "opencode" || profile.provider === "qwen_code") &&
      profile.effort !== null &&
      !profile.effortSupported
    )
      context.addIssue({
        code: "custom",
        path: ["effort"],
        message: "installed launch surface has no verified effort argument",
      });
  });
export type ManagedAgentProfile = z.infer<typeof ManagedAgentProfileSchema>;

export interface ProtectedEnvironmentPort {
  resolve(
    reference: string | null,
  ): Promise<
    | { readonly ok: true; readonly value: Readonly<Record<string, string>> }
    | { readonly ok: false; readonly message: string }
  >;
}

export interface ProviderLaunch {
  readonly executable: string;
  readonly args: readonly string[];
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
}

export interface ManagedProviderDriver {
  readonly provider: z.infer<typeof ManagedProviderIdSchema>;
  launch(input: {
    readonly profile: ManagedAgentProfile;
    readonly selection: ModelSelection;
    readonly instructions: string;
    readonly environment: Readonly<Record<string, string>>;
    readonly trustedZapMcp: boolean;
  }): ProviderLaunch;
}

export function createManagedProviderDrivers(
  options: { readonly proxyPolicy?: ProxyPolicySchemaType } = {},
): ReadonlyMap<z.infer<typeof ManagedProviderIdSchema>, ManagedProviderDriver> {
  return new Map(
    ManagedProviderIdSchema.options.map((provider) => [
      provider,
      driver(provider, options.proxyPolicy),
    ]),
  );
}

type ProxyPolicySchemaType = z.infer<typeof ProxyPolicySchema>;

function driver(
  provider: z.infer<typeof ManagedProviderIdSchema>,
  globalProxy?: ProxyPolicySchemaType,
): ManagedProviderDriver {
  return {
    provider,
    launch({ profile, selection, instructions, environment, trustedZapMcp }) {
      const model = selection.modelId;
      const effort = effectiveEffort(selection);
      const proxyResolution = resolveProxyEnvironment({
        ambient: { ...process.env, ...environment },
        ...(globalProxy === undefined ? {} : { global: globalProxy }),
        ...(profile.proxy === undefined ? {} : { profile: profile.proxy }),
      });
      const providerProxy =
        proxyResolution.mode === "direct"
          ? undefined
          : (proxyResolution.environment["HTTPS_PROXY"] ??
            proxyResolution.environment["ALL_PROXY"] ??
            proxyResolution.environment["HTTP_PROXY"]);
      const args =
        provider === "codex"
          ? [
              "-m",
              model,
              ...(effort === null
                ? []
                : ["-c", `model_reasoning_effort=${JSON.stringify(effort)}`]),
              ...codexMcpOverrides(profile.mcpConfigPath, trustedZapMcp),
              instructions,
            ]
          : provider === "claude_code"
            ? [
                "--model",
                model,
                ...(effort === null ? [] : ["--effort", effort]),
                "--mcp-config",
                profile.mcpConfigPath,
                ...(trustedZapMcp
                  ? [
                      "--allowedTools",
                      zapPreauthorizedToolNames(true)
                        .map((tool) => `mcp__${ZAP_MCP_SERVER_NAME}__${tool}`)
                        .join(","),
                    ]
                  : []),
                instructions,
              ]
            : provider === "opencode"
              ? [
                  profile.cwd,
                  "--model",
                  model,
                  ...(effort === null || !profile.effortSupported ? [] : ["--agent", "managed"]),
                  "--prompt",
                  instructions,
                ]
              : [
                  "--bare",
                  "--model",
                  model,
                  "--approval-mode",
                  "default",
                  ...(providerProxy === undefined ? [] : ["--proxy", providerProxy]),
                  "--mcp-config",
                  profile.mcpConfigPath,
                  ...qwenZapMcpPermissionArguments(trustedZapMcp),
                  "--prompt-interactive",
                  instructions,
                ];
      const providerEnv =
        provider === "opencode"
          ? {
              OPENCODE_CONFIG: profile.mcpConfigPath,
              ...(effort !== null && profile.effortSupported
                ? {
                    OPENCODE_CONFIG_CONTENT: JSON.stringify({
                      ...readJsonObject(profile.mcpConfigPath),
                      agent: { managed: { reasoningEffort: effort } },
                    }),
                  }
                : {}),
            }
          : {};
      const inheritedEnvironment = Object.fromEntries(
        Object.entries(proxyResolution.environment).filter(
          (entry): entry is [string, string] => entry[1] !== undefined,
        ),
      );
      return {
        executable: profile.executablePath,
        args: [...profile.argumentPrefix, ...args],
        cwd: profile.cwd,
        env: {
          ...inheritedEnvironment,
          ...providerEnv,
        },
      };
    },
  };
}

function readJsonObject(path: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    const checked = z.record(z.string(), z.unknown()).safeParse(parsed);
    return checked.success ? checked.data : {};
  } catch {
    return {};
  }
}

function effectiveEffort(selection: ModelSelection): string | null {
  const effort = selection.effectiveEffort;
  return effort.state === "explicit" ||
    effort.state === "configured_default" ||
    effort.state === "inherited"
    ? effort.value
    : null;
}

function codexMcpOverrides(path: string, trustedZapMcp: boolean): readonly string[] {
  const config = readJsonObject(path);
  const servers = z.record(z.string(), z.unknown()).safeParse(config["mcp_servers"]);
  const server = servers.success
    ? z
        .object({
          command: z.string(),
          args: z.array(z.string()),
          env: z.record(z.string(), z.string()),
        })
        .strict()
        .safeParse(servers.data["zap-wayfinder"])
    : { success: false as const };
  if (!server.success) return [];
  const connection = [
    "-c",
    `mcp_servers.zap-wayfinder.command=${JSON.stringify(server.data.command)}`,
    "-c",
    `mcp_servers.zap-wayfinder.args=${JSON.stringify(server.data.args)}`,
    "-c",
    `mcp_servers.zap-wayfinder.env=${JSON.stringify(server.data.env)}`,
  ];
  return trustedZapMcp
    ? [
        ...connection,
        "-c",
        'mcp_servers.zap-wayfinder.default_tools_approval_mode="prompt"',
        ...zapPreauthorizedToolNames(true).flatMap((tool) => [
          "-c",
          `mcp_servers.zap-wayfinder.tools.${tool}.approval_mode="approve"`,
        ]),
      ]
    : connection;
}
