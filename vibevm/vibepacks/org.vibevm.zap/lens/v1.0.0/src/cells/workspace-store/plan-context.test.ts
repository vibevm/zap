/** Additive plan-context persistence proof. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import {
  ActorIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
} from "../protocol/index.ts";
import {
  ProjectPlanRecordSchema,
  RepositoryWorktreeRecordSchema,
} from "../repository-model/index.ts";
import { WorkspaceStoreSimulationSchema } from "../mock-simulation/index.ts";
import { ProductPlanContextSchema, ProductProjectSchema } from "../workspace-model/index.ts";
import { createProductAppService, openProductAppRegistry } from "../product-app/index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  CoordinatorSessionSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import {
  openWorkspaceStore,
  ExistingContextPlanningBindingSchema,
  TrustedPlanContextRegistrationSchema,
  TrustedProjectRegistrationSchema,
} from "./index.ts";

const fixturePath = fileURLToPath(new URL("./plan-context.simulation.json", import.meta.url));
const fixture = WorkspaceStoreSimulationSchema.parse(JSON.parse(readFileSync(fixturePath, "utf8")));
const now = "2026-09-16T00:00:00.000Z";
const commit = "0".repeat(40);

test("one project adds an exact isolated plan context without changing its default", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? fixture.scenarioId, fixture.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? fixture.seed;
  const root = mkdtempSync(join(tmpdir(), "zap-plan-context-"));
  const databasePath = join(root, "workspace.sqlite");
  const baseCwd = join(root, "registered");
  const planCwd = join(root, "plan-root");
  mkdirSync(baseCwd);
  mkdirSync(planCwd);
  const base = baseRegistration(baseCwd);
  let store = opened(databasePath);
  assert.equal(store.registerProject(base).ok, true);
  store.close();
  const raw = new DatabaseSync(databasePath);
  raw.exec("DELETE FROM workspace_context_launch_options");
  raw.close();
  store = opened(databasePath);
  assert.equal(store.registerProject(base).ok, fixture.expected.legacyDigestStable);
  const prepared = planRegistration(planCwd);
  const registered = store.registerPlanContext(prepared);
  assert.equal(registered.ok, true, JSON.stringify(registered));
  if (!registered.ok) return;
  assert.equal(registered.value.context.planId, fixture.inputs.planId);
  assert.equal(registered.value.context.repositoryId, fixture.inputs.repositoryId);
  assert.equal(registered.value.context.rootWorktreeId, fixture.inputs.rootWorktreeId);
  assert.equal(registered.value.context.planning.state, "unavailable");
  assert.equal(registered.value.coordinatorLaunchOptions[0]?.profileId, "profile.parallel");
  const planningBinding = ExistingContextPlanningBindingSchema.parse({
    projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    contextId: WorkContextIdSchema.parse(fixture.inputs.planContextId),
    expectedRevision: registered.value.context.revision,
    planning: {
      state: "configured",
      storeId: "store.parallel",
      campaignId: "campaign.parallel",
      baseId: "base.parallel",
    },
  });
  const planningBound = store.bindExistingContextPlanning(planningBinding);
  assert.equal(planningBound.ok, true);
  if (planningBound.ok) assert.equal(planningBound.value.planning.state, "configured");
  assert.equal(store.bindExistingContextPlanning(planningBinding).ok, true);
  const replay = store.registerPlanContext(prepared);
  assert.equal(replay.ok, fixture.expected.idempotentReplay);
  const changed = TrustedPlanContextRegistrationSchema.parse({
    ...prepared,
    context: { ...prepared.context, displayName: "Changed replay" },
  });
  assert.equal(store.registerPlanContext(changed).ok, false);
  const baseLaunch = store.resolveProjectLaunch(
    ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    WorkContextIdSchema.parse(fixture.inputs.baseContextId),
  );
  const planLaunch = store.resolveProjectLaunch(
    ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    WorkContextIdSchema.parse(fixture.inputs.planContextId),
  );
  assert.ok(baseLaunch.ok && planLaunch.ok);
  if (!baseLaunch.ok || !planLaunch.ok) return;
  assert.equal(baseLaunch.value.cwd, baseCwd);
  assert.equal(baseLaunch.value.launchProfileRef, "profile.default");
  assert.equal(planLaunch.value.cwd, planCwd);
  assert.equal(planLaunch.value.launchProfileRef, "profile.parallel");
  assert.equal(planLaunch.value.agentScope?.workspaceId, "workspace.parallel");
  assert.equal(
    baseLaunch.value.cwd !== planLaunch.value.cwd,
    fixture.expected.exactContextLaunches,
  );
  assert.equal(
    store.upsertCoordinatorSession(session("main", fixture.inputs.baseContextId)).ok,
    true,
  );
  assert.equal(
    store.upsertCoordinatorSession(session("parallel", fixture.inputs.planContextId)).ok,
    true,
  );
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.plan-context"),
    actorId: ActorIdSchema.parse("actor.plan-context"),
    clientId: ClientIdSchema.parse("client.plan-context"),
    authorizedProjectIds: [ProjectIdSchema.parse(fixture.inputs.baseProjectId)],
  });
  const project = store.read(access, {
    operation: "project.get.v1",
    projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
  });
  assert.equal(project.ok, true);
  if (!project.ok || project.value.operation !== "project.get.v1") return;
  assert.equal(project.value.detail.contexts.length, fixture.expected.contextCount);
  assert.equal(
    project.value.detail.project.defaultContextId === fixture.inputs.baseContextId,
    fixture.expected.defaultContextPreserved,
  );
  assert.equal(
    project.value.detail.coordinator?.contextId === fixture.inputs.baseContextId,
    fixture.expected.contextScopedCoordinator,
  );
  assert.equal(project.value.detail.coordinatorLaunchOptions[0]?.profileId, "profile.default");
  const selectedParallel = store.read(access, {
    operation: "project.get.v1",
    projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    contextId: WorkContextIdSchema.parse(fixture.inputs.planContextId),
  });
  assert.equal(selectedParallel.ok, true);
  if (selectedParallel.ok && selectedParallel.value.operation === "project.get.v1") {
    assert.equal(
      selectedParallel.value.detail.coordinator?.contextId,
      fixture.inputs.planContextId,
    );
    assert.equal(
      selectedParallel.value.detail.coordinatorLaunchOptions[0]?.profileId,
      "profile.parallel",
    );
  }
  assert.equal(
    store.read(access, {
      operation: "project.get.v1",
      projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
      contextId: WorkContextIdSchema.parse("context.plan-context.unknown"),
    }).ok,
    false,
  );
  const parallelContext = store.read(access, {
    operation: "context.get.v1",
    projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    contextId: WorkContextIdSchema.parse(fixture.inputs.planContextId),
  });
  assert.equal(parallelContext.ok, true);
  if (parallelContext.ok && parallelContext.value.operation === "context.get.v1")
    assert.equal(parallelContext.value.context.planId, fixture.inputs.planId);
  const registryPath = join(root, "product-projects.json");
  const registry = openProductAppRegistry(registryPath);
  assert.equal(registry.ok, true);
  if (!registry.ok) return;
  assert.equal(
    registry.value.put({
      project: ProductProjectSchema.parse({
        projectId: fixture.inputs.baseProjectId,
        contextId: fixture.inputs.baseContextId,
        displayName: "Plan context project",
        directoryPath: baseCwd,
        profileId: "profile.default",
        registeredAt: now,
      }),
      registration: base,
      requestDigest: "a".repeat(64),
      plans: [],
    }).ok,
    true,
  );
  const product = createProductAppService({
    registry: registry.value,
    workspaceStore: store,
    providers: [],
  });
  assert.equal(product.ok, true);
  if (!product.ok) return;
  const productPlan = ProductPlanContextSchema.parse({
    planId: fixture.inputs.planId,
    contextId: fixture.inputs.planContextId,
    displayName: "Parallel plan",
    profileId: "profile.parallel",
    rootWorktreeId: fixture.inputs.rootWorktreeId,
    registeredAt: now,
  });
  assert.equal(
    (
      await product.value.registerPlanContext({
        context: productPlan,
        registration: prepared,
        requestDigest: "b".repeat(64),
      })
    ).ok,
    true,
  );
  const setup = await product.value.request({ operation: "product.setup.get.v1" });
  assert.equal(setup.ok, true);
  if (setup.ok && setup.value.operation === "product.setup.get.v1")
    assert.equal(setup.value.snapshot.projects[0]?.planContexts[0]?.planId, fixture.inputs.planId);
  store.close();
  store = opened(databasePath);
  const reopenedRegistry = openProductAppRegistry(registryPath);
  assert.equal(reopenedRegistry.ok, true);
  if (!reopenedRegistry.ok) return;
  const reopenedProduct = createProductAppService({
    registry: reopenedRegistry.value,
    workspaceStore: store,
    providers: [],
  });
  assert.equal(reopenedProduct.ok, true);
  if (!reopenedProduct.ok) return;
  assert.equal(reopenedProduct.value.hydrate().ok, true);
  assert.equal(
    reopenedProduct.value.planRegistrations().length === 1,
    fixture.expected.productCatalogRehydrates,
  );
  assert.equal(store.registerPlanContext(prepared).ok, true);
  assert.equal(
    store.resolveProjectLaunch(
      ProjectIdSchema.parse(fixture.inputs.baseProjectId),
      WorkContextIdSchema.parse(fixture.inputs.planContextId),
    ).ok,
    true,
  );
  store.close();
  rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  process.stdout.write(
    `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
      scenarioId: fixture.scenarioId,
      seed,
      passed: true,
      inputPosition: 6,
      zeroLlmInference: fixture.expected.zeroLlmInference,
    })}\n`,
  );
});

function opened(databasePath: string) {
  const result = openWorkspaceStore({ databasePath, clock: () => new Date(now) });
  assert.equal(result.ok, true);
  if (!result.ok) throw new Error();
  return result.value;
}

function baseRegistration(cwd: string) {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.plan-context",
    projectId: fixture.inputs.baseProjectId,
    displayName: "Plan context project",
    repositoryRootRefs: [fixture.inputs.repositoryId],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: fixture.inputs.baseContextId,
      displayName: "Default context",
      workspaceRef: "workspace.main",
      branchLabel: "main",
      revisionBinding: commit,
      planning: { state: "unavailable", reason: "Default fixture" },
      coordinatorConversationId: "conversation.main",
      brokerScope: { workspaceId: "workspace.main", conversationId: "conversation.main" },
    },
    coordinatorLaunchOptions: [launch("profile.default")],
    protected: { cwd, launchProfileRef: "profile.default" },
  });
}

function planRegistration(cwd: string) {
  const rootWorktree = RepositoryWorktreeRecordSchema.parse({
    worktreeId: fixture.inputs.rootWorktreeId,
    repositoryId: fixture.inputs.repositoryId,
    executionHostId: "execution-host.local",
    projectId: fixture.inputs.baseProjectId,
    contextId: fixture.inputs.planContextId,
    planId: fixture.inputs.planId,
    kind: "plan_root",
    parentWorktreeId: "worktree.registered.fixture",
    branchRef: "refs/heads/zap/plan-fixture",
    basisCommit: commit,
    headCommit: commit,
    state: "ready",
    assignments: [],
    revision: "1",
    createdAt: now,
  });
  const plan = ProjectPlanRecordSchema.parse({
    planId: fixture.inputs.planId,
    repositoryId: fixture.inputs.repositoryId,
    executionHostId: "execution-host.local",
    projectId: fixture.inputs.baseProjectId,
    contextId: fixture.inputs.planContextId,
    displayName: "Parallel plan",
    rootWorktreeId: fixture.inputs.rootWorktreeId,
    integrationTargetWorktreeId: "worktree.registered.fixture",
    algorithmBinding: { state: "pending" },
    state: "ready",
    creatorPrincipalId: "principal.plan-context",
    lastUpdatedByPrincipalId: "principal.plan-context",
    revision: "1",
    createdAt: now,
  });
  return TrustedPlanContextRegistrationSchema.parse({
    registrationId: "registration.plan-context.parallel",
    projectId: fixture.inputs.baseProjectId,
    plan,
    rootWorktree,
    context: {
      displayName: "Parallel plan",
      workspaceRef: "workspace.parallel",
      branchLabel: "zap/plan-fixture",
      revisionBinding: commit,
      planning: { state: "unavailable", reason: "Algorithm binding pending" },
      coordinatorConversationId: "conversation.parallel",
      brokerScope: { workspaceId: "workspace.parallel", conversationId: "conversation.parallel" },
    },
    coordinatorLaunchOptions: [launch("profile.parallel")],
    protected: { cwd, launchProfileRef: "profile.parallel" },
  });
}

function launch(profileId: string) {
  return {
    profileId,
    label: profileId,
    interactionKind: "structured" as const,
    availability: { state: "available" as const },
  };
}

function session(suffix: string, contextId: string) {
  return CoordinatorSessionSchema.parse({
    sessionId: AgentSessionIdSchema.parse(`session.${suffix}`),
    projectId: ProjectIdSchema.parse(fixture.inputs.baseProjectId),
    contextId: WorkContextIdSchema.parse(contextId),
    conversationId: ConversationIdSchema.parse(`conversation.${suffix}`),
    coordinatorActorId: ActorIdSchema.parse(`actor.${suffix}`),
    role: "coordinator" as const,
    launchOrigin: "lens" as const,
    interactionKind: "structured" as const,
    hostId: ExecutionHostIdSchema.parse("host.local"),
    nativeRef: null,
    terminal: { state: "unavailable" as const, reason: "native_session" as const },
    state: "ready" as const,
    actions: {},
    bootstrapBasis: null,
    revision: DecimalSchema.parse("1"),
    createdAt: now,
    updatedAt: now,
  });
}
