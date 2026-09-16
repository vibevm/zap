/** Actual no-LLM repository-managed product gate. @scope spec://org.vibevm.zap/lens/PROP-014#verification */
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { isAbsolute, join, relative, resolve } from "node:path";
import test from "node:test";
import { createGitArgvAdapter } from "../repository-workspaces/index.ts";
import { createZapMockManagedControlAdapter } from "../managed-work/index.ts";
import { RepositoryManagedProductSimulationSchema } from "../mock-simulation/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import {
  ManagedWorkViewSchema,
  ProjectIdSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import {
  MOCK_CONTEXT_ID,
  MOCK_PROJECT_ID,
  mockProductConfig,
  mockUnusedHost,
  writeMockProductScenario,
} from "./mock-managed.support.ts";
import { commitRepositoryProductFixture } from "./repository-product.test-support.ts";

const scenario = RepositoryManagedProductSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./repository-managed-product.simulation.json", import.meta.url), "utf8"),
  ),
);

test("normal runtime starts a ZapMock worker in an assigned isolated child worktree", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
  const tempParent = resolve(tmpdir());
  const root = mkdtempSync(join(tempParent, "zap-repository-product-"));
  const repository = join(root, "repository");
  const state = join(root, "state");
  mkdirSync(repository, { recursive: true });
  mkdirSync(state, { recursive: true });
  writeFileSync(
    join(repository, scenario.inputs.initialPath),
    scenario.inputs.initialContent,
    "utf8",
  );
  const behavior = writeMockProductScenario(state, scenario.inputs.mockBehaviorSeed);
  const git = createGitArgvAdapter();
  await gitRequired(git, repository, ["init", "--initial-branch=main"]);
  await gitRequired(git, repository, ["config", "core.autocrlf", "false"]);
  await gitRequired(git, repository, ["add", "--", scenario.inputs.initialPath]);
  await gitRequired(git, repository, ["commit", "-m", "Fixture"], fixtureIdentity());
  const baseConfig = mockProductConfig(state, behavior.path);
  const config = {
    ...baseConfig,
    repositoryWorkspaces: {
      executionHostId: "host.repository.product",
      mergeIdentity: { name: "Zap Fixture", email: "fixture@invalid.local" },
      testProfiles: ["repository.consistency" as const],
    },
    managedAgents: baseConfig.managedAgents.map((profile) => ({ ...profile, cwd: repository })),
    projects: baseConfig.projects.map((project) => ({
      ...project,
      protected: { ...project.protected, cwd: repository },
    })),
  };
  const runtime = createWayfinderRuntime(config, {
    hosts: [mockUnusedHost()],
    managedControlAdapters: [
      createZapMockManagedControlAdapter({ directory: join(state, "mock-control") }),
    ],
  });
  assert.equal(runtime.ok, true);
  if (!runtime.ok) return;
  try {
    const started = await runtime.value.start();
    assert.equal(started.ok, true, JSON.stringify(started));
    if (!started.ok) return;
    const ticket = runtime.value.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://mock-product.test",
    });
    assert.notEqual(client, null);
    if (client === null) return;
    const projectId = ProjectIdSchema.parse(MOCK_PROJECT_ID);
    const contextId = WorkContextIdSchema.parse(MOCK_CONTEXT_ID);
    const repositoryView = await client.read({
      operation: "repository.get.v1",
      projectId,
      contextId,
    });
    assert.equal(repositoryView.ok, true, JSON.stringify(repositoryView));
    if (!repositoryView.ok || repositoryView.value.operation !== "repository.get.v1") return;
    const project = await client.read({ operation: "project.get.v1", projectId, contextId });
    assert.equal(project.ok, true);
    if (!project.ok || project.value.operation !== "project.get.v1") return;
    const context = project.value.detail.contexts.find(
      (candidate) => candidate.contextId === contextId,
    );
    assert.equal(
      context?.planId !== null && context?.planId !== undefined,
      scenario.expected.defaultPlanAdopted,
    );
    if (context?.planId === null || context?.planId === undefined) return;
    const created = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.create.v1",
        clientRequestId: "request.repository-product.create",
        projectId,
        contextId,
        planId: context.planId,
        workspaceRequest: {
          mode: "isolated_child",
          parentWorktreeId: repositoryView.value.contextWorktree.worktreeId,
          expectedParentHead: repositoryView.value.observedContextHead,
        },
        selection: {
          mode: "profile_override",
          profileId: scenario.inputs.workerProfileId,
          reasonMarkdown: "Run the deterministic repository assignment fixture.",
        },
        goal: scenario.inputs.workerGoal,
        expectedResult: scenario.inputs.workerExpectedResult,
        targetRefs: [],
      }),
    );
    assert.equal(created.ok, true, JSON.stringify(created));
    if (
      !created.ok ||
      created.value.operation !== "managed-work.create.v1" ||
      !("work" in created.value)
    )
      return;
    const createdWork = ManagedWorkViewSchema.parse(created.value.work);
    assert.equal(createdWork.workspaceAssignment?.kind, scenario.expected.assignmentKind);
    const assignment = createdWork.workspaceAssignment;
    assert.notEqual(assignment?.worktreeId, null);
    assert.notEqual(assignment?.worktreeId, undefined);
    if (assignment?.worktreeId === null || assignment?.worktreeId === undefined) return;
    const startedWork = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.start.v1",
        projectId,
        contextId,
        clientRequestId: "request.repository-product.start",
        runId: createdWork.runId,
        expectedRevision: createdWork.revision,
      }),
    );
    assert.equal(startedWork.ok, true, JSON.stringify(startedWork));
    const question = await poll(async () => {
      const result = await client.read({
        operation: "question.list.v1",
        projectId,
        contextId,
        state: "open",
        limit: 10,
      });
      return result.ok && result.value.operation === "question.list.v1"
        ? result.value.questions[0]
        : undefined;
    });
    const item = question.items[0];
    const option = item?.options[0];
    assert.ok(item !== undefined && option !== undefined);
    if (item === undefined || option === undefined) return;
    const sourcePlan = await client.command({
      operation: "plan.workspace.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.source-plan"),
      projectId,
      contextId,
      displayName: scenario.inputs.sourcePlanDisplayName,
      expectedBaseHead: repositoryView.value.observedContextHead,
    });
    assert.equal(sourcePlan.ok, true, JSON.stringify(sourcePlan));
    if (!sourcePlan.ok || sourcePlan.value.operation !== "plan.workspace.prepare.v1") return;
    const sourceChild = await client.command({
      operation: "worktree.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.source-child"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      planId: sourcePlan.value.plan.planId,
      parentWorktreeId: sourcePlan.value.worktree.worktreeId,
      expectedParentHead: sourcePlan.value.worktree.headCommit,
    });
    assert.equal(sourceChild.ok, true);
    if (!sourceChild.ok || sourceChild.value.operation !== "worktree.prepare.v1") return;
    const sourceHead = await commitRepositoryProductFixture(
      resolve(state, "repository-worktrees", sourceChild.value.worktree.worktreeId),
      scenario.inputs.changePath,
      scenario.inputs.changeContent,
    );
    const candidate = await client.command({
      operation: "integration.prepare.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.integration"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      planId: sourcePlan.value.plan.planId,
      sourceWorktreeId: sourceChild.value.worktree.worktreeId,
      targetWorktreeId: repositoryView.value.registeredWorktree.worktreeId,
      expectedSourceHead: sourceHead,
      expectedTargetHead: repositoryView.value.observedContextHead,
    });
    assert.equal(candidate.ok, true, JSON.stringify(candidate));
    if (!candidate.ok || candidate.value.operation !== "integration.prepare.v1") return;
    const tested = await client.command({
      operation: "integration.test.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.integration-test"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      integrationId: candidate.value.integration.integrationId,
      expectedRevision: candidate.value.integration.revision,
      profileId: "repository.consistency",
    });
    assert.equal(tested.ok, true);
    if (!tested.ok || tested.value.operation !== "integration.test.v1") return;
    const accepted = await client.command({
      operation: "integration.review.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.integration-review"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      integrationId: tested.value.integration.integrationId,
      expectedRevision: tested.value.integration.revision,
      accepted: true,
      rationale: "Exact deterministic candidate accepted for writer-fence proof.",
    });
    assert.equal(accepted.ok, true);
    if (!accepted.ok || accepted.value.operation !== "integration.review.v1") return;
    const blocked = await client.command({
      operation: "integration.promote.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.blocked-promote"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      integrationId: accepted.value.integration.integrationId,
      expectedRevision: accepted.value.integration.revision,
    });
    assert.equal(
      !blocked.ok,
      scenario.expected.promotionBlockedWhileRunning,
      JSON.stringify(blocked),
    );
    const executionBeforePause = await client.read({
      operation: "project.execution.get.v1",
      projectId,
      contextId,
    });
    assert.equal(executionBeforePause.ok, true);
    if (
      !executionBeforePause.ok ||
      executionBeforePause.value.operation !== "project.execution.get.v1"
    )
      return;
    const paused = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "project.pause.v1",
        clientRequestId: "request.repository-product.pause",
        projectId,
        contextId,
        sessionId: null,
        expectedRevision: executionBeforePause.value.execution.revision,
        reasonMarkdown: "Settle the managed target before repository promotion.",
      }),
    );
    assert.equal(paused.ok, scenario.expected.projectPauseSettled, JSON.stringify(paused));
    const promotedWhilePaused = await client.command({
      operation: "integration.promote.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.repository-product.promote-paused"),
      projectId,
      contextId: sourcePlan.value.context.contextId,
      integrationId: accepted.value.integration.integrationId,
      expectedRevision: accepted.value.integration.revision,
    });
    assert.equal(
      promotedWhilePaused.ok,
      scenario.expected.promotionWhilePaused,
      JSON.stringify(promotedWhilePaused),
    );
    const answered = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "question.answer.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.repository-product.answer"),
        projectId,
        contextId,
        questionGroupId: QuestionGroupIdSchema.parse(question.questionGroupId),
        expectedRevision: question.revision,
        submission: {
          answers: [
            {
              questionItemId: QuestionItemIdSchema.parse(item.questionItemId),
              answer: {
                kind: "single_choice",
                optionId: QuestionOptionIdSchema.parse(option.optionId),
              },
            },
          ],
          noteMarkdown: scenario.inputs.answerNoteMarkdown,
        },
      }),
    );
    assert.equal(answered.ok, true, JSON.stringify(answered));
    const executionBeforeContinue = await client.read({
      operation: "project.execution.get.v1",
      projectId,
      contextId,
    });
    assert.equal(executionBeforeContinue.ok, true);
    if (
      !executionBeforeContinue.ok ||
      executionBeforeContinue.value.operation !== "project.execution.get.v1"
    )
      return;
    const continued = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "project.continue.v1",
        clientRequestId: "request.repository-product.continue",
        projectId,
        contextId,
        sessionId: null,
        expectedRevision: executionBeforeContinue.value.execution.revision,
        reasonMarkdown: "Deliver the retained answer after safe promotion.",
      }),
    );
    assert.equal(
      continued.ok,
      scenario.expected.answerDeliveredAfterContinue,
      JSON.stringify(continued),
    );
    const reported = await poll(async () => {
      const result = await client.read({
        operation: "managed-work.get.v1",
        projectId,
        contextId,
        runId: createdWork.runId,
      });
      if (!result.ok || result.value.operation !== "managed-work.get.v1") return undefined;
      const work = ManagedWorkViewSchema.parse(result.value.work);
      return work.state === "reported" ? work : undefined;
    });
    assert.equal(reported.report?.summaryMarkdown.includes(scenario.expected.reportIncludes), true);
    assert.equal(reported.workspaceAssignment?.worktreeId, assignment.worktreeId);
    const worktree = await client.read({
      operation: "worktree.get.v1",
      projectId,
      contextId,
      worktreeId: assignment.worktreeId,
    });
    assert.equal(worktree.ok, true);
    if (worktree.ok && worktree.value.operation === "worktree.get.v1") {
      assert.equal(
        worktree.value.worktree.parentWorktreeId ===
          repositoryView.value.contextWorktree.worktreeId,
        scenario.expected.isolatedChildAgainstContextRoot,
      );
      const owned = worktree.value.worktree.assignments.find(
        (candidate) => candidate.attemptId === reported.attemptId,
      );
      assert.equal(
        owned?.runId === reported.runId && owned.actorId === reported.actorId,
        scenario.expected.assignmentOwnershipRetained,
      );
    }
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed,
        passed: true,
        inputPosition: 9,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await runtime.value.close();
    cleanupOwned(tempParent, root);
  }
});

async function poll<T>(read: () => Promise<T | undefined>): Promise<T> {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const value = await read();
    if (value !== undefined) return value;
    await new Promise((resolvePoll) => setTimeout(resolvePoll, 25));
  }
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-014#verification: managed repository fixture timed out; fix surface: inspect the retained product events",
  );
}

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
function cleanupOwned(parent: string, target: string): void {
  const child = relative(parent, resolve(target));
  if (child === "" || child.startsWith("..") || isAbsolute(child))
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-014#verification: cleanup escaped TEMP; fix surface: remove only the owned fixture root",
    );
  rmSync(target, { recursive: true, force: true });
}
