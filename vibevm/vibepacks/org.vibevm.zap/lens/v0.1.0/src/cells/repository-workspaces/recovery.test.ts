import assert from "node:assert/strict";
import test from "node:test";
import type { RepositoryGitPort } from "./index.ts";
import { commitFixture, createGitFixture, openFixtureService } from "./test-support.ts";

const common = {
  principalId: "principal.fixture-owner",
  executionHostId: "host.fixture",
};

test("a persisted plan preparation resumes the exact worktree after a Git interruption", async () => {
  const fixture = await createGitFixture({
    seed: "recovery.prepare",
    projectRelativePath: "apps/demo",
    initialFiles: [{ path: "value.txt", content: "base\n" }],
    ignoredFiles: [],
  });
  let failed = false;
  const interrupted: RepositoryGitPort = {
    async run(command) {
      if (!failed && command.args.includes("worktree") && command.args.includes("add")) {
        failed = true;
        return { exitCode: -1, stdout: "", stderr: "fixture interruption" };
      }
      return fixture.git.run(command);
    },
  };
  let service = openFixtureService(fixture, "recovery.prepare", interrupted);
  try {
    const registered = await service.registerRepository({
      ...common,
      requestId: "register",
      projectId: "project.fixture",
      contextId: "context.main",
      trustedProjectCwd: fixture.projectCwd,
      displayName: "Fixture",
    });
    assert.equal(registered.ok, true);
    if (!registered.ok) return;
    const request = {
      ...common,
      requestId: "prepare",
      projectId: "project.fixture",
      contextId: "context.plan",
      planId: "plan.recovery",
      displayName: "Recovery",
      baseWorktreeId: registered.value.worktree.worktreeId,
      expectedBaseHead: registered.value.worktree.headCommit,
      algorithmBinding: { state: "pending" as const },
    };
    const first = await service.preparePlanRoot(request);
    assert.equal(first.ok, false);
    service.close();
    service = openFixtureService(fixture, "recovery.prepare");
    const resumed = await service.preparePlanRoot(request);
    assert.equal(resumed.ok, true);
    if (resumed.ok) {
      assert.equal(resumed.value.plan.state, "ready");
      assert.equal(resumed.value.worktree.headCommit, fixture.initialHead);
    }
  } finally {
    service.close();
    fixture.cleanup();
  }
});

test("promotion reconciles an ff-only merge completed before receipt persistence", async () => {
  const fixture = await createGitFixture({
    seed: "recovery.promote",
    projectRelativePath: "apps/demo",
    initialFiles: [{ path: "value.txt", content: "base\n" }],
    ignoredFiles: [],
  });
  let interruptPromotion = false;
  let interrupted = false;
  const flaky: RepositoryGitPort = {
    async run(command) {
      if (
        interruptPromotion &&
        !interrupted &&
        command.args.includes("merge") &&
        command.args.includes("--ff-only")
      ) {
        interrupted = true;
        const completed = await fixture.git.run(command);
        assert.equal(completed.exitCode, 0);
        return { exitCode: -1, stdout: completed.stdout, stderr: "fixture receipt interruption" };
      }
      return fixture.git.run(command);
    },
  };
  const service = openFixtureService(fixture, "recovery.promote", flaky);
  try {
    const registered = await service.registerRepository({
      ...common,
      requestId: "register",
      projectId: "project.fixture",
      contextId: "context.main",
      trustedProjectCwd: fixture.projectCwd,
      displayName: "Fixture",
    });
    assert.equal(registered.ok, true);
    if (!registered.ok) return;
    const plan = await service.preparePlanRoot({
      ...common,
      requestId: "plan",
      projectId: "project.fixture",
      contextId: "context.plan",
      planId: "plan.promotion",
      displayName: "Promotion",
      baseWorktreeId: registered.value.worktree.worktreeId,
      expectedBaseHead: registered.value.worktree.headCommit,
      algorithmBinding: { state: "pending" },
    });
    assert.equal(plan.ok, true);
    if (!plan.ok) return;
    const child = await service.prepareChildWorktree({
      ...common,
      requestId: "child",
      projectId: "project.fixture",
      contextId: "context.plan",
      planId: plan.value.plan.planId,
      parentWorktreeId: plan.value.worktree.worktreeId,
      expectedParentHead: plan.value.worktree.headCommit,
    });
    assert.equal(child.ok, true);
    if (!child.ok) return;
    const childWorkspace = await service.resolveExecutionWorkspace({
      projectId: child.value.projectId,
      contextId: child.value.contextId,
      worktreeId: child.value.worktreeId,
      executionHostId: child.value.executionHostId,
      expectedRevision: child.value.revision,
    });
    assert.equal(childWorkspace.ok, true);
    if (!childWorkspace.ok) return;
    const childHead = await commitFixture(
      fixture.git,
      childWorkspace.value.projectCwd,
      [{ path: "result.txt", content: "candidate\n" }],
      "candidate",
    );
    const observed = await service.recordWorktreeHead({
      ...common,
      requestId: "observe",
      worktreeId: child.value.worktreeId,
      expectedRevision: child.value.revision,
      expectedOldHead: child.value.headCommit,
      newHead: childHead,
    });
    assert.equal(observed.ok, true);
    if (!observed.ok) return;
    const prepared = await service.prepareIntegration({
      ...common,
      requestId: "integrate",
      integrationId: "integration.recovery",
      planId: plan.value.plan.planId,
      sourceWorktreeId: observed.value.worktreeId,
      targetWorktreeId: plan.value.worktree.worktreeId,
      expectedSourceHead: observed.value.headCommit,
      expectedTargetHead: plan.value.worktree.headCommit,
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    const tested = await service.runIntegrationTest({
      ...common,
      requestId: "test",
      integrationId: prepared.value.integrationId,
      expectedRevision: prepared.value.revision,
      profileId: "fixture.clean",
    });
    assert.equal(tested.ok, true);
    if (!tested.ok) return;
    const reviewed = await service.recordIntegrationReview({
      ...common,
      requestId: "review",
      integrationId: tested.value.integrationId,
      expectedRevision: tested.value.revision,
      accepted: true,
      rationale: "Exact fixture candidate accepted.",
    });
    assert.equal(reviewed.ok, true);
    if (!reviewed.ok) return;
    const diff = await service.readIntegrationDiff({
      integrationId: reviewed.value.integrationId,
      executionHostId: "host.fixture",
      projectId: "project.fixture",
      contextId: "context.plan",
      maximumBytes: 16_384,
    });
    assert.equal(diff.ok, true);
    if (diff.ok) {
      assert.equal(diff.value.changedFiles.includes("apps/demo/result.txt"), true);
      assert.equal(diff.value.unifiedText.includes("candidate"), true);
      assert.equal(diff.value.truncated, false);
    }
    interruptPromotion = true;
    const interruptedResult = await service.promoteIntegration({
      ...common,
      requestId: "promote",
      integrationId: reviewed.value.integrationId,
      expectedRevision: reviewed.value.revision,
    });
    assert.equal(interruptedResult.ok, false);
    const reconciled = await service.promoteIntegration({
      ...common,
      requestId: "promote-retry",
      integrationId: reviewed.value.integrationId,
      expectedRevision: reviewed.value.revision,
    });
    assert.equal(reconciled.ok, true);
    if (reconciled.ok) assert.equal(reconciled.value.state, "promoted");
  } finally {
    service.close();
    fixture.cleanup();
  }
});
