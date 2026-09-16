/** Installed provider discovery for normal startup. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import {
  resolveInstalledCodexExecutable,
  type CodexCoordinatorProfile,
} from "../codex-coordinator/index.ts";
import type { ProductProviderProfile } from "../workspace-model/index.ts";
import {
  ProviderCoordinatorProfileSchema,
  type ProviderCoordinatorProfile,
} from "../provider-coordinators/index.ts";
import { access, constants } from "node:fs/promises";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { basename, delimiter, dirname, isAbsolute, join, resolve } from "node:path";
import type { ProductLocalSettings } from "./settings.ts";
import type { ManagedWorkerTemplate } from "./settings.ts";
import {
  ProtectedExecutionBindingSchema,
  createCodexModelCapabilityPort,
  createExecutionAccountIsolation,
  type ProtectedExecutionBinding,
} from "../execution-accounts/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";

export interface LocalProductProviders {
  readonly coordinatorProfiles: readonly CodexCoordinatorProfile[];
  readonly providerCoordinatorProfiles: readonly ProviderCoordinatorProfile[];
  readonly productProviders: readonly ProductProviderProfile[];
  readonly managedWorkers: readonly ManagedWorkerTemplate[];
  readonly executionBindings: readonly ProtectedExecutionBinding[];
  readonly launchTemplates: readonly LocalProductLaunchTemplate[];
}

export type LocalProductLaunchTemplate =
  | {
      readonly templateId: string;
      readonly agentProduct: "codex";
      readonly profile: CodexCoordinatorProfile;
    }
  | {
      readonly templateId: string;
      readonly agentProduct: "claude_code";
      readonly profile: ProviderCoordinatorProfile;
    };

export async function discoverLocalProductProviders(
  settings: ProductLocalSettings,
  options: { readonly hostId?: string } = {},
): Promise<LocalProductProviders> {
  const hostId = ExecutionHostIdSchema.parse(options.hostId ?? "host.local");
  const executable = await resolveInstalledCodexExecutable();
  const protectedProviders = bindProviderProfiles(settings.providerCoordinators, hostId);
  const observedProviders = await observeProviderProfiles(protectedProviders.profiles);
  const configuredProviders = observedProviders
    .filter((observation) => observation.installed)
    .map((observation) => observation.profile);
  const configuredCatalog = observedProviders.map((observation) =>
    productProjection(observation.profile, observation.installed),
  );
  const detectedCatalog = await detectedUnconfiguredProviders(protectedProviders.profiles);
  const executionBindings = await discoveredBindings(
    executable.ok,
    observedProviders,
    [...protectedProviders.bindings, ...settings.executionBindings],
    hostId,
  );
  const launchTemplates = await discoveredLaunchTemplates(
    settings,
    executable.ok ? executable.value : null,
    observedProviders,
    executionBindings,
    hostId,
  );
  const managedWorkers: readonly ManagedWorkerTemplate[] =
    settings.managedWorkers.length > 0
      ? settings.managedWorkers
      : settings.coordinatorDefaults === undefined
        ? []
        : [
            {
              profileId: "worker.codex.small",
              sourceProfileId: "profile.codex.local",
              tier: "small",
              modelId: settings.coordinatorDefaults.modelId,
              effort: settings.coordinatorDefaults.effort,
            },
          ];
  if (!executable.ok)
    return {
      coordinatorProfiles: [],
      providerCoordinatorProfiles: configuredProviders,
      productProviders: [...configuredCatalog, ...detectedCatalog],
      managedWorkers,
      executionBindings,
      launchTemplates,
    };
  const profileId = "profile.codex.local";
  const defaults = settings.coordinatorDefaults;
  if (defaults === undefined)
    return {
      coordinatorProfiles: [],
      providerCoordinatorProfiles: configuredProviders,
      productProviders: [
        {
          profileId,
          provider: "codex",
          displayName: "Codex · model selection required",
          modelId: "not-selected",
          effort: null,
          interactionKind: "structured",
          installed: true,
          configured: false,
          authenticated: "not_observed",
          launchable: false,
          evidence: [
            "Executable observed through the protected local resolver",
            "Select a model and effort in protected Zap local settings before launch",
          ],
        },
        ...configuredCatalog,
        ...detectedCatalog,
      ],
      managedWorkers,
      executionBindings,
      launchTemplates,
    };
  return {
    coordinatorProfiles: [
      configuredCodexProfile({ ...settings, coordinatorDefaults: defaults }, executable.value),
    ],
    providerCoordinatorProfiles: configuredProviders,
    productProviders: [
      {
        profileId,
        provider: "codex",
        displayName: "Codex · local account",
        modelId: defaults.modelId,
        effort: defaults.effort,
        interactionKind: "structured",
        installed: true,
        configured: true,
        authenticated: "not_observed",
        launchable: true,
        evidence: [
          "Executable observed through the protected local resolver",
          "Authentication is checked only when the user explicitly starts development",
        ],
      },
      ...configuredCatalog,
      ...detectedCatalog,
    ],
    managedWorkers,
    executionBindings,
    launchTemplates,
  };
}

async function discoveredLaunchTemplates(
  settings: ProductLocalSettings,
  codexExecutable: string | null,
  providers: readonly {
    readonly profile: ProviderCoordinatorProfile;
    readonly installed: boolean;
  }[],
  bindings: readonly ProtectedExecutionBinding[],
  hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
): Promise<readonly LocalProductLaunchTemplate[]> {
  const templates: LocalProductLaunchTemplate[] = [];
  if (codexExecutable !== null) {
    const defaults = settings.coordinatorDefaults ?? {
      modelId: "gpt-5.6-luna",
      effort: "low" as const,
    };
    const isolation = createExecutionAccountIsolation(bindings);
    for (const binding of bindings) {
      if (binding.kind !== "codex_home" || !binding.enabled) continue;
      const stem = createHash("sha256").update(binding.bindingId).digest("hex").slice(0, 20);
      let profile: CodexCoordinatorProfile = {
        ...configuredCodexProfile({ ...settings, coordinatorDefaults: defaults }, codexExecutable),
        profileId: `template.codex.${stem}`,
        accountBindingId: binding.bindingId,
        contextWindowTokens: 1_050_000,
      };
      if (isolation.ok) {
        const observed = await createCodexModelCapabilityPort({
          isolation: isolation.value,
          hostId,
          executablePath: codexExecutable,
          launchCwd: process.cwd(),
          proxyPolicy: settings.coordinatorDefaults?.proxy ?? settings.proxy,
        }).read(binding.bindingId);
        if (observed.ok)
          profile = {
            ...profile,
            observedModelCapabilities: observed.value.models.map((model) => ({
              ...model,
              supportedEfforts: [...model.supportedEfforts],
            })),
            capabilityObservedAt: observed.value.observedAt,
          };
      }
      templates.push({
        templateId: profile.profileId,
        agentProduct: "codex",
        profile,
      });
    }
  }
  for (const provider of providers) {
    if (provider.installed && provider.profile.provider === "claude_code")
      templates.push({
        templateId: `template.${provider.profile.profileId}`,
        agentProduct: "claude_code",
        profile: provider.profile,
      });
  }
  if (!templates.some((entry) => entry.agentProduct === "claude_code")) {
    const discovered = await resolveExecutableOnPath("claude");
    const executablePath =
      discovered === null ? null : await resolveClaudeLaunchExecutable(discovered);
    if (executablePath !== null)
      templates.push({
        templateId: "template.claude.local",
        agentProduct: "claude_code",
        profile: ProviderCoordinatorProfileSchema.parse({
          profileId: "template.claude.local",
          provider: "claude_code",
          executablePath,
          cwd: process.cwd(),
          modelId: "claude-haiku-4-5-20251001",
          effort: null,
          endpoint: null,
          accountBindingId: "binding.claude.default",
          executionHostId: hostId,
        }),
      });
  }
  return templates;
}

export function configuredCodexProfile(
  settings: ProductLocalSettings & {
    readonly coordinatorDefaults: NonNullable<ProductLocalSettings["coordinatorDefaults"]>;
  },
  executablePath: string,
): CodexCoordinatorProfile {
  return {
    profileId: "profile.codex.local",
    executablePath,
    requestTimeoutMs: 30_000,
    accountBindingId: "binding.codex.default",
    proxy: settings.coordinatorDefaults.proxy ?? settings.proxy,
    model: settings.coordinatorDefaults.modelId,
    effort: settings.coordinatorDefaults.effort,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "zap-quick-lens",
  };
}

function bindProviderProfiles(
  profiles: readonly ProviderCoordinatorProfile[],
  hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
): {
  readonly profiles: readonly ProviderCoordinatorProfile[];
  readonly bindings: readonly ProtectedExecutionBinding[];
} {
  const bindings: ProtectedExecutionBinding[] = [];
  const protectedProfiles = profiles.map((profile) => {
    if (profile.accountBindingId !== undefined)
      return {
        ...profile,
        executionHostId: profile.executionHostId ?? hostId,
      };
    if (profile.environmentRef !== undefined && profile.environmentRef !== null) {
      const bindingId = `binding.provider.${createHash("sha256").update(profile.profileId).digest("hex").slice(0, 20)}`;
      bindings.push(
        ProtectedExecutionBindingSchema.parse({
          bindingId,
          hostId,
          displayName: `${providerLabel(profile.provider)} protected environment`,
          enabled: true,
          setupGuidance:
            "Select this existing protected environment binding, or register another server-side environment file for a different account.",
          kind: "environment_reference",
          agentProduct: profile.provider,
          environmentRef: profile.environmentRef,
        }),
      );
      return { ...profile, accountBindingId: bindingId, executionHostId: hostId };
    }
    return profile.provider === "claude_code"
      ? {
          ...profile,
          accountBindingId: "binding.claude.default",
          executionHostId: hostId,
        }
      : profile;
  });
  return { profiles: protectedProfiles, bindings };
}

async function discoveredBindings(
  codexInstalled: boolean,
  providers: readonly {
    readonly profile: ProviderCoordinatorProfile;
    readonly installed: boolean;
  }[],
  configured: readonly ProtectedExecutionBinding[],
  hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
): Promise<readonly ProtectedExecutionBinding[]> {
  const result = new Map(configured.map((binding) => [binding.bindingId, binding]));
  if (codexInstalled && !result.has("binding.codex.default")) {
    const configuredHome = process.env["CODEX_HOME"];
    result.set(
      "binding.codex.default",
      ProtectedExecutionBindingSchema.parse({
        bindingId: "binding.codex.default",
        hostId,
        displayName: "Codex default account",
        enabled: true,
        setupGuidance:
          "Run codex login for this host's protected CODEX_HOME. To add another account, register a different absolute Codex home and sign in there before enabling it.",
        kind: "codex_home",
        agentProduct: "codex",
        homePath:
          configuredHome !== undefined && isAbsolute(configuredHome)
            ? resolve(configuredHome)
            : join(homedir(), ".codex"),
      }),
    );
  }
  const claudeInstalled =
    providers.some((entry) => entry.profile.provider === "claude_code" && entry.installed) ||
    (await executableOnPath("claude"));
  if (claudeInstalled && !result.has("binding.claude.default")) {
    const configuredHome = process.env["CLAUDE_CONFIG_DIR"];
    result.set(
      "binding.claude.default",
      ProtectedExecutionBindingSchema.parse({
        bindingId: "binding.claude.default",
        hostId,
        displayName: "Claude Code default account",
        enabled: true,
        setupGuidance:
          "Run Claude Code sign-in for this host's protected CLAUDE_CONFIG_DIR. To add another account, register a different absolute Claude config directory and sign in there before enabling it.",
        kind: "claude_config_dir",
        agentProduct: "claude_code",
        homePath:
          configuredHome !== undefined && isAbsolute(configuredHome)
            ? resolve(configuredHome)
            : join(homedir(), ".claude"),
      }),
    );
  }
  return [...result.values()].sort((left, right) => left.bindingId.localeCompare(right.bindingId));
}

async function observeProviderProfiles(
  profiles: readonly ProviderCoordinatorProfile[],
): Promise<
  readonly { readonly profile: ProviderCoordinatorProfile; readonly installed: boolean }[]
> {
  return Promise.all(
    profiles.map(async (profile) => {
      const executablePath =
        profile.provider === "claude_code"
          ? await resolveClaudeLaunchExecutable(profile.executablePath)
          : profile.executablePath;
      return executablePath === null
        ? { profile, installed: false }
        : {
            profile:
              executablePath === profile.executablePath
                ? profile
                : ProviderCoordinatorProfileSchema.parse({ ...profile, executablePath }),
            installed: await executable(executablePath),
          };
    }),
  );
}

export async function resolveClaudeLaunchExecutable(
  executablePath: string,
  platform: NodeJS.Platform = process.platform,
): Promise<string | null> {
  if (platform !== "win32" || basename(executablePath).toLowerCase() !== "claude.cmd")
    return (await executable(executablePath)) ? executablePath : null;
  const native = join(
    dirname(executablePath),
    "node_modules",
    "@anthropic-ai",
    "claude-code",
    "bin",
    "claude.exe",
  );
  return (await executable(native)) ? native : null;
}

async function executable(path: string): Promise<boolean> {
  try {
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function productProjection(
  profile: ProviderCoordinatorProfile,
  installed: boolean,
): ProductProviderProfile {
  return {
    profileId: profile.profileId,
    provider: profile.provider,
    displayName: `${providerLabel(profile.provider)} · protected local profile`,
    modelId: profile.modelId,
    effort: profile.effort,
    interactionKind: "structured",
    installed,
    configured: true,
    authenticated: "not_observed",
    launchable: installed,
    evidence: [
      installed
        ? "Configured executable was observed by the protected runtime"
        : "Configured executable is unavailable to the protected runtime",
      "Authentication is checked only after explicit Start development",
    ],
  };
}

async function detectedUnconfiguredProviders(
  configured: readonly ProviderCoordinatorProfile[],
): Promise<ProductProviderProfile[]> {
  const configuredProviders = new Set(configured.map((profile) => profile.provider));
  const definitions: readonly {
    readonly provider: ProviderCoordinatorProfile["provider"];
    readonly command: string;
    readonly label: string;
  }[] = [
    { provider: "claude_code", command: "claude", label: "Claude Code" },
    { provider: "opencode", command: "opencode", label: "OpenCode" },
    { provider: "qwen_code", command: "qwen", label: "Qwen Code" },
  ];
  const rows = await Promise.all(
    definitions.map(async (definition) => ({
      definition,
      installed:
        !configuredProviders.has(definition.provider) &&
        (await executableOnPath(definition.command)),
    })),
  );
  return rows
    .filter((row) => row.installed)
    .map(({ definition }) => ({
      profileId: `profile.${definition.provider}.detected`,
      provider: definition.provider,
      displayName: `${definition.label} · model selection required`,
      modelId: "not-selected",
      effort: null,
      interactionKind: "structured",
      installed: true,
      configured: false,
      authenticated: "not_observed",
      launchable: false,
      evidence: [
        "Executable observed through protected PATH discovery",
        "Select protected model and environment settings before launch",
      ],
    }));
}

async function executableOnPath(command: string): Promise<boolean> {
  return (await resolveExecutableOnPath(command)) !== null;
}

async function resolveExecutableOnPath(command: string): Promise<string | null> {
  const extensions =
    process.platform === "win32"
      ? (process.env["PATHEXT"] ?? ".COM;.EXE;.BAT;.CMD").split(";")
      : [""];
  const directories = (process.env["PATH"] ?? "").split(delimiter).filter((path) => path !== "");
  for (const directory of directories) {
    for (const extension of extensions) {
      const candidate = join(directory, `${command}${extension.toLowerCase()}`);
      if (await executable(candidate)) return candidate;
    }
  }
  return null;
}

function providerLabel(provider: ProviderCoordinatorProfile["provider"]): string {
  return provider === "claude_code"
    ? "Claude Code"
    : provider === "opencode"
      ? "OpenCode"
      : "Qwen Code";
}
