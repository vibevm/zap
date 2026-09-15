/** Runnable local Zap Wayfinder composition over the shared workspace service.
 * @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership
 */
import { readFile } from "node:fs/promises";
import { mkdirSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { z } from "zod";
import type { AgentHost, CoordinatorAdapter } from "../agent-runtime/index.ts";
import {
  createCodexCoordinatorAdapter,
  CodexCoordinatorProfileSchema,
  createNodeCodexProcessFactory,
  type CodexProcessFactory,
  type CodexCoordinatorProfile,
} from "../codex-coordinator/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import {
  WORKSPACE_SERVICE_ACTIONS,
  createCoordinatorAdapterRegistry,
  createWorkspaceService,
  type CoordinatorRoutingBridge,
  type WorkspaceService,
  type WorkspaceManagedTerminalPort,
} from "../workspace-service/index.ts";
import { createModelPolicyService } from "../model-policy-service/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  openModelPolicyStore,
  type ModelPolicyStore,
} from "../model-policy-store/index.ts";
import {
  CoordinatorRoutingConfigSchema,
  createConfiguredCoordinatorRoutingProvider,
  initializeConfiguredCoordinatorPolicies,
  resolveCoordinatorLaunch,
  type CoordinatorRoutingProvider,
  type CoordinatorRoutingConfig,
} from "../coordinator-routing/index.ts";
import {
  openWorkspaceStore,
  TrustedProjectRegistrationSchema,
  type WorkspaceStore,
} from "../workspace-store/index.ts";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  WorkspaceAccessContextSchema,
  type ClientId,
  type ExecutionHostId,
  type ProjectId,
  type WorkContextId,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
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
  createWorkspacePlanningController,
  WorkspacePlanningRuntimeConfigSchema,
  type WorkspacePlanningController,
} from "../workspace-planning/index.ts";
import {
  openWayfinderWebRuntime,
  WayfinderWebConfigSchema,
  type WayfinderWebRuntime,
} from "./web.ts";
import { createRuntimeTrustedContextProvider } from "./model-policy-wire.ts";
import {
  createQuicklensGateway,
  type QuicklensGateway,
  type WorkspaceSessionIdentity,
} from "../quicklens-service/index.ts";
import {
  ClientIdSchema,
  AgentSessionIdSchema,
  AttemptIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";

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
      })
      .strict(),
    gateway: GatewaySchema,
    agentGateway: WayfinderAgentGatewayConfigSchema.optional(),
    managedTerminals: ManagedRuntimeConfigSchema.optional(),
    planning: WorkspacePlanningRuntimeConfigSchema.optional(),
    profiles: z.array(CodexCoordinatorProfileSchema).min(1).max(32),
    projects: z.array(TrustedProjectRegistrationSchema).min(1).max(256),
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

export type WayfinderResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: { readonly code: string; readonly message: string } };

export interface WayfinderRuntimeOptions {
  readonly store?: WorkspaceStore;
  readonly hosts?: readonly AgentHost[];
  readonly processFactory?: CodexProcessFactory;
  readonly terminals?: WorkspaceManagedTerminalPort;
}

export interface WayfinderReceipt {
  readonly host: string;
  readonly port: number;
  readonly basePath: string;
  readonly databasePath: string;
  readonly projectIds: readonly string[];
  readonly agentGateway: { readonly host: string; readonly port: number } | null;
  readonly web?: { readonly host: string; readonly port: number } | null;
}

export interface WayfinderRuntime {
  readonly service: WorkspaceService;
  readonly store: WorkspaceStore;
  readonly receipt: WayfinderReceipt | null;
  start(): Promise<WayfinderResult<WayfinderReceipt>>;
  issuePairingTicket(): WayfinderResult<{ readonly ticket: string; readonly expiresAt: string }>;
  close(): Promise<void>;
}

export async function loadWayfinderConfig(
  path: string,
): Promise<WayfinderResult<WayfinderRuntimeConfig>> {
  try {
    const raw: unknown = JSON.parse(await readFile(resolve(path), "utf8"));
    const parsed = WayfinderRuntimeConfigSchema.safeParse(raw);
    return parsed.success ? { ok: true, value: parsed.data } : invalid("config schema is invalid");
  } catch {
    return invalid("config file could not be read");
  }
}

export function createWayfinderRuntime(
  rawConfig: unknown,
  options: WayfinderRuntimeOptions = {},
): WayfinderResult<WayfinderRuntime> {
  const parsed = WayfinderRuntimeConfigSchema.safeParse(rawConfig);
  if (!parsed.success) return invalid("config schema is invalid");
  const config = parsed.data;
  let managedController: ManagedRuntimeController | null = null;
  if (config.managedTerminals !== undefined && options.terminals === undefined) {
    const managed = createManagedRuntimeController(config.managedTerminals);
    if (!managed.ok) return invalid(managed.error.message);
    managedController = managed.value;
  }
  let planningController: WorkspacePlanningController | null = null;
  const databasePath = resolve(config.state.databasePath);
  try {
    mkdirSync(dirname(databasePath), { recursive: true });
  } catch {
    return invalid("workspace state directory could not be created");
  }
  const storeResult =
    options.store === undefined
      ? openWorkspaceStore({ databasePath })
      : { ok: true as const, value: options.store };
  if (!storeResult.ok) return invalid("workspace store could not be opened");
  const store = storeResult.value;
  if (config.planning !== undefined) {
    const planning = createWorkspacePlanningController(config.planning, store);
    if (!planning.ok) {
      if (options.store === undefined) store.close();
      return invalid(planning.error.message);
    }
    planningController = planning.value;
  }
  const modelPolicyPath = resolve(
    config.state.modelPolicyDatabasePath ?? `${databasePath}.model-policy`,
  );
  const openedPolicyStore = openModelPolicyStore({ databasePath: modelPolicyPath });
  if (!openedPolicyStore.ok) {
    if (options.store === undefined) store.close();
    return invalid("model policy store could not be opened");
  }
  const modelPolicyStore: ModelPolicyStore = openedPolicyStore.value;
  for (const project of config.projects) {
    const registered = store.registerProject(project);
    if (!registered.ok) {
      modelPolicyStore.close();
      if (options.store === undefined) store.close();
      return invalid("trusted project registration failed");
    }
  }
  const policyAccess = ModelPolicyStoreAccessSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.wayfinder.runtime"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.wayfinder.runtime"),
    authorizedProjectIds: config.projects.map((project) => project.projectId),
  });
  const initializedPolicies =
    config.routing === undefined
      ? initializeLegacyPolicies(modelPolicyStore, policyAccess, config.modelPolicies)
      : initializeConfiguredCoordinatorPolicies(modelPolicyStore, policyAccess, config.routing);
  if (!initializedPolicies.ok) {
    modelPolicyStore.close();
    if (options.store === undefined) store.close();
    return invalid("configured model policy initialization failed");
  }
  const profiles = config.profiles.map((profile) => CodexCoordinatorProfileSchema.parse(profile));
  const routingProvider =
    config.routing === undefined
      ? undefined
      : createConfiguredCoordinatorRoutingProvider(config.routing);
  const coordinatorRouting =
    config.routing === undefined || routingProvider === undefined
      ? undefined
      : createRuntimeRoutingBridge(config.routing, modelPolicyStore, routingProvider);
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
      return invalid(openedAgent.message);
    }
    agentFoundation = openedAgent.value;
  }
  const ownedHosts: readonly OwnedAgentHost[] =
    options.hosts === undefined
      ? profiles.map((profile) => createCodexHost(profile, options.processFactory))
      : [];
  const hosts: readonly AgentHost[] = options.hosts ?? ownedHosts;
  const registrations = hosts.flatMap((host) =>
    host.profileIds.map((profileRef) => ({ profileRef, host })),
  );
  const modelPolicy = createModelPolicyService({
    store: modelPolicyStore,
    trustedContext: createRuntimeTrustedContextProvider(routingProvider),
  });
  const service = createWorkspaceService({
    store,
    adapters: createCoordinatorAdapterRegistry(registrations),
    coordinatorRouting,
    modelPolicy,
    terminals: options.terminals ?? managedController?.service,
    interactions: agentFoundation?.interactions,
    ownedCoordinatorAgents: agentFoundation?.ownedCoordinators,
    planning: planningController?.feature,
  });
  let gateway: QuicklensGateway | null = null;
  let webRuntime: WayfinderWebRuntime | undefined;
  let webReceipt: { readonly host: string; readonly port: number } | null = null;
  let receipt: WayfinderReceipt | null = null;
  const projectIds = config.projects.map((project) => project.projectId);
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
        return invalid("managed terminal runtime could not start");
      const agentStarted =
        agentFoundation === null
          ? { ok: true as const, value: null }
          : await agentFoundation.start();
      if (!agentStarted.ok) return invalid("agent gateway could not bind");
      const planningStarted = await planningController?.start();
      if (planningStarted !== undefined && !planningStarted.ok)
        return invalid("shared planning runtime could not start");
      const opened = createQuicklensGateway({
        source: unavailableDataSource(
          "Zap Wayfinder ZAP plan source is not configured for this local profile.",
        ),
        namespace: config.gateway.namespace,
        pairingToken: config.gateway.pairingToken,
        allowedHosts: config.gateway.allowedHosts,
        allowedOrigins: config.gateway.allowedOrigins,
        multiSession: true,
        workspaceSource: (identity) => bindWorkspace(service, identity, projectIds),
      });
      if (!opened.ok) {
        await agentFoundation?.close();
        return invalid("workspace gateway configuration is invalid");
      }
      gateway = opened.value;
      const started = await gateway.start({ host: config.gateway.host, port: config.gateway.port });
      if (!started.ok) {
        gateway = null;
        await agentFoundation?.close();
        return invalid("workspace gateway could not bind");
      }
      if (config.web !== undefined) {
        const openedWeb = await openWayfinderWebRuntime(config.web, service);
        if (!openedWeb.ok) {
          await gateway.close();
          gateway = null;
          await agentFoundation?.close();
          return invalid(openedWeb.error.message);
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
            return invalid(webStarted.error.message);
          }
          webReceipt = webStarted.value;
        }
      }
      const nextReceipt: WayfinderReceipt = {
        ...started.value,
        databasePath,
        projectIds,
        agentGateway: agentStarted.value,
        ...(webReceipt === null ? {} : { web: webReceipt }),
      };
      receipt = nextReceipt;
      return { ok: true, value: nextReceipt };
    },
    issuePairingTicket() {
      return gateway?.issuePairingTicket?.() ?? invalid("Wayfinder gateway is not started");
    },
    async close(): Promise<void> {
      if (webRuntime !== undefined) await webRuntime.close();
      webRuntime = undefined;
      webReceipt = null;
      if (gateway !== null) await gateway.close();
      await agentFoundation?.close();
      managedController?.close();
      planningController?.close();
      gateway = null;
      receipt = null;
      service.close();
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

interface OwnedAgentHost extends AgentHost {
  closeOwned(): void;
}

function createCodexHost(
  profile: CodexCoordinatorProfile,
  processFactory = createNodeCodexProcessFactory(),
): OwnedAgentHost {
  const hostId: ExecutionHostId = ExecutionHostIdSchema.parse(
    `host.wayfinder.${profile.profileId}`,
  );
  let adapter: CoordinatorAdapter | undefined;
  return {
    hostId,
    profileIds: [profile.profileId],
    openCoordinator(profileId) {
      if (profileId !== profile.profileId) {
        return Promise.resolve({
          ok: false,
          error: { code: "not_found", message: "profile is not registered", retry: "never" },
        });
      }
      if (adapter !== undefined) return Promise.resolve({ ok: true as const, value: adapter });
      const created = createCodexCoordinatorAdapter({ profiles: [profile], processFactory });
      if (created.ok) adapter = created.value;
      return Promise.resolve(created);
    },
    closeOwned() {
      adapter?.close();
      adapter = undefined;
    },
  };
}

function disposeOwnedHosts(hosts: readonly OwnedAgentHost[]): void {
  for (const host of hosts) {
    host.closeOwned();
  }
}

function createRuntimeRoutingBridge(
  config: CoordinatorRoutingConfig,
  store: ModelPolicyStore,
  provider: CoordinatorRoutingProvider,
): CoordinatorRoutingBridge {
  const options = { store, provider };
  const profileFor = (projectId: ProjectId, contextId: WorkContextId, profileId: string) =>
    config.profiles.find(
      (binding) =>
        binding.scope.projectId === projectId &&
        binding.scope.contextId === contextId &&
        binding.profile.profileId === profileId,
    )?.profile;
  return {
    async resolve(access, input) {
      const profile = profileFor(input.projectId, input.contextId, input.explicitProfileId);
      return resolveCoordinatorLaunch(
        access,
        {
          projectId: input.projectId,
          contextId: input.contextId,
          sessionId: AgentSessionIdSchema.parse(input.sessionId),
          runId: RunIdSchema.parse(input.runId),
          attemptId: AttemptIdSchema.parse(input.attemptId),
          clientRequestId: ClientRequestIdSchema.parse(input.clientRequestId),
          sourceEventId: input.sourceEventId,
          policyEnabled: config.policies.length > 0,
          purpose: "development_implementation",
          taskClass: "integration",
          role: "coordinator",
          executionMode: "native",
          invocationScope: "coordinator",
          productId: profile?.productId ?? "codex",
          productVersion: profile?.productVersion ?? "0.152.1",
          selectionRef: `selection.${input.sessionId}`,
          override: null,
          explicitProfileId: input.explicitProfileId,
        },
        options,
      );
    },
    async resume(access, input) {
      const profile = config.profiles.find(
        (binding) =>
          binding.scope.projectId === input.projectId &&
          binding.scope.contextId === input.contextId,
      );
      return this.resolve(access, {
        ...input,
        clientRequestId: `request.resume.${input.sessionId}`,
        sourceEventId: `resume.${input.sessionId}`,
        explicitProfileId: profile?.profile.profileId ?? "profile.unknown",
      });
    },
  };
}

function initializeLegacyPolicies(
  store: ModelPolicyStore,
  access: z.infer<typeof ModelPolicyStoreAccessSchema>,
  policies: readonly {
    readonly projectId: string;
    readonly contextId: string;
    readonly policyId: string;
  }[],
) {
  const results = [];
  for (const policy of policies) {
    const initialized = store.initializeDefaultPolicy(access, {
      projectId: ProjectIdSchema.parse(policy.projectId),
      contextId: WorkContextIdSchema.parse(policy.contextId),
      clientRequestId: ClientRequestIdSchema.parse(`request.wayfinder.policy.${policy.policyId}`),
      sourceEventId: `wayfinder.policy.${policy.policyId}`,
      policyId: policy.policyId,
    });
    if (!initialized.ok && initialized.error.code !== "conflict") return initialized;
    if (initialized.ok) results.push(initialized.value);
  }
  return { ok: true as const, value: results };
}

function bindWorkspace(
  service: WorkspaceService,
  identity: WorkspaceSessionIdentity,
  projectIds: readonly ProjectId[],
): WorkspaceClientPort {
  const clientId: ClientId = ClientIdSchema.parse(identity.clientId);
  const principalId = PrincipalIdSchema.parse(`principal.wayfinder.${identity.clientId}`);
  const access = WorkspaceAccessContextSchema.parse({
    principalId,
    actorId: null,
    clientId,
    authorizedProjectIds: projectIds,
  });
  return service.bind({ access, allowedActions: WORKSPACE_SERVICE_ACTIONS });
}

function invalid(message: string): WayfinderResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_config",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: ${message}`,
    },
  };
}
