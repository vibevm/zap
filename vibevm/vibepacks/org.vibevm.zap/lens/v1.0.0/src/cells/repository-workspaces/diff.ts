/** Bounded exact-candidate integration diff. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type {
  IntegrationDiff,
  IntegrationDiffRequest,
  RepositoryWorkspaceResult,
} from "./contracts.ts";
import { fail, hostAvailable, type RepositoryWorkspaceRuntime } from "./runtime.ts";

export async function readIntegrationDiff(
  runtime: RepositoryWorkspaceRuntime,
  input: IntegrationDiffRequest,
): Promise<RepositoryWorkspaceResult<IntegrationDiff>> {
  const host = hostAvailable(runtime, input.executionHostId);
  if (!host.ok) return host;
  if (
    !Number.isSafeInteger(input.maximumBytes) ||
    input.maximumBytes < 1_024 ||
    input.maximumBytes > 1_000_000
  )
    return fail("invalid_input", "integration diff byte bound is invalid");
  const integration = runtime.store.getIntegration(input.integrationId);
  if (integration === null || integration.candidateCommit === null)
    return fail("not_ready", "integration has no reviewable candidate");
  const worktree = runtime.store.getWorktree(integration.integrationWorktreeId);
  if (
    worktree === null ||
    worktree.record.repositoryId !== integration.repositoryId ||
    worktree.record.projectId !== input.projectId ||
    worktree.record.contextId !== input.contextId
  )
    return fail("unavailable", "integration workspace is unavailable");
  const files = await runtime.git.run({
    cwd: worktree.directory,
    args: [
      "diff",
      "--no-ext-diff",
      "--no-textconv",
      "--name-only",
      "-z",
      `${integration.expectedTargetHead}..${integration.candidateCommit}`,
      "--",
    ],
  });
  if (files.exitCode !== 0) return fail("unavailable", "integration file diff is unavailable");
  const diff = await runtime.git.run({
    cwd: worktree.directory,
    args: [
      "diff",
      "--no-ext-diff",
      "--no-textconv",
      "--unified=3",
      `${integration.expectedTargetHead}..${integration.candidateCommit}`,
      "--",
    ],
  });
  if (diff.exitCode !== 0) return fail("unavailable", "integration unified diff is unavailable");
  const changedFiles = files.stdout.split("\0").filter((path) => path.length > 0);
  const bytes = Buffer.from(diff.stdout, "utf8");
  const truncated = bytes.length > input.maximumBytes;
  return {
    ok: true,
    value: {
      targetCommit: integration.expectedTargetHead,
      candidateCommit: integration.candidateCommit,
      changedFiles: changedFiles.slice(0, 2_048),
      unifiedText: truncated ? bytes.subarray(0, input.maximumBytes).toString("utf8") : diff.stdout,
      truncated: truncated || changedFiles.length > 2_048,
    },
  };
}
