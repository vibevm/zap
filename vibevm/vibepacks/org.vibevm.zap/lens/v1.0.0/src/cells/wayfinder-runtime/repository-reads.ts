/** Scoped repository workspace reads with fresh Git observation. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
  type WorkspaceReadResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { RepositoryWorkspaceReadRequest } from "../workspace-service/index.ts";
import { ensureRepository, ensureRuntimeRepositoryContextPlan } from "./repository-context.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";

interface RepositoryReadInput {
  readonly runtime: RuntimeRepositoryWorkspaces;
  readonly store: WorkspaceStore;
}

export async function readRuntimeRepository(
  input: RepositoryReadInput,
  principalId: string,
  request: RepositoryWorkspaceReadRequest,
): Promise<WorkspaceResult<WorkspaceReadResponse>> {
  if (request.operation === "repository.get.v1") return readRepository(input, principalId, request);
  if (request.operation === "plan.workspace.list.v1") {
    const plans = input.runtime.service.listPlans(request.projectId);
    return plans.ok
      ? { ok: true, value: { operation: request.operation, plans: [...plans.value] } }
      : repositoryFailure(plans);
  }
  if (request.operation === "plan.workspace.get.v1") {
    const plan = input.runtime.service.getPlan(request.planId);
    return plan.ok &&
      plan.value.projectId === request.projectId &&
      plan.value.contextId === request.contextId
      ? { ok: true, value: { operation: request.operation, plan: plan.value } }
      : plan.ok
        ? failure("not_found", "plan is outside requested context")
        : repositoryFailure(plan);
  }
  if (request.operation === "worktree.list.v1") {
    const scoped = planInScope(input, request.planId, request.projectId, request.contextId);
    if (!scoped.ok) return scoped;
    const worktrees = input.runtime.service.listWorktrees(request.planId);
    if (!worktrees.ok) return repositoryFailure(worktrees);
    const observed = await Promise.all(
      worktrees.value.map(async (worktree) => {
        const current = await input.runtime.service.observeWorktree({
          worktreeId: worktree.worktreeId,
          executionHostId: input.runtime.executionHostId,
          projectId: worktree.projectId,
          contextId: worktree.contextId,
        });
        return current.ok ? { ...worktree, headCommit: current.value.currentHead } : worktree;
      }),
    );
    return { ok: true, value: { operation: request.operation, worktrees: observed } };
  }
  if (request.operation === "worktree.get.v1") return readWorktree(input, request);
  if (request.operation === "integration.list.v1") {
    const scoped = planInScope(input, request.planId, request.projectId, request.contextId);
    if (!scoped.ok) return scoped;
    const integrations = input.runtime.service.listIntegrations(request.planId);
    return integrations.ok
      ? { ok: true, value: { operation: request.operation, integrations: [...integrations.value] } }
      : repositoryFailure(integrations);
  }
  if (request.operation === "integration.diff.v1") {
    const diff = await input.runtime.service.readIntegrationDiff({
      integrationId: request.integrationId,
      executionHostId: input.runtime.executionHostId,
      projectId: request.projectId,
      contextId: request.contextId,
      maximumBytes: request.maximumBytes,
    });
    return diff.ok
      ? {
          ok: true,
          value: {
            operation: request.operation,
            ...diff.value,
            changedFiles: [...diff.value.changedFiles],
          },
        }
      : repositoryFailure(diff);
  }
  const integration = input.runtime.service.getIntegration(request.integrationId);
  if (!integration.ok) return repositoryFailure(integration);
  const plan = input.runtime.service.getPlan(integration.value.planId);
  return plan.ok &&
    plan.value.projectId === request.projectId &&
    plan.value.contextId === request.contextId
    ? { ok: true, value: { operation: request.operation, integration: integration.value } }
    : failure("not_found", "integration is outside requested context");
}

async function readRepository(
  input: RepositoryReadInput,
  principalId: string,
  request: Extract<RepositoryWorkspaceReadRequest, { operation: "repository.get.v1" }>,
): Promise<WorkspaceResult<WorkspaceReadResponse>> {
  const registered = await ensureRepository(input, principalId, request.projectId);
  if (!registered.ok) return registered;
  if (registered.value.worktree.contextId === request.contextId) {
    const adopted = await ensureRuntimeRepositoryContextPlan(
      input,
      principalId,
      request.projectId,
      request.contextId,
    );
    if (!adopted.ok) return adopted;
  }
  const context = input.store.read(access(principalId, request.projectId), {
    operation: "context.get.v1",
    projectId: request.projectId,
    contextId: request.contextId,
  });
  if (!context.ok) return context;
  if (context.value.operation !== "context.get.v1")
    return failure("storage_failure", "context read returned another operation");
  let worktree = registered.value.worktree;
  if (context.value.context.rootWorktreeId !== null) {
    const selected = input.runtime.service.getWorktree(context.value.context.rootWorktreeId);
    if (!selected.ok) return repositoryFailure(selected);
    worktree = selected.value;
  }
  const observed = await input.runtime.service.observeWorktree({
    worktreeId: worktree.worktreeId,
    executionHostId: input.runtime.executionHostId,
    projectId: request.projectId,
    contextId: request.contextId,
  });
  return observed.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          repository: registered.value.repository,
          binding: registered.value.binding,
          registeredWorktree: registered.value.worktree,
          contextWorktree: observed.value.worktree,
          observedContextHead: observed.value.currentHead,
          workingTreeState: observed.value.dirty ? "dirty" : "clean",
          testProfiles: [...input.runtime.testProfiles],
        },
      }
    : repositoryFailure(observed);
}

async function readWorktree(
  input: RepositoryReadInput,
  request: Extract<RepositoryWorkspaceReadRequest, { operation: "worktree.get.v1" }>,
): Promise<WorkspaceResult<WorkspaceReadResponse>> {
  const worktree = input.runtime.service.getWorktree(request.worktreeId);
  if (!worktree.ok) return repositoryFailure(worktree);
  if (
    worktree.value.projectId !== request.projectId ||
    worktree.value.contextId !== request.contextId
  )
    return failure("not_found", "worktree is outside requested context");
  const observed = await input.runtime.service.observeWorktree({
    worktreeId: worktree.value.worktreeId,
    executionHostId: input.runtime.executionHostId,
    projectId: request.projectId,
    contextId: request.contextId,
  });
  return observed.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          worktree: { ...worktree.value, headCommit: observed.value.currentHead },
        },
      }
    : repositoryFailure(observed);
}

function planInScope(
  input: RepositoryReadInput,
  planId: string,
  projectId: string,
  contextId: string,
): WorkspaceResult<null> {
  const plan = input.runtime.service.getPlan(planId);
  if (!plan.ok) return repositoryFailure(plan);
  return plan.value.projectId === projectId && plan.value.contextId === contextId
    ? { ok: true, value: null }
    : failure("not_found", "plan workspace is outside requested context");
}

function access(principalId: string, projectId: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(principalId),
    actorId: null,
    clientId: ClientIdSchema.parse("client.repository.read"),
    authorizedProjectIds: [ProjectIdSchema.parse(projectId)],
  });
}

function repositoryFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): WorkspaceResult<never> {
  return failure(
    result.error.code === "invalid_input"
      ? "invalid_input"
      : result.error.code === "denied"
        ? "forbidden"
        : ["conflict", "stale", "dirty", "not_ready", "conflicted"].includes(result.error.code)
          ? "conflict"
          : "unavailable",
    result.error.message,
  );
}

function failure(
  code:
    | "invalid_input"
    | "forbidden"
    | "not_found"
    | "conflict"
    | "unavailable"
    | "storage_failure",
  message: string,
): WorkspaceResult<never> {
  return { ok: false, error: { code, message } };
}
