import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  RunIdSchema,
  WorkspaceAccessContextSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { classifyRepositoryWriterActivity } from "./repository-activity.ts";
import { isExactOwnedCoordinatorRoute } from "./repository-agent.ts";
import {
  commitRepositoryProductFixture,
  openRepositoryProductFixture,
} from "./repository-product.test-support.ts";

test("repository public reads reject another plan context and read-only clients cannot mutate", async () => {
  const fixture = await openRepositoryProductFixture();
  try {
    const repository = await fixture.client.read({
      operation: "repository.get.v1",
      projectId: fixture.projectId,
      contextId: fixture.contextId,
    });
    assert.equal(repository.ok, true);
    if (!repository.ok || repository.value.operation !== "repository.get.v1") return;
    const plans = [];
    for (const [suffix, displayName] of [
      ["a", "Authority A"],
      ["b", "Authority B"],
    ] as const) {
      const prepared = await fixture.client.command({
        operation: "plan.workspace.prepare.v1",
        clientRequestId: ClientRequestIdSchema.parse(`request.authority.${suffix}`),
        projectId: fixture.projectId,
        contextId: fixture.contextId,
        displayName,
        expectedBaseHead: repository.value.observedContextHead,
      });
      assert.equal(prepared.ok, true);
      if (!prepared.ok || prepared.value.operation !== "plan.workspace.prepare.v1") return;
      plans.push(prepared.value);
    }
    const planA = plans[0];
    const planB = plans[1];
    assert.ok(planA !== undefined && planB !== undefined);
    if (planA === undefined || planB === undefined) return;
    const wrongPlan = await fixture.client.read({
      operation: "plan.workspace.get.v1",
      projectId: fixture.projectId,
      contextId: planB.context.contextId,
      planId: planA.plan.planId,
    });
    assert.equal(wrongPlan.ok, false);
    const wrongList = await fixture.client.read({
      operation: "worktree.list.v1",
      projectId: fixture.projectId,
      contextId: planB.context.contextId,
      planId: planA.plan.planId,
    });
    assert.equal(wrongList.ok, false);
    const wrongWorktree = await fixture.client.read({
      operation: "worktree.get.v1",
      projectId: fixture.projectId,
      contextId: planB.context.contextId,
      worktreeId: planA.worktree.worktreeId,
    });
    assert.equal(wrongWorktree.ok, false);
    const child = await fixture.client.command({
      operation: "worktree.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.authority.child"),
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      planId: planA.plan.planId,
      parentWorktreeId: planA.worktree.worktreeId,
      expectedParentHead: planA.worktree.headCommit,
    });
    assert.equal(child.ok, true);
    if (!child.ok || child.value.operation !== "worktree.prepare.v1") return;
    const sourceHead = await commitRepositoryProductFixture(
      resolve(fixture.worktreeRoot, child.value.worktree.worktreeId),
      "authority.txt",
      "integrate into the registered checkout\n",
    );
    const candidate = await fixture.client.command({
      operation: "integration.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.authority.integration"),
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      planId: planA.plan.planId,
      sourceWorktreeId: child.value.worktree.worktreeId,
      targetWorktreeId: repository.value.registeredWorktree.worktreeId,
      expectedSourceHead: sourceHead,
      expectedTargetHead: repository.value.observedContextHead,
    });
    assert.equal(candidate.ok, true, JSON.stringify(candidate));
    if (!candidate.ok || candidate.value.operation !== "integration.prepare.v1") return;
    assert.equal(candidate.value.integration.state, "candidate");
    const integrationTree = await fixture.client.read({
      operation: "worktree.get.v1",
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      worktreeId: candidate.value.integration.integrationWorktreeId,
    });
    assert.equal(integrationTree.ok, true);
    const tested = await fixture.client.command({
      operation: "integration.test.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.authority.test"),
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      integrationId: candidate.value.integration.integrationId,
      expectedRevision: candidate.value.integration.revision,
      profileId: "repository.consistency",
    });
    assert.equal(tested.ok, true);
    if (!tested.ok || tested.value.operation !== "integration.test.v1") return;
    const accepted = await fixture.client.command({
      operation: "integration.review.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.authority.accept"),
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      integrationId: tested.value.integration.integrationId,
      expectedRevision: tested.value.integration.revision,
      accepted: true,
      rationale: "Exact cross-context registered target candidate accepted.",
    });
    assert.equal(accepted.ok, true);
    if (!accepted.ok || accepted.value.operation !== "integration.review.v1") return;
    const promoted = await fixture.client.command({
      operation: "integration.promote.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.authority.promote-registered"),
      projectId: fixture.projectId,
      contextId: planA.context.contextId,
      integrationId: accepted.value.integration.integrationId,
      expectedRevision: accepted.value.integration.revision,
    });
    assert.equal(promoted.ok, true, JSON.stringify(promoted));
    const readOnly = fixture.runtime.service.bind({
      access: WorkspaceAccessContextSchema.parse({
        principalId: PrincipalIdSchema.parse("principal.authority-reader"),
        actorId: null,
        clientId: ClientIdSchema.parse("client.authority-reader"),
        authorizedProjectIds: [fixture.projectId],
      }),
      allowedActions: ["read"],
    });
    const review = await readOnly.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "integration.review.v1",
        clientRequestId: "request.authority.review",
        projectId: fixture.projectId,
        contextId: planA.context.contextId,
        integrationId: "integration.authority.fixture",
        expectedRevision: "1",
        accepted: true,
        rationale: "Must be rejected before repository lookup.",
      }),
    );
    assert.equal(review.ok, false);
    if (!review.ok) assert.equal(review.error.code, "forbidden");
    const promote = await readOnly.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "integration.promote.v1",
        clientRequestId: "request.authority.promote",
        projectId: fixture.projectId,
        contextId: planA.context.contextId,
        integrationId: "integration.authority.fixture",
        expectedRevision: "1",
      }),
    );
    assert.equal(promote.ok, false);
    if (!promote.ok) assert.equal(promote.error.code, "forbidden");
  } finally {
    await fixture.close();
  }
});

test("repository agent commands require the caller itself to be the owned coordinator", () => {
  assert.equal(
    isExactOwnedCoordinatorRoute("actor.coordinator", {
      coordinatorActorId: "actor.coordinator",
      forwarding: false,
    }),
    true,
  );
  assert.equal(
    isExactOwnedCoordinatorRoute("actor.worker", {
      coordinatorActorId: "actor.coordinator",
      forwarding: true,
    }),
    false,
  );
  assert.equal(
    isExactOwnedCoordinatorRoute("actor.other", {
      coordinatorActorId: "actor.coordinator",
      forwarding: false,
    }),
    false,
  );
});

test("repository writer activity distinguishes active, settled paused and empty targets", () => {
  assert.equal(classifyRepositoryWriterActivity([], []), "idle");
  assert.equal(
    classifyRepositoryWriterActivity(
      [{ runId: RunIdSchema.parse("run.active"), state: "running", managedControl: null }],
      [],
    ),
    "active",
  );
  assert.equal(
    classifyRepositoryWriterActivity(
      [
        {
          runId: RunIdSchema.parse("run.paused"),
          state: "paused",
          managedControl: {
            processEpoch: "1",
            readiness: "idle",
            providerSessionId: "session.paused",
            providerTurnId: null,
            observationId: "observation.paused",
            automationControlEpoch: "2",
            pauseRequested: true,
            continuation: "live",
          },
        },
      ],
      [{ runId: RunIdSchema.parse("run.paused"), state: "running" }],
    ),
    "idle",
  );
  assert.equal(
    classifyRepositoryWriterActivity(
      [{ runId: RunIdSchema.parse("run.pausing"), state: "running", managedControl: null }],
      [{ runId: RunIdSchema.parse("run.pausing"), state: "running" }],
    ),
    "active",
  );
});
