/** Bounded exact-candidate integration diff. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type {
  IntegrationDiff,
  IntegrationDiffRequest,
  RepositoryWorkspaceResult,
} from "./contracts.ts";
import { fail, gitText, hostAvailable, type RepositoryWorkspaceRuntime } from "./runtime.ts";

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
  const files = await gitText(runtime, worktree.directory, [
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--name-only",
    "-z",
    `${integration.expectedTargetHead}..${integration.candidateCommit}`,
    "--",
  ]);
  if (!files.ok) return files;
  const diff = await gitText(runtime, worktree.directory, [
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--unified=3",
    `${integration.expectedTargetHead}..${integration.candidateCommit}`,
    "--",
  ]);
  if (!diff.ok) return diff;
  const bytes = Buffer.from(diff.value, "utf8");
  const truncated = bytes.length > input.maximumBytes;
  return {
    ok: true,
    value: {
      targetCommit: integration.expectedTargetHead,
      candidateCommit: integration.candidateCommit,
      changedFiles: files.value
        .split("\0")
        .filter((path) => path.length > 0)
        .slice(0, 2_048),
      unifiedText: truncated ? bytes.subarray(0, input.maximumBytes).toString("utf8") : diff.value,
      truncated:
        truncated || files.value.split("\0").filter((path) => path.length > 0).length > 2_048,
    },
  };
}
