import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { CodexCoordinatorProfileSchema } from "../codex-coordinator/index.ts";
import { createExecutionCatalogService } from "../execution-catalog-service/index.ts";
import {
  ExecutionCatalogStoreAccessSchema,
  openExecutionCatalogStore,
} from "../execution-catalog-store/index.ts";
import { createExecutionAccountIsolation } from "../execution-accounts/index.ts";
import {
  ManagedAgentProfileSchema,
  ManagedWorkClaimSchema,
  ManagedWorkRequestSchema,
} from "../managed-work/index.ts";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import type { DecimalSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import type { ExecutionSelection } from "../execution-catalog/index.ts";
import type { ModelSelection } from "../model-policy/index.ts";
import type { ManagedAgentProfile } from "../managed-work/index.ts";
import { createRuntimeExecutionCatalogAuthority } from "./execution-catalog.ts";
import { revalidateCatalogManaged, selectCatalogManaged } from "./managed-catalog-selection.ts";

test("managed catalog selection materializes and later revocation fences launch", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-managed-catalog-"));
  const home = join(root, "codex-home");
  const secondHome = join(root, "codex-home-second");
  await Promise.all([mkdir(home), mkdir(secondHome)]);
  const projectId = ProjectIdSchema.parse("project.catalog.test");
  const contextId = WorkContextIdSchema.parse("context.catalog.test");
  const caller = {
    principalId: PrincipalIdSchema.parse("principal.catalog.test"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.catalog.test"),
    authorizedProjectIds: [projectId],
  };
  const storeAccess = ExecutionCatalogStoreAccessSchema.parse({
    ...caller,
    hostId: "host.catalog.test",
    catalogAdministrator: true,
  });
  const accounts = createExecutionAccountIsolation([
    {
      bindingId: "binding.codex.test",
      hostId: ExecutionHostIdSchema.parse("host.catalog.test"),
      displayName: "Test Codex account",
      enabled: true,
      setupGuidance: "Synthetic protected account fixture.",
      kind: "codex_home",
      agentProduct: "codex",
      homePath: home,
    },
    {
      bindingId: "binding.codex.second",
      hostId: ExecutionHostIdSchema.parse("host.catalog.test"),
      displayName: "Second Codex account",
      enabled: true,
      setupGuidance: "Second synthetic protected account fixture.",
      kind: "codex_home",
      agentProduct: "codex",
      homePath: secondHome,
    },
  ]);
  assert.equal(accounts.ok, true);
  if (!accounts.ok) return;
  const profile = managedProfile(root, projectId, contextId);
  const opened = openExecutionCatalogStore({
    databasePath: join(root, "catalog.sqlite"),
    hostId: "host.catalog.test",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  try {
    const service = createExecutionCatalogService({
      store: opened.value,
      access: { resolve: () => ({ ok: true, value: storeAccess }) },
      authority: createRuntimeExecutionCatalogAuthority({
        accounts: accounts.value,
        codexProfiles: [codexProfile(root)],
        providerProfiles: [],
        managedProfiles: [profile],
      }),
      clock: () => new Date("2026-09-16T12:00:00.000Z"),
      idFactory: (kind) => `${kind}.catalog.test`,
    });
    const initial = await service.get(caller, {});
    assert.equal(initial.ok, true);
    if (!initial.ok) return;
    const connection = await service.createConnection(caller, {
      ...mutation(initial.value.snapshot, "connection"),
      bindingId: "binding.codex.test",
      displayName: "Test Codex account",
    });
    assert.equal(connection.ok, true);
    if (!connection.ok) return;
    const reference = initial.value.modelReferences.find(
      (candidate) => candidate.modelId === "gpt-5.6-sol",
    );
    assert.notEqual(reference, undefined);
    if (reference === undefined) return;
    const connectionRecord = connection.value.snapshot.connections[0];
    assert.notEqual(connectionRecord, undefined);
    if (connectionRecord === undefined) return;
    const configuration = await service.createConfiguration(caller, {
      ...mutation(connection.value.snapshot, "configuration"),
      connectionId: connectionRecord.connectionId,
      referenceId: reference.referenceId,
      displayName: "Sol managed test",
    });
    assert.equal(configuration.ok, true);
    if (!configuration.ok) return;
    const originalConfiguration = configuration.value.snapshot.configurations.find(
      (candidate) => candidate.connectionId === connectionRecord.connectionId,
    );
    assert.notEqual(originalConfiguration, undefined);
    if (originalConfiguration === undefined) return;
    const archivedConfiguration = await service.upsertConfiguration(caller, {
      ...mutation(configuration.value.snapshot, "archive-configuration"),
      configuration: { ...originalConfiguration, enabled: false },
    });
    assert.equal(archivedConfiguration.ok, true);
    if (!archivedConfiguration.ok) return;
    const archivedConnection = await service.upsertConnection(caller, {
      ...mutation(archivedConfiguration.value.snapshot, "archive-connection"),
      connection: { ...connectionRecord, enabled: false },
    });
    assert.equal(archivedConnection.ok, true);
    if (!archivedConnection.ok) return;
    const restoredConnection = await service.createConnection(caller, {
      ...mutation(archivedConnection.value.snapshot, "restore-connection-through-add"),
      bindingId: "binding.codex.test",
      displayName: null,
    });
    assert.equal(restoredConnection.ok, true);
    if (!restoredConnection.ok) return;
    const restoredConnectionRecord = restoredConnection.value.snapshot.connections.find(
      (candidate) => candidate.launchBindingId === "binding.codex.test",
    );
    assert.equal(restoredConnectionRecord?.connectionId, connectionRecord.connectionId);
    assert.equal(restoredConnectionRecord?.enabled, true);
    const restoredConfiguration = await service.createConfiguration(caller, {
      ...mutation(restoredConnection.value.snapshot, "restore-configuration-through-add"),
      connectionId: connectionRecord.connectionId,
      referenceId: reference.referenceId,
      displayName: null,
    });
    assert.equal(restoredConfiguration.ok, true);
    if (!restoredConfiguration.ok) return;
    const restoredConfigurationRecord = restoredConfiguration.value.snapshot.configurations.find(
      (candidate) => candidate.connectionId === connectionRecord.connectionId,
    );
    assert.equal(
      restoredConfigurationRecord?.configurationId,
      originalConfiguration.configurationId,
    );
    assert.equal(restoredConfigurationRecord?.enabled, true);
    const secondConnection = await service.createConnection(caller, {
      ...mutation(restoredConfiguration.value.snapshot, "connection-second"),
      bindingId: "binding.codex.second",
      displayName: "Second Codex account",
    });
    assert.equal(secondConnection.ok, true);
    if (!secondConnection.ok) return;
    const lunaReference = initial.value.modelReferences.find(
      (candidate) => candidate.modelId === "gpt-5.6-luna",
    );
    const secondConnectionRecord = secondConnection.value.snapshot.connections.find(
      (candidate) => candidate.launchBindingId === "binding.codex.second",
    );
    assert.notEqual(lunaReference, undefined);
    assert.notEqual(secondConnectionRecord, undefined);
    if (lunaReference === undefined || secondConnectionRecord === undefined) return;
    const secondConfiguration = await service.createConfiguration(caller, {
      ...mutation(secondConnection.value.snapshot, "configuration-second"),
      connectionId: secondConnectionRecord.connectionId,
      referenceId: lunaReference.referenceId,
      displayName: "Luna second account",
    });
    assert.equal(secondConfiguration.ok, true);
    if (!secondConfiguration.ok) return;
    const lunaConfiguration = secondConfiguration.value.snapshot.configurations.find(
      (candidate) => candidate.connectionId === secondConnectionRecord.connectionId,
    );
    assert.notEqual(lunaConfiguration, undefined);
    if (lunaConfiguration === undefined) return;
    const request = ManagedWorkRequestSchema.parse({
      clientRequestId: "request.managed.catalog.test",
      projectId,
      contextId,
      selection: {
        mode: "catalog_policy",
        effort: { mode: "explicit", value: "low" },
        context: { mode: "explicit", tokens: 1_050_000 },
      },
      specialization: "backend",
      goal: "Exercise managed catalog selection.",
      expectedResult: "A pinned catalog selection.",
      targetRefs: [],
      contextRefs: [],
      parentTaskId: null,
      parentRunId: null,
      projectedParentActorId: null,
      sourceBasisRef: "basis.catalog.test",
      planRevision: null,
      depth: 0,
      budgets: { maximumTurns: 2, wallTimeMs: 60_000 },
    });
    const runId = RunIdSchema.parse("run.catalog.test");
    const attemptId = AttemptIdSchema.parse("attempt.catalog.test");
    const selected = await selectCatalogManaged({
      catalog: { service, hostId: "host.catalog.test" },
      access: caller,
      request,
      profiles: [profile],
      identity: { runId, attemptId },
      parentSelection: null,
    });
    assert.equal(selected.ok, true);
    if (!selected.ok) return;
    assert.equal(selected.value.profile.modelId, "gpt-5.6-sol");
    assert.equal(selected.value.profile.contextWindowTokens, 1_050_000);
    assert.equal(selected.value.profile.accountBindingId, "binding.codex.test");
    const secondSelected = await selectCatalogManaged({
      catalog: { service, hostId: "host.catalog.test" },
      access: caller,
      request: ManagedWorkRequestSchema.parse({
        ...request,
        clientRequestId: "request.managed.catalog.second",
        specialization: "general",
        selection: {
          mode: "catalog_override",
          configurationId: lunaConfiguration.configurationId,
          effort: { mode: "explicit", value: "low" },
          context: { mode: "explicit", tokens: 1_050_000 },
          reasonMarkdown: "Use the second isolated account for this synthetic task.",
        },
      }),
      profiles: [profile],
      identity: {
        runId: RunIdSchema.parse("run.catalog.second"),
        attemptId: AttemptIdSchema.parse("attempt.catalog.second"),
      },
      parentSelection: null,
    });
    assert.equal(
      secondSelected.ok,
      true,
      secondSelected.ok ? undefined : secondSelected.error.message,
    );
    if (!secondSelected.ok) return;
    assert.equal(secondSelected.value.profile.modelId, "gpt-5.6-luna");
    assert.equal(secondSelected.value.profile.accountBindingId, "binding.codex.second");
    assert.notEqual(secondSelected.value.profile.profileId, selected.value.profile.profileId);
    const claim = claimFor(selected.value, request, runId, attemptId);
    const admitted = await revalidateCatalogManaged({
      catalog: { service, hostId: "host.catalog.test" },
      access: caller,
      claim,
    });
    assert.equal(admitted.ok, true);
    const current = await service.get(caller, {});
    assert.equal(current.ok, true);
    if (!current.ok) return;
    const selectedConfiguration = current.value.snapshot.configurations.find(
      (candidate) =>
        candidate.configurationId === selected.value.executionSelection.configurationId,
    );
    assert.notEqual(selectedConfiguration, undefined);
    if (selectedConfiguration === undefined) return;
    const narrowed = await service.upsertConfiguration(caller, {
      ...mutation(current.value.snapshot, "narrow-effort"),
      configuration: {
        ...selectedConfiguration,
        effort: { mode: "configurable", allowedValues: ["high"], defaultValue: "high" },
      },
    });
    assert.equal(narrowed.ok, true, narrowed.ok ? undefined : narrowed.error.message);
    const refused = await revalidateCatalogManaged({
      catalog: { service, hostId: "host.catalog.test" },
      access: caller,
      claim,
    });
    assert.equal(refused.ok, false);
    assert.equal(refused.ok ? "unexpected" : refused.error.code, "forbidden");
  } finally {
    opened.value.close();
    await rm(root, { recursive: true, force: true });
  }
});

function codexProfile(root: string) {
  return CodexCoordinatorProfileSchema.parse({
    profileId: "template.codex.coordinator",
    executablePath: join(root, "codex.exe"),
    requestTimeoutMs: 5_000,
    model: "gpt-5.6-sol",
    effort: "low",
    contextWindowTokens: 1_050_000,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    accountBindingId: "binding.codex.test",
    observedModelCapabilities: [
      {
        modelId: "gpt-5.6-sol",
        displayName: "GPT-5.6 Sol",
        supportedEfforts: ["low", "high"],
        defaultEffort: "low",
      },
    ],
    capabilityObservedAt: "2026-09-16T12:00:00.000Z",
  });
}

function managedProfile(
  root: string,
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
) {
  return ManagedAgentProfileSchema.parse({
    profileId: "template.codex.managed",
    projectId,
    contextId,
    provider: "codex",
    executablePath: join(root, "codex.exe"),
    cwd: root,
    modelId: "gpt-5.6-luna",
    effort: "low",
    effortSupported: true,
    contextWindowTokens: 1_050_000,
    accountBindingId: "binding.codex.test",
    executionHostId: "host.catalog.test",
    environmentRef: null,
    mcpConfigPath: join(root, "mcp.json"),
    capabilities: {
      provider: "codex",
      observedVersion: "fixture",
      installed: true,
      launchable: true,
      authenticated: "not_observed",
      structuredConversation: "supported",
      nativeChildren: "supported",
      nativeQuestions: "supported",
      nativeApprovals: "supported",
      interrupt: "supported",
      resume: "supported",
      interactiveTerminal: "supported",
      evidence: ["synthetic catalog-managed fixture"],
    },
  });
}

function mutation(
  snapshot: {
    readonly catalogRevision: ReturnType<typeof DecimalSchema.parse>;
    readonly preferencesRevision: ReturnType<typeof DecimalSchema.parse>;
  },
  suffix: string,
) {
  return {
    clientRequestId: ClientRequestIdSchema.parse(`request.catalog.${suffix}`),
    sourceEventId: `event.catalog.${suffix}`,
    expectedCatalogRevision: snapshot.catalogRevision,
    expectedPreferencesRevision: snapshot.preferencesRevision,
  };
}

function claimFor(
  selected: {
    readonly profile: ManagedAgentProfile;
    readonly modelSelection: ModelSelection;
    readonly executionSelection: ExecutionSelection;
  },
  request: ReturnType<typeof ManagedWorkRequestSchema.parse>,
  runId: ReturnType<typeof RunIdSchema.parse>,
  attemptId: ReturnType<typeof AttemptIdSchema.parse>,
) {
  return ManagedWorkClaimSchema.parse({
    taskId: "task.catalog.test",
    runId,
    attemptId,
    actorId: "actor.catalog.test",
    creatorActorId: null,
    parentActorId: null,
    adapterSessionId: "adapter.catalog.test.00000001",
    sessionId: "session.catalog.test",
    terminalId: "terminal.catalog.test",
    controlLeaseId: null,
    controlEpoch: null,
    provider: selected.profile.provider,
    profileId: selected.profile.profileId,
    packet: {
      taskId: "task.catalog.test",
      parentTaskId: null,
      projectId: request.projectId,
      contextId: request.contextId,
      planId: null,
      workspaceAssignment: null,
      goal: request.goal,
      specialization: request.specialization,
      contextRefs: [],
      expectedResult: request.expectedResult,
      targetRefs: [],
      sourceBasisRef: request.sourceBasisRef,
      planRevision: null,
      capabilities: [],
      depth: 0,
      budgets: request.budgets,
      routing: { preferredProduct: selected.profile.provider, executionMode: "managed" },
    },
    targetRefs: [],
    modelSelection: selected.modelSelection,
    executionSelection: selected.executionSelection,
    managedControl: null,
    state: "prepared",
    processExit: null,
    report: null,
    review: null,
    revision: "1",
  });
}
