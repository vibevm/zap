/** Managed prelaunch admission against the repository writer lease. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type { ManagedWorkResult } from "../managed-work/index.ts";
import { ProjectIdSchema, type WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";

export function createRepositoryWriterAdmission(runtime: RuntimeRepositoryWorkspaces) {
  return (
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
  ): ManagedWorkResult<null> => {
    const scopedProjectId = ProjectIdSchema.safeParse(projectId);
    if (!scopedProjectId.success || !access.authorizedProjectIds.includes(scopedProjectId.data))
      return failure("managed writer admission is outside authenticated project scope");
    const plans = runtime.service.listPlans(projectId);
    if (!plans.ok) return failure(plans.error.message);
    const plan = plans.value.find((candidate) => candidate.contextId === contextId);
    if (plan === undefined) return { ok: true, value: null };
    const worktrees = runtime.service.listWorktrees(plan.planId);
    if (!worktrees.ok) return failure(worktrees.error.message);
    for (const worktree of worktrees.value) {
      if (runtime.service.currentWriterLease(worktree.worktreeId) !== null)
        return {
          ok: false,
          error: {
            code: "conflict",
            message: "repository target is held by an exact integration writer lease",
          },
        };
    }
    return { ok: true, value: null };
  };
}

function failure(message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code: "unavailable", message } };
}
