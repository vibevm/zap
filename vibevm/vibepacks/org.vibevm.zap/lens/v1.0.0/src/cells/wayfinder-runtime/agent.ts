/** Shared Wayfinder broker/agent composition. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { openBroker, type LensBroker } from "../broker/index.ts";
import { createLensHttpGateway, type GatewayAddress, type LensHttpGateway } from "../http/index.ts";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  EnrollPrincipalInputSchema,
  type ActorId,
} from "../protocol/index.ts";
import {
  openSqliteAdapterSessionVault,
  type SqliteAdapterSessionVault,
} from "../session-vault/index.ts";
import {
  AdapterSessions,
  createLocalPrincipalTransport,
  createRetainedAgentTransport,
  failure,
} from "../transport/index.ts";
import {
  createBrokerAgentAnswerDelivery,
  createWayfinderAgentPublisher,
} from "../wayfinder-agent/index.ts";
import { createWorkspaceInteractionFeature } from "../workspace-interaction/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type {
  OwnedCoordinatorAgentPort,
  OwnedCoordinatorAgentBinding,
} from "../workspace-service/index.ts";
import { OwnedCoordinatorAgentBindingSchema } from "../workspace-service/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { ManagedActorBindingPort, ManagedAgentBackend } from "../managed-work/index.ts";
import { createManagedWorkAgentPort } from "./managed-agent.ts";
import type { ManagedRepositoryPlanPort } from "./managed-agent.ts";
import { agentDigest as digest } from "./agent-id.ts";
import { createRepositoryWorkspaceAgentPort } from "./repository-agent.ts";
import type { RepositoryWorkspaceFeature } from "../workspace-service/index.ts";
import { createNativeWorkAgentPort } from "./native-agent.ts";
import {
  createDeclaredNativeWorkTargetBridge,
  type DeclaredNativeWorkTargetBridge,
} from "./native-targets.ts";
import { openAgentScopeManager, type AgentScopeManager } from "./agent-scope.ts";
import { WayfinderAgentGatewayConfigSchema } from "./agent-config.ts";
import { prepareCoordinatorMcpLaunch } from "./coordinator-mcp.ts";
import {
  defaultMcpLaunch,
  managedBindingFailure,
  writeManagedMcpConfig,
} from "./agent-mcp-config.ts";
export { WayfinderAgentGatewayConfigSchema } from "./agent-config.ts";
export type { WayfinderAgentGatewayConfig } from "./agent-config.ts";
export type { CoordinatorMcpLaunch, CoordinatorMcpLaunchInput } from "./coordinator-mcp.ts";
import type { WayfinderAgentFoundation, WayfinderAgentFoundationResult } from "./agent-contract.ts";
export type { WayfinderAgentFoundation, WayfinderAgentFoundationResult } from "./agent-contract.ts";

export function openWayfinderAgentFoundation(
  raw: unknown,
  store: WorkspaceStore,
  planning?: WorkspacePlanningFeature,
): WayfinderAgentFoundationResult<WayfinderAgentFoundation> {
  const config = WayfinderAgentGatewayConfigSchema.safeParse(raw);
  if (!config.success) return { ok: false, message: "agent gateway configuration is invalid" };
  const broker = openBroker({ databasePath: config.data.databasePath });
  if (!broker.ok) return { ok: false, message: "agent broker could not be opened" };
  const vault = openSqliteAdapterSessionVault(config.data.databasePath);
  if (!vault.ok) {
    broker.value.close();
    return { ok: false, message: "agent session vault could not be opened" };
  }
  const scopeManager = openAgentScopeManager({
    databasePath: config.data.databasePath,
    broker: broker.value,
    store,
    configured: config.data.scopes,
  });
  if (!scopeManager.ok) {
    vault.value.close();
    broker.value.close();
    return { ok: false, message: scopeManager.message };
  }
  const interactions = createWorkspaceInteractionFeature({
    store,
    agentAnswers: {
      deliver: (input) => {
        const scope = scopeManager.value.resolve(
          input.binding.workspaceId,
          input.binding.conversationId,
        );
        return scope === null
          ? Promise.resolve({
              ok: false,
              error: {
                code: "forbidden",
                message:
                  "violates REQ spec://org.vibevm.zap/lens/PROP-005#question-routing: no exact human responder is configured for this actor scope",
              },
            })
          : createBrokerAgentAnswerDelivery(
              createLocalPrincipalTransport(broker.value, scope.humanPrincipalToken),
            ).deliver(input);
      },
    },
  });
  let address: GatewayAddress | null = null;
  const owned = new Map<ActorId, OwnedCoordinatorBinding>();
  const ownedCoordinators = ownedCoordinatorPort(
    broker.value,
    vault.value,
    store,
    scopeManager.value,
    owned,
    () => address,
  );
  let managedBackend: ManagedAgentBackend | undefined;
  let nativeBridge: DeclaredNativeWorkTargetBridge | undefined;
  let repositoryPlans: ManagedRepositoryPlanPort | undefined;
  let repositoryFeature: RepositoryWorkspaceFeature | undefined;
  const retainedAgent = createRetainedAgentTransport({
    broker: broker.value,
    principalToken: config.data.statusToken,
    sessions: new AdapterSessions(
      () => `adapter.foundation.${randomUUID().replaceAll("-", "")}`,
      vault.value,
    ),
  });
  const managedAgentPort = createManagedWorkAgentPort({
    agent: retainedAgent,
    backend: () => managedBackend,
    store,
    coordinatorAgents: ownedCoordinators,
    ensurePlan: (input) =>
      repositoryPlans === undefined
        ? Promise.resolve(
            failure(
              "unsupported_operation",
              "repository plan adoption is not configured for this agent runtime",
            ),
          )
        : repositoryPlans(input),
  });
  const nativeAgentPort = createNativeWorkAgentPort({
    agent: retainedAgent,
    bridge: () => nativeBridge,
    store,
  });
  const repositoryAgentPort = createRepositoryWorkspaceAgentPort({
    agent: retainedAgent,
    store,
    coordinatorAgents: ownedCoordinators,
    feature: () => repositoryFeature,
  });
  const gateway: LensHttpGateway = createLensHttpGateway({
    broker: broker.value,
    agentQuestions: createWayfinderAgentPublisher({ store }),
    allowedHosts: config.data.allowedHosts,
    allowedOrigins: config.data.allowedOrigins,
    statusToken: config.data.statusToken,
    adapterSessionIdFactory: () => `adapter.${randomUUID().replaceAll("-", "")}`,
    adapterSessionVault: vault.value,
    managedWork: () => (managedBackend === undefined ? undefined : managedAgentPort),
    nativeWork: () => (nativeBridge === undefined ? undefined : nativeAgentPort),
    repositoryWork: () => (repositoryFeature === undefined ? undefined : repositoryAgentPort),
    ...(planning === undefined ? {} : { planning }),
  });
  let closed = false;
  return {
    ok: true,
    value: {
      interactions,
      ownedCoordinators,
      managedActors: managedActorPort(
        broker.value,
        vault.value,
        store,
        scopeManager.value,
        () => address,
      ),
      ensureScope: (input) => scopeManager.value.ensure(input, address),
      prepareOwnedCoordinatorLaunch: (input) =>
        prepareCoordinatorMcpLaunch({
          raw: input,
          store,
          scopes: scopeManager.value,
          vault: vault.value,
          bindings: owned,
          address,
        }),
      bindManagedWork(backend) {
        if (managedBackend !== undefined && managedBackend !== backend)
          return { ok: false, message: "managed work runtime is already bound" };
        managedBackend = backend;
        return { ok: true, value: null };
      },
      bindNativeWork(attachments) {
        const next = createDeclaredNativeWorkTargetBridge(attachments);
        if (nativeBridge !== undefined)
          return { ok: false, message: "native work attachments are already bound" };
        nativeBridge = next;
        return { ok: true, value: null };
      },
      bindRepositoryPlans(port) {
        if (repositoryPlans !== undefined && repositoryPlans !== port)
          return { ok: false, message: "repository plan adoption is already bound" };
        repositoryPlans = port;
        return { ok: true, value: null };
      },
      bindRepositoryWork(feature) {
        if (repositoryFeature !== undefined && repositoryFeature !== feature)
          return { ok: false, message: "repository workspace feature is already bound" };
        repositoryFeature = feature;
        return { ok: true, value: null };
      },
      async start() {
        if (closed) return { ok: false, message: "agent gateway is closed" };
        if (address !== null) return { ok: true, value: address };
        const started = await gateway.start({ host: config.data.host, port: config.data.port });
        if (!started.ok) return { ok: false, message: started.error.message };
        address = started.value;
        return { ok: true, value: started.value };
      },
      async close() {
        if (closed) return;
        closed = true;
        await gateway.close();
        address = null;
        scopeManager.value.close();
        vault.value.close();
        broker.value.close();
      },
    },
  };
}

function managedActorPort(
  broker: LensBroker,
  vault: SqliteAdapterSessionVault,
  store: WorkspaceStore,
  scopes: AgentScopeManager,
  address: () => GatewayAddress | null,
): ManagedActorBindingPort {
  return {
    async prepare(input) {
      await Promise.resolve();
      const launch = store.resolveProjectLaunch(input.request.projectId, input.request.contextId);
      if (!launch.ok || launch.value.agentScope === null)
        return managedBindingFailure("forbidden", "managed work has no registered broker scope");
      const agentScope = launch.value.agentScope;
      const ensured = await scopes.ensure(
        {
          projectId: input.request.projectId,
          contextId: input.request.contextId,
          workspaceId: agentScope.workspaceId,
          conversationId: agentScope.conversationId,
        },
        address(),
      );
      if (!ensured.ok) return managedBindingFailure("unavailable", ensured.message);
      const configured = scopes.resolve(agentScope.workspaceId, agentScope.conversationId);
      if (configured === null)
        return managedBindingFailure("unavailable", "managed work broker scope is unavailable");
      const adapterSessionId = AdapterSessionIdSchema.parse(
        `adapter.managed.${digest(input.runId)}`,
      );
      const clientRequestId = ClientRequestIdSchema.parse(`request.managed.${digest(input.runId)}`);
      const gatewayAddress = address();
      if (gatewayAddress === null)
        return managedBindingFailure("unavailable", "managed agent gateway is not started");
      const host = {
        kind: input.provider,
        sessionId: input.runId,
        subagentId: input.taskId,
        provenance: "attested" as const,
      };
      const capabilities = [
        "message:emit",
        "question:ask",
        "question:cancel",
        "inbox:read",
        "inbox:ack",
        "actor:delegate",
      ] as const;
      const listed = vault.list();
      if (!listed.ok) return managedBindingFailure("unavailable", listed.error.message);
      const parent =
        input.requesterActorId === null
          ? undefined
          : listed.value.find(
              ([, session]) =>
                session.connection.actor.actorId === input.requesterActorId &&
                session.connection.actor.workspaceId === configured.workspaceId &&
                session.connection.actor.conversationId === configured.conversationId,
            );
      if (input.requesterActorId !== null && parent === undefined)
        return managedBindingFailure("forbidden", "managed actor parent binding is unavailable");
      const enrolled =
        parent === undefined
          ? broker.enrollPrincipal(
              EnrollPrincipalInputSchema.parse({
                kind: "agent",
                workspaceIds: [configured.workspaceId],
                conversationIds: [configured.conversationId],
                capabilities,
              }),
            )
          : null;
      if (enrolled !== null && !enrolled.ok)
        return managedBindingFailure("unavailable", enrolled.error.message);
      const managedPrincipalToken =
        parent?.[1].principalToken ?? (enrolled?.ok ? enrolled.value.principalToken : null);
      if (managedPrincipalToken === null)
        return managedBindingFailure("unavailable", "managed principal is unavailable");
      const connected =
        parent === undefined
          ? broker.connect({
              principalToken: managedPrincipalToken,
              clientRequestId,
              workspaceId: configured.workspaceId,
              conversationId: configured.conversationId,
              capabilities: [...capabilities],
              host,
              replyPolicy: { kind: "retain" },
            })
          : broker.delegate(
              {
                principalToken: parent[1].principalToken,
                bindingToken: parent[1].connection.credentials.bindingToken,
              },
              {
                clientRequestId,
                capabilities: [...capabilities],
                host,
                replyPolicy: { kind: "retain" },
              },
            );
      if (!connected.ok) return managedBindingFailure("unavailable", connected.error.message);
      const generated = writeManagedMcpConfig({
        basePath: input.mcpConfigPath,
        runId: input.runId,
        provider: input.provider,
        ...defaultMcpLaunch(input.mcpCommandPath, input.mcpArgs),
        brokerUrl: `http://${gatewayAddress.host}:${String(gatewayAddress.port)}`,
        credential: managedPrincipalToken,
        adapterSessionId,
        workspaceId: configured.workspaceId,
        conversationId: configured.conversationId,
      });
      if (!generated.ok) return generated;
      const stored = vault.put(adapterSessionId, {
        principalToken: managedPrincipalToken,
        connection: connected.value,
        host,
        replyPolicy: { kind: "retain" },
      });
      if (!stored.ok) return managedBindingFailure("unavailable", stored.error.message);
      return {
        ok: true,
        value: {
          actorId: connected.value.actor.actorId,
          adapterSessionId,
          mcpConfigPath: generated.value.mcpConfigPath,
          environment: generated.value.environment,
        },
      };
    },
    async activate(input) {
      await Promise.resolve();
      const listed = vault.list();
      if (!listed.ok) return managedBindingFailure("unavailable", listed.error.message);
      const retained = listed.value.find(
        ([id, session]) =>
          id === input.adapterSessionId && session.connection.actor.actorId === input.actorId,
      );
      if (retained === undefined)
        return managedBindingFailure("unavailable", "managed broker actor binding is unavailable");
      const actor = retained[1].connection.actor;
      const gatewayAddress = address();
      if (gatewayAddress === null)
        return managedBindingFailure("unavailable", "managed agent gateway is not started");
      const generated = writeManagedMcpConfig({
        basePath: input.mcpConfigPath,
        runId: input.runId,
        provider: input.provider,
        ...defaultMcpLaunch(input.mcpCommandPath, input.mcpArgs),
        brokerUrl: `http://${gatewayAddress.host}:${String(gatewayAddress.port)}`,
        credential: retained[1].principalToken,
        adapterSessionId: input.adapterSessionId,
        workspaceId: actor.workspaceId,
        conversationId: actor.conversationId,
      });
      return generated;
    },
  };
}

interface OwnedCoordinatorBinding extends OwnedCoordinatorAgentBinding {
  readonly coordinatorSessionId: Parameters<
    OwnedCoordinatorAgentPort["bind"]
  >[0]["coordinatorSessionId"];
  readonly coordinatorActorId: Parameters<
    OwnedCoordinatorAgentPort["bind"]
  >[0]["coordinatorActorId"];
}

function ownedCoordinatorPort(
  broker: LensBroker,
  vault: SqliteAdapterSessionVault,
  store: WorkspaceStore,
  scopes: AgentScopeManager,
  owned: Map<ActorId, OwnedCoordinatorBinding>,
  address: () => GatewayAddress | null,
): OwnedCoordinatorAgentPort {
  return {
    async bind(input) {
      await Promise.resolve();
      const launch = store.resolveProjectLaunch(input.projectId, input.contextId);
      if (
        !launch.ok ||
        launch.value.agentScope === null ||
        launch.value.agentScope.workspaceId !== input.workspaceId ||
        launch.value.agentScope.conversationId !== input.conversationId
      )
        return workspaceBrokerFailure("forbidden", "owned coordinator scope is not registered");
      const ensured = await scopes.ensure(
        {
          projectId: input.projectId,
          contextId: input.contextId,
          workspaceId: input.workspaceId,
          conversationId: input.conversationId,
        },
        address(),
      );
      if (!ensured.ok) return workspaceBrokerFailure("unavailable", ensured.message);
      const configured = scopes.resolve(input.workspaceId, input.conversationId);
      if (configured === null)
        return workspaceBrokerFailure("unavailable", "owned coordinator scope is unavailable");
      const principalToken = configured.agentPrincipalToken;
      const requestId = ClientRequestIdSchema.parse(
        `request.owned-coordinator.${digest(input.coordinatorSessionId)}`,
      );
      const adapterSessionId = AdapterSessionIdSchema.parse(
        `adapter.owned.${digest(input.coordinatorSessionId)}`,
      );
      const host = {
        kind: "codex" as const,
        sessionId: input.coordinatorSessionId,
        provenance: "attested" as const,
      };
      const replyPolicy = { kind: "retain" as const };
      const retained = vault.list();
      if (!retained.ok) return workspaceBrokerFailure(retained.error.code, retained.error.message);
      const prior = retained.value.find(([id]) => id === adapterSessionId)?.[1];
      if (
        prior !== undefined &&
        (prior.principalToken !== principalToken ||
          prior.connection.actor.workspaceId !== input.workspaceId ||
          prior.connection.actor.conversationId !== input.conversationId)
      ) {
        return workspaceBrokerFailure("conflict", "owned coordinator binding scope changed");
      }
      const verified =
        prior === undefined
          ? null
          : broker.context({
              principalToken,
              bindingToken: prior.connection.credentials.bindingToken,
            });
      const connected =
        prior === undefined
          ? broker.connect({
              principalToken,
              clientRequestId: requestId,
              workspaceId: input.workspaceId,
              conversationId: input.conversationId,
              capabilities: [
                "message:emit",
                "question:ask",
                "inbox:read",
                "inbox:ack",
                "actor:delegate",
                "actor:expire",
                "inbox:forward",
                "plan:propose",
              ],
              host,
              replyPolicy,
            })
          : verified?.ok
            ? { ok: true as const, value: prior.connection }
            : broker.resume({
                principalToken,
                clientRequestId: ClientRequestIdSchema.parse(
                  `request.resume-owned.${digest(input.coordinatorSessionId)}`,
                ),
                actorId: prior.connection.actor.actorId,
                resumeCredential: prior.connection.credentials.resumeCredential,
                host,
              });
      if (!connected.ok)
        return workspaceBrokerFailure(connected.error.code, connected.error.message);
      const stored = vault.put(adapterSessionId, {
        principalToken,
        connection: connected.value,
        host,
        replyPolicy,
      });
      if (!stored.ok) return workspaceBrokerFailure(stored.error.code, stored.error.message);
      const binding = {
        actorId: connected.value.actor.actorId,
        adapterSessionId,
        coordinatorSessionId: input.coordinatorSessionId,
        coordinatorActorId: input.coordinatorActorId,
      } satisfies OwnedCoordinatorBinding;
      owned.set(binding.actorId, binding);
      return {
        ok: true,
        value: OwnedCoordinatorAgentBindingSchema.parse({
          actorId: binding.actorId,
          adapterSessionId: binding.adapterSessionId,
        }),
      };
    },
    route(actorId) {
      const retained = vault.list();
      if (!retained.ok) return workspaceBrokerFailure(retained.error.code, retained.error.message);
      const actors = new Map(
        retained.value.map(([, session]) => [
          session.connection.actor.actorId,
          session.connection.actor,
        ]),
      );
      let current: ActorId | null = ActorIdSchema.parse(actorId);
      const visited = new Set<ActorId>();
      for (let depth = 0; current !== null && depth < 100; depth += 1) {
        if (visited.has(current))
          return workspaceBrokerFailure("conflict", "broker actor ancestry contains a cycle");
        visited.add(current);
        const direct = owned.get(current);
        if (direct !== undefined) {
          return {
            ok: true,
            value: {
              coordinatorSessionId: direct.coordinatorSessionId,
              coordinatorActorId: direct.coordinatorActorId,
              adapterSessionId: direct.adapterSessionId,
              forwarding: current !== actorId,
            },
          };
        }
        current = actors.get(current)?.parentActorId ?? null;
      }
      return { ok: true, value: null };
    },
  };
}

function workspaceBrokerFailure(code: string, message: string) {
  const mapped = new Set([
    "invalid_input",
    "unauthorized",
    "forbidden",
    "not_found",
    "conflict",
    "stale_revision",
    "idempotency_conflict",
    "unsupported_operation",
    "storage_failure",
    "closed",
  ]).has(code)
    ? code
    : "unavailable";
  const safe = z
    .enum([
      "invalid_input",
      "unauthorized",
      "forbidden",
      "not_found",
      "conflict",
      "stale_revision",
      "idempotency_conflict",
      "unsupported_operation",
      "storage_failure",
      "closed",
      "unavailable",
    ])
    .parse(mapped);
  return {
    ok: false as const,
    error: {
      code: safe,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#question-routing: ${message}`,
    },
  };
}
