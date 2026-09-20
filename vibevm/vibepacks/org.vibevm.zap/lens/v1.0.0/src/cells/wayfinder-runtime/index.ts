/** Runnable local Zap Wayfinder composition over the shared workspace service.
 * @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership
 */
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { CodexCoordinatorProfileSchema } from "../codex-coordinator/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import {
  createDynamicWorkspacePort,
  createProductAppService,
  openProductAppRegistry,
} from "../product-app/index.ts";
import { createWorkspaceService } from "../workspace-service/index.ts";
import { createWayfinderAnnotationBindings } from "./annotations.ts";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import { createModelPolicyService } from "../model-policy-service/index.ts";
import { openModelPolicyStore, type ModelPolicyStore } from "../model-policy-store/index.ts";
import { createConfiguredCoordinatorRoutingProvider } from "../coordinator-routing/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import type { ProjectId } from "../workspace-model/index.ts";
import { openWayfinderAgentFoundation, type WayfinderAgentFoundation } from "./agent.ts";
import { createManagedRuntimeController, type ManagedRuntimeController } from "./managed.ts";
import {
  openConfiguredManagedWorkRuntime,
  type ConfiguredManagedWorkRuntime,
} from "./managed-work.ts";
import {
  createWorkspacePlanningController,
  type WorkspacePlanningController,
} from "../workspace-planning/index.ts";
import { openWayfinderWebRuntime, type WayfinderWebRuntime } from "./web.ts";
import {
  createRuntimeRoutingBridge,
  disposeOwnedHosts,
  initializeRuntimePolicies,
} from "./coordinator-runtime.ts";
import type {
  WayfinderReceipt,
  WayfinderResult,
  WayfinderRuntime,
  WayfinderRuntimeOptions,
} from "./types.ts";
import { prepareAgentScope } from "./product-agent.ts";
import { openRuntimeAnnotations } from "./annotation-composition.ts";
import { invalidConfig } from "./runtime-result.ts";
import { createRuntimeTrustedContextProvider } from "./model-policy-wire.ts";
import { createQuicklensGateway, type QuicklensGateway } from "../quicklens-service/index.ts";
import { createWayfinderManagedWake } from "./managed-wake-composition.ts";
import {
  openRuntimeRepositoryWorkspaces,
  type RuntimeRepositoryWorkspaces,
} from "./repository-runtime.ts";
import {
  createRuntimeRepositoryFeature,
  ensureRuntimeRepositoryContextPlan,
} from "./repository-feature.ts";
import { createRepositoryResolutionComposition } from "./repository-resolution.ts";
import { createRepositoryWriterAdmission } from "./repository-writer.ts";
import { attachRuntimePlanningSource } from "./planning-attachment.ts";
import { createRuntimeRepositoryWriterActivity } from "./repository-activity.ts";
import { reconcileDetachedPlanningSources } from "./planning-recovery.ts";
import { failure as transportFailure } from "../transport/index.ts";
import { WayfinderRuntimeConfigSchema } from "./runtime-config.ts";
import {
  openConfiguredRuntimeExecutionCatalog,
  seedConfiguredExecutionCatalog,
} from "./execution-catalog-runtime.ts";
import { createRuntimeCoordinatorHosts } from "./coordinator-host-composition.ts";
import { createDeferredProductExecutionResolver } from "./catalog-coordinator.ts";
import { knownRuntimeProfileIds, validateRuntimeProfileIds } from "./runtime-profiles.ts";
export { loadWayfinderConfig, WayfinderRuntimeConfigSchema } from "./runtime-config.ts";
export type { WayfinderRuntimeConfig } from "./runtime-config.ts";

export function createWayfinderRuntime(
  rawConfig: unknown,
  options: WayfinderRuntimeOptions = {},
): WayfinderResult<WayfinderRuntime> {
  const parsed = WayfinderRuntimeConfigSchema.safeParse(rawConfig);
  if (!parsed.success) return invalidConfig("config schema is invalid");
  const config = parsed.data;
  const profileError = validateRuntimeProfileIds(config, options);
  if (profileError !== null) return invalidConfig(profileError);
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
  let repositoryRuntime: RuntimeRepositoryWorkspaces | null = null;
  const profiles = config.profiles.map((profile) => CodexCoordinatorProfileSchema.parse(profile));
  const executionRuntime = openConfiguredRuntimeExecutionCatalog({
    config,
    databasePath,
    profiles,
  });
  if (!executionRuntime.ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig(executionRuntime.message);
  }
  const catalogCodexProfiles = executionRuntime.value.codexProfiles;
  const catalogProviderProfiles = executionRuntime.value.providerProfiles;
  const runtimeProductProviders = executionRuntime.value.productProviders;
  const accountIsolation = executionRuntime.value.runtime.accounts;
  const executionCatalogStore = executionRuntime.value.runtime.store;
  const executionCatalog = executionRuntime.value.runtime.service;
  const productCatalogCaller = executionRuntime.value.runtime.productCaller;
  const knownCoordinatorProfileIds = knownRuntimeProfileIds(config, profiles);
  for (const profile of config.managedAgents)
    if (profile.provider === "zap_mock") knownCoordinatorProfileIds.push(profile.profileId);
  let agentFoundation: WayfinderAgentFoundation | null = null;
  if (config.agentGateway !== undefined) {
    const openedAgent = openWayfinderAgentFoundation(
      config.agentGateway,
      store,
      planningController?.feature,
    );
    if (!openedAgent.ok) {
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(openedAgent.message);
    }
    agentFoundation = openedAgent.value;
    const nativeWork = agentFoundation.bindNativeWork(annotationBindings.attachments);
    if (!nativeWork.ok) {
      void agentFoundation.close();
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(nativeWork.message);
    }
  }
  let managedWorkRuntime: ConfiguredManagedWorkRuntime | null = null;
  const managedBackendBinding: { backend: ManagedAgentBackend | undefined } = {
    backend: undefined,
  };
  const repositoryWriterActivity = createRuntimeRepositoryWriterActivity({
    managedWork: () => managedBackendBinding.backend,
    terminals: () => options.terminals ?? managedController?.service,
  });
  const productExecution = createDeferredProductExecutionResolver();
  let repositoryManaged: ReturnType<typeof createRepositoryResolutionComposition> | null = null;
  const registry = openProductAppRegistry(
    resolve(config.state.productRegistryPath ?? `${databasePath}.projects.json`),
  );
  if (!registry.ok) {
    executionCatalogStore.close();
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig(registry.message);
  }
  const product = createProductAppService({
    registry: registry.value,
    workspaceStore: store,
    providers: runtimeProductProviders,
    additionalProviders: () => runtimeProductProviders,
    executionCatalog,
    executionCatalogCaller: productCatalogCaller,
    executionConfigurations: productExecution,
    prepareRegistration: (registration) =>
      prepareAgentScope(
        agentFoundation,
        profiles,
        registration,
        modelPolicyStore,
        knownCoordinatorProfileIds,
        managedWorkRuntime?.backend ?? options.managedWork,
        runtimeProductProviders,
        config.managedWorkerProfiles,
        catalogProviderProfiles,
        resolve(`${databasePath}.managed-mcp`),
        config.proxy,
      ),
  });
  if (!product.ok || !product.value.hydrate().ok) {
    executionCatalogStore.close();
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
    executionCatalogStore.close();
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
  if (config.repositoryWorkspaces !== undefined) {
    const openedRepository = openRuntimeRepositoryWorkspaces({
      config: config.repositoryWorkspaces,
      workspaceDatabasePath: databasePath,
      workspaceStore: store,
      writerActivity: repositoryWriterActivity,
    });
    if (!openedRepository.ok) {
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(openedRepository.message);
    }
    repositoryRuntime = openedRepository.value;
    repositoryManaged = createRepositoryResolutionComposition({
      repositories: repositoryRuntime.service,
      executionHostId: repositoryRuntime.executionHostId,
      backend: () => managedBackendBinding.backend,
    });
  }
  const repositoryFeatureInput =
    repositoryRuntime === null
      ? null
      : { runtime: repositoryRuntime, store, product: product.value };
  if (agentFoundation !== null && repositoryFeatureInput !== null) {
    const bound = agentFoundation.bindRepositoryPlans(async (input) => {
      const ensured = await ensureRuntimeRepositoryContextPlan(
        repositoryFeatureInput,
        input.access.principalId,
        input.projectId,
        input.contextId,
      );
      return ensured.ok
        ? { ok: true, value: ensured.value.plan.planId }
        : transportFailure(
            ensured.error.code === "forbidden" ? "forbidden" : "storage_failure",
            ensured.error.message,
            "retry through the exact registered repository context",
          );
    });
    if (!bound.ok) {
      repositoryRuntime?.close();
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(bound.message);
    }
  }
  if (options.managedWork === undefined) {
    const terminals = options.terminals ?? managedController?.service;
    if (terminals === undefined || agentFoundation === null) {
      if (config.managedAgents.length === 0) {
        managedWorkRuntime = null;
      } else {
        executionCatalogStore.close();
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
        accounts: accountIsolation,
        executionCatalog: { service: executionCatalog, hostId: config.executionHostId },
        routingProvider,
        proxyPolicy: config.proxy,
        workspaceStore: store,
        controlAdapters: options.managedControlAdapters ?? [],
        ...(options.managedEnvironment === undefined
          ? {}
          : { environment: options.managedEnvironment }),
        attachments: options.managedAttachments ?? annotationBindings.attachments,
        ...(repositoryManaged === null ? {} : { workspaces: repositoryManaged.workspaces }),
        ...(repositoryRuntime === null
          ? {}
          : { canOpenWriter: createRepositoryWriterAdmission(repositoryRuntime) }),
      });
      if (!managed.ok) {
        void agentFoundation.close();
        executionCatalogStore.close();
        modelPolicyStore.close();
        if (options.store === undefined) store.close();
        return invalidConfig(managed.message);
      }
      managedWorkRuntime = managed.value;
    }
  }
  const activeManagedBackend = options.managedWork ?? managedWorkRuntime?.backend;
  managedBackendBinding.backend = activeManagedBackend;
  if (agentFoundation !== null && activeManagedBackend !== undefined) {
    const bound = agentFoundation.bindManagedWork(activeManagedBackend);
    if (!bound.ok) {
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(bound.message);
    }
  }
  const coordinatorHosts = createRuntimeCoordinatorHosts({
    config,
    options,
    databasePath,
    accounts: accountIsolation,
    foundation: agentFoundation,
    profiles,
    codexTemplates: catalogCodexProfiles,
    providerTemplates: catalogProviderProfiles,
    knownProfileIds: knownCoordinatorProfileIds,
    catalog: executionCatalog,
    catalogCaller: productCatalogCaller,
    retainProductProfile(profile) {
      if (!runtimeProductProviders.some((candidate) => candidate.profileId === profile.profileId))
        runtimeProductProviders.push(profile);
    },
  });
  const adapterRegistry = coordinatorHosts.adapters;
  const ownedHosts = coordinatorHosts.ownedHosts;
  productExecution.bind(coordinatorHosts.resolveProductExecution);
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
    ...(repositoryRuntime === null ? {} : { repositories: repositoryRuntime.service }),
    ...(options.annotationRestoreIntent === undefined
      ? {}
      : { restoreIntent: options.annotationRestoreIntent }),
  });
  if (!annotations.ok) {
    executionCatalogStore.close();
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalidConfig(annotations.message);
  }
  annotationBindings.bind(annotations.value.runtime);
  const managedWake =
    activeManagedBackend === undefined
      ? undefined
      : createWayfinderManagedWake({
          backend: activeManagedBackend,
          projects: () =>
            [...config.projects, ...product.value.registrations()].map((project) => ({
              projectId: project.projectId,
              contextId: project.context.contextId,
            })),
        });
  const repositoryFeature =
    repositoryFeatureInput === null
      ? undefined
      : createRuntimeRepositoryFeature({
          ...repositoryFeatureInput,
          annotations: options.annotations ?? annotations.value.runtime.service,
          ...(repositoryManaged === null ? {} : { resolution: repositoryManaged.resolution }),
        });
  if (agentFoundation !== null && repositoryFeature !== undefined) {
    const bound = agentFoundation.bindRepositoryWork(repositoryFeature);
    if (!bound.ok) {
      repositoryRuntime?.close();
      executionCatalogStore.close();
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalidConfig(bound.message);
    }
  }
  const service = createWorkspaceService({
    store,
    adapters: adapterRegistry,
    coordinatorRouting,
    modelPolicy,
    executionCatalog,
    terminals: options.terminals ?? managedController?.service,
    interactions: agentFoundation?.interactions,
    ownedCoordinatorAgents: agentFoundation?.ownedCoordinators,
    planning: planningController?.feature,
    managedWork: options.managedWork ?? managedWorkRuntime?.backend,
    ...(managedWake === undefined ? {} : { managedWake }),
    annotations: options.annotations ?? annotations.value.runtime.service,
    ...(repositoryFeature === undefined ? {} : { repositoryWorkspaces: repositoryFeature }),
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
      const seededCatalog = await seedConfiguredExecutionCatalog({
        service: executionCatalog,
        caller: productCatalogCaller,
        codexProfiles: config.profiles,
        providerProfiles: config.providerCoordinatorProfiles,
        managedProfiles: config.managedAgents,
      });
      if (!seededCatalog.ok) return invalidConfig(seededCatalog.message);
      const unavailableCatalogProjects = await coordinatorHosts.restoreProductProfiles(
        registry.value.entries(),
      );
      for (const registration of [...config.projects, ...product.value.registrations()]) {
        if (unavailableCatalogProjects.has(registration.projectId)) continue;
        const prepared = await prepareAgentScope(
          agentFoundation,
          profiles,
          registration,
          modelPolicyStore,
          knownCoordinatorProfileIds,
          managedWorkRuntime?.backend ?? options.managedWork,
          runtimeProductProviders,
          config.managedWorkerProfiles,
          catalogProviderProfiles,
          resolve(`${databasePath}.managed-mcp`),
          config.proxy,
        );
        if (!prepared.ok) return invalidConfig(prepared.error.message);
      }
      const planningStarted = await planningController?.start();
      if (planningStarted !== undefined && !planningStarted.ok)
        return invalidConfig("shared planning runtime could not start");
      const planningRecovery = await reconcileDetachedPlanningSources({
        store,
        planning: planningController,
        repositories: repositoryRuntime,
        projectIds: projectIds(),
      });
      if (!planningRecovery.ok) return planningRecovery;
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
        productCatalogAdministrator: true,
        ...(options.observeGatewayRequest === undefined
          ? {}
          : { observeRequest: options.observeGatewayRequest }),
        workspaceSource: (identity) =>
          createDynamicWorkspacePort({
            service,
            product: product.value,
            baselineProjectIds: initialProjectIds,
            clientId: identity.clientId,
            principalId: `principal.wayfinder.${identity.clientId}`,
            catalogAdministrator: true,
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
    async attachPlanningSource(input) {
      return attachRuntimePlanningSource({
        planning: planningController,
        repositories: repositoryRuntime,
        store,
        attachment: input,
      });
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
      executionCatalogStore.close();
      modelPolicyStore.close();
      repositoryRuntime?.close();
      disposeOwnedHosts(ownedHosts);
      if (options.store === undefined) store.close();
    },
  };
  return { ok: true, value: runtime };
}

export * from "./public.ts";
