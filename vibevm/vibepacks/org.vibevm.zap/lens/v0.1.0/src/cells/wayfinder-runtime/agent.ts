/** Shared Wayfinder broker/agent composition. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { randomUUID } from "node:crypto";
import { createHash } from "node:crypto";
import { isAbsolute } from "node:path";
import { z } from "zod";
import { openBroker, type LensBroker } from "../broker/index.ts";
import { createLensHttpGateway, type GatewayAddress, type LensHttpGateway } from "../http/index.ts";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  ConversationIdSchema,
  CredentialSchema,
  WorkspaceIdSchema,
  type ActorId,
} from "../protocol/index.ts";
import {
  openSqliteAdapterSessionVault,
  type SqliteAdapterSessionVault,
} from "../session-vault/index.ts";
import { createLocalPrincipalTransport } from "../transport/index.ts";
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

export const WayfinderAgentGatewayConfigSchema = z
  .object({
    databasePath: z.string().min(1).refine(isAbsolute, "broker database path must be absolute"),
    host: z.enum(["127.0.0.1", "localhost", "::1"]),
    port: z.number().int().min(0).max(65_535),
    allowedHosts: z.array(z.string().min(1)).min(1).max(16),
    allowedOrigins: z.array(z.string().min(1)).max(16),
    statusToken: CredentialSchema,
    scopes: z
      .array(
        z
          .object({
            workspaceId: WorkspaceIdSchema,
            conversationId: ConversationIdSchema,
            humanPrincipalToken: CredentialSchema,
            agentPrincipalToken: CredentialSchema,
          })
          .strict(),
      )
      .min(1)
      .max(256),
  })
  .strict()
  .superRefine((config, context) => {
    const keys = config.scopes.map((scope) => scopeKey(scope.workspaceId, scope.conversationId));
    if (new Set(keys).size !== keys.length) {
      context.addIssue({
        code: "custom",
        path: ["scopes"],
        message: "agent gateway scopes must be unique",
      });
    }
  });
export type WayfinderAgentGatewayConfig = z.infer<typeof WayfinderAgentGatewayConfigSchema>;

export interface WayfinderAgentFoundation {
  readonly interactions: ReturnType<typeof createWorkspaceInteractionFeature>;
  readonly ownedCoordinators: OwnedCoordinatorAgentPort;
  start(): Promise<WayfinderAgentFoundationResult<GatewayAddress>>;
  close(): Promise<void>;
}

export type WayfinderAgentFoundationResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };

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
  const answerPorts = new Map(
    config.data.scopes.map((scope) => [
      scopeKey(scope.workspaceId, scope.conversationId),
      createBrokerAgentAnswerDelivery(
        createLocalPrincipalTransport(broker.value, scope.humanPrincipalToken),
      ),
    ]),
  );
  const interactions = createWorkspaceInteractionFeature({
    store,
    agentAnswers: {
      deliver: (input) => {
        const port = answerPorts.get(
          scopeKey(input.binding.workspaceId, input.binding.conversationId),
        );
        return port === undefined
          ? Promise.resolve({
              ok: false,
              error: {
                code: "forbidden",
                message:
                  "violates REQ spec://org.vibevm.zap/lens/PROP-005#question-routing: no exact human responder is configured for this actor scope",
              },
            })
          : port.deliver(input);
      },
    },
  });
  const owned = new Map<ActorId, OwnedCoordinatorBinding>();
  const ownedCoordinators = ownedCoordinatorPort(
    broker.value,
    vault.value,
    config.data.scopes,
    owned,
  );
  const gateway: LensHttpGateway = createLensHttpGateway({
    broker: broker.value,
    agentQuestions: createWayfinderAgentPublisher({ store }),
    allowedHosts: config.data.allowedHosts,
    allowedOrigins: config.data.allowedOrigins,
    statusToken: config.data.statusToken,
    adapterSessionIdFactory: () => `adapter.${randomUUID().replaceAll("-", "")}`,
    adapterSessionVault: vault.value,
    ...(planning === undefined ? {} : { planning }),
  });
  let address: GatewayAddress | null = null;
  let closed = false;
  return {
    ok: true,
    value: {
      interactions,
      ownedCoordinators,
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
        vault.value.close();
        broker.value.close();
      },
    },
  };
}

interface OwnedCoordinatorBinding extends OwnedCoordinatorAgentBinding {
  readonly coordinatorSessionId: Parameters<
    OwnedCoordinatorAgentPort["bind"]
  >[0]["coordinatorSessionId"];
}

function ownedCoordinatorPort(
  broker: LensBroker,
  vault: SqliteAdapterSessionVault,
  scopes: z.infer<typeof WayfinderAgentGatewayConfigSchema>["scopes"],
  owned: Map<ActorId, OwnedCoordinatorBinding>,
): OwnedCoordinatorAgentPort {
  return {
    async bind(input) {
      await Promise.resolve();
      const configured = scopes.find(
        (scope) =>
          scope.workspaceId === input.workspaceId && scope.conversationId === input.conversationId,
      );
      if (configured === undefined)
        return workspaceBrokerFailure("forbidden", "owned coordinator scope is not configured");
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
              coordinatorActorId: direct.actorId,
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

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function scopeKey(workspaceId: string, conversationId: string): string {
  return `${workspaceId}\u0000${conversationId}`;
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
