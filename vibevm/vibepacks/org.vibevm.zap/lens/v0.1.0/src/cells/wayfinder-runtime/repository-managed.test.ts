import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { relative, resolve, sep } from "node:path";
import test from "node:test";
import {
  createGitArgvAdapter,
  createLocalIdleWriterGate,
  createRepositoryWorkspaceService,
  openRepositoryWorkspaceStore,
} from "../repository-workspaces/index.ts";
import { ManagedWorkRequestSchema } from "../managed-work/index.ts";
import { WorkspaceAccessContextSchema } from "../workspace-model/index.ts";
import { createRepositoryManagedWorkspaceProvisioningPort } from "./repository-managed.ts";

test("managed isolated work is assigned and resolves its exact child cwd before launch", async () => {
  const tempParent = realpathSync(tmpdir());
  const fixtureRoot = mkdtempSync(resolve(tempParent, "zap-managed-worktree-"));
  const repositoryRoot = resolve(fixtureRoot, "repository");
  const projectCwd = resolve(repositoryRoot, "apps", "demo");
  const worktreeRoot = resolve(fixtureRoot, "worktrees");
  mkdirSync(projectCwd, { recursive: true });
  mkdirSync(worktreeRoot, { recursive: true });
  writeFileSync(resolve(projectCwd, "value.txt"), "base\n", "utf8");
  const git = createGitArgvAdapter();
  await gitRequired(git, repositoryRoot, ["init", "--initial-branch=main"]);
  await gitRequired(git, repositoryRoot, ["config", "core.autocrlf", "false"]);
  await gitRequired(git, repositoryRoot, ["add", "--", "apps/demo/value.txt"]);
  await gitRequired(git, repositoryRoot, ["commit", "-m", "Fixture"], fixtureIdentity());
  let counter = 0;
  const opened = createRepositoryWorkspaceService({
    executionHostId: "host.fixture",
    trustedWorktreeRoot: worktreeRoot,
    store: openRepositoryWorkspaceStore(resolve(fixtureRoot, "repository.sqlite")),
    git,
    writerGate: createLocalIdleWriterGate({
      observeActiveWriters: () => Promise.resolve(0),
      createLeaseId: () => `lease.${(++counter).toString()}`,
    }),
    testRunner: {
      run: () =>
        Promise.resolve({
          ok: true,
          value: {
            passed: true,
            summary: "fixture",
            runnerId: "fixture",
            runnerAuthorityId: "principal.fixture-runner",
          },
        }),
    },
    idFactory: { create: (kind) => `${kind}.fixture.${(++counter).toString()}` },
    mergeIdentity: { name: "Zap Fixture", email: "fixture@invalid.local" },
    now: () => "2026-01-01T00:00:00.000Z",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const service = opened.value;
  try {
    const registered = await service.registerRepository({
      requestId: "register",
      principalId: "principal.fixture",
      executionHostId: "host.fixture",
      projectId: "project.fixture",
      contextId: "context.main",
      trustedProjectCwd: projectCwd,
      displayName: "Fixture",
    });
    assert.equal(registered.ok, true);
    if (!registered.ok) return;
    const adopted = await service.adoptRegisteredPlan({
      requestId: "adopt-default",
      principalId: "principal.fixture",
      executionHostId: "host.fixture",
      projectId: "project.fixture",
      contextId: "context.main",
      planId: "plan.default.fixture",
      displayName: "Existing default plan",
      registeredWorktreeId: registered.value.worktree.worktreeId,
      expectedHead: registered.value.worktree.headCommit,
      algorithmBinding: { state: "pending" },
    });
    assert.equal(adopted.ok, true);
    if (adopted.ok) {
      assert.equal(adopted.value.worktree.kind, "registered");
      assert.equal(adopted.value.plan.rootWorktreeId, registered.value.worktree.worktreeId);
    }
    const plan = await service.preparePlanRoot({
      requestId: "plan",
      principalId: "principal.fixture",
      executionHostId: "host.fixture",
      projectId: "project.fixture",
      contextId: "context.plan",
      planId: "plan.fixture",
      displayName: "Plan",
      baseWorktreeId: registered.value.worktree.worktreeId,
      expectedBaseHead: registered.value.worktree.headCommit,
      algorithmBinding: { state: "pending" },
    });
    assert.equal(plan.ok, true);
    if (!plan.ok) return;
    const port = createRepositoryManagedWorkspaceProvisioningPort({
      repositories: service,
      executionHostId: "host.fixture",
      id: () => `assignment.${(++counter).toString()}`,
      now: () => "2026-01-01T00:00:00.000Z",
      authorizeIntegrationResolution: () => ({ ok: false, message: "not a resolution task" }),
    });
    const access = WorkspaceAccessContextSchema.parse({
      principalId: "principal.fixture",
      actorId: null,
      clientId: "client.fixture",
      authorizedProjectIds: ["project.fixture"],
    });
    const request = ManagedWorkRequestSchema.parse({
      clientRequestId: "managed.fixture",
      projectId: "project.fixture",
      contextId: "context.plan",
      planId: "plan.fixture",
      workspaceRequest: {
        mode: "isolated_child",
        parentWorktreeId: plan.value.worktree.worktreeId,
        expectedParentHead: plan.value.worktree.headCommit,
      },
      goal: "Write the fixture result",
      expectedResult: "A committed result",
      targetRefs: [],
      contextRefs: [],
      parentTaskId: null,
      parentRunId: null,
      projectedParentActorId: null,
      sourceBasisRef: "basis.fixture",
      planRevision: null,
      depth: 0,
      budgets: { maximumTurns: 4, wallTimeMs: 10_000 },
    });
    const assignment = await port.prepare({
      access,
      request,
      taskId: "task.fixture",
      runId: "run.fixture",
      attemptId: "attempt.fixture",
      actorId: "actor.fixture",
      parentAssignment: null,
    });
    assert.equal(assignment.ok, true);
    if (!assignment.ok) return;
    const initial = await port.resolveInitialLaunch({
      access,
      attemptId: "attempt.fixture",
      assignment: assignment.value,
      legacyProtectedCwd: "C:/must-not-win",
    });
    assert.equal(initial.ok, true);
    if (!initial.ok) return;
    assert.equal(relative(initial.value.cwd, resolve(initial.value.cwd)), "");
    assert.equal(initial.value.cwd.endsWith(`apps${sep}demo`), true);
    assert.equal(initial.value.cwd.includes("worktree.child"), true);
    writeFileSync(resolve(initial.value.cwd, "uncommitted.txt"), "retained\n", "utf8");
    const resumed = await port.resolveResume({
      access,
      attemptId: "attempt.fixture",
      assignment: assignment.value,
      legacyProtectedCwd: "C:/must-not-win",
    });
    assert.equal(resumed.ok, true);
    if (resumed.ok) {
      assert.equal(resumed.value.cwd, initial.value.cwd);
      assert.equal(resumed.value.dirty, true);
    }
    const recorded = service.getWorktree(assignment.value.worktreeId ?? "missing");
    assert.equal(recorded.ok, true);
    if (recorded.ok) {
      const owned = recorded.value.assignments.at(-1);
      assert.equal(owned?.attemptId, "attempt.fixture");
      assert.equal(owned?.actorId, "actor.fixture");
    }
  } finally {
    service.close();
    cleanupOwnedFixture(tempParent, fixtureRoot);
  }
});

async function gitRequired(
  git: ReturnType<typeof createGitArgvAdapter>,
  cwd: string,
  args: readonly string[],
  environment?: Readonly<Record<string, string>>,
): Promise<void> {
  const result = await git.run(
    environment === undefined ? { cwd, args } : { cwd, args, environment },
  );
  if (result.exitCode !== 0) throw new Error(result.stderr || "fixture Git command failed");
}

function cleanupOwnedFixture(tempParent: string, fixtureRoot: string): void {
  const resolved = resolve(fixtureRoot);
  const child = relative(tempParent, resolved);
  if (child.startsWith("..") || child === "")
    throw new Error(
      reqMessage("fixture cleanup escaped TEMP", "remove only the verified owned fixture root"),
    );
  rmSync(resolved, { recursive: true, force: true });
}

function reqMessage(why: string, fix: string): string {
  return `spec://org.vibevm.zap/lens/PROP-014#verification: ${why}; fix: ${fix}`;
}

function fixtureIdentity(): Readonly<Record<string, string>> {
  return {
    GIT_AUTHOR_NAME: "Zap Fixture",
    GIT_AUTHOR_EMAIL: "fixture@invalid.local",
    GIT_COMMITTER_NAME: "Zap Fixture",
    GIT_COMMITTER_EMAIL: "fixture@invalid.local",
    GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
    GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
  };
}
