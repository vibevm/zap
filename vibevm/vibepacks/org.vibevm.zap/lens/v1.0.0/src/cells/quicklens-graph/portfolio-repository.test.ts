/** Parallel-plan repository canvas scenario. @scope spec://org.vibevm.zap/lens/PROP-014#simulation */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { z } from "zod";
import {
  IntegrationAttemptSchema,
  ProjectPlanIdSchema,
  ProjectPlanRecordSchema,
  RepositoryProjectBindingSchema,
  RepositoryRecordSchema,
  RepositoryIdSchema,
  RepositoryWorktreeIdSchema,
  RepositoryWorktreeRecordSchema,
} from "../repository-model/index.ts";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import { readWorkspaceCanvas, type WorkspaceCanvasProject } from "../workspace-client/index.ts";
import {
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkContextIdSchema,
  projectObjectReferenceKey,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema, DecimalSchema } from "../protocol/index.ts";
import { LIGHT_GRAPH_THEME, projectPortfolioGraph } from "./index.ts";

const ScenarioSchema = z.object({
  protocol: z.literal("zap-mock-behavior/1"),
  kind: z.literal("portfolio_repository"),
  scenarioId: z.string(),
  runnerId: z.literal("quicklens.portfolio-repository"),
  seed: z.string(),
  tags: z.array(z.string()).min(1),
  coverage: z.array(z.string()),
  inputs: z.object({
    projectId: z.string(),
    repositoryId: z.string(),
    registered: z.object({
      contextId: z.string(),
      worktreeId: z.string(),
      branchRef: z.string(),
      headCommit: z.string(),
    }),
    plans: z.array(
      z.object({
        planId: z.string(),
        contextId: z.string(),
        displayName: z.string(),
        rootWorktreeId: z.string(),
        branchRef: z.string(),
      }),
    ),
    worker: z.object({
      worktreeId: z.string(),
      parentWorktreeId: z.string(),
      branchRef: z.string(),
      actorId: z.string(),
      taskId: z.string(),
      runId: z.string(),
    }),
    integration: z.object({
      integrationId: z.string(),
      sourceWorktreeId: z.string(),
      targetWorktreeId: z.string(),
      integrationWorktreeId: z.string(),
      conflictPaths: z.array(z.string()),
    }),
  }),
  expected: z.object({
    planCount: z.literal(2),
    sharedRegisteredRoot: z.literal(true),
    workerAssignmentVisible: z.literal(true),
    conflictVisible: z.literal(true),
    contextFrameCount: z.literal(12),
    zeroLlmInference: z.literal(true),
  }),
});

test("same repository contexts retain unique keys, shared root and sourced fork/merge links", async () => {
  const scenario = ScenarioSchema.parse(
    JSON.parse(
      readFileSync(new URL("./portfolio-repository.simulation.json", import.meta.url), "utf8"),
    ),
  );
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const inputs = scenario.inputs;
  const executionSeed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
  const seed = await readWorkspaceCanvas(createWorkspaceDemoPort(), [
    {
      projectId: ProjectIdSchema.parse("project.lens"),
      contextId: WorkContextIdSchema.parse("context.lens.main"),
      fallbackLabel: "Lens",
    },
  ]);
  assert.equal(seed.ok, true);
  if (!seed.ok) return;
  const template = seed.value.projects[0];
  assert.equal(template?.state, "ready");
  if (template?.state !== "ready") return;
  const projectId = ProjectIdSchema.parse(inputs.projectId);
  const originalContext = WorkContextIdSchema.parse(inputs.registered.contextId);
  const contexts = inputs.plans.map((plan) => ({
    contextId: WorkContextIdSchema.parse(plan.contextId),
    projectId,
    displayName: plan.displayName,
    workspaceRef: `workspace.${plan.planId}`,
    branchLabel: plan.branchRef,
    revisionBinding: inputs.registered.headCommit,
    planning: { state: "unavailable" as const, reason: "Simulation" },
    coordinatorConversationId: ConversationIdSchema.parse(`conversation.${plan.planId}`),
    planId: ProjectPlanIdSchema.parse(plan.planId),
    repositoryId: RepositoryIdSchema.parse(inputs.repositoryId),
    rootWorktreeId: RepositoryWorktreeIdSchema.parse(plan.rootWorktreeId),
    revision: DecimalSchema.parse("1"),
    createdAt: "2026-09-16T12:00:00.000Z",
    updatedAt: "2026-09-16T12:00:00.000Z",
  }));
  const registered = worktree(
    inputs.registered.worktreeId,
    originalContext,
    null,
    "registered",
    inputs.registered.branchRef,
    inputs.registered.headCommit,
    [],
  );
  const originalDescriptor = {
    contextId: originalContext,
    projectId,
    displayName: "Registered checkout",
    workspaceRef: "workspace.registered",
    branchLabel: inputs.registered.branchRef,
    revisionBinding: inputs.registered.headCommit,
    planning: { state: "unavailable" as const, reason: "Simulation" },
    coordinatorConversationId: ConversationIdSchema.parse("conversation.registered"),
    planId: null,
    repositoryId: RepositoryIdSchema.parse(inputs.repositoryId),
    rootWorktreeId: registered.worktreeId,
    revision: DecimalSchema.parse("1"),
    createdAt: "2026-09-16T12:00:00.000Z",
    updatedAt: "2026-09-16T12:00:00.000Z",
  };
  const allContexts = [originalDescriptor, ...contexts];
  const planProjects = inputs.plans.map((plan, index) => {
    const contextId = WorkContextIdSchema.parse(plan.contextId);
    const root = worktree(
      plan.rootWorktreeId,
      contextId,
      plan.planId,
      "plan_root",
      plan.branchRef,
      String(index + 2).repeat(40),
      [],
    );
    const planRecord = ProjectPlanRecordSchema.parse({
      planId: plan.planId,
      repositoryId: inputs.repositoryId,
      executionHostId: "host.local",
      projectId,
      contextId,
      displayName: plan.displayName,
      rootWorktreeId: root.worktreeId,
      integrationTargetWorktreeId: registered.worktreeId,
      algorithmBinding: { state: "pending" },
      state: "ready",
      creatorPrincipalId: "principal.owner",
      lastUpdatedByPrincipalId: "principal.owner",
      revision: "1",
      createdAt: "2026-09-16T12:00:00.000Z",
    });
    const alpha = index === 0;
    const worker = alpha
      ? worktree(
          inputs.worker.worktreeId,
          contextId,
          plan.planId,
          "worker",
          inputs.worker.branchRef,
          "d".repeat(40),
          [
            {
              assignmentId: "assignment.alpha",
              attemptId: "attempt.alpha",
              assignerPrincipalId: "principal.owner",
              basisCommit: root.headCommit,
              actorId: inputs.worker.actorId,
              taskId: inputs.worker.taskId,
              runId: inputs.worker.runId,
              semanticTargetRefs: [],
              assignedAt: "2026-09-16T12:01:00.000Z",
              releasedAt: null,
            },
          ],
        )
      : null;
    const integrationTree = alpha
      ? worktree(
          inputs.integration.integrationWorktreeId,
          contextId,
          plan.planId,
          "integration",
          "refs/heads/integration/payments-main",
          "e".repeat(40),
          [],
        )
      : null;
    const integration =
      worker === null || integrationTree === null
        ? []
        : [
            IntegrationAttemptSchema.parse({
              integrationId: inputs.integration.integrationId,
              repositoryId: inputs.repositoryId,
              executionHostId: "host.local",
              planId: plan.planId,
              sourceWorktreeId: worker.worktreeId,
              targetWorktreeId: registered.worktreeId,
              integrationWorktreeId: integrationTree.worktreeId,
              expectedSourceHead: worker.headCommit,
              expectedTargetHead: registered.headCommit,
              candidateCommit: null,
              conflictPaths: inputs.integration.conflictPaths,
              state: "conflicted",
              testEvidence: null,
              review: null,
              creatorPrincipalId: "principal.owner",
              revision: "1",
              createdAt: "2026-09-16T12:02:00.000Z",
            }),
          ];
    const actor = template.view.network.agents[0];
    const network =
      actor === undefined
        ? template.view.network
        : {
            ...template.view.network,
            agents: [
              {
                ...actor,
                actorId: ActorIdSchema.parse(inputs.worker.actorId),
                projectId,
                contextId,
              },
            ],
            relationships: [],
          };
    return {
      ...template,
      selection: { projectId, contextId, fallbackLabel: `Shared · ${plan.displayName}` },
      contextId,
      view: {
        ...template.view,
        project: {
          ...template.view.project,
          projectId,
          displayName: "Shared repository",
          defaultContextId: contexts[0]?.contextId ?? contextId,
        },
        contexts: allContexts,
        execution: { ...template.view.execution, projectId, contextId },
        network,
      },
      repository: {
        state: "ready" as const,
        value: {
          repository: RepositoryRecordSchema.parse({
            repositoryId: inputs.repositoryId,
            executionHostId: "host.local",
            displayName: "Shared repository",
            objectFormat: "sha1",
            revision: "1",
            createdAt: "2026-09-16T12:00:00.000Z",
          }),
          binding: RepositoryProjectBindingSchema.parse({
            repositoryId: inputs.repositoryId,
            executionHostId: "host.local",
            projectId,
            registeredWorktreeId: registered.worktreeId,
            creatorPrincipalId: "principal.owner",
            revision: "1",
          }),
          registeredWorktree: registered,
          contextWorktree: root,
          observedContextHead: root.headCommit,
          workingTreeState: "clean" as const,
          testProfiles: [],
          plans: [planRecord],
          worktrees: [
            registered,
            root,
            ...(worker === null ? [] : [worker]),
            ...(integrationTree === null ? [] : [integrationTree]),
          ],
          integrations: integration,
        },
      },
    } satisfies WorkspaceCanvasProject;
  });
  const originalProject: WorkspaceCanvasProject = {
    ...template,
    selection: {
      projectId,
      contextId: originalContext,
      fallbackLabel: "Shared · Registered checkout",
    },
    contextId: originalContext,
    view: {
      ...template.view,
      project: {
        ...template.view.project,
        projectId,
        displayName: "Shared repository",
        defaultContextId: originalContext,
      },
      contexts: allContexts,
      execution: { ...template.view.execution, projectId, contextId: originalContext },
      network: { ...template.view.network, agents: [], relationships: [] },
    },
    repository: {
      state: "ready",
      value: {
        repository: RepositoryRecordSchema.parse({
          repositoryId: inputs.repositoryId,
          executionHostId: "host.local",
          displayName: "Shared repository",
          objectFormat: "sha1",
          revision: "1",
          createdAt: "2026-09-16T12:00:00.000Z",
        }),
        binding: RepositoryProjectBindingSchema.parse({
          repositoryId: inputs.repositoryId,
          executionHostId: "host.local",
          projectId,
          registeredWorktreeId: registered.worktreeId,
          creatorPrincipalId: "principal.owner",
          revision: "1",
        }),
        registeredWorktree: registered,
        contextWorktree: registered,
        observedContextHead: registered.headCommit,
        workingTreeState: "clean",
        testProfiles: [],
        plans: [],
        worktrees: [registered],
        integrations: [],
      },
    },
  };
  const projects = [originalProject, ...planProjects];
  const projection = projectPortfolioGraph(
    { projects, completeness: "complete" },
    { collapsedProjects: new Set(), collapsedMilestones: new Set(), selectedKey: null },
    LIGHT_GRAPH_THEME,
  );
  const labels = projection.graph.mapEdges((_edge, attributes) => attributes.label);
  assert.ok(labels.includes("forked from"));
  assert.ok(labels.includes("executes in"));
  assert.ok(labels.includes("merge source"));
  assert.ok(labels.includes("merges to"));
  assert.equal(labels.includes("resolves conflict"), scenario.expected.conflictVisible);
  const assignmentEdge = projection.graph
    .edges()
    .find((edge) => edge.startsWith("assignment:assignment.alpha:"));
  assert.equal(assignmentEdge !== undefined, scenario.expected.workerAssignmentVisible);
  if (assignmentEdge !== undefined) {
    assert.equal(
      projection.graph.getNodeAttribute(projection.graph.source(assignmentEdge), "nodeKind"),
      "agent",
    );
    assert.equal(
      projection.graph.getNodeAttribute(projection.graph.target(assignmentEdge), "nodeKind"),
      "worktree",
    );
  }
  const sharedReference = ProjectObjectReferenceSchema.parse({
    projectId,
    contextId: originalContext,
    domain: "worktree",
    ref: registered.worktreeId,
  });
  const sharedKey = projectObjectReferenceKey(sharedReference);
  assert.equal(projection.cards.get(sharedKey)?.reference.contextId, originalContext);
  assert.equal(
    projection.graph.nodes().filter((key) => key === sharedKey).length,
    scenario.expected.sharedRegisteredRoot ? 1 : 0,
    "shared original worktree is one physical node",
  );
  assert.equal(
    projection.graph.nodes().filter((key) => key.startsWith(`region:${projectId}:`)).length,
    scenario.expected.contextFrameCount,
    "three context-qualified region frames remain distinct",
  );
  const planCards = [...projection.cards.values()].filter((card) => card.kind === "plan_workspace");
  assert.equal(planCards.length, scenario.expected.planCount);
  assert.deepEqual(planCards.map((card) => card.plan.displayName).sort(), [
    "Payments reliability",
    "Search indexing",
  ]);
  process.stdout.write(
    `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
      scenarioId: scenario.scenarioId,
      seed: executionSeed,
      passed: true,
      inputPosition: 5,
      zeroLlmInference: scenario.expected.zeroLlmInference,
    })}\n`,
  );
});

function worktree(
  worktreeId: string,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
  planId: string | null,
  kind: "registered" | "plan_root" | "worker" | "integration",
  branchRef: string,
  headCommit: string,
  assignments: readonly unknown[],
) {
  return RepositoryWorktreeRecordSchema.parse({
    worktreeId,
    repositoryId: "repository.shared",
    executionHostId: "host.local",
    projectId: "project.shared",
    contextId,
    planId,
    kind,
    parentWorktreeId: kind === "worker" ? "worktree.alpha" : null,
    branchRef,
    basisCommit: headCommit,
    headCommit,
    state: kind === "integration" ? "conflicted" : "ready",
    assignments,
    revision: "1",
    createdAt: "2026-09-16T12:00:00.000Z",
  });
}
