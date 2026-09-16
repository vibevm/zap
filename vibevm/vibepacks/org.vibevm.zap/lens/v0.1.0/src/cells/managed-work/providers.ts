/** Registered managed-agent provider launch profiles. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { isAbsolute } from "node:path";
import { readFileSync } from "node:fs";
import { z } from "zod";
import { ManagedProviderCapabilitiesSchema, ManagedProviderIdSchema } from "./provider-types.ts";
import { ModelTierSchema, type ModelSelection } from "../model-policy/index.ts";
import { ProxyPolicySchema, resolveProxyEnvironment } from "../proxy-policy/index.ts";
import { ZAP_MCP_SERVER_NAME, zapPreauthorizedToolNames } from "../protocol/index.ts";
import { qwenZapMcpPermissionArguments } from "../provider-coordinators/index.ts";
import { ExecutionCatalogIdSchema } from "../execution-catalog/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { isolatedExecutionEnvironment } from "../execution-accounts/index.ts";

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
    contextWindowTokens: z.number().int().positive().max(10_000_000).optional(),
    accountBindingId: ExecutionCatalogIdSchema.optional(),
    executionHostId: ExecutionHostIdSchema.optional(),
    environmentRef: z.string().min(3).max(512).nullable(),
    mcpConfigPath: AbsolutePathSchema,
    mcpCommandPath: AbsolutePathSchema.optional(),
    mcpArgs: z.array(z.string().max(16_384)).max(64).optional(),
    proxy: ProxyPolicySchema.optional(),
    mockScenarioPath: AbsolutePathSchema.optional(),
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
      ["opencode", "qwen_code", "zap_mock"].includes(profile.provider) &&
      profile.effort !== null &&
      !profile.effortSupported
    )
      context.addIssue({
        code: "custom",
        path: ["effort"],
        message: "installed launch surface has no verified effort argument",
      });
    if (profile.provider === "zap_mock" && profile.mockScenarioPath === undefined)
      context.addIssue({
        code: "custom",
        path: ["mockScenarioPath"],
        message: "explicit ZapMock managed profiles require a scenario file",
      });
    if (
      profile.provider === "zap_mock" &&
      (profile.environmentRef !== null || profile.proxy !== undefined)
    )
      context.addIssue({
        code: "custom",
        message: "ZapMock profiles cannot resolve provider environment or proxy configuration",
      });
    if (profile.provider !== "zap_mock" && profile.mockScenarioPath !== undefined)
      context.addIssue({
        code: "custom",
        path: ["mockScenarioPath"],
        message: "mock scenario is valid only for ZapMock profiles",
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
    readonly workspaceCwd: string;
    readonly selection: ModelSelection;
    readonly instructions: string;
    readonly environment: Readonly<Record<string, string>>;
    readonly trustedZapMcp: boolean;
  }): ProviderLaunch;
  resume(input: {
    readonly profile: ManagedAgentProfile;
    readonly workspaceCwd: string;
    readonly selection: ModelSelection;
    readonly providerSessionId: string;
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
  if (provider === "zap_mock") return mockDriver();
  const managedDriver: ManagedProviderDriver = {
    provider,
    launch({ profile, workspaceCwd, selection, instructions, environment, trustedZapMcp }) {
      const model = selection.modelId;
      const effort = effectiveEffort(selection);
      const proxyResolution = resolveProxyEnvironment({
        ambient:
          profile.accountBindingId === undefined
            ? { ...process.env, ...environment }
            : isolatedExecutionEnvironment(process.env, environment),
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
              ...(profile.contextWindowTokens === undefined
                ? []
                : ["-c", `model_context_window=${String(profile.contextWindowTokens)}`]),
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
                  workspaceCwd,
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
        cwd: workspaceCwd,
        env: {
          ...inheritedEnvironment,
          ...providerEnv,
        },
      };
    },
    resume(input) {
      const marker = "__zap_resume_without_bootstrap__";
      const launch = managedDriver.launch({ ...input, instructions: marker });
      return {
        ...launch,
        args: resumeArguments(provider, launch.args, marker, input.providerSessionId),
      };
    },
  };
  return managedDriver;
}

function mockDriver(): ManagedProviderDriver {
  const launch = (input: Parameters<ManagedProviderDriver["launch"]>[0]): ProviderLaunch => {
    const scenario = input.profile.mockScenarioPath;
    if (scenario === undefined)
      throw new Error(
        "violates REQ spec://org.vibevm.zap/lens/PROP-013#identity: explicit ZapMock scenario is missing; fix surface: register mockScenarioPath on the mock profile",
      );
    return {
      executable: input.profile.executablePath,
      args: [
        ...input.profile.argumentPrefix,
        "managed",
        "--scenario",
        scenario,
        "--model",
        "zap-mock/deterministic-v1",
        "--mcp-config",
        input.profile.mcpConfigPath,
        "--instructions",
        input.instructions,
      ],
      cwd: input.workspaceCwd,
      env: mockEnvironment(input.environment),
    };
  };
  return {
    provider: "zap_mock",
    launch,
    resume(input) {
      const resumed = launch({ ...input, instructions: "" });
      const instructions = resumed.args.indexOf("--instructions");
      return {
        ...resumed,
        args: [
          ...resumed.args.slice(0, instructions),
          "--resume",
          input.providerSessionId,
          ...resumed.args.slice(instructions + 2),
        ],
      };
    },
  };
}

function mockEnvironment(
  environment: Readonly<Record<string, string>>,
): Readonly<Record<string, string>> {
  const codlens = new Set([
    "CODLENS_URL",
    "CODLENS_CREDENTIAL_FILE",
    "CODLENS_ADAPTER_SESSION_ID",
    "CODLENS_WORKSPACE_ID",
    "CODLENS_CONVERSATION_ID",
  ]);
  const runtime = new Set(["SystemRoot", "WINDIR", "PATH", "Path", "PATHEXT", "TEMP", "TMP"]);
  const inherited = Object.entries(process.env).filter(
    (entry): entry is [string, string] => runtime.has(entry[0]) && entry[1] !== undefined,
  );
  const binding = Object.entries(environment).filter(([name]) => codlens.has(name));
  return Object.fromEntries([...inherited, ...binding]);
}

function resumeArguments(
  provider: z.infer<typeof ManagedProviderIdSchema>,
  args: readonly string[],
  marker: string,
  providerSessionId: string,
): readonly string[] {
  if (provider === "codex")
    return args.flatMap((argument) =>
      argument === marker ? ["resume", providerSessionId] : [argument],
    );
  if (provider === "claude_code")
    return args.flatMap((argument) =>
      argument === marker ? ["--resume", providerSessionId] : [argument],
    );
  if (provider === "qwen_code") {
    const prompt = args.indexOf("--prompt-interactive");
    return prompt < 0
      ? [...args, "--resume", providerSessionId]
      : [...args.slice(0, prompt), "--resume", providerSessionId, ...args.slice(prompt + 2)];
  }
  const prompt = args.indexOf("--prompt");
  return prompt < 0
    ? [...args, "--session", providerSessionId]
    : [...args.slice(0, prompt), "--session", providerSessionId, ...args.slice(prompt + 2)];
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
