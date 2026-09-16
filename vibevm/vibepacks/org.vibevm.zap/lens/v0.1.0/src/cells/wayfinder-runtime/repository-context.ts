/** Repository workspace context adoption and writer fencing. @scope spec://org.vibevm.zap/lens/PROP-014#identity */
import { createHash } from "node:crypto";
import { PrincipalIdSchema } from "../protocol/index.ts";
import type { PreparedPlan, RegisteredRepository } from "../repository-workspaces/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
  type WorkspaceAccessContext,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";

interface RepositoryContextInput {
  readonly runtime: RuntimeRepositoryWorkspaces;
  readonly store: WorkspaceStore;
}

export function canOpenRepositoryWriter(
  input: RepositoryContextInput,
  accessContext: WorkspaceAccessContext,
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
): WorkspaceResult<null> {
  if (!accessContext.authorizedProjectIds.includes(projectId))
    return failure("forbidden", "writer scope is outside authenticated project access");
  const context = input.store.read(accessContext, {
    operation: "context.get.v1",
    projectId,
    contextId,
  });
  if (!context.ok) return context;
  if (context.value.operation !== "context.get.v1")
    return failure("storage_failure", "context read returned another operation");
  if (context.value.context.planId === null) return { ok: true, value: null };
  const worktrees = input.runtime.service.listWorktrees(context.value.context.planId);
  if (!worktrees.ok) return repositoryFailure(worktrees);
  return worktrees.value.some(
    (worktree) => input.runtime.service.currentWriterLease(worktree.worktreeId) !== null,
  )
    ? failure("conflict", "repository integration holds the context writer lease")
    : { ok: true, value: null };
}

export async function ensureRuntimeRepositoryContextPlan(
  input: RepositoryContextInput,
  principalId: string,
  projectId: string,
  contextId: string,
): Promise<WorkspaceResult<PreparedPlan>> {
  const context = input.store.read(access(principalId, projectId), {
    operation: "context.get.v1",
    projectId: ProjectIdSchema.parse(projectId),
    contextId: WorkContextIdSchema.parse(contextId),
  });
  if (!context.ok) return context;
  if (context.value.operation !== "context.get.v1")
    return failure("storage_failure", "context read returned another operation");
  if (context.value.context.planId !== null) {
    const plan = input.runtime.service.getPlan(context.value.context.planId);
    if (!plan.ok) return repositoryFailure(plan);
    const worktree = input.runtime.service.getWorktree(plan.value.rootWorktreeId);
    return worktree.ok
      ? { ok: true, value: { plan: plan.value, worktree: worktree.value } }
      : repositoryFailure(worktree);
  }
  const registered = await ensureRepository(input, principalId, projectId);
  if (!registered.ok) return registered;
  if (registered.value.worktree.contextId !== contextId)
    return failure("conflict", "only the registered repository context can be adopted in place");
  const observed = await input.runtime.service.observeWorktree({
    worktreeId: registered.value.worktree.worktreeId,
    executionHostId: input.runtime.executionHostId,
    projectId,
    contextId,
  });
  if (!observed.ok) return repositoryFailure(observed);
  let worktree = observed.value.worktree;
  if (worktree.headCommit !== observed.value.currentHead) {
    const recorded = await input.runtime.service.recordWorktreeHead({
      requestId: `request.plan-adoption-head.${digest(`${projectId}\u0000${contextId}`)}`,
      principalId,
      executionHostId: input.runtime.executionHostId,
      worktreeId: worktree.worktreeId,
      expectedRevision: worktree.revision,
      expectedOldHead: worktree.headCommit,
      newHead: observed.value.currentHead,
    });
    if (!recorded.ok) return repositoryFailure(recorded);
    worktree = recorded.value;
  }
  const key = digest(`${projectId}\u0000${contextId}\u0000registered-plan`);
  const adopted = await input.runtime.service.adoptRegisteredPlan({
    requestId: `request.plan-adoption.${key}`,
    principalId,
    executionHostId: input.runtime.executionHostId,
    projectId,
    contextId,
    planId: `plan.workspace.${key}`,
    displayName: context.value.context.displayName,
    registeredWorktreeId: worktree.worktreeId,
    expectedHead: worktree.headCommit,
    algorithmBinding: { state: "pending" },
  });
  if (!adopted.ok) return repositoryFailure(adopted);
  const bound = input.store.bindExistingContextPlan({
    projectId: ProjectIdSchema.parse(projectId),
    contextId: WorkContextIdSchema.parse(contextId),
    plan: adopted.value.plan,
    rootWorktree: adopted.value.worktree,
  });
  return bound.ok ? { ok: true, value: adopted.value } : bound;
}

export async function ensureRepository(
  input: RepositoryContextInput,
  principalId: string,
  projectId: string,
): Promise<WorkspaceResult<RegisteredRepository>> {
  const scopedAccess = access(principalId, projectId);
  const project = input.store.read(scopedAccess, {
    operation: "project.get.v1",
    projectId: ProjectIdSchema.parse(projectId),
  });
  if (!project.ok) return project;
  if (project.value.operation !== "project.get.v1")
    return failure("storage_failure", "project read returned another operation");
  const defaultContextId = project.value.detail.project.defaultContextId;
  const launch = input.store.resolveProjectLaunch(
    ProjectIdSchema.parse(projectId),
    defaultContextId,
  );
  if (!launch.ok) return launch;
  const registered = await input.runtime.service.registerRepository({
    requestId: `request.repository.${digest(`${projectId}\u0000${input.runtime.executionHostId}`)}`,
    principalId,
    executionHostId: input.runtime.executionHostId,
    projectId,
    contextId: defaultContextId,
    trustedProjectCwd: launch.value.cwd,
    displayName: project.value.detail.project.displayName,
  });
  return registered.ok ? { ok: true, value: registered.value } : repositoryFailure(registered);
}

function access(principalId: string, projectId: string): WorkspaceAccessContext {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(principalId),
    actorId: null,
    clientId: ClientIdSchema.parse(`client.repository.${digest(principalId)}`),
    authorizedProjectIds: [ProjectIdSchema.parse(projectId)],
  });
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
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
