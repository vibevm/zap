/** Installed provider discovery for normal startup. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import {
  resolveInstalledCodexExecutable,
  type CodexCoordinatorProfile,
} from "../codex-coordinator/index.ts";
import type { ProductProviderProfile } from "../workspace-model/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";
import { access, constants } from "node:fs/promises";
import { delimiter, join } from "node:path";
import type { ProductLocalSettings } from "./settings.ts";
import type { ManagedWorkerTemplate } from "./settings.ts";

export interface LocalProductProviders {
  readonly coordinatorProfiles: readonly CodexCoordinatorProfile[];
  readonly providerCoordinatorProfiles: readonly ProviderCoordinatorProfile[];
  readonly productProviders: readonly ProductProviderProfile[];
  readonly managedWorkers: readonly ManagedWorkerTemplate[];
}

export async function discoverLocalProductProviders(
  settings: ProductLocalSettings,
): Promise<LocalProductProviders> {
  const executable = await resolveInstalledCodexExecutable();
  const observedProviders = await observeProviderProfiles(settings.providerCoordinators);
  const configuredProviders = observedProviders
    .filter((observation) => observation.installed)
    .map((observation) => observation.profile);
  const configuredCatalog = observedProviders.map((observation) =>
    productProjection(observation.profile, observation.installed),
  );
  const detectedCatalog = await detectedUnconfiguredProviders(settings.providerCoordinators);
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
  };
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
    proxy: settings.coordinatorDefaults.proxy ?? settings.proxy,
    model: settings.coordinatorDefaults.modelId,
    effort: settings.coordinatorDefaults.effort,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "zap-quick-lens",
  };
}

async function observeProviderProfiles(
  profiles: readonly ProviderCoordinatorProfile[],
): Promise<
  readonly { readonly profile: ProviderCoordinatorProfile; readonly installed: boolean }[]
> {
  return Promise.all(
    profiles.map(async (profile) => ({
      profile,
      installed: await executable(profile.executablePath),
    })),
  );
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
  const extensions =
    process.platform === "win32"
      ? (process.env["PATHEXT"] ?? ".COM;.EXE;.BAT;.CMD").split(";")
      : [""];
  const directories = (process.env["PATH"] ?? "").split(delimiter).filter((path) => path !== "");
  for (const directory of directories) {
    for (const extension of extensions) {
      if (await executable(join(directory, `${command}${extension.toLowerCase()}`))) return true;
    }
  }
  return false;
}

function providerLabel(provider: ProviderCoordinatorProfile["provider"]): string {
  return provider === "claude_code"
    ? "Claude Code"
    : provider === "opencode"
      ? "OpenCode"
      : "Qwen Code";
}
