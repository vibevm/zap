import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { RepositoryProductSimulationSchema } from "../mock-simulation/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  commitRepositoryProductFixture,
  openRepositoryProductFixture,
} from "./repository-product.test-support.ts";

const scenario = RepositoryProductSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./repository-product.simulation.json", import.meta.url), "utf8"),
  ),
);

test("public runtime prepares and reads two independent plan workspaces", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
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
    for (const [index, displayName] of scenario.inputs.planDisplayNames.entries()) {
      const prepared = await fixture.client.command({
        operation: "plan.workspace.prepare.v1",
        clientRequestId: ClientRequestIdSchema.parse(`request.repository-plan-${String(index)}`),
        projectId: fixture.projectId,
        contextId: fixture.contextId,
        displayName,
        expectedBaseHead: repository.value.observedContextHead,
      });
      assert.equal(prepared.ok, true);
      if (!prepared.ok || prepared.value.operation !== "plan.workspace.prepare.v1") return;
      plans.push(prepared.value);
    }
    assert.equal(plans.length, scenario.expected.planCount);
    assert.notEqual(plans[0]?.plan.planId, plans[1]?.plan.planId);
    assert.notEqual(plans[0]?.context.contextId, plans[1]?.context.contextId);
    const first = plans[0];
    const second = plans[1];
    assert.notEqual(first, undefined);
    assert.notEqual(second, undefined);
    if (first === undefined || second === undefined) return;
    const crossContext = await fixture.client.read({
      operation: "worktree.list.v1",
      projectId: fixture.projectId,
      contextId: second.context.contextId,
      planId: first.plan.planId,
    });
    assert.equal(crossContext.ok, false);
    if (!crossContext.ok) {
      assert.equal(crossContext.error.code, scenario.expected.crossContextErrorCode);
      assert.match(crossContext.error.message, /^violates REQ spec:\/\//);
    }
    const note = await fixture.client.command({
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-worktree-note"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      target: {
        projectId: fixture.projectId,
        contextId: first.context.contextId,
        domain: "worktree",
        ref: first.worktree.worktreeId,
      },
      kind: "passive",
      title: scenario.inputs.noteTitle,
      bodyMarkdown: scenario.inputs.noteBodyMarkdown,
      sourceBasisRef: "client-placeholder",
      targetSnapshot: null,
    });
    assert.equal(note.ok, true);
    if (!note.ok || note.value.operation !== "annotation.note.create.v1") return;
    const readNote = await fixture.client.read({
      operation: "annotation.note.get.v1",
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      noteId: note.value.note.noteId,
    });
    assert.equal(readNote.ok, true);
    if (readNote.ok && readNote.value.operation === "annotation.note.get.v1") {
      assert.equal(readNote.value.note.target.domain, scenario.expected.noteTargetDomain);
      assert.equal(readNote.value.note.target.ref, first.worktree.worktreeId);
      assert.equal(readNote.value.note.sourceBasisRef, first.worktree.revision);
    }
    const child = await fixture.client.command({
      operation: "worktree.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-child"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      planId: first.plan.planId,
      parentWorktreeId: first.worktree.worktreeId,
      expectedParentHead: first.worktree.headCommit,
    });
    assert.equal(child.ok, true);
    if (!child.ok || child.value.operation !== "worktree.prepare.v1") return;
    const sourceHead = await commitRepositoryProductFixture(
      resolve(fixture.worktreeRoot, child.value.worktree.worktreeId),
      scenario.inputs.changePath,
      scenario.inputs.changeContent,
    );
    const observedChild = await fixture.client.read({
      operation: "worktree.get.v1",
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      worktreeId: child.value.worktree.worktreeId,
    });
    assert.equal(observedChild.ok, true);
    if (observedChild.ok && observedChild.value.operation === "worktree.get.v1")
      assert.equal(observedChild.value.worktree.headCommit, sourceHead);
    const integrated = await fixture.client.command({
      operation: "integration.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-integration"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      planId: first.plan.planId,
      sourceWorktreeId: child.value.worktree.worktreeId,
      targetWorktreeId: repository.value.registeredWorktree.worktreeId,
      expectedSourceHead: sourceHead,
      expectedTargetHead: repository.value.registeredWorktree.headCommit,
    });
    assert.equal(integrated.ok, true);
    if (!integrated.ok || integrated.value.operation !== "integration.prepare.v1") return;
    assert.equal(
      integrated.value.integration.state,
      scenario.expected.integrationStateAfterPrepare,
    );
    const diff = await fixture.client.read({
      operation: "integration.diff.v1",
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      integrationId: integrated.value.integration.integrationId,
      maximumBytes: 100_000,
    });
    assert.equal(diff.ok, true);
    if (diff.ok && diff.value.operation === "integration.diff.v1") {
      assert.equal(
        diff.value.changedFiles.includes(scenario.inputs.changePath) &&
          diff.value.unifiedText.includes(scenario.inputs.changeContent.trim()),
        scenario.expected.diffIncludesDeclaredChange,
      );
    }
    const tested = await fixture.client.command({
      operation: "integration.test.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-integration-test"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      integrationId: integrated.value.integration.integrationId,
      expectedRevision: integrated.value.integration.revision,
      profileId: scenario.inputs.testProfileId,
    });
    assert.equal(tested.ok, true);
    if (!tested.ok || tested.value.operation !== "integration.test.v1") return;
    const reviewed = await fixture.client.command({
      operation: "integration.review.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-integration-review"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      integrationId: tested.value.integration.integrationId,
      expectedRevision: tested.value.integration.revision,
      accepted: true,
      rationale: scenario.inputs.reviewRationale,
    });
    assert.equal(reviewed.ok, true);
    if (!reviewed.ok || reviewed.value.operation !== "integration.review.v1") return;
    const promoted = await fixture.client.command({
      operation: "integration.promote.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-integration-promote"),
      projectId: fixture.projectId,
      contextId: first.context.contextId,
      integrationId: reviewed.value.integration.integrationId,
      expectedRevision: reviewed.value.integration.revision,
    });
    assert.equal(promoted.ok, true);
    if (promoted.ok && promoted.value.operation === "integration.promote.v1")
      assert.equal(
        promoted.value.integration.state,
        scenario.expected.integrationStateAfterPromote,
      );
    for (const prepared of plans) {
      const project = await fixture.client.read({
        operation: "project.get.v1",
        projectId: fixture.projectId,
        contextId: prepared.context.contextId,
      });
      assert.equal(project.ok, true);
      if (!project.ok || project.value.operation !== "project.get.v1") return;
      const selected = project.value.detail.contexts.find(
        (context) => context.contextId === prepared.context.contextId,
      );
      assert.equal(
        selected?.contextId === prepared.context.contextId,
        scenario.expected.exactContextReads,
      );
      assert.equal(selected?.planId, prepared.plan.planId);
      const worktree = await fixture.client.read({
        operation: "worktree.get.v1",
        projectId: fixture.projectId,
        contextId: prepared.context.contextId,
        worktreeId: prepared.worktree.worktreeId,
      });
      assert.equal(worktree.ok, true);
      if (worktree.ok && worktree.value.operation === "worktree.get.v1")
        assert.equal(
          worktree.value.worktree.contextId === prepared.context.contextId,
          scenario.expected.exactContextReads,
        );
    }
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed,
        passed: true,
        inputPosition: 10,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await fixture.close();
  }
});
