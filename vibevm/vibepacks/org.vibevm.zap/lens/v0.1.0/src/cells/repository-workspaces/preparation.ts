/** Plan and worker workspace preparation. @scope spec://org.vibevm.zap/lens/PROP-014#preparation */
import { existsSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import type { RepositoryWorktreeRecord } from "../repository-model/index.ts";
import {
  AssignWorktreeRequestSchema,
  PrepareChildRequestSchema,
  PreparePlanRequestSchema,
  RecordWorktreeHeadRequestSchema,
  UpdateAlgorithmBindingRequestSchema,
  type AssignWorktreeRequest,
  type PrepareChildRequest,
  type PreparedPlan,
  type PreparePlanRequest,
  type RepositoryWorkspaceResult,
  type RecordWorktreeHeadRequest,
  type UpdateAlgorithmBindingRequest,
} from "./contracts.ts";
import { beginOperation, completeOperation, operationIdentity } from "./operations.ts";
import {
  branchSegment,
  contains,
  fail,
  gitText,
  hostAvailable,
  nextRevision,
  ownedPath,
  type RepositoryWorkspaceRuntime,
} from "./runtime.ts";
import type { ProtectedBinding, ProtectedRepository, ProtectedWorktree } from "./store.ts";

export async function preparePlanRoot(
  runtime: RepositoryWorkspaceRuntime,
  raw: PreparePlanRequest,
): Promise<RepositoryWorkspaceResult<PreparedPlan>> {
  const parsed = PreparePlanRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "plan workspace request is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const scope = `plan:${input.planId}:prepare`;
  const operation = beginOperation(runtime, input, scope, input, {
    kind: "plan",
    id: input.planId,
  });
  if (!operation.ok) return operation;
  if (operation.value.state === "complete") return loadPlan(runtime, input.planId);
  const priorPlan = runtime.store.getPlan(input.planId);
  if (priorPlan !== null) {
    if (
      priorPlan.projectId !== input.projectId ||
      priorPlan.contextId !== input.contextId ||
      JSON.stringify(priorPlan.algorithmBinding) !== JSON.stringify(input.algorithmBinding)
    )
      return fail("conflict", "plan identity is already bound to different content");
    const reconciled = await reconcilePrepared(runtime, priorPlan.rootWorktreeId);
    if (!reconciled.ok) return reconciled;
    const readyPlan =
      priorPlan.state === "ready"
        ? priorPlan
        : {
            ...priorPlan,
            state: "ready" as const,
            revision: nextRevision(priorPlan.revision),
            lastUpdatedByPrincipalId: input.principalId,
          };
    if (!runtime.store.putPlan(readyPlan))
      return fail("unavailable", "reconciled plan workspace could not be persisted");
    return finishPlan(runtime, input, scope, operation.value, readyPlan, reconciled.value);
  }
  const binding = runtime.store.getBinding(input.projectId);
  const base = runtime.store.getWorktree(input.baseWorktreeId);
  const checked = await validateBasis(
    runtime,
    input.projectId,
    binding,
    base,
    input.expectedBaseHead,
  );
  if (!checked.ok) return checked;
  const worktreeId = `worktree.plan.${branchSegment(input.planId)}`;
  const directory = ownedPath(runtime, worktreeId);
  if (directory === null) return fail("invalid_input", "plan workspace path is invalid");
  const branchRef = `codex/zap/plan/${branchSegment(input.planId)}`;
  const now = runtime.now();
  const worktree: ProtectedWorktree = {
    record: {
      worktreeId,
      repositoryId: checked.value.repository.record.repositoryId,
      executionHostId: runtime.executionHostId,
      projectId: input.projectId,
      contextId: input.contextId,
      planId: input.planId,
      kind: "plan_root",
      parentWorktreeId: input.baseWorktreeId,
      branchRef,
      basisCommit: input.expectedBaseHead,
      headCommit: input.expectedBaseHead,
      state: "preparing",
      assignments: [],
      revision: "1",
      createdAt: now,
    },
    directory,
    projectDirectory: resolve(directory, checked.value.binding.projectRelativePath),
  };
  const plan = {
    planId: input.planId,
    repositoryId: checked.value.repository.record.repositoryId,
    executionHostId: runtime.executionHostId,
    projectId: input.projectId,
    contextId: input.contextId,
    displayName: input.displayName,
    rootWorktreeId: worktreeId,
    integrationTargetWorktreeId: input.baseWorktreeId,
    algorithmBinding: input.algorithmBinding,
    state: "preparing" as const,
    creatorPrincipalId: input.principalId,
    lastUpdatedByPrincipalId: input.principalId,
    revision: "1",
    createdAt: now,
  };
  if (!runtime.store.putWorktree(worktree) || !runtime.store.putPlan(plan))
    return fail("unavailable", "plan workspace intent could not be persisted");
  const created = await createWorktree(
    runtime,
    checked.value.repository,
    checked.value.binding,
    worktree,
  );
  if (!created.ok) return created;
  const readyPlan = { ...plan, state: "ready" as const, revision: "2" };
  if (!runtime.store.putWorktree(created.value) || !runtime.store.putPlan(readyPlan))
    return fail("unavailable", "prepared plan workspace could not be persisted");
  return finishPlan(runtime, input, scope, operation.value, readyPlan, created.value);
}

export async function prepareChildWorktree(
  runtime: RepositoryWorkspaceRuntime,
  raw: PrepareChildRequest,
): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>> {
  const parsed = PrepareChildRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "child workspace request is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const scope = `plan:${input.planId}:child`;
  const reservedId = `worktree.child.${branchSegment(`${input.planId}:${input.principalId}:${input.requestId}`)}`;
  const operation = beginOperation(runtime, input, scope, input, {
    kind: "worktree",
    id: reservedId,
  });
  if (!operation.ok) return operation;
  const worktreeId =
    operation.value.state === "new" ? operation.value.resultId : operation.value.operation.resultId;
  if (worktreeId === null) return fail("unavailable", "child workspace reservation is invalid");
  if (operation.value.state === "complete") return publicWorktree(runtime, worktreeId);
  const prior = runtime.store.getWorktree(worktreeId);
  if (prior !== null) {
    const reconciled = await reconcilePrepared(runtime, worktreeId);
    if (!reconciled.ok) return reconciled;
    return finishChild(runtime, input, scope, operation.value, reconciled.value);
  }
  const plan = runtime.store.getPlan(input.planId);
  const parent = runtime.store.getWorktree(input.parentWorktreeId);
  const binding = runtime.store.getBinding(input.projectId);
  if (plan === null || plan.state !== "ready" || plan.contextId !== input.contextId)
    return fail("not_ready", "parent plan is not ready for child workspace preparation");
  const checked = await validateBasis(
    runtime,
    input.projectId,
    binding,
    parent,
    input.expectedParentHead,
  );
  if (!checked.ok) return checked;
  if (parent?.record.planId !== input.planId)
    return fail("invalid_input", "parent workspace does not belong to the requested plan");
  const clean = await requireClean(runtime, parent.directory);
  if (!clean.ok) return clean;
  const directory = ownedPath(runtime, worktreeId);
  if (directory === null) return fail("invalid_input", "child workspace path is invalid");
  const branchRef = `codex/zap/worker/${branchSegment(`${input.planId}:${input.principalId}:${input.requestId}`)}`;
  const worktree: ProtectedWorktree = {
    record: {
      worktreeId,
      repositoryId: checked.value.repository.record.repositoryId,
      executionHostId: runtime.executionHostId,
      projectId: input.projectId,
      contextId: input.contextId,
      planId: input.planId,
      kind: "worker",
      parentWorktreeId: input.parentWorktreeId,
      branchRef,
      basisCommit: input.expectedParentHead,
      headCommit: input.expectedParentHead,
      state: "preparing",
      assignments: [],
      revision: "1",
      createdAt: runtime.now(),
    },
    directory,
    projectDirectory: resolve(directory, checked.value.binding.projectRelativePath),
  };
  if (!runtime.store.putWorktree(worktree))
    return fail("unavailable", "child workspace intent could not be persisted");
  const created = await createWorktree(
    runtime,
    checked.value.repository,
    checked.value.binding,
    worktree,
  );
  if (!created.ok) return created;
  if (!runtime.store.putWorktree(created.value))
    return fail("unavailable", "prepared child workspace could not be persisted");
  return finishChild(runtime, input, scope, operation.value, created.value);
}

export function updateAlgorithmBinding(
  runtime: RepositoryWorkspaceRuntime,
  raw: UpdateAlgorithmBindingRequest,
): RepositoryWorkspaceResult<PreparedPlan["plan"]> {
  const parsed = UpdateAlgorithmBindingRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "algorithm binding update is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const plan = runtime.store.getPlan(input.planId);
  if (plan === null) return fail("unavailable", "project plan is unavailable");
  if (plan.revision !== input.expectedRevision)
    return fail("conflict", "project plan revision changed");
  const next = {
    ...plan,
    algorithmBinding: input.algorithmBinding,
    lastUpdatedByPrincipalId: input.principalId,
    revision: nextRevision(plan.revision),
  };
  return runtime.store.putPlan(next)
    ? { ok: true, value: next }
    : fail("unavailable", "algorithm binding could not be persisted");
}

export async function recordWorktreeHead(
  runtime: RepositoryWorkspaceRuntime,
  raw: RecordWorktreeHeadRequest,
): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>> {
  const parsed = RecordWorktreeHeadRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "workspace HEAD observation is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (worktree === null || worktree.record.state !== "ready")
    return fail("not_ready", "workspace is not ready for a HEAD observation");
  if (
    worktree.record.revision !== input.expectedRevision ||
    worktree.record.headCommit !== input.expectedOldHead
  )
    return fail("conflict", "workspace HEAD basis changed");
  const head = await gitText(runtime, worktree.directory, ["rev-parse", "--verify", "HEAD"]);
  if (!head.ok) return head;
  if (head.value !== input.newHead) return fail("stale", "reported workspace HEAD is not current");
  const ancestry = await runtime.git.run({
    cwd: worktree.directory,
    args: ["merge-base", "--is-ancestor", input.expectedOldHead, input.newHead],
  });
  if (ancestry.exitCode !== 0)
    return fail("conflict", "workspace HEAD is not a descendant of its recorded basis");
  const next: ProtectedWorktree = {
    ...worktree,
    record: {
      ...worktree.record,
      headCommit: input.newHead,
      revision: nextRevision(worktree.record.revision),
    },
  };
  return runtime.store.putWorktree(next)
    ? { ok: true, value: next.record }
    : fail("unavailable", "workspace HEAD observation could not be persisted");
}

export function assignWorktree(
  runtime: RepositoryWorkspaceRuntime,
  raw: AssignWorktreeRequest,
  allowConflictedIntegration = false,
): RepositoryWorkspaceResult<RepositoryWorktreeRecord> {
  const parsed = AssignWorktreeRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "workspace assignment is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (worktree === null) return fail("not_ready", "workspace is not ready for assignment");
  const assignable =
    worktree.record.state === "ready" ||
    (allowConflictedIntegration &&
      worktree.record.kind === "integration" &&
      worktree.record.state === "conflicted");
  if (!assignable) return fail("not_ready", "workspace is not ready for assignment");
  if (worktree.record.headCommit !== input.basisCommit)
    return fail("stale", "workspace assignment basis changed");
  if (worktree.record.revision !== input.expectedRevision)
    return fail("conflict", "workspace assignment revision changed");
  const duplicate = worktree.record.assignments.find(
    (item) => item.assignmentId === input.assignmentId,
  );
  if (duplicate !== undefined)
    return JSON.stringify(duplicate) ===
      JSON.stringify({
        assignmentId: input.assignmentId,
        attemptId: input.attemptId,
        assignerPrincipalId: input.principalId,
        basisCommit: input.basisCommit,
        actorId: input.actorId,
        taskId: input.taskId,
        runId: input.runId,
        semanticTargetRefs: input.semanticTargetRefs,
        assignedAt: duplicate.assignedAt,
        releasedAt: null,
      })
      ? { ok: true, value: worktree.record }
      : fail("conflict", "assignment identity changed content");
  const next: ProtectedWorktree = {
    ...worktree,
    record: {
      ...worktree.record,
      assignments: [
        ...worktree.record.assignments,
        {
          assignmentId: input.assignmentId,
          attemptId: input.attemptId,
          assignerPrincipalId: input.principalId,
          basisCommit: input.basisCommit,
          actorId: input.actorId,
          taskId: input.taskId,
          runId: input.runId,
          semanticTargetRefs: input.semanticTargetRefs,
          assignedAt: runtime.now(),
          releasedAt: null,
        },
      ],
      revision: nextRevision(worktree.record.revision),
    },
  };
  return runtime.store.putWorktree(next)
    ? { ok: true, value: next.record }
    : fail("unavailable", "workspace assignment could not be persisted");
}

async function createWorktree(
  runtime: RepositoryWorkspaceRuntime,
  repository: ProtectedRepository,
  binding: ProtectedBinding,
  worktree: ProtectedWorktree,
): Promise<RepositoryWorkspaceResult<ProtectedWorktree>> {
  if (!existsSync(worktree.directory)) {
    const added = await gitText(runtime, repository.topLevel, [
      "worktree",
      "add",
      "-b",
      worktree.record.branchRef,
      worktree.directory,
      worktree.record.basisCommit,
    ]);
    if (!added.ok) return added;
  }
  return verifyPrepared(runtime, repository, binding, worktree);
}

async function verifyPrepared(
  runtime: RepositoryWorkspaceRuntime,
  repository: ProtectedRepository,
  binding: ProtectedBinding,
  worktree: ProtectedWorktree,
): Promise<RepositoryWorkspaceResult<ProtectedWorktree>> {
  try {
    const directory = realpathSync(worktree.directory);
    const projectDirectory = realpathSync(resolve(directory, binding.projectRelativePath));
    if (!contains(runtime.worktreeRoot, directory) || !contains(directory, projectDirectory))
      return fail("unavailable", "prepared workspace escaped its trusted root");
    const common = await gitText(runtime, directory, [
      "rev-parse",
      "--path-format=absolute",
      "--git-common-dir",
    ]);
    const head = await gitText(runtime, directory, ["rev-parse", "--verify", "HEAD"]);
    const branch = await gitText(runtime, directory, ["rev-parse", "--abbrev-ref", "HEAD"]);
    if (!common.ok || !head.ok || !branch.ok)
      return fail("unavailable", "prepared workspace cannot be verified");
    if (
      realpathSync(common.value) !== repository.commonDirectory ||
      branch.value !== worktree.record.branchRef
    )
      return fail("conflict", "prepared workspace identity does not match its durable intent");
    if (head.value !== worktree.record.basisCommit && worktree.record.state === "preparing")
      return fail("stale", "prepared workspace HEAD changed before readiness");
    return {
      ok: true,
      value: {
        ...worktree,
        directory,
        projectDirectory,
        record: {
          ...worktree.record,
          headCommit: head.value,
          state: "ready",
          revision: nextRevision(worktree.record.revision),
        },
      },
    };
  } catch {
    return fail("unavailable", "prepared workspace project directory is unavailable");
  }
}

async function reconcilePrepared(
  runtime: RepositoryWorkspaceRuntime,
  worktreeId: string,
): Promise<RepositoryWorkspaceResult<ProtectedWorktree>> {
  const worktree = runtime.store.getWorktree(worktreeId);
  if (worktree === null) return fail("unavailable", "prepared workspace record is unavailable");
  const repository = runtime.store.getRepository(worktree.record.repositoryId);
  const binding = runtime.store.getBinding(worktree.record.projectId);
  if (repository === null || binding === null)
    return fail("unavailable", "prepared workspace binding is unavailable");
  const verified = existsSync(worktree.directory)
    ? await verifyPrepared(runtime, repository, binding, worktree)
    : await createWorktree(runtime, repository, binding, worktree);
  if (!verified.ok) return verified;
  if (!runtime.store.putWorktree(verified.value))
    return fail("unavailable", "reconciled workspace could not be persisted");
  return verified;
}

async function validateBasis(
  runtime: RepositoryWorkspaceRuntime,
  projectId: string,
  binding: ProtectedBinding | null,
  worktree: ProtectedWorktree | null,
  expectedHead: string,
): Promise<
  RepositoryWorkspaceResult<{ repository: ProtectedRepository; binding: ProtectedBinding }>
> {
  if (binding === null || worktree === null || worktree.record.projectId !== projectId)
    return fail("invalid_input", "workspace basis does not belong to the project");
  if (worktree.record.executionHostId !== runtime.executionHostId)
    return fail("host_unavailable", "workspace basis is registered on another host");
  if (worktree.record.headCommit !== expectedHead)
    return fail("stale", "workspace basis commit changed");
  const repository = runtime.store.getRepository(worktree.record.repositoryId);
  if (repository === null) return fail("unavailable", "workspace repository is unavailable");
  const actualHead = await gitText(runtime, worktree.directory, ["rev-parse", "--verify", "HEAD"]);
  const actualBranch = await gitText(runtime, worktree.directory, [
    "rev-parse",
    "--abbrev-ref",
    "HEAD",
  ]);
  const actualCommon = await gitText(runtime, worktree.directory, [
    "rev-parse",
    "--path-format=absolute",
    "--git-common-dir",
  ]);
  if (!actualHead.ok || !actualBranch.ok || !actualCommon.ok)
    return fail("unavailable", "workspace basis cannot be observed");
  try {
    if (realpathSync(actualCommon.value) !== repository.commonDirectory)
      return fail("conflict", "workspace repository identity changed");
  } catch {
    return fail("unavailable", "workspace common directory is unavailable");
  }
  if (actualHead.value !== expectedHead || actualBranch.value !== worktree.record.branchRef)
    return fail("stale", "workspace Git basis changed");
  return { ok: true, value: { repository, binding } };
}

async function requireClean(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
): Promise<RepositoryWorkspaceResult<null>> {
  const status = await gitText(runtime, cwd, ["status", "--porcelain=v1", "--untracked-files=all"]);
  if (!status.ok) return status;
  return status.value.length === 0
    ? { ok: true, value: null }
    : fail("dirty", "workspace has uncommitted or untracked changes");
}

function loadPlan(
  runtime: RepositoryWorkspaceRuntime,
  planId: string,
): RepositoryWorkspaceResult<PreparedPlan> {
  const plan = runtime.store.getPlan(planId);
  const worktree = plan === null ? null : runtime.store.getWorktree(plan.rootWorktreeId);
  return plan !== null && worktree !== null
    ? { ok: true, value: { plan, worktree: worktree.record } }
    : fail("unavailable", "completed plan workspace is unavailable");
}

function publicWorktree(
  runtime: RepositoryWorkspaceRuntime,
  id: string,
): RepositoryWorkspaceResult<RepositoryWorktreeRecord> {
  const worktree = runtime.store.getWorktree(id);
  return worktree === null
    ? fail("unavailable", "completed child workspace is unavailable")
    : { ok: true, value: worktree.record };
}

function finishPlan(
  runtime: RepositoryWorkspaceRuntime,
  input: PreparePlanRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  plan: PreparedPlan["plan"],
  worktree: ProtectedWorktree,
): RepositoryWorkspaceResult<PreparedPlan> {
  const identity = operationIdentity(operation);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "plan",
    plan.planId,
  );
  return completed.ok ? { ok: true, value: { plan, worktree: worktree.record } } : completed;
}

function finishChild(
  runtime: RepositoryWorkspaceRuntime,
  input: PrepareChildRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  worktree: ProtectedWorktree,
): RepositoryWorkspaceResult<RepositoryWorktreeRecord> {
  const identity = operationIdentity(operation);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "worktree",
    worktree.record.worktreeId,
  );
  return completed.ok ? { ok: true, value: worktree.record } : completed;
}
