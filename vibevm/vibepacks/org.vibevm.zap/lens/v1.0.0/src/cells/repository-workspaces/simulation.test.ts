import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import type {
  IntegrationAttempt,
  ProjectPlanRecord,
  RepositoryWorktreeRecord,
} from "../repository-model/index.ts";
import {
  RepositoryGitSimulationSchema,
  type RepositoryGitSimulation,
  type RepositoryWorkspaceErrorCode,
  type RepositoryWorkspaceResult,
  type RepositoryWorkspaceService,
} from "./index.ts";
import {
  commitFixture,
  createGitFixture,
  openFixtureService,
  readFixtureFile,
  writeFixture,
  type GitFixture,
} from "./test-support.ts";

const scenariosDirectory = resolve(dirname(fileURLToPath(import.meta.url)), "scenarios");
const requestedScenario = process.env["ZAP_MOCK_SIMULATION_ID"];
const requestedSeed = process.env["ZAP_MOCK_SIMULATION_SEED"];
const requestedIteration = process.env["ZAP_MOCK_SIMULATION_ITERATION"];
if (
  requestedScenario !== undefined &&
  (requestedSeed === undefined ||
    requestedIteration === undefined ||
    !/^(?:0|[1-9][0-9]*)$/.test(requestedIteration))
)
  throw new Error(
    reqMessage(
      "repository simulation selection omitted its seed or decimal iteration",
      "pass all three ZAP_MOCK_SIMULATION variables",
    ),
  );
const documents = readdirSync(scenariosDirectory)
  .filter((name) => name.endsWith(".simulation.json"))
  .map((name) => {
    const raw: unknown = JSON.parse(readFileSync(resolve(scenariosDirectory, name), "utf8"));
    const parsed = RepositoryGitSimulationSchema.safeParse(raw);
    if (!parsed.success) throw new Error(`${name}: ${parsed.error.message}`);
    return parsed.data;
  })
  .filter(
    (document) => requestedScenario === undefined || document.scenarioId === requestedScenario,
  );
if (requestedScenario !== undefined && documents.length !== 1)
  throw new Error(`unknown repository simulation ${requestedScenario}`);

for (const document of documents) {
  test(`real Git repository scenario: ${document.scenarioId}`, async () => {
    const executionSeed = requestedSeed ?? document.seed;
    await runScenario(document, executionSeed);
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: document.scenarioId,
        seed: executionSeed,
        passed: true,
        inputPosition: document.inputs.inputTape.length,
        zeroLlmInference: true,
      })}\n`,
    );
  });
}

async function runScenario(
  document: RepositoryGitSimulation,
  executionSeed: string,
): Promise<void> {
  const fixture = await createGitFixture({ seed: executionSeed, ...document.inputs.fixture });
  const state = createState(openFixtureService(fixture, executionSeed), executionSeed);
  try {
    for (const action of document.inputs.inputTape) await runAction(fixture, state, action);
    for (const [actionId, expected] of Object.entries(document.expected.finalActionStates))
      assert.equal(state.actionStates.get(actionId), expected, actionId);
    for (const [actionId, expected] of Object.entries(document.expected.refusalCodes))
      assert.equal(state.refusalCodes.get(actionId), expected, actionId);
    assert.deepEqual([...state.conflictPaths].sort(), [...document.expected.conflictPaths].sort());
    const target = required(state.worktrees, document.expected.targetWorktreeKey);
    const targetWorkspace = await resolveWorkspace(state.service, target);
    for (const file of document.expected.targetFiles)
      assert.equal(readFixtureFile(targetWorkspace.projectCwd, file.path), file.content);
    for (const [path, content] of fixture.initialFiles)
      assert.equal(readFixtureFile(fixture.projectCwd, path), content, "original checkout changed");
  } finally {
    state.service.close();
    fixture.cleanup();
  }
}

type Action = RepositoryGitSimulation["inputs"]["inputTape"][number];
interface ScenarioState {
  service: RepositoryWorkspaceService;
  readonly executionSeed: string;
  readonly plans: Map<string, ProjectPlanRecord>;
  readonly worktrees: Map<string, RepositoryWorktreeRecord>;
  readonly integrations: Map<string, IntegrationAttempt>;
  readonly actionStates: Map<string, "succeeded" | "refused" | "conflicted">;
  readonly refusalCodes: Map<string, RepositoryWorkspaceErrorCode>;
  readonly conflictPaths: Set<string>;
}

function createState(service: RepositoryWorkspaceService, executionSeed: string): ScenarioState {
  return {
    service,
    executionSeed,
    plans: new Map(),
    worktrees: new Map(),
    integrations: new Map(),
    actionStates: new Map(),
    refusalCodes: new Map(),
    conflictPaths: new Set(),
  };
}

async function runAction(fixture: GitFixture, state: ScenarioState, action: Action): Promise<void> {
  const requestId = action.requestKey ?? action.actionId;
  const common = {
    requestId,
    principalId: "principal.fixture-owner",
    executionHostId: "host.fixture",
  };
  if (action.kind === "register_repository") {
    const result = await state.service.registerRepository({
      ...common,
      projectId: action.projectId,
      contextId: `context.${action.projectKey}`,
      trustedProjectCwd: fixture.projectCwd,
      displayName: "Fixture repository",
    });
    record(state, action.actionId, result);
    if (result.ok) state.worktrees.set(action.projectKey, result.value.worktree);
    return;
  }
  if (action.kind === "prepare_plan") {
    const base = required(state.worktrees, action.baseWorktreeKey);
    const result = await state.service.preparePlanRoot({
      ...common,
      projectId: base.projectId,
      contextId: action.contextId,
      planId: action.planId,
      displayName: action.planId,
      baseWorktreeId: base.worktreeId,
      expectedBaseHead: base.headCommit,
      algorithmBinding: { state: "pending" },
    });
    record(state, action.actionId, result);
    if (result.ok) {
      state.plans.set(action.planKey, result.value.plan);
      state.worktrees.set(action.planKey, result.value.worktree);
    }
    return;
  }
  if (action.kind === "prepare_child") {
    const plan = required(state.plans, action.planKey);
    const parent = required(state.worktrees, action.parentWorktreeKey);
    const result = await state.service.prepareChildWorktree({
      ...common,
      projectId: plan.projectId,
      contextId: plan.contextId,
      planId: plan.planId,
      parentWorktreeId: parent.worktreeId,
      expectedParentHead: parent.headCommit,
    });
    record(state, action.actionId, result);
    if (result.ok) state.worktrees.set(action.worktreeKey, result.value);
    return;
  }
  if (action.kind === "fixture_commit") {
    const worktree = required(state.worktrees, action.worktreeKey);
    const workspace = await resolveWorkspace(state.service, worktree);
    const head = await commitFixture(
      fixture.git,
      workspace.projectCwd,
      action.files,
      action.actionId,
    );
    const result = await state.service.recordWorktreeHead({
      ...common,
      worktreeId: worktree.worktreeId,
      expectedRevision: worktree.revision,
      expectedOldHead: worktree.headCommit,
      newHead: head,
    });
    record(state, action.actionId, result);
    if (result.ok) state.worktrees.set(action.worktreeKey, result.value);
    return;
  }
  if (action.kind === "fixture_write") {
    const worktree = required(state.worktrees, action.worktreeKey);
    const workspace = await resolveWorkspace(state.service, worktree);
    writeFixture(workspace.projectCwd, action.files);
    state.actionStates.set(action.actionId, "succeeded");
    return;
  }
  if (action.kind === "prepare_integration") {
    const plan = required(state.plans, action.planKey);
    const source = required(state.worktrees, action.sourceWorktreeKey);
    const target = required(state.worktrees, action.targetWorktreeKey);
    const result = await state.service.prepareIntegration({
      ...common,
      integrationId: action.integrationId,
      planId: plan.planId,
      sourceWorktreeId: source.worktreeId,
      targetWorktreeId: target.worktreeId,
      expectedSourceHead: source.headCommit,
      expectedTargetHead: target.headCommit,
    });
    record(state, action.actionId, result);
    if (result.ok) {
      state.integrations.set(action.integrationKey, result.value);
      if (result.value.state === "conflicted") {
        state.actionStates.set(action.actionId, "conflicted");
        for (const path of result.value.conflictPaths) state.conflictPaths.add(path);
      }
    }
    return;
  }
  if (action.kind === "fixture_resolve") {
    const integration = required(state.integrations, action.integrationKey);
    const worktreeResult = state.service.getWorktree(integration.integrationWorktreeId);
    if (!worktreeResult.ok) throw new Error(worktreeResult.error.message);
    const workspace = await state.service.resolveIntegrationWorkspace({
      integrationId: integration.integrationId,
      projectId: worktreeResult.value.projectId,
      contextId: worktreeResult.value.contextId,
      worktreeId: worktreeResult.value.worktreeId,
      executionHostId: worktreeResult.value.executionHostId,
      expectedRevision: worktreeResult.value.revision,
    });
    if (!workspace.ok) throw new Error(workspace.error.message);
    const head = await commitFixture(
      fixture.git,
      workspace.value.projectCwd,
      action.files,
      action.actionId,
    );
    const result = await state.service.recordResolution({
      ...common,
      integrationId: integration.integrationId,
      expectedRevision: integration.revision,
      resolutionCommit: head,
    });
    record(state, action.actionId, result);
    if (result.ok) state.integrations.set(action.integrationKey, result.value);
    return;
  }
  if (action.kind === "run_test") {
    const integration = required(state.integrations, action.integrationKey);
    const result = await state.service.runIntegrationTest({
      ...common,
      integrationId: integration.integrationId,
      expectedRevision: integration.revision,
      profileId: action.profileId,
    });
    record(state, action.actionId, result);
    if (result.ok) state.integrations.set(action.integrationKey, result.value);
    return;
  }
  if (action.kind === "record_review") {
    const integration = required(state.integrations, action.integrationKey);
    const result = await state.service.recordIntegrationReview({
      ...common,
      integrationId: integration.integrationId,
      expectedRevision: integration.revision,
      accepted: action.accepted,
      rationale: "Fixture reviewer accepted exact tested candidate.",
    });
    record(state, action.actionId, result);
    if (result.ok) state.integrations.set(action.integrationKey, result.value);
    return;
  }
  if (action.kind === "promote") {
    const integration = required(state.integrations, action.integrationKey);
    const result = await state.service.promoteIntegration({
      ...common,
      integrationId: integration.integrationId,
      expectedRevision: integration.revision,
    });
    record(state, action.actionId, result);
    if (result.ok) {
      state.integrations.set(action.integrationKey, result.value);
      const target = state.service.getWorktree(result.value.targetWorktreeId);
      if (target.ok) replaceWorktree(state, target.value);
    }
    return;
  }
  if (action.kind === "reopen_store") {
    state.service.close();
    state.service = openFixtureService(fixture, state.executionSeed);
    state.actionStates.set(action.actionId, "succeeded");
    return;
  }
  state.actionStates.set(action.actionId, "succeeded");
}

async function resolveWorkspace(
  service: RepositoryWorkspaceService,
  worktree: RepositoryWorktreeRecord,
) {
  const result = await service.resolveExecutionWorkspace({
    projectId: worktree.projectId,
    contextId: worktree.contextId,
    worktreeId: worktree.worktreeId,
    executionHostId: worktree.executionHostId,
    expectedRevision: worktree.revision,
  });
  if (!result.ok) throw new Error(result.error.message);
  return result.value;
}

function record<T>(
  state: ScenarioState,
  actionId: string,
  result: RepositoryWorkspaceResult<T>,
): void {
  if (result.ok) state.actionStates.set(actionId, "succeeded");
  else {
    state.actionStates.set(actionId, "refused");
    state.refusalCodes.set(actionId, result.error.code);
  }
}

function required<T>(map: ReadonlyMap<string, T>, key: string): T {
  const value = map.get(key);
  if (value === undefined) throw new Error(`missing scenario key ${key}`);
  return value;
}

function replaceWorktree(state: ScenarioState, worktree: RepositoryWorktreeRecord): void {
  for (const [key, value] of state.worktrees)
    if (value.worktreeId === worktree.worktreeId) state.worktrees.set(key, worktree);
}

function reqMessage(why: string, fix: string): string {
  return `spec://org.vibevm.zap/lens/PROP-014#verification: ${why}; fix: ${fix}`;
}
