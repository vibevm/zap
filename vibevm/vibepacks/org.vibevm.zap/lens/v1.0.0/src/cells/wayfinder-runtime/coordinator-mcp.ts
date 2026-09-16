/** Trusted coordinator MCP launch preparation. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { z } from "zod";
import { ActorIdSchema, type ActorId } from "../protocol/index.ts";
import type { SqliteAdapterSessionVault } from "../session-vault/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import {
  AgentSessionIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { GatewayAddress } from "../http/index.ts";
import type { AgentScopeManager } from "./agent-scope.ts";
import { defaultMcpLaunch, writeManagedMcpConfig } from "./agent-mcp-config.ts";

export const CoordinatorMcpLaunchInputSchema = z
  .object({
    provider: z.enum(["claude_code", "opencode", "qwen_code"]),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    coordinatorSessionId: AgentSessionIdSchema,
    actorId: ActorIdSchema,
    adapterSessionId: AdapterSessionIdSchema,
    mcpConfigPath: z.string().min(1).max(32_768),
    mcpCommandPath: z.string().min(1).max(32_768).optional(),
    mcpArgs: z.array(z.string().max(16_384)).max(64).optional(),
  })
  .strict();
export type CoordinatorMcpLaunchInput = z.infer<typeof CoordinatorMcpLaunchInputSchema>;

export interface CoordinatorMcpLaunch {
  readonly mcpConfigPath: string;
  readonly environment: Readonly<Record<string, string>>;
}

export async function prepareCoordinatorMcpLaunch(options: {
  readonly raw: CoordinatorMcpLaunchInput;
  readonly store: WorkspaceStore;
  readonly scopes: AgentScopeManager;
  readonly vault: SqliteAdapterSessionVault;
  readonly bindings: ReadonlyMap<
    ActorId,
    {
      readonly actorId: ActorId;
      readonly adapterSessionId: string;
      readonly coordinatorSessionId: string;
    }
  >;
  readonly address: GatewayAddress | null;
}): Promise<
  | { readonly ok: true; readonly value: CoordinatorMcpLaunch }
  | { readonly ok: false; readonly message: string }
> {
  const input = CoordinatorMcpLaunchInputSchema.safeParse(options.raw);
  if (!input.success) return { ok: false, message: "coordinator MCP launch input is invalid" };
  const launch = options.store.resolveProjectLaunch(input.data.projectId, input.data.contextId);
  if (!launch.ok || launch.value.agentScope === null)
    return { ok: false, message: "coordinator MCP scope is not registered" };
  const binding = options.bindings.get(input.data.actorId);
  if (
    binding === undefined ||
    binding.coordinatorSessionId !== input.data.coordinatorSessionId ||
    binding.adapterSessionId !== input.data.adapterSessionId
  )
    return { ok: false, message: "coordinator MCP actor binding does not match" };
  const retained = options.vault.list();
  if (!retained.ok) return { ok: false, message: retained.error.message };
  const session = retained.value.find(
    ([id, candidate]) =>
      id === input.data.adapterSessionId &&
      candidate.connection.actor.actorId === input.data.actorId,
  )?.[1];
  const scope = launch.value.agentScope;
  if (
    session === undefined ||
    session.connection.actor.workspaceId !== scope.workspaceId ||
    session.connection.actor.conversationId !== scope.conversationId
  )
    return { ok: false, message: "coordinator MCP retained session is unavailable" };
  const ensured = await options.scopes.ensure(
    {
      projectId: input.data.projectId,
      contextId: input.data.contextId,
      workspaceId: scope.workspaceId,
      conversationId: scope.conversationId,
    },
    options.address,
  );
  if (!ensured.ok) return ensured;
  const generated = writeManagedMcpConfig({
    basePath: input.data.mcpConfigPath,
    runId: input.data.coordinatorSessionId,
    provider: input.data.provider,
    ...defaultMcpLaunch(input.data.mcpCommandPath, input.data.mcpArgs),
    brokerUrl: ensured.value.brokerUrl,
    credential: session.principalToken,
    adapterSessionId: input.data.adapterSessionId,
    workspaceId: scope.workspaceId,
    conversationId: scope.conversationId,
  });
  return generated.ok
    ? { ok: true, value: generated.value }
    : { ok: false, message: generated.error.message };
}
