/** Checked fast-forward promotion. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import type { IntegrationAttempt } from "../repository-model/index.ts";
import {
  PromoteIntegrationRequestSchema,
  type PromoteIntegrationRequest,
  type RepositoryWorkspaceResult,
} from "./contracts.ts";
import { currentIntegration, headOf, requireClean } from "./integration.ts";
import {
  contains,
  fail,
  gitText,
  nextRevision,
  type RepositoryWorkspaceRuntime,
} from "./runtime.ts";
import type { ProtectedWorktree } from "./store.ts";
export async function promoteIntegration(
  runtime: RepositoryWorkspaceRuntime,
  raw: PromoteIntegrationRequest,
): Promise<RepositoryWorkspaceResult<IntegrationAttempt>> {
  const parsed = PromoteIntegrationRequestSchema.safeParse(raw);
  if (!parsed.success) return fail("invalid_input", "integration promotion is invalid");
  const input = parsed.data;
  const current = currentIntegration(
    runtime,
    input.integrationId,
    input.executionHostId,
    input.expectedRevision,
  );
  if (!current.ok) return current;
  const integration = current.value.integration;
  const candidate = integration.candidateCommit;
  if (
    integration.state !== "accepted" ||
    candidate === null ||
    integration.review?.accepted !== true
  )
    return fail("not_ready", "accepted semantic review is required before promotion");
  const target = runtime.store.getWorktree(integration.targetWorktreeId);
  if (target === null || target.record.repositoryId !== integration.repositoryId)
    return fail("unavailable", "integration target workspace is unavailable");
  const lease = await runtime.writerGate.acquire({
    repositoryId: integration.repositoryId,
    executionHostId: runtime.executionHostId,
    planId: integration.planId,
    targetWorktreeId: target.record.worktreeId,
    expectedHead: integration.expectedTargetHead,
    operationId: integration.integrationId,
  });
  if (!lease.ok) return lease;
  try {
    const validated = await runtime.writerGate.validate(lease.value);
    if (!validated.ok) return validated;
    const actualTarget = await headOf(runtime, target.directory);
    if (!actualTarget.ok) return actualTarget;
    if (actualTarget.value === candidate)
      return reconcilePromoted(runtime, integration, target, candidate);
    if (actualTarget.value !== integration.expectedTargetHead)
      return markStale(runtime, integration, "target HEAD changed before promotion");
    const clean = await requireClean(runtime, target.directory);
    if (!clean.ok) return clean;
    const ignored = await ignoredOverwrite(
      runtime,
      target.directory,
      integration.expectedTargetHead,
      candidate,
    );
    if (!ignored.ok) return ignored;
    const promoted = await gitText(runtime, target.directory, [
      "merge",
      "--ff-only",
      "--no-overwrite-ignore",
      candidate,
    ]);
    if (!promoted.ok) return promoted;
    const finalHead = await headOf(runtime, target.directory);
    if (!finalHead.ok || finalHead.value !== candidate)
      return fail("unavailable", "promoted target HEAD could not be verified");
    const next: IntegrationAttempt = {
      ...integration,
      state: "promoted",
      revision: nextRevision(integration.revision),
    };
    const targetNext: ProtectedWorktree = {
      ...target,
      record: {
        ...target.record,
        headCommit: candidate,
        revision: nextRevision(target.record.revision),
      },
    };
    return runtime.store.putWorktree(targetNext) && runtime.store.putIntegration(next)
      ? { ok: true, value: next }
      : fail("unavailable", "promoted integration could not be persisted");
  } finally {
    await runtime.writerGate.release(lease.value);
  }
}

async function ignoredOverwrite(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
  base: string,
  candidate: string,
): Promise<RepositoryWorkspaceResult<null>> {
  const changed = await gitText(runtime, cwd, ["diff", "--name-only", "-z", base, candidate]);
  if (!changed.ok) return changed;
  for (const path of changed.value.split("\0").filter((item) => item.length > 0)) {
    const absolute = resolve(cwd, path);
    if (!contains(cwd, absolute)) return fail("conflict", "candidate contains an unsafe path");
    if (!existsSync(absolute)) continue;
    const ignored = await runtime.git.run({ cwd, args: ["check-ignore", "--quiet", "--", path] });
    if (ignored.exitCode === 0)
      return fail("dirty", "candidate would overwrite an ignored target path");
  }
  return { ok: true, value: null };
}
function markStale(
  runtime: RepositoryWorkspaceRuntime,
  integration: IntegrationAttempt,
  message: string,
): RepositoryWorkspaceResult<never> {
  runtime.store.putIntegration({
    ...integration,
    state: "stale",
    revision: nextRevision(integration.revision),
  });
  return fail("stale", message);
}
function reconcilePromoted(
  runtime: RepositoryWorkspaceRuntime,
  integration: IntegrationAttempt,
  target: ProtectedWorktree,
  candidate: string,
): RepositoryWorkspaceResult<IntegrationAttempt> {
  const next = {
    ...integration,
    state: "promoted" as const,
    revision: nextRevision(integration.revision),
  };
  const targetNext = {
    ...target,
    record: {
      ...target.record,
      headCommit: candidate,
      revision: nextRevision(target.record.revision),
    },
  };
  return runtime.store.putWorktree(targetNext) && runtime.store.putIntegration(next)
    ? { ok: true, value: next }
    : fail("unavailable", "promoted integration receipt could not be reconciled");
}
