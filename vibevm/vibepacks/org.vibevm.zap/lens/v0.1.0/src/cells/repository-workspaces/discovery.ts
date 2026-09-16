/** Trusted repository discovery and registration. @scope spec://org.vibevm.zap/lens/PROP-014#root */
import { realpathSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
import {
  RegisterRepositoryRequestSchema,
  type RegisterRepositoryRequest,
  type RegisteredRepository,
  type RepositoryWorkspaceResult,
} from "./contracts.ts";
import { beginOperation, completeOperation, operationIdentity } from "./operations.ts";
import {
  contains,
  fail,
  gitText,
  hostAvailable,
  type RepositoryWorkspaceRuntime,
} from "./runtime.ts";
import type { ProtectedBinding, ProtectedRepository, ProtectedWorktree } from "./store.ts";

export async function registerRepository(
  runtime: RepositoryWorkspaceRuntime,
  raw: RegisterRepositoryRequest,
): Promise<RepositoryWorkspaceResult<RegisteredRepository>> {
  const parsed = RegisterRepositoryRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "repository registration is invalid");
  const input = parsed.data;
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  const scope = `project:${input.projectId}:repository`;
  const operation = beginOperation(runtime, input, scope, input, {
    kind: "repository",
    id: runtime.idFactory.create("repository"),
  });
  if (!operation.ok) return operation;
  if (operation.value.state === "complete") return loadRegistered(runtime, input.projectId);

  const discovered = await discover(runtime, input.trustedProjectCwd);
  if (!discovered.ok) return discovered;
  const existingBinding = runtime.store.getBinding(input.projectId);
  if (existingBinding !== null) {
    if (
      existingBinding.record.executionHostId !== runtime.executionHostId ||
      existingBinding.projectDirectory !== discovered.value.projectDirectory
    )
      return fail("conflict", "project is already bound to a different repository location");
    return finishExisting(runtime, input, scope, operation.value, existingBinding);
  }

  const priorRepository = runtime.store.findRepositoryByHostPath(
    runtime.executionHostId,
    discovered.value.commonDirectory,
  );
  const reservedRepositoryId =
    operation.value.state === "new" ? operation.value.resultId : operation.value.operation.resultId;
  if (reservedRepositoryId === null)
    return fail("unavailable", "repository operation reservation is invalid");
  const createdAt = runtime.now();
  const repository: ProtectedRepository = priorRepository ?? {
    record: {
      repositoryId: reservedRepositoryId,
      executionHostId: runtime.executionHostId,
      displayName: input.displayName,
      objectFormat: discovered.value.objectFormat,
      revision: "1",
      createdAt,
    },
    commonDirectory: discovered.value.commonDirectory,
    topLevel: discovered.value.topLevel,
  };
  if (priorRepository === null && !runtime.store.putRepository(repository))
    return fail("unavailable", "repository identity could not be persisted");
  const worktreeId = runtime.idFactory.create("worktree");
  const worktree: ProtectedWorktree = {
    record: {
      worktreeId,
      repositoryId: repository.record.repositoryId,
      executionHostId: runtime.executionHostId,
      projectId: input.projectId,
      contextId: input.contextId,
      planId: null,
      kind: "registered",
      parentWorktreeId: null,
      branchRef: discovered.value.branchRef,
      basisCommit: discovered.value.head,
      headCommit: discovered.value.head,
      state: "ready",
      assignments: [],
      revision: "1",
      createdAt,
    },
    directory: discovered.value.topLevel,
    projectDirectory: discovered.value.projectDirectory,
  };
  const binding: ProtectedBinding = {
    record: {
      repositoryId: repository.record.repositoryId,
      executionHostId: runtime.executionHostId,
      projectId: input.projectId,
      registeredWorktreeId: worktreeId,
      creatorPrincipalId: input.principalId,
      revision: "1",
    },
    projectDirectory: discovered.value.projectDirectory,
    projectRelativePath: discovered.value.projectRelativePath,
  };
  if (!runtime.store.putWorktree(worktree) || !runtime.store.putBinding(binding))
    return fail("unavailable", "repository project binding could not be persisted");
  const identity = operationIdentity(operation.value);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "repository",
    repository.record.repositoryId,
  );
  return completed.ok
    ? { ok: true, value: publicRegistration(repository, binding, worktree) }
    : completed;
}

interface Discovery {
  readonly commonDirectory: string;
  readonly topLevel: string;
  readonly projectDirectory: string;
  readonly projectRelativePath: string;
  readonly objectFormat: "sha1" | "sha256";
  readonly branchRef: string;
  readonly head: string;
}

async function discover(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
): Promise<RepositoryWorkspaceResult<Discovery>> {
  let projectDirectory: string;
  try {
    projectDirectory = realpathSync(cwd);
  } catch {
    return fail("unavailable", "registered project directory is unavailable");
  }
  const top = await gitText(runtime, projectDirectory, [
    "rev-parse",
    "--path-format=absolute",
    "--show-toplevel",
  ]);
  const common = await gitText(runtime, projectDirectory, [
    "rev-parse",
    "--path-format=absolute",
    "--git-common-dir",
  ]);
  const format = await gitText(runtime, projectDirectory, ["rev-parse", "--show-object-format"]);
  const branch = await gitText(runtime, projectDirectory, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const head = await gitText(runtime, projectDirectory, ["rev-parse", "--verify", "HEAD"]);
  if (!top.ok || !common.ok || !format.ok || !branch.ok || !head.ok)
    return firstFailure(top, common, format, branch, head);
  if (format.value !== "sha1" && format.value !== "sha256")
    return fail("unavailable", "repository object format is unsupported");
  try {
    const topLevel = realpathSync(top.value);
    const commonPath = isAbsolute(common.value)
      ? common.value
      : resolve(projectDirectory, common.value);
    const commonDirectory = realpathSync(commonPath);
    if (!contains(topLevel, projectDirectory))
      return fail("invalid_input", "project directory is outside the repository top level");
    return {
      ok: true,
      value: {
        commonDirectory,
        topLevel,
        projectDirectory,
        projectRelativePath: relative(topLevel, projectDirectory),
        objectFormat: format.value,
        branchRef: branch.value,
        head: head.value,
      },
    };
  } catch {
    return fail("unavailable", "repository paths could not be canonicalized");
  }
}

function loadRegistered(
  runtime: RepositoryWorkspaceRuntime,
  projectId: string,
): RepositoryWorkspaceResult<RegisteredRepository> {
  const binding = runtime.store.getBinding(projectId);
  const repository =
    binding === null ? null : runtime.store.getRepository(binding.record.repositoryId);
  const worktree =
    binding === null ? null : runtime.store.getWorktree(binding.record.registeredWorktreeId);
  return binding !== null && repository !== null && worktree !== null
    ? { ok: true, value: publicRegistration(repository, binding, worktree) }
    : fail("unavailable", "completed repository registration is unavailable");
}

function finishExisting(
  runtime: RepositoryWorkspaceRuntime,
  input: RegisterRepositoryRequest,
  scope: string,
  operation: Parameters<typeof operationIdentity>[0],
  binding: ProtectedBinding,
): RepositoryWorkspaceResult<RegisteredRepository> {
  const identity = operationIdentity(operation);
  const completed = completeOperation(
    runtime,
    input,
    scope,
    identity.digest,
    identity.createdAt,
    "repository",
    binding.record.repositoryId,
  );
  return completed.ok ? loadRegistered(runtime, input.projectId) : completed;
}

function publicRegistration(
  repository: ProtectedRepository,
  binding: ProtectedBinding,
  worktree: ProtectedWorktree,
): RegisteredRepository {
  return { repository: repository.record, binding: binding.record, worktree: worktree.record };
}

function firstFailure(
  ...results: readonly RepositoryWorkspaceResult<string>[]
): RepositoryWorkspaceResult<never> {
  for (const result of results) if (!result.ok) return result;
  return fail("unavailable", "repository discovery failed");
}
