/** Metadata-only adoption of an existing registered checkout. @scope spec://org.vibevm.zap/lens/PROP-014#identity */
import { realpathSync } from "node:fs";
import {
  AdoptRegisteredPlanRequestSchema,
  type AdoptRegisteredPlanRequest,
  type PreparedPlan,
  type RepositoryWorkspaceResult,
} from "./contracts.ts";
import { beginOperation, completeOperation, operationIdentity } from "./operations.ts";
import { fail, gitText, hostAvailable, type RepositoryWorkspaceRuntime } from "./runtime.ts";

export async function adoptRegisteredPlan(
  runtime: RepositoryWorkspaceRuntime,
  raw: AdoptRegisteredPlanRequest,
): Promise<RepositoryWorkspaceResult<PreparedPlan>> {
  const parsed = AdoptRegisteredPlanRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "registered plan adoption is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const scope = `plan:${input.planId}:adopt-registered`;
  const operation = beginOperation(runtime, input, scope, input, {
    kind: "plan",
    id: input.planId,
  });
  if (!operation.ok) return operation;
  const prior = runtime.store.getPlan(input.planId);
  if (prior !== null) {
    if (
      prior.projectId !== input.projectId ||
      prior.contextId !== input.contextId ||
      prior.rootWorktreeId !== input.registeredWorktreeId
    )
      return fail("conflict", "plan identity is already bound to another workspace");
    const worktree = runtime.store.getWorktree(prior.rootWorktreeId);
    if (worktree === null)
      return fail("unavailable", "adopted registered workspace is unavailable");
    return finish(runtime, input, scope, operation.value, {
      plan: prior,
      worktree: worktree.record,
    });
  }
  if (
    runtime.store
      .listPlans(input.projectId)
      .some((plan) => plan.rootWorktreeId === input.registeredWorktreeId)
  )
    return fail("conflict", "registered workspace is already associated with another plan");
  const binding = runtime.store.getBinding(input.projectId);
  const worktree = runtime.store.getWorktree(input.registeredWorktreeId);
  if (
    binding === null ||
    worktree === null ||
    binding.record.registeredWorktreeId !== input.registeredWorktreeId ||
    worktree.record.kind !== "registered" ||
    worktree.record.contextId !== input.contextId ||
    worktree.record.headCommit !== input.expectedHead
  )
    return fail("invalid_input", "registered workspace does not match the default project context");
  const repository = runtime.store.getRepository(worktree.record.repositoryId);
  if (repository === null) return fail("unavailable", "registered repository is unavailable");
  const head = await gitText(runtime, worktree.directory, ["rev-parse", "--verify", "HEAD"]);
  const branch = await gitText(runtime, worktree.directory, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const common = await gitText(runtime, worktree.directory, [
    "rev-parse",
    "--path-format=absolute",
    "--git-common-dir",
  ]);
  if (!head.ok || !branch.ok || !common.ok)
    return fail("unavailable", "registered workspace identity cannot be observed");
  try {
    if (
      head.value !== input.expectedHead ||
      branch.value !== worktree.record.branchRef ||
      realpathSync(common.value) !== repository.commonDirectory
    )
      return fail("stale", "registered workspace identity changed before plan adoption");
  } catch {
    return fail("unavailable", "registered repository common directory is unavailable");
  }
  const now = runtime.now();
  const plan = {
    planId: input.planId,
    repositoryId: repository.record.repositoryId,
    executionHostId: runtime.executionHostId,
    projectId: input.projectId,
    contextId: input.contextId,
    displayName: input.displayName,
    rootWorktreeId: worktree.record.worktreeId,
    integrationTargetWorktreeId: worktree.record.worktreeId,
    algorithmBinding: input.algorithmBinding,
    state: "ready" as const,
    creatorPrincipalId: input.principalId,
    lastUpdatedByPrincipalId: input.principalId,
    revision: "1",
    createdAt: now,
  };
  if (!runtime.store.putPlan(plan))
    return fail("unavailable", "adopted plan could not be persisted");
  return finish(runtime, input, scope, operation.value, { plan, worktree: worktree.record });
}

function finish(
  runtime: RepositoryWorkspaceRuntime,
  input: AdoptRegisteredPlanRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  value: PreparedPlan,
): RepositoryWorkspaceResult<PreparedPlan> {
  const identity = operationIdentity(operation);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "plan",
    value.plan.planId,
  );
  return completed.ok ? { ok: true, value } : completed;
}
