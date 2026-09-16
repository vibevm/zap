/** Compose configured and catalog-materialized coordinator hosts. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import { resolve } from "node:path";
import type { AgentHost } from "../agent-runtime/index.ts";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import {
  createAccountIsolatedCodexProcessFactory,
  type ExecutionAccountIsolationPort,
} from "../execution-accounts/index.ts";
import type {
  ExecutionCatalogCaller,
  ExecutionCatalogService,
} from "../execution-catalog-service/index.ts";
import type { ProductExecutionConfigurationResolver } from "../product-app/index.ts";
import {
  createClaudeStreamJsonTransportFactory,
  createOpenCodeOwnedTransportFactory,
  createProviderCoordinatorHost,
  createQwenStreamJsonTransportFactory,
  type ProviderCoordinatorProfile,
} from "../provider-coordinators/index.ts";
import type { ProductProviderProfile } from "../workspace-model/index.ts";
import {
  createCoordinatorAdapterRegistry,
  type CoordinatorAdapterRegistry,
} from "../workspace-service/index.ts";
import { createCatalogCoordinatorResolver } from "./catalog-coordinator.ts";
import { createCodexHost, type OwnedAgentHost } from "./coordinator-runtime.ts";
import { createProviderLaunchPreparation } from "./product-agent.ts";
import type { WayfinderAgentFoundation } from "./agent.ts";
import type { WayfinderRuntimeConfig } from "./runtime-config.ts";
import type { WayfinderRuntimeOptions } from "./types.ts";

export function createRuntimeCoordinatorHosts(input: {
  readonly config: WayfinderRuntimeConfig;
  readonly options: WayfinderRuntimeOptions;
  readonly databasePath: string;
  readonly accounts: ExecutionAccountIsolationPort;
  readonly foundation: WayfinderAgentFoundation | null;
  readonly profiles: CodexCoordinatorProfile[];
  readonly codexTemplates: CodexCoordinatorProfile[];
  readonly providerTemplates: ProviderCoordinatorProfile[];
  readonly knownProfileIds: string[];
  readonly catalog: ExecutionCatalogService;
  readonly catalogCaller: ExecutionCatalogCaller;
  readonly retainProductProfile: (profile: ProductProviderProfile) => void;
}): {
  readonly adapters: CoordinatorAdapterRegistry;
  readonly ownedHosts: OwnedAgentHost[];
  readonly resolveProductExecution: ProductExecutionConfigurationResolver["resolve"];
  restoreProductProfiles(
    entries: readonly {
      readonly project: {
        readonly projectId: string;
        readonly profileId: string;
        readonly directoryPath: string;
        readonly executionConfigurationId: string | null;
      };
    }[],
  ): Promise<ReadonlySet<string>>;
} {
  const processFactory =
    input.options.processFactory ??
    createAccountIsolatedCodexProcessFactory({
      isolation: input.accounts,
      hostId: input.config.executionHostId,
      proxyPolicy: input.config.proxy,
    });
  const ownedHosts: OwnedAgentHost[] =
    input.options.hosts === undefined
      ? input.profiles.map((profile) => createCodexHost(profile, processFactory))
      : [];
  const launchPreparation = createProviderLaunchPreparation({
    foundation: input.foundation,
    environment: input.options.managedEnvironment,
    mcpRoot: resolve(`${input.databasePath}.provider-mcp`),
    accounts: input.accounts,
  });
  const providerHost = (profile: ProviderCoordinatorProfile) =>
    createProviderCoordinatorHost({
      hostId: `host.provider.${profile.profileId}`,
      profile,
      transportFactory:
        profile.provider === "claude_code"
          ? createClaudeStreamJsonTransportFactory({
              proxyPolicy: input.config.proxy,
              prepareLaunch: launchPreparation,
            })
          : profile.provider === "opencode"
            ? createOpenCodeOwnedTransportFactory({
                proxyPolicy: input.config.proxy,
                prepareLaunch: launchPreparation,
              })
            : createQwenStreamJsonTransportFactory({
                proxyPolicy: input.config.proxy,
                prepareLaunch: launchPreparation,
              }),
    });
  const providerHosts = input.config.providerCoordinatorProfiles.map(providerHost);
  const hosts: readonly AgentHost[] =
    input.options.hosts === undefined
      ? [...ownedHosts, ...providerHosts]
      : [...input.options.hosts, ...providerHosts];
  const adapters = createCoordinatorAdapterRegistry(
    hosts.flatMap((host) => host.profileIds.map((profileRef) => ({ profileRef, host }))),
  );
  const resolver = createCatalogCoordinatorResolver({
    catalog: input.catalog,
    caller: input.catalogCaller,
    codexTemplates: input.codexTemplates,
    providerTemplates: input.providerTemplates,
    processFactory,
    providerHost,
    registerHost(profileId, host) {
      const registered = adapters.register?.({ profileRef: profileId, host });
      if (registered === undefined)
        return { ok: false, message: "coordinator registry does not support dynamic profiles" };
      return registered.ok ? { ok: true } : { ok: false, message: registered.error.message };
    },
    retainCodexHost(host, profile) {
      ownedHosts.push(host);
      input.profiles.push(profile);
      input.codexTemplates.push(profile);
      input.knownProfileIds.push(profile.profileId);
    },
    retainProviderProfile(profile) {
      input.providerTemplates.push(profile);
      input.knownProfileIds.push(profile.profileId);
    },
  });
  const guardedAdapters: CoordinatorAdapterRegistry = {
    ...(adapters.register === undefined
      ? {}
      : { register: (registration) => adapters.register?.(registration) ?? registrationFailure() }),
    async resolve(profileRef) {
      const allowed = await resolver.validateProfile(profileRef);
      return allowed.ok
        ? adapters.resolve(profileRef)
        : {
            ok: false,
            error: { code: "policy_denied", message: allowed.message, retry: "never" },
          };
    },
  };
  const resolveProductExecution: ProductExecutionConfigurationResolver["resolve"] = async (
    request,
  ) => {
    const result = await resolver.resolve(request);
    if (result.ok) input.retainProductProfile(result.value);
    return result;
  };
  return {
    adapters: guardedAdapters,
    ownedHosts,
    resolveProductExecution,
    async restoreProductProfiles(entries) {
      const unavailable = new Set<string>();
      for (const entry of entries) {
        const configurationId = entry.project.executionConfigurationId;
        if (configurationId === null) continue;
        const restored = await resolveProductExecution({
          configurationId,
          directoryPath: entry.project.directoryPath,
        });
        if (!restored.ok || restored.value.profileId !== entry.project.profileId)
          unavailable.add(entry.project.projectId);
      }
      return unavailable;
    },
  };
}

function registrationFailure() {
  return {
    ok: false as const,
    error: {
      code: "host_refused" as const,
      message: "coordinator registry refused dynamic profile registration",
      retry: "never" as const,
    },
  };
}
