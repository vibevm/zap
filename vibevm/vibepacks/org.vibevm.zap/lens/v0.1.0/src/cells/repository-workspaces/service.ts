/** Trusted repository-workspace service composition. @scope spec://org.vibevm.zap/lens/PROP-014#root */
import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import {
  AssignIntegrationWorktreeRequestSchema,
  type RepositoryWorkspaceResult,
  type RepositoryWorkspaceService,
  type ObserveWorktreeRequest,
  type ResolveAssignedWorkspaceRequest,
  type ResolveExecutionWorkspaceRequest,
  type ResolveIntegrationWorkspaceRequest,
  type TrustedExecutionWorkspace,
  type TrustedWorktreeObservation,
} from "./contracts.ts";
import { adoptRegisteredPlan } from "./adoption.ts";
import { readIntegrationDiff } from "./diff.ts";
import { registerRepository } from "./discovery.ts";
import {
  prepareIntegration,
  recordIntegrationReview,
  recordResolution,
  runIntegrationTest,
} from "./integration.ts";
import { promoteIntegration } from "./promotion.ts";
import {
  assignWorktree,
  prepareChildWorktree,
  preparePlanRoot,
  recordWorktreeHead,
  updateAlgorithmBinding,
} from "./preparation.ts";
import {
  contains,
  fail,
  gitText,
  hostAvailable,
  prepareRuntime,
  type RepositoryWorkspaceRuntime,
  type RepositoryWorkspaceRuntimeOptions,
} from "./runtime.ts";
import type { ProtectedWorktree } from "./store.ts";

export function createRepositoryWorkspaceService(
  options: RepositoryWorkspaceRuntimeOptions,
): RepositoryWorkspaceResult<RepositoryWorkspaceService> {
  const prepared = prepareRuntime(options);
  if (!prepared.ok) return prepared;
  const runtime = prepared.value;
  const serial = createSerialExecutor();
  return {
    ok: true,
    value: {
      registerRepository: (input) =>
        serial(`project:${input.projectId}`, () => registerRepository(runtime, input)),
      preparePlanRoot: (input) =>
        serial(`plan:${input.planId}`, () => preparePlanRoot(runtime, input)),
      adoptRegisteredPlan: (input) =>
        serial(`plan:${input.planId}`, () => adoptRegisteredPlan(runtime, input)),
      prepareChildWorktree: (input) =>
        serial(`plan:${input.planId}`, () => prepareChildWorktree(runtime, input)),
      updateAlgorithmBinding: (input) =>
        serial(`plan:${input.planId}`, () => updateAlgorithmBinding(runtime, input)),
      recordWorktreeHead: (input) =>
        serial(`worktree:${input.worktreeId}`, () => recordWorktreeHead(runtime, input)),
      assignWorktree: (input) =>
        serial(`worktree:${input.worktreeId}`, () => assignWorktree(runtime, input)),
      assignIntegrationWorktree: (raw) =>
        serial(`integration:${raw.integrationId}:assign`, () => {
          const parsed = AssignIntegrationWorktreeRequestSchema.safeParse(raw);
          if (!parsed.success) return fail("invalid_input", "integration assignment is invalid");
          const input = parsed.data;
          const integration = runtime.store.getIntegration(input.integrationId);
          if (
            integration === null ||
            integration.revision !== input.expectedIntegrationRevision ||
            integration.integrationWorktreeId !== input.worktreeId ||
            integration.state !== "conflicted" ||
            integration.conflictPaths.length === 0
          )
            return fail("conflict", "integration conflict assignment basis changed");
          return assignWorktree(runtime, input, true);
        }),
      prepareIntegration: (input) =>
        serial(`integration:${input.integrationId}`, () => prepareIntegration(runtime, input)),
      recordResolution: (input) =>
        serial(`integration:${input.integrationId}`, () => recordResolution(runtime, input)),
      runIntegrationTest: (input) =>
        serial(`integration:${input.integrationId}`, () => runIntegrationTest(runtime, input)),
      recordIntegrationReview: (input) =>
        serial(`integration:${input.integrationId}`, () => recordIntegrationReview(runtime, input)),
      promoteIntegration: (input) =>
        serial(`integration:${input.integrationId}`, () => promoteIntegration(runtime, input)),
      getRepository(repositoryId) {
        const repository = runtime.store.getRepository(repositoryId);
        return repository === null
          ? fail("unavailable", "repository is unavailable")
          : { ok: true, value: repository.record };
      },
      getPlan(planId) {
        const plan = runtime.store.getPlan(planId);
        return plan === null
          ? fail("unavailable", "project plan is unavailable")
          : { ok: true, value: plan };
      },
      getWorktree(worktreeId) {
        const worktree = runtime.store.getWorktree(worktreeId);
        return worktree === null
          ? fail("unavailable", "repository workspace is unavailable")
          : { ok: true, value: worktree.record };
      },
      getIntegration(integrationId) {
        const integration = runtime.store.getIntegration(integrationId);
        return integration === null
          ? fail("unavailable", "integration is unavailable")
          : { ok: true, value: integration };
      },
      listPlans: (projectId) => ({ ok: true, value: runtime.store.listPlans(projectId) }),
      listWorktrees: (planId) => ({
        ok: true,
        value: runtime.store.listWorktrees(planId).map((entry) => entry.record),
      }),
      listIntegrations: (planId) => ({
        ok: true,
        value: runtime.store.listIntegrations(planId),
      }),
      resolveExecutionWorkspace: (input) =>
        serial(`worktree:${input.worktreeId}:resolve`, () =>
          resolveExecutionWorkspace(runtime, input),
        ),
      resolveAssignedWorkspace: (input) =>
        serial(`worktree:${input.worktreeId}:resolve`, () =>
          resolveAssignedWorkspace(runtime, input),
        ),
      resolveIntegrationWorkspace: (input) =>
        serial(`integration:${input.integrationId}:resolve`, () =>
          resolveIntegrationWorkspace(runtime, input),
        ),
      observeWorktree: (input) =>
        serial(`worktree:${input.worktreeId}:observe`, () => observeWorktree(runtime, input)),
      readIntegrationDiff: (input) =>
        serial(`integration:${input.integrationId}:diff`, () =>
          readIntegrationDiff(runtime, input),
        ),
      close() {
        runtime.store.close();
      },
    },
  };
}

async function observeWorktree(
  runtime: RepositoryWorkspaceRuntime,
  input: ObserveWorktreeRequest,
): Promise<RepositoryWorkspaceResult<TrustedWorktreeObservation>> {
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (
    worktree === null ||
    worktree.record.projectId !== input.projectId ||
    worktree.record.contextId !== input.contextId
  )
    return fail("invalid_input", "workspace is outside the requested project context");
  const observed = await verifyWorkspace(runtime, worktree, null);
  return observed.ok
    ? {
        ok: true,
        value: {
          worktree: observed.value.worktree,
          currentHead: observed.value.verifiedHead,
          dirty: observed.value.dirty,
        },
      }
    : observed;
}

async function resolveExecutionWorkspace(
  runtime: RepositoryWorkspaceRuntime,
  input: ResolveExecutionWorkspaceRequest,
): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>> {
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (
    worktree === null ||
    worktree.record.projectId !== input.projectId ||
    worktree.record.contextId !== input.contextId
  )
    return fail("invalid_input", "workspace is not bound to the requested project context");
  if (worktree.record.state !== "ready") return fail("not_ready", "workspace is not ready");
  if (input.expectedRevision !== undefined && worktree.record.revision !== input.expectedRevision)
    return fail("conflict", "workspace revision changed");
  return verifyWorkspace(runtime, worktree, worktree.record.headCommit);
}

async function resolveAssignedWorkspace(
  runtime: RepositoryWorkspaceRuntime,
  input: ResolveAssignedWorkspaceRequest,
): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>> {
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (
    worktree === null ||
    worktree.record.projectId !== input.projectId ||
    worktree.record.contextId !== input.contextId
  )
    return fail("invalid_input", "assigned workspace is outside the requested project context");
  const assignment = worktree.record.assignments.find(
    (candidate) =>
      candidate.assignmentId === input.assignmentId &&
      candidate.attemptId === input.attemptId &&
      candidate.releasedAt === null,
  );
  if (assignment === undefined || assignment.basisCommit !== input.expectedInitialHead)
    return fail("denied", "active workspace assignment does not match the work attempt");
  const allowedState =
    worktree.record.state === "ready" ||
    (worktree.record.kind === "integration" && worktree.record.state === "conflicted");
  if (!allowedState) return fail("not_ready", "assigned workspace is not usable");
  return verifyWorkspace(
    runtime,
    worktree,
    input.mode === "initial" ? input.expectedInitialHead : null,
  );
}

async function resolveIntegrationWorkspace(
  runtime: RepositoryWorkspaceRuntime,
  input: ResolveIntegrationWorkspaceRequest,
): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>> {
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const integration = runtime.store.getIntegration(input.integrationId);
  const worktree = runtime.store.getWorktree(input.worktreeId);
  if (
    integration === null ||
    worktree === null ||
    integration.integrationWorktreeId !== input.worktreeId ||
    integration.state !== "conflicted" ||
    integration.conflictPaths.length === 0 ||
    worktree.record.kind !== "integration" ||
    worktree.record.state !== "conflicted" ||
    worktree.record.projectId !== input.projectId ||
    worktree.record.contextId !== input.contextId ||
    worktree.record.revision !== input.expectedRevision
  )
    return fail("denied", "integration conflict workspace is not authorized");
  const mergeHead = await gitText(runtime, worktree.directory, [
    "rev-parse",
    "-q",
    "--verify",
    "MERGE_HEAD",
  ]);
  const conflicts = await gitText(runtime, worktree.directory, [
    "diff",
    "--name-only",
    "--diff-filter=U",
  ]);
  if (
    !mergeHead.ok ||
    mergeHead.value !== integration.expectedSourceHead ||
    !conflicts.ok ||
    conflicts.value.length === 0
  )
    return fail("conflict", "integration conflict state no longer matches its durable record");
  return verifyWorkspace(runtime, worktree, integration.expectedTargetHead);
}

async function verifyWorkspace(
  runtime: RepositoryWorkspaceRuntime,
  worktree: ProtectedWorktree,
  expectedHead: string | null,
): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>> {
  const repository = runtime.store.getRepository(worktree.record.repositoryId);
  const binding = runtime.store.getBinding(worktree.record.projectId);
  if (repository === null || binding === null)
    return fail("unavailable", "workspace repository binding is unavailable");
  try {
    const directory = realpathSync(worktree.directory);
    const projectCwd = realpathSync(resolve(directory, binding.projectRelativePath));
    if (!contains(directory, projectCwd))
      return fail("unavailable", "workspace project path escaped its repository worktree");
    const common = await gitText(runtime, directory, [
      "rev-parse",
      "--path-format=absolute",
      "--git-common-dir",
    ]);
    if (!common.ok) return common;
    if (realpathSync(common.value) !== repository.commonDirectory)
      return fail("conflict", "workspace repository binding changed");
    const branch = await gitText(runtime, directory, ["rev-parse", "--abbrev-ref", "HEAD"]);
    if (!branch.ok) return branch;
    if (branch.value !== worktree.record.branchRef)
      return fail("conflict", "workspace branch changed from its durable binding");
    const head = await gitText(runtime, directory, ["rev-parse", "--verify", "HEAD"]);
    if (!head.ok) return head;
    if (expectedHead !== null && head.value !== expectedHead)
      return fail("stale", "workspace HEAD changed from its durable binding");
    const status = await gitText(runtime, directory, [
      "status",
      "--porcelain=v1",
      "--untracked-files=all",
    ]);
    if (!status.ok) return status;
    return {
      ok: true,
      value: {
        worktree: worktree.record,
        projectCwd,
        verifiedHead: head.value,
        dirty: status.value.length > 0,
      },
    };
  } catch {
    return fail("unavailable", "workspace paths cannot be resolved");
  }
}

function createSerialExecutor() {
  const tails = new Map<string, Promise<void>>();
  return async <T>(key: string, action: () => T | Promise<T>): Promise<T> => {
    const found = tails.get(key);
    const prior = found === undefined ? Promise.resolve() : found;
    let release = (): void => undefined;
    const current = new Promise<void>((resolveCurrent) => {
      release = resolveCurrent;
    });
    const tail = prior.then(() => current);
    tails.set(key, tail);
    await prior;
    try {
      return await action();
    } finally {
      release();
      if (tails.get(key) === tail) tails.delete(key);
    }
  };
}
