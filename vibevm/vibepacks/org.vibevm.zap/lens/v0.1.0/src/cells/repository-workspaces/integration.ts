/** Separate-tree integration and checked promotion. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import { existsSync, mkdirSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import type { IntegrationAttempt } from "../repository-model/index.ts";
import {
  PrepareIntegrationRequestSchema,
  RecordIntegrationReviewRequestSchema,
  RecordResolutionRequestSchema,
  RunIntegrationTestRequestSchema,
  type PrepareIntegrationRequest,
  type RecordIntegrationReviewRequest,
  type RecordResolutionRequest,
  type RepositoryWorkspaceResult,
  type RunIntegrationTestRequest,
} from "./contracts.ts";
import { beginOperation, completeOperation, operationIdentity } from "./operations.ts";
import {
  branchSegment,
  contains,
  fail,
  gitText,
  hostAvailable,
  mergeIdentityEnvironment,
  nextRevision,
  ownedPath,
  type RepositoryWorkspaceRuntime,
} from "./runtime.ts";
import type { ProtectedBinding, ProtectedRepository, ProtectedWorktree } from "./store.ts";

export async function prepareIntegration(
  runtime: RepositoryWorkspaceRuntime,
  raw: PrepareIntegrationRequest,
): Promise<RepositoryWorkspaceResult<IntegrationAttempt>> {
  const parsed = PrepareIntegrationRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "integration request is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const scope = `integration:${input.integrationId}:prepare`;
  const operation = beginOperation(runtime, input, scope, input, {
    kind: "integration",
    id: input.integrationId,
  });
  if (!operation.ok) return operation;
  if (operation.value.state === "complete") return publicIntegration(runtime, input.integrationId);
  const prior = runtime.store.getIntegration(input.integrationId);
  if (prior !== null) return reconcileIntegration(runtime, input, scope, operation.value, prior);
  const basis = await validateIntegrationBasis(runtime, input);
  if (!basis.ok) return basis;
  const worktreeId = `worktree.integration.${branchSegment(input.integrationId)}`;
  const directory = ownedPath(runtime, worktreeId);
  if (directory === null) return fail("invalid_input", "integration workspace path is invalid");
  const now = runtime.now();
  const worktree: ProtectedWorktree = {
    record: {
      worktreeId,
      repositoryId: basis.value.repository.record.repositoryId,
      executionHostId: runtime.executionHostId,
      projectId: basis.value.target.record.projectId,
      contextId: basis.value.target.record.contextId,
      planId: input.planId,
      kind: "integration",
      parentWorktreeId: input.targetWorktreeId,
      branchRef: `codex/zap/integration/${branchSegment(input.integrationId)}`,
      basisCommit: input.expectedTargetHead,
      headCommit: input.expectedTargetHead,
      state: "preparing",
      assignments: [],
      revision: "1",
      createdAt: now,
    },
    directory,
    projectDirectory: resolve(directory, basis.value.binding.projectRelativePath),
  };
  const integration: IntegrationAttempt = {
    integrationId: input.integrationId,
    repositoryId: basis.value.repository.record.repositoryId,
    executionHostId: runtime.executionHostId,
    planId: input.planId,
    sourceWorktreeId: input.sourceWorktreeId,
    targetWorktreeId: input.targetWorktreeId,
    integrationWorktreeId: worktreeId,
    expectedSourceHead: input.expectedSourceHead,
    expectedTargetHead: input.expectedTargetHead,
    candidateCommit: null,
    conflictPaths: [],
    state: "preparing",
    testEvidence: null,
    review: null,
    creatorPrincipalId: input.principalId,
    revision: "1",
    createdAt: now,
  };
  if (!runtime.store.putWorktree(worktree) || !runtime.store.putIntegration(integration))
    return fail("unavailable", "integration intent could not be persisted");
  const prepared = await createIntegrationWorktree(
    runtime,
    basis.value.repository,
    basis.value.binding,
    worktree,
  );
  if (!prepared.ok) return prepared;
  const merged = await mergeCandidate(runtime, prepared.value, integration);
  if (!merged.ok) return merged;
  if (
    !runtime.store.putWorktree(merged.value.worktree) ||
    !runtime.store.putIntegration(merged.value.integration)
  )
    return fail("unavailable", "integration result could not be persisted");
  return finishPrepare(runtime, input, scope, operation.value, merged.value.integration);
}

export async function recordResolution(
  runtime: RepositoryWorkspaceRuntime,
  raw: RecordResolutionRequest,
): Promise<RepositoryWorkspaceResult<IntegrationAttempt>> {
  const parsed = RecordResolutionRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "integration resolution is invalid");
  const input = parsed.data;
  const current = currentIntegration(
    runtime,
    input.integrationId,
    input.executionHostId,
    input.expectedRevision,
  );
  if (!current.ok) return current;
  if (current.value.integration.state !== "conflicted")
    return fail("conflict", "integration is not awaiting conflict resolution");
  const statusResult = await status(runtime, current.value.worktree.directory);
  if (!statusResult.ok) return statusResult;
  if (statusResult.value.length !== 0)
    return fail("dirty", "integration resolution is not committed and clean");
  const head = await headOf(runtime, current.value.worktree.directory);
  if (!head.ok) return head;
  if (head.value !== input.resolutionCommit)
    return fail("stale", "integration resolution commit does not match the workspace HEAD");
  const includesTarget = await isAncestor(
    runtime,
    current.value.worktree.directory,
    current.value.integration.expectedTargetHead,
    head.value,
  );
  const includesSource = await isAncestor(
    runtime,
    current.value.worktree.directory,
    current.value.integration.expectedSourceHead,
    head.value,
  );
  if (!includesTarget || !includesSource)
    return fail("conflict", "resolution commit does not conserve both integration histories");
  const next: IntegrationAttempt = {
    ...current.value.integration,
    candidateCommit: head.value,
    conflictPaths: [],
    state: "candidate",
    revision: nextRevision(current.value.integration.revision),
  };
  const nextWorktree: ProtectedWorktree = {
    ...current.value.worktree,
    record: {
      ...current.value.worktree.record,
      headCommit: head.value,
      state: "ready",
      revision: nextRevision(current.value.worktree.record.revision),
    },
  };
  return runtime.store.putWorktree(nextWorktree) && runtime.store.putIntegration(next)
    ? { ok: true, value: next }
    : fail("unavailable", "integration resolution could not be persisted");
}

export async function runIntegrationTest(
  runtime: RepositoryWorkspaceRuntime,
  raw: RunIntegrationTestRequest,
): Promise<RepositoryWorkspaceResult<IntegrationAttempt>> {
  const parsed = RunIntegrationTestRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "integration test request is invalid");
  const input = parsed.data;
  const current = currentIntegration(
    runtime,
    input.integrationId,
    input.executionHostId,
    input.expectedRevision,
  );
  if (!current.ok) return current;
  const candidate = current.value.integration.candidateCommit;
  if (candidate === null || !["candidate", "tested"].includes(current.value.integration.state))
    return fail("not_ready", "integration has no candidate to test");
  const head = await headOf(runtime, current.value.worktree.directory);
  if (!head.ok) return head;
  if (head.value !== candidate) return fail("stale", "integration candidate workspace changed");
  const cleanBefore = await requireClean(runtime, current.value.worktree.directory);
  if (!cleanBefore.ok) return cleanBefore;
  const result = await runtime.testRunner.run({
    integrationId: input.integrationId,
    repositoryId: current.value.integration.repositoryId,
    planId: current.value.integration.planId,
    commit: candidate,
    profileId: input.profileId,
    projectDirectory: current.value.worktree.projectDirectory,
  });
  if (!result.ok) return result;
  const cleanAfter = await requireClean(runtime, current.value.worktree.directory);
  if (!cleanAfter.ok) return cleanAfter;
  const headAfter = await headOf(runtime, current.value.worktree.directory);
  if (!headAfter.ok) return headAfter;
  if (headAfter.value !== candidate)
    return fail("stale", "integration test changed the candidate commit");
  const next: IntegrationAttempt = {
    ...current.value.integration,
    state: result.value.passed ? "tested" : "candidate",
    testEvidence: {
      profileId: input.profileId,
      runnerId: result.value.runnerId,
      runnerAuthorityId: result.value.runnerAuthorityId,
      requestedByPrincipalId: input.principalId,
      commit: candidate,
      passed: result.value.passed,
      summary: result.value.summary,
      observedAt: runtime.now(),
    },
    review: null,
    revision: nextRevision(current.value.integration.revision),
  };
  return runtime.store.putIntegration(next)
    ? { ok: true, value: next }
    : fail("unavailable", "integration test evidence could not be persisted");
}

export function recordIntegrationReview(
  runtime: RepositoryWorkspaceRuntime,
  raw: RecordIntegrationReviewRequest,
): RepositoryWorkspaceResult<IntegrationAttempt> {
  const parsed = RecordIntegrationReviewRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "integration review is invalid");
  const input = parsed.data;
  const current = currentIntegration(
    runtime,
    input.integrationId,
    input.executionHostId,
    input.expectedRevision,
  );
  if (!current.ok) return current;
  const candidate = current.value.integration.candidateCommit;
  if (candidate === null) return fail("not_ready", "integration has no candidate to review");
  if (
    input.accepted &&
    (current.value.integration.testEvidence?.passed !== true ||
      current.value.integration.testEvidence.commit !== candidate)
  )
    return fail("not_ready", "acceptance requires trusted test evidence for the exact candidate");
  const next: IntegrationAttempt = {
    ...current.value.integration,
    state: input.accepted ? "accepted" : "rejected",
    review: {
      commit: candidate,
      accepted: input.accepted,
      reviewerPrincipalId: input.principalId,
      rationale: input.rationale,
      observedAt: runtime.now(),
    },
    revision: nextRevision(current.value.integration.revision),
  };
  return runtime.store.putIntegration(next)
    ? { ok: true, value: next }
    : fail("unavailable", "integration review could not be persisted");
}

async function validateIntegrationBasis(
  runtime: RepositoryWorkspaceRuntime,
  input: PrepareIntegrationRequest,
) {
  const source = runtime.store.getWorktree(input.sourceWorktreeId);
  const target = runtime.store.getWorktree(input.targetWorktreeId);
  if (
    source === null ||
    target === null ||
    source.record.state !== "ready" ||
    target.record.state !== "ready"
  )
    return fail("not_ready", "source and target workspaces must be ready");
  if (
    source.record.executionHostId !== runtime.executionHostId ||
    target.record.executionHostId !== runtime.executionHostId
  )
    return fail("host_unavailable", "cross-host integration is not supported");
  if (source.record.repositoryId !== target.record.repositoryId)
    return fail("invalid_input", "cross-repository integration is not supported");
  const plan = runtime.store.getPlan(input.planId);
  if (
    plan === null ||
    plan.repositoryId !== source.record.repositoryId ||
    source.record.planId !== input.planId ||
    ![plan.rootWorktreeId, plan.integrationTargetWorktreeId].includes(target.record.worktreeId)
  )
    return fail(
      "invalid_input",
      "integration source or target is outside the recorded plan boundary",
    );
  if (
    source.record.headCommit !== input.expectedSourceHead ||
    target.record.headCommit !== input.expectedTargetHead
  )
    return fail("stale", "integration basis changed");
  const sourceHead = await headOf(runtime, source.directory);
  const targetHead = await headOf(runtime, target.directory);
  if (!sourceHead.ok || !targetHead.ok)
    return fail("unavailable", "integration basis cannot be verified");
  if (
    sourceHead.value !== input.expectedSourceHead ||
    targetHead.value !== input.expectedTargetHead
  )
    return fail("stale", "integration basis Git HEAD changed");
  const sourceClean = await requireClean(runtime, source.directory);
  const targetClean = await requireClean(runtime, target.directory);
  if (!sourceClean.ok || !targetClean.ok)
    return fail("dirty", "source or target workspace is dirty");
  const repository = runtime.store.getRepository(source.record.repositoryId);
  const binding = runtime.store.getBinding(target.record.projectId);
  return repository !== null && binding !== null
    ? { ok: true as const, value: { source, target, repository, binding } }
    : fail("unavailable", "integration repository binding is unavailable");
}

async function createIntegrationWorktree(
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
  try {
    const directory = realpathSync(worktree.directory);
    const projectDirectory = realpathSync(resolve(directory, binding.projectRelativePath));
    if (!contains(runtime.worktreeRoot, directory) || !contains(directory, projectDirectory))
      return fail("unavailable", "integration workspace escaped its trusted root");
    const common = await gitText(runtime, directory, [
      "rev-parse",
      "--path-format=absolute",
      "--git-common-dir",
    ]);
    const branch = await gitText(runtime, directory, ["rev-parse", "--abbrev-ref", "HEAD"]);
    if (!common.ok || !branch.ok)
      return fail("unavailable", "integration workspace Git identity is unavailable");
    if (
      realpathSync(common.value) !== repository.commonDirectory ||
      branch.value !== worktree.record.branchRef
    )
      return fail("conflict", "integration workspace Git identity changed");
    return { ok: true, value: { ...worktree, directory, projectDirectory } };
  } catch {
    return fail("unavailable", "integration workspace cannot be verified");
  }
}

async function mergeCandidate(
  runtime: RepositoryWorkspaceRuntime,
  worktree: ProtectedWorktree,
  integration: IntegrationAttempt,
): Promise<
  RepositoryWorkspaceResult<{ worktree: ProtectedWorktree; integration: IntegrationAttempt }>
> {
  const hooks = resolve(runtime.worktreeRoot, ".empty-hooks");
  mkdirSync(hooks, { recursive: true });
  const existingConflicts = await conflictPaths(runtime, worktree.directory);
  if (!existingConflicts.ok) return existingConflicts;
  if (existingConflicts.value.length > 0) {
    const nextWorktree = {
      ...worktree,
      record: {
        ...worktree.record,
        state: "conflicted" as const,
        revision: nextRevision(worktree.record.revision),
      },
    };
    const nextIntegration = {
      ...integration,
      conflictPaths: existingConflicts.value,
      state: "conflicted" as const,
      revision: nextRevision(integration.revision),
    };
    return { ok: true, value: { worktree: nextWorktree, integration: nextIntegration } };
  }
  let mergeHead = await runtime.git.run({
    cwd: worktree.directory,
    args: ["rev-parse", "-q", "--verify", "MERGE_HEAD"],
  });
  const currentHead = await headOf(runtime, worktree.directory);
  if (!currentHead.ok) return currentHead;
  if (mergeHead.exitCode !== 0 && currentHead.value === integration.expectedTargetHead) {
    const merge = await runtime.git.run({
      cwd: worktree.directory,
      args: [
        "-c",
        `core.hooksPath=${hooks}`,
        "merge",
        "--no-commit",
        "--no-ff",
        integration.expectedSourceHead,
      ],
    });
    if (merge.exitCode !== 0) {
      const conflicts = await conflictPaths(runtime, worktree.directory);
      if (!conflicts.ok || conflicts.value.length === 0)
        return fail("unavailable", merge.stderr.trim().slice(0, 1_000) || "Git merge failed");
      const nextWorktree = {
        ...worktree,
        record: {
          ...worktree.record,
          state: "conflicted" as const,
          revision: nextRevision(worktree.record.revision),
        },
      };
      const nextIntegration = {
        ...integration,
        conflictPaths: conflicts.value,
        state: "conflicted" as const,
        revision: nextRevision(integration.revision),
      };
      return { ok: true, value: { worktree: nextWorktree, integration: nextIntegration } };
    }
    mergeHead = await runtime.git.run({
      cwd: worktree.directory,
      args: ["rev-parse", "-q", "--verify", "MERGE_HEAD"],
    });
  } else if (mergeHead.exitCode !== 0) {
    const includesSource = await isAncestor(
      runtime,
      worktree.directory,
      integration.expectedSourceHead,
      currentHead.value,
    );
    const includesTarget = await isAncestor(
      runtime,
      worktree.directory,
      integration.expectedTargetHead,
      currentHead.value,
    );
    if (!includesSource || !includesTarget)
      return fail("conflict", "integration workspace diverged from its durable intent");
  }
  if (mergeHead.exitCode === 0) {
    const identity = mergeIdentityEnvironment(runtime);
    if (!identity.ok) return identity;
    const committed = await gitText(
      runtime,
      worktree.directory,
      [
        "-c",
        `core.hooksPath=${hooks}`,
        "commit",
        "-m",
        `Integrate workspace ${integration.sourceWorktreeId}`,
      ],
      identity.value,
    );
    if (!committed.ok) return committed;
  }
  const head = await headOf(runtime, worktree.directory);
  if (!head.ok) return head;
  const nextWorktree = {
    ...worktree,
    record: {
      ...worktree.record,
      headCommit: head.value,
      state: "ready" as const,
      revision: nextRevision(worktree.record.revision),
    },
  };
  const nextIntegration = {
    ...integration,
    candidateCommit: head.value,
    state: "candidate" as const,
    revision: nextRevision(integration.revision),
  };
  return { ok: true, value: { worktree: nextWorktree, integration: nextIntegration } };
}

export function currentIntegration(
  runtime: RepositoryWorkspaceRuntime,
  id: string,
  hostId: string,
  revision: string,
) {
  const host = hostAvailable(runtime, hostId);
  if (!host.ok) return host;
  const integration = runtime.store.getIntegration(id);
  if (integration === null) return fail("unavailable", "integration is unavailable");
  if (integration.revision !== revision) return fail("conflict", "integration revision changed");
  const worktree = runtime.store.getWorktree(integration.integrationWorktreeId);
  return worktree === null
    ? fail("unavailable", "integration workspace is unavailable")
    : { ok: true as const, value: { integration, worktree } };
}

async function reconcileIntegration(
  runtime: RepositoryWorkspaceRuntime,
  input: PrepareIntegrationRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  integration: IntegrationAttempt,
) {
  if (
    integration.expectedSourceHead !== input.expectedSourceHead ||
    integration.expectedTargetHead !== input.expectedTargetHead
  )
    return fail("conflict", "integration identity is already bound to different commits");
  if (integration.state === "preparing") {
    const worktree = runtime.store.getWorktree(integration.integrationWorktreeId);
    const repository = runtime.store.getRepository(integration.repositoryId);
    const binding = worktree === null ? null : runtime.store.getBinding(worktree.record.projectId);
    if (worktree === null || repository === null || binding === null)
      return fail("unavailable", "pending integration intent is incomplete");
    const prepared = await createIntegrationWorktree(runtime, repository, binding, worktree);
    if (!prepared.ok) return prepared;
    const merged = await mergeCandidate(runtime, prepared.value, integration);
    if (!merged.ok) return merged;
    if (
      !runtime.store.putWorktree(merged.value.worktree) ||
      !runtime.store.putIntegration(merged.value.integration)
    )
      return fail("unavailable", "reconciled integration could not be persisted");
    return finishPrepare(runtime, input, scope, operation, merged.value.integration);
  }
  return finishPrepare(runtime, input, scope, operation, integration);
}

function finishPrepare(
  runtime: RepositoryWorkspaceRuntime,
  input: PrepareIntegrationRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  integration: IntegrationAttempt,
): RepositoryWorkspaceResult<IntegrationAttempt> {
  const identity = operationIdentity(operation);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "integration",
    integration.integrationId,
  );
  return completed.ok ? { ok: true, value: integration } : completed;
}

export async function headOf(runtime: RepositoryWorkspaceRuntime, cwd: string) {
  return gitText(runtime, cwd, ["rev-parse", "--verify", "HEAD"]);
}
async function status(runtime: RepositoryWorkspaceRuntime, cwd: string) {
  return gitText(runtime, cwd, ["status", "--porcelain=v1", "--untracked-files=all"]);
}
export async function requireClean(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
): Promise<RepositoryWorkspaceResult<null>> {
  const result = await status(runtime, cwd);
  if (!result.ok) return result;
  return result.value.length === 0
    ? { ok: true, value: null }
    : fail("dirty", "workspace has uncommitted or untracked changes");
}
async function conflictPaths(runtime: RepositoryWorkspaceRuntime, cwd: string) {
  const result = await gitText(runtime, cwd, ["diff", "--name-only", "--diff-filter=U"]);
  return result.ok
    ? { ok: true as const, value: result.value.split(/\r?\n/).filter((path) => path.length > 0) }
    : result;
}
async function isAncestor(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
  ancestor: string,
  descendant: string,
): Promise<boolean> {
  const result = await runtime.git.run({
    cwd,
    args: ["merge-base", "--is-ancestor", ancestor, descendant],
  });
  return result.exitCode === 0;
}
function publicIntegration(
  runtime: RepositoryWorkspaceRuntime,
  id: string,
): RepositoryWorkspaceResult<IntegrationAttempt> {
  const integration = runtime.store.getIntegration(id);
  return integration === null
    ? fail("unavailable", "completed integration is unavailable")
    : { ok: true, value: integration };
}
