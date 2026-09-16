/** Runnable local Zap Wayfinder composition over the shared workspace service.
 * @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership
 */
import { readFile } from "node:fs/promises";
import { mkdirSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { z } from "zod";
import type { AgentHost } from "../agent-runtime/index.ts";
import {
  CodexCoordinatorProfileSchema,
  createNodeCodexProcessFactory,
} from "../codex-coordinator/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import {
  createClaudeStreamJsonTransportFactory,
  createOpenCodeOwnedTransportFactory,
  createProviderCoordinatorHost,
  createQwenStreamJsonTransportFactory,
  ProviderCoordinatorProfileSchema,
} from "../provider-coordinators/index.ts";
import {
  createDynamicWorkspacePort,
  createProductAppService,
  ManagedWorkerTemplateSchema,
  openProductAppRegistry,
} from "../product-app/index.ts";
import {
  createCoordinatorAdapterRegistry,
  createWorkspaceService,
} from "../workspace-service/index.ts";
import { createWayfinderAnnotationBindings } from "./annotations.ts";
import { ManagedAgentProfileSchema } from "../managed-work/index.ts";
import { createModelPolicyService } from "../model-policy-service/index.ts";
import { openModelPolicyStore, type ModelPolicyStore } from "../model-policy-store/index.ts";
import {
  CoordinatorRoutingConfigSchema,
  createConfiguredCoordinatorRoutingProvider,
} from "../coordinator-routing/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { type ProjectId, ProductProviderProfileSchema } from "../workspace-model/index.ts";
import {
  openWayfinderAgentFoundation,
  WayfinderAgentGatewayConfigSchema,
  type WayfinderAgentFoundation,
} from "./agent.ts";
import {
  createManagedRuntimeController,
  ManagedRuntimeConfigSchema,
  type ManagedRuntimeController,
} from "./managed.ts";
import {
  openConfiguredManagedWorkRuntime,
  type ConfiguredManagedWorkRuntime,
} from "./managed-work.ts";
import {
  createWorkspacePlanningController,
  WorkspacePlanningRuntimeConfigSchema,
  type WorkspacePlanningController,
} from "../workspace-planning/index.ts";
import {
  openWayfinderWebRuntime,
  WayfinderWebConfigSchema,
  type WayfinderWebRuntime,
} from "./web.ts";
import {
  createCodexHost,
  createRuntimeRoutingBridge,
  disposeOwnedHosts,
  initializeRuntimePolicies,
  type OwnedAgentHost,
} from "./coordinator-runtime.ts";
import type {
  WayfinderReceipt,
  WayfinderResult,
  WayfinderRuntime,
  WayfinderRuntimeOptions,
} from "./types.ts";
import { createProviderLaunchPreparation, prepareAgentScope } from "./product-agent.ts";
import { openRuntimeAnnotations } from "./annotation-composition.ts";
import { invalidConfig } from "./runtime-result.ts";
import { createRuntimeTrustedContextProvider } from "./model-policy-wire.ts";
import { createQuicklensGateway, type QuicklensGateway } from "../quicklens-service/index.ts";

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
      ) {
        context.addIssue({
          code: "custom",
          path: ["profiles", index, "lensMcp", "brokerUrl"],
          message: "Lens MCP URL must match the fixed configured agent gateway",
        });
      }
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

export function createWayfinderRuntime(
  rawConfig: unknown,
  options: WayfinderRuntimeOptions = {},
): WayfinderResult<WayfinderRuntime> {
  const parsed = WayfinderRuntimeConfigSchema.safeParse(rawConfig);
  if (!parsed.success) return invalidConfig("config schema is invalid");
  const config = parsed.data;
  const providerProfileIds = new Set(
    config.providerCoordinatorProfiles.map((profile) => profile.profileId),
  );
  const injectedProfileIds = options.hosts?.flatMap((host) => host.profileIds) ?? [];
  if (
    new Set(injectedProfileIds).size !== injectedProfileIds.length ||
    injectedProfileIds.some((profileId) => providerProfileIds.has(profileId))
  )
    return invalidConfig("injected and configured provider profile IDs must be unique");
  let managedController: ManagedRuntimeController | null = null;
  if (config.managedTerminals !== undefined && options.terminals === undefined) {
    const managed = createManagedRuntimeController(config.managedTerminals);
    if (!managed.ok) return invalidConfig(managed.error.message);
    managedController = managed.value;
  }
  let planningController: WorkspacePlanningController | null = null;
  const databasePath = resolve(config.state.databasePath);
  try {
    mkdirSync(dirname(databasePath), { recursive: true });
  } catch {
    return invalidConfig("workspace state directory could not be created");
  }
  const storeResult =
    options.store === undefined
      ? openWorkspaceStore({ databasePath })
      : { ok: true as const, value: options.store };
  if (!storeResult.ok) return invalidConfig("workspace store could not be opened");
  const store = storeResult.value;
  const annotationBindings = createWayfinderAnnotationBindings(store);
  if (config.planning !== undefined) {
    const planning = createWorkspacePlanningController(
      config.planning,
      store,
      annotationBindings.sourceObserver,
    );
    if (!planning.ok) {
      if (options.store === undefined) store.close();
      return invalidConfig(planning.error.message);
    }
    planningController = planning.value;
  }
  const modelPolicyPath = resolve(
    config.state.modelPolicyDatabasePath ?? `${databasePath}.model-policy`,
  );
  const openedPolicyStore = openModelPolicyStore({ databasePath: modelPolicyPath });
  if (!openedPolicyStore.ok) {
    if (options.store === undefined) store.close();
    return invalidConfig("model policy store could not be opened");
  }
  const modelPolicyStore: ModelPolicyStore = openedPolicyStore.value;
  for (const project of config.projects) {
    const registered = store.registerProject(project);
    if (!registered.ok) {
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig("trusted project registration failed");
    }
  }
  const profiles = config.profiles.map((profile) => CodexCoordinatorProfileSchema.parse(profile));
  const knownCoordinatorProfileIds = [
    ...profiles.map((profile) => profile.profileId),
    ...config.providerCoordinatorProfiles.map((profile) => profile.profileId),
  ];
  let agentFoundation: WayfinderAgentFoundation | null = null;
  if (config.agentGateway !== undefined) {
    const openedAgent = openWayfinderAgentFoundation(
      config.agentGateway,
      store,
      planningController?.feature,
    );
    if (!openedAgent.ok) {
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(openedAgent.message);
    }
    agentFoundation = openedAgent.value;
    const nativeWork = agentFoundation.bindNativeWork(annotationBindings.attachments);
    if (!nativeWork.ok) {
      void agentFoundation.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(nativeWork.message);
    }
  }
  let managedWorkRuntime: ConfiguredManagedWorkRuntime | null = null;
  const registry = openProductAppRegistry(
    resolve(config.state.productRegistryPath ?? `${databasePath}.projects.json`),
  );
  if (!registry.ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig(registry.message);
  }
  const product = createProductAppService({
    registry: registry.value,
    workspaceStore: store,
    providers: config.productProviders,
    prepareRegistration: (registration) =>
      prepareAgentScope(
        agentFoundation,
        profiles,
        registration,
        modelPolicyStore,
        knownCoordinatorProfileIds,
        managedWorkRuntime?.backend ?? options.managedWork,
        config.productProviders,
        config.managedWorkerProfiles,
        config.providerCoordinatorProfiles,
        resolve(`${databasePath}.managed-mcp`),
        config.proxy,
      ),
  });
  if (!product.ok || !product.value.hydrate().ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig("product project registry could not be hydrated");
  }
  const initialProjectIds = config.projects.map((project) => project.projectId);
  const initializedPolicies = initializeRuntimePolicies(
    modelPolicyStore,
    initialProjectIds,
    config,
  );
  if (!initializedPolicies.ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig("configured model policy initialization failed");
  }
  const routingProvider =
    config.routing === undefined
      ? undefined
      : createConfiguredCoordinatorRoutingProvider(config.routing);
  const coordinatorRouting =
    config.routing === undefined || routingProvider === undefined
      ? undefined
      : createRuntimeRoutingBridge(config.routing, modelPolicyStore, routingProvider);
  if (options.managedWork === undefined) {
    const terminals = options.terminals ?? managedController?.service;
    if (terminals === undefined || agentFoundation === null) {
      if (config.managedAgents.length === 0) {
        managedWorkRuntime = null;
      } else {
        modelPolicyStore.close();
        if (options.store === undefined) store.close();
        return invalidConfig("managed agent profiles require managed terminals and agent broker");
      }
    } else {
      const managed = openConfiguredManagedWorkRuntime({
        profiles: config.managedAgents,
        databasePath: resolve(`${databasePath}.managed-work`),
        terminals,
        bindings: agentFoundation.managedActors,
        policyStore: modelPolicyStore,
        routingProvider,
        proxyPolicy: config.proxy,
        workspaceStore: store,
        ...(options.managedEnvironment === undefined
          ? {}
          : { environment: options.managedEnvironment }),
        attachments: options.managedAttachments ?? annotationBindings.attachments,
      });
      if (!managed.ok) {
        void agentFoundation.close();
        modelPolicyStore.close();
        if (options.store === undefined) store.close();
        return invalidConfig(managed.message);
      }
      managedWorkRuntime = managed.value;
    }
  }
  const activeManagedBackend = options.managedWork ?? managedWorkRuntime?.backend;
  if (agentFoundation !== null && activeManagedBackend !== undefined) {
    const bound = agentFoundation.bindManagedWork(activeManagedBackend);
    if (!bound.ok) {
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(bound.message);
    }
  }
  const ownedProcessFactory =
    options.processFactory ?? createNodeCodexProcessFactory({ proxyPolicy: config.proxy });
  const ownedHosts: readonly OwnedAgentHost[] =
    options.hosts === undefined
      ? profiles.map((profile) => createCodexHost(profile, ownedProcessFactory))
      : [];
  const providerLaunchPreparation = createProviderLaunchPreparation({
    foundation: agentFoundation,
    environment: options.managedEnvironment,
    mcpRoot: resolve(`${databasePath}.provider-mcp`),
  });
  const providerHosts = config.providerCoordinatorProfiles.map((profile) =>
    createProviderCoordinatorHost({
      hostId: `host.provider.${profile.profileId}`,
      profile,
      transportFactory:
        profile.provider === "claude_code"
          ? createClaudeStreamJsonTransportFactory({
              proxyPolicy: config.proxy,
              prepareLaunch: providerLaunchPreparation,
            })
          : profile.provider === "opencode"
            ? createOpenCodeOwnedTransportFactory({
                proxyPolicy: config.proxy,
                prepareLaunch: providerLaunchPreparation,
              })
            : createQwenStreamJsonTransportFactory({
                proxyPolicy: config.proxy,
                prepareLaunch: providerLaunchPreparation,
              }),
    }),
  );
  const hosts: readonly AgentHost[] =
    options.hosts === undefined
      ? [...ownedHosts, ...providerHosts]
      : [...options.hosts, ...providerHosts];
  const registrations = hosts.flatMap((host) =>
    host.profileIds.map((profileRef) => ({ profileRef, host })),
  );
  const modelPolicy = createModelPolicyService({
    store: modelPolicyStore,
    trustedContext: createRuntimeTrustedContextProvider(routingProvider),
  });
  const annotations = openRuntimeAnnotations({
    databasePath,
    ...(config.state.annotationsDatabasePath === undefined
      ? {}
      : { configuredPath: config.state.annotationsDatabasePath }),
    store,
    ...(planningController === null ? {} : { planning: planningController.feature }),
    ...(managedWorkRuntime === null ? {} : { managedWork: managedWorkRuntime.backend }),
    ...(options.annotationRestoreIntent === undefined
      ? {}
      : { restoreIntent: options.annotationRestoreIntent }),
  });
  if (!annotations.ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig(annotations.message);
  }
  annotationBindings.bind(annotations.value.runtime);
  const service = createWorkspaceService({
    store,
    adapters: createCoordinatorAdapterRegistry(registrations),
    coordinatorRouting,
    modelPolicy,
    terminals: options.terminals ?? managedController?.service,
    interactions: agentFoundation?.interactions,
    ownedCoordinatorAgents: agentFoundation?.ownedCoordinators,
    planning: planningController?.feature,
    managedWork: options.managedWork ?? managedWorkRuntime?.backend,
    annotations: options.annotations ?? annotations.value.runtime.service,
  });
  annotations.value.bindService(service);
  let gateway: QuicklensGateway | null = null;
  let webRuntime: WayfinderWebRuntime | undefined;
  let webReceipt: { readonly host: string; readonly port: number } | null = null;
  let receipt: WayfinderReceipt | null = null;
  const projectIds = (): readonly ProjectId[] => [
    ...initialProjectIds,
    ...product.value.projectIds().filter((projectId) => !initialProjectIds.includes(projectId)),
  ];
  const runtime: WayfinderRuntime = {
    service,
    store,
    get receipt() {
      return receipt;
    },
    async start(): Promise<WayfinderResult<WayfinderReceipt>> {
      if (gateway !== null && receipt !== null) return { ok: true, value: receipt };
      const managedStarted = await managedController?.start();
      if (managedStarted !== undefined && !managedStarted.ok)
        return invalidConfig("managed terminal runtime could not start");
      const agentStarted =
        agentFoundation === null
          ? { ok: true as const, value: null }
          : await agentFoundation.start();
      if (!agentStarted.ok) return invalidConfig("agent gateway could not bind");
      for (const registration of [...config.projects, ...product.value.registrations()]) {
        const prepared = await prepareAgentScope(
          agentFoundation,
          profiles,
          registration,
          modelPolicyStore,
          knownCoordinatorProfileIds,
          managedWorkRuntime?.backend ?? options.managedWork,
          config.productProviders,
          config.managedWorkerProfiles,
          config.providerCoordinatorProfiles,
          resolve(`${databasePath}.managed-mcp`),
          config.proxy,
        );
        if (!prepared.ok) return invalidConfig(prepared.error.message);
      }
      const planningStarted = await planningController?.start();
      if (planningStarted !== undefined && !planningStarted.ok)
        return invalidConfig("shared planning runtime could not start");
      const opened = createQuicklensGateway({
        source: unavailableDataSource(
          "Zap Wayfinder ZAP plan source is not configured for this local profile.",
        ),
        namespace: config.gateway.namespace,
        pairingToken: config.gateway.pairingToken,
        allowedHosts: config.gateway.allowedHosts,
        allowedOrigins: config.gateway.allowedOrigins,
        multiSession: true,
        productSource: product.value,
        workspaceSource: (identity) =>
          createDynamicWorkspacePort({
            service,
            product: product.value,
            baselineProjectIds: initialProjectIds,
            clientId: identity.clientId,
            principalId: `principal.wayfinder.${identity.clientId}`,
          }),
      });
      if (!opened.ok) {
        await agentFoundation?.close();
        return invalidConfig("workspace gateway configuration is invalid");
      }
      gateway = opened.value;
      const started = await gateway.start({ host: config.gateway.host, port: config.gateway.port });
      if (!started.ok) {
        gateway = null;
        await agentFoundation?.close();
        return invalidConfig("workspace gateway could not bind");
      }
      if (config.web !== undefined) {
        const openedWeb = await openWayfinderWebRuntime(config.web, service);
        if (!openedWeb.ok) {
          await gateway.close();
          gateway = null;
          await agentFoundation?.close();
          return invalidConfig(openedWeb.error.message);
        }
        if (openedWeb.value !== undefined) {
          webRuntime = openedWeb.value;
          const webStarted = await webRuntime.start();
          if (!webStarted.ok) {
            await webRuntime.close();
            webRuntime = undefined;
            await gateway.close();
            gateway = null;
            await agentFoundation?.close();
            return invalidConfig(webStarted.error.message);
          }
          webReceipt = webStarted.value;
        }
      }
      const nextReceipt: WayfinderReceipt = {
        ...started.value,
        databasePath,
        projectIds: projectIds(),
        agentGateway: agentStarted.value,
        ...(webReceipt === null ? {} : { web: webReceipt }),
      };
      receipt = nextReceipt;
      return { ok: true, value: nextReceipt };
    },
    issuePairingTicket() {
      return gateway?.issuePairingTicket?.() ?? invalidConfig("Wayfinder gateway is not started");
    },
    async close(): Promise<void> {
      if (webRuntime !== undefined) await webRuntime.close();
      webRuntime = undefined;
      webReceipt = null;
      if (gateway !== null) await gateway.close();
      await agentFoundation?.close();
      managedController?.close();
      managedWorkRuntime?.close();
      planningController?.close();
      gateway = null;
      receipt = null;
      service.close();
      annotations.value.runtime.close();
      modelPolicyStore.close();
      disposeOwnedHosts(ownedHosts);
      if (options.store === undefined) store.close();
    },
  };
  return { ok: true, value: runtime };
}

export {
  createManagedRuntimeController,
  ManagedRuntimeConfigSchema,
  openConfiguredManagedRuntime,
} from "./managed.ts";
export type {
  ConfiguredManagedTerminalService,
  ManagedRuntimeConfig,
  ManagedRuntimeController,
  ManagedWorkerLaunchRequest,
} from "./managed.ts";
export { acquireWayfinderOwner, requestRunningOwnerTicket } from "./owner.ts";
export type { OwnerResult, WayfinderOwnerLease } from "./owner.ts";
export type {
  WayfinderReceipt,
  WayfinderResult,
  WayfinderRuntime,
  WayfinderRuntimeOptions,
} from "./types.ts";
