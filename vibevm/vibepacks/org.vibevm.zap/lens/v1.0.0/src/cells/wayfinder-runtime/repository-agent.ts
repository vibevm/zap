/** Authenticated owned-coordinator repository tools. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { createHash } from "node:crypto";
import {
  RepositoryIntegrationDiffAgentInputSchema,
  RepositoryIntegrationGetAgentInputSchema,
  RepositoryIntegrationListAgentInputSchema,
  RepositoryIntegrationPrepareAgentInputSchema,
  RepositoryIntegrationTestAgentInputSchema,
  RepositoryPlanListAgentInputSchema,
  RepositoryWorktreeGetAgentInputSchema,
  RepositoryWorktreeListAgentInputSchema,
  type RepositoryWorkspaceAgentPort,
} from "../managed-work/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import { failure, type AdapterSessionId, type AgentTransportPort } from "../transport/index.ts";
import {
  ClientIdSchema,
  type ProjectId,
  type WorkContextId,
  WorkspaceAccessContextSchema,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type {
  OwnedCoordinatorAgentPort,
  RepositoryWorkspaceFeature,
} from "../workspace-service/index.ts";

export function createRepositoryWorkspaceAgentPort(options: {
  readonly agent: AgentTransportPort;
  readonly store: WorkspaceStore;
  readonly coordinatorAgents: OwnedCoordinatorAgentPort;
  readonly feature: () => RepositoryWorkspaceFeature | undefined;
}): RepositoryWorkspaceAgentPort {
  const bound = (session: AdapterSessionId) => boundCoordinator(options, session);
  return {
    async planList(session, raw) {
      const input = RepositoryPlanListAgentInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "repository plan list input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const listed = await context.value.feature.read(context.value.access, {
        operation: "plan.workspace.list.v1",
        projectId: context.value.projectId,
      });
      return listed.ok && listed.value.operation === "plan.workspace.list.v1"
        ? workspaceJson({
            ok: true,
            value: {
              ...listed.value,
              plans: listed.value.plans.filter(
                (plan) => plan.contextId === context.value.contextId,
              ),
            },
          })
        : workspaceJson(listed);
    },
    async worktreeList(session, raw) {
      const input = RepositoryWorktreeListAgentInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "repository worktree list input is invalid");
      const context = await bound(session);
      return context.ok
        ? workspaceJson(
            await context.value.feature.read(context.value.access, {
              operation: "worktree.list.v1",
              projectId: context.value.projectId,
              contextId: context.value.contextId,
              planId: input.data.planId,
            }),
          )
        : context;
    },
    async worktreeGet(session, raw) {
      const input = RepositoryWorktreeGetAgentInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "repository worktree input is invalid");
      const context = await bound(session);
      return context.ok
        ? workspaceJson(
            await context.value.feature.read(context.value.access, {
              operation: "worktree.get.v1",
              projectId: context.value.projectId,
              contextId: context.value.contextId,
              worktreeId: input.data.worktreeId,
            }),
          )
        : context;
    },
    async integrationList(session, raw) {
      const input = RepositoryIntegrationListAgentInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "repository integration list input is invalid");
      const context = await bound(session);
      return context.ok
        ? workspaceJson(
            await context.value.feature.read(context.value.access, {
              operation: "integration.list.v1",
              projectId: context.value.projectId,
              contextId: context.value.contextId,
              planId: input.data.planId,
            }),
          )
        : context;
    },
    async integrationGet(session, raw) {
      const input = RepositoryIntegrationGetAgentInputSchema.safeParse(raw);
      return input.success
        ? integrationRead(options, session, input.data.integrationId, null)
        : failure("invalid_input", "repository integration read is invalid");
    },
    async integrationDiff(session, raw) {
      const input = RepositoryIntegrationDiffAgentInputSchema.safeParse(raw);
      return input.success
        ? integrationRead(options, session, input.data.integrationId, input.data.maximumBytes)
        : failure("invalid_input", "repository integration diff is invalid");
    },
    async integrationPrepare(session, raw) {
      const input = RepositoryIntegrationPrepareAgentInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "repository integration prepare input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const planId = contextPlan(options.store, context.value.access, context.value);
      if (!planId.ok) return planId;
      return workspaceJson(
        await context.value.feature.command(context.value.access, {
          operation: "integration.prepare.v1",
          clientRequestId: input.data.clientRequestId,
          projectId: context.value.projectId,
          contextId: context.value.contextId,
          planId: planId.value,
          sourceWorktreeId: input.data.sourceWorktreeId,
          targetWorktreeId: input.data.targetWorktreeId,
          expectedSourceHead: input.data.expectedSourceHead,
          expectedTargetHead: input.data.expectedTargetHead,
        }),
      );
    },
    async integrationTest(session, raw) {
      const input = RepositoryIntegrationTestAgentInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "repository integration test input is invalid");
      const context = await bound(session);
      return context.ok
        ? workspaceJson(
            await context.value.feature.command(context.value.access, {
              operation: "integration.test.v1",
              clientRequestId: input.data.clientRequestId,
              projectId: context.value.projectId,
              contextId: context.value.contextId,
              integrationId: input.data.integrationId,
              expectedRevision: input.data.expectedRevision,
              profileId: input.data.profileId,
            }),
          )
        : context;
    },
  };
}

interface Bound {
  readonly feature: RepositoryWorkspaceFeature;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly access: WorkspaceAccessContext;
}

async function boundCoordinator(
  options: Parameters<typeof createRepositoryWorkspaceAgentPort>[0],
  session: AdapterSessionId,
) {
  const actor = await options.agent.context(session);
  if (!actor.ok) return actor;
  const scope = options.store.resolveAgentScope(
    actor.value.actor.workspaceId,
    actor.value.actor.conversationId,
  );
  if (!scope.ok) return failure("forbidden", "repository actor has no exact project scope");
  const routed = options.coordinatorAgents.route(actor.value.actor.actorId);
  if (
    !routed.ok ||
    routed.value === null ||
    !isExactOwnedCoordinatorRoute(actor.value.actor.actorId, routed.value)
  )
    return failure("forbidden", "repository commands require the exact owned coordinator");
  const feature = options.feature();
  if (feature === undefined)
    return failure("unsupported_operation", "repository workspace service is not configured");
  return {
    ok: true as const,
    value: {
      feature,
      projectId: scope.value.projectId,
      contextId: scope.value.contextId,
      access: WorkspaceAccessContextSchema.parse({
        principalId: actor.value.actor.principalId,
        actorId: actor.value.actor.actorId,
        clientId: ClientIdSchema.parse(
          `client.repository-agent.${digest(actor.value.actor.actorId)}`,
        ),
        authorizedProjectIds: [scope.value.projectId],
      }),
    },
  };
}

export function isExactOwnedCoordinatorRoute(
  actorId: string,
  route: { readonly coordinatorActorId: string; readonly forwarding: boolean },
): boolean {
  return !route.forwarding && route.coordinatorActorId === actorId;
}

async function integrationRead(
  options: Parameters<typeof createRepositoryWorkspaceAgentPort>[0],
  session: AdapterSessionId,
  integrationId: string,
  maximumBytes: number | null,
) {
  const context = await boundCoordinator(options, session);
  if (!context.ok) return context;
  return workspaceJson(
    await context.value.feature.read(
      context.value.access,
      maximumBytes === null
        ? {
            operation: "integration.get.v1",
            projectId: context.value.projectId,
            contextId: context.value.contextId,
            integrationId,
          }
        : {
            operation: "integration.diff.v1",
            projectId: context.value.projectId,
            contextId: context.value.contextId,
            integrationId,
            maximumBytes,
          },
    ),
  );
}

function contextPlan(store: WorkspaceStore, access: WorkspaceAccessContext, context: Bound) {
  const result = store.read(access, {
    operation: "context.get.v1",
    projectId: context.projectId,
    contextId: context.contextId,
  });
  if (!result.ok || result.value.operation !== "context.get.v1")
    return failure("not_found", "repository plan context is unavailable");
  return result.value.context.planId === null
    ? failure("conflict", "repository plan context has no adopted plan")
    : { ok: true as const, value: result.value.context.planId };
}

function workspaceJson(result: {
  readonly ok: boolean;
  readonly value?: unknown;
  readonly error?: { readonly code: string; readonly message: string };
}) {
  if (!result.ok)
    return failure(
      result.error?.code === "forbidden" ? "forbidden" : "storage_failure",
      result.error?.message ?? "repository workspace operation failed",
    );
  const parsed = JsonValueSchema.safeParse(result.value);
  return parsed.success
    ? { ok: true as const, value: parsed.data }
    : failure("storage_failure", "repository workspace result is not JSON-safe");
}
function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 32);
}
