/** Public-service execution catalog scenario. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createExecutionCatalogService } from "../execution-catalog-service/index.ts";
import type {
  ExecutionCatalogSnapshot,
  ExecutionSelection,
  ExecutionSelectionRequest,
  TaskSpecialization,
} from "./index.ts";
import {
  ExecutionCatalogStoreAccessSchema,
  openExecutionCatalogStore,
  type ExecutionCatalogStore,
} from "../execution-catalog-store/index.ts";
import { createZapMockModel, ZAP_MOCK_MODEL_ID } from "../mock-model/index.ts";
import { ExecutionCatalogProductSimulationSchema } from "../mock-simulation/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { expandCatalogFixture } from "./product-simulation.support.ts";

const scenario = ExecutionCatalogProductSimulationSchema.parse(
  JSON.parse(readFileSync(new URL("./catalog-routing.simulation.json", import.meta.url), "utf8")),
);

test("catalog routes synthetic personas through the real resolver and ZapMock process", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
  const root = await mkdtemp(join(tmpdir(), "zap-execution-catalog-"));
  const databasePath = join(root, "catalog.sqlite");
  const stores: ExecutionCatalogStore[] = [];
  let clockNow = scenario.inputs.now;
  try {
    const fixture = expandCatalogFixture(scenario);
    const projectId = ProjectIdSchema.parse(scenario.inputs.projectId);
    const contextId = WorkContextIdSchema.parse(scenario.inputs.contextId);
    const access = storeAccess(fixture.caller, projectId);
    const opened = openExecutionCatalogStore({
      databasePath,
      hostId: "host.catalog.product",
      clock: () => new Date(clockNow),
      idFactory: (() => {
        let next = 0;
        return (kind) => `${kind}.catalog-${String(++next)}`;
      })(),
    });
    assert.equal(opened.ok, true);
    if (!opened.ok) return;
    stores.push(opened.value);
    const seedRequest = seedCatalog(opened.value, access, fixture.snapshot);
    const replay = opened.value.replaceSnapshot(access, seedRequest);
    assert.equal(replay.ok, scenario.expected.replayStable);

    const service = createExecutionCatalogService({
      store: opened.value,
      access: {
        resolve: (caller) => ({ ok: true, value: storeAccess(caller, projectId) }),
      },
      authority: fixture.authority,
      clock: () => new Date(clockNow),
    });
    const initial = await service.get(fixture.caller, {});
    assert.equal(initial.ok, true);
    if (!initial.ok) return;
    assert.equal(
      new Set(initial.value.snapshot.configurations.map((value) => value.modelFamilyId)).size,
      scenario.expected.uniqueFamilyCount,
    );
    assert.equal(
      new Set(initial.value.snapshot.configurations.map((value) => value.displayName)).size ===
        initial.value.snapshot.configurations.length,
      scenario.expected.generatedNamesUnique,
    );
    assert.equal(
      initial.value.snapshot.connections.filter((connection) =>
        connection.displayName.startsWith("Synthetic Codex"),
      ).length,
      scenario.expected.sameAgentAccountCount,
    );
    assert.ok(
      initial.value.snapshot.connections.every(
        (connection) => connection.agentProduct === scenario.expected.materializedAgentProduct,
      ),
    );
    assert.equal(
      initial.value.snapshot.connections.some((connection) =>
        connection.setupGuidance.includes("codex"),
      ),
      scenario.expected.emulatedIdentitySeparate,
    );

    const renameTarget = initial.value.snapshot.configurations.find(
      (candidate) => candidate.configurationId === scenario.inputs.rename.configurationId,
    );
    assert.notEqual(renameTarget, undefined);
    if (renameTarget === undefined) return;
    const renameRequest = {
      ...mutationIdentity(initial.value.snapshot, "rename"),
      configuration: { ...renameTarget, displayName: scenario.inputs.rename.displayName },
    };
    const renamed = await service.upsertConfiguration(fixture.caller, renameRequest);
    assert.equal(renamed.ok, true, renamed.ok ? undefined : renamed.error.message);
    if (!renamed.ok) return;
    assert.equal(
      renamed.value.snapshot.configurations.find(
        (candidate) => candidate.configurationId === renameTarget.configurationId,
      )?.displayName,
      scenario.expected.renamedDisplayName,
    );
    clockNow = "2026-09-16T12:00:01.000Z";

    const dispatches: RecordingDispatch[] = [];
    const routedSelections = new Map<string, ExecutionSelection>();
    const pinned = new Map<
      string,
      {
        runId: ReturnType<typeof RunIdSchema.parse>;
        attemptId: ReturnType<typeof AttemptIdSchema.parse>;
      }
    >();
    let current = renamed.value.snapshot;
    for (const [position, route] of scenario.inputs.routeCases.entries()) {
      const preference = service.updatePreferences(fixture.caller, {
        ...mutationIdentity(current, `preference-${String(position)}`),
        preferences: {
          ...current.preferences,
          economyQuality: route.weight,
          quota: {
            ...current.preferences.quota,
            deprioritizeLowRemaining: route.quotaEnabled,
          },
        },
      });
      assert.equal(preference.ok, true);
      if (!preference.ok) return;
      current = preference.value.snapshot;
      const runId = RunIdSchema.parse(`run.catalog.${String(position)}`);
      const attemptId = AttemptIdSchema.parse(`attempt.catalog.${String(position)}`);
      const selection = await service.selectAndPin(fixture.caller, {
        projectId,
        contextId,
        runId,
        attemptId,
        clientRequestId: ClientRequestIdSchema.parse(`request.catalog.select-${String(position)}`),
        sourceEventId: `catalog.select.${String(position)}`,
        request: selectionRequest(route.specialization, `selection.catalog.${route.caseId}`),
      });
      assert.equal(
        selection.ok,
        true,
        selection.ok ? route.caseId : `${route.caseId}: ${selection.error.message}`,
      );
      if (!selection.ok) return;
      assert.equal(selection.value.selection.configurationId, route.expectedConfigurationId);
      pinned.set(route.caseId, { runId, attemptId });
      routedSelections.set(route.caseId, selection.value.selection);
      dispatches.push(
        materializeZapMockDispatch(
          seed,
          position,
          selection.value.selection,
          scenario.expected.imageArtifactRef,
        ),
      );
    }
    assert.equal(dispatches.length, scenario.expected.recordingDispatchCount);
    assert.ok(dispatches.every((dispatch) => dispatch.agentProduct === "zap_mock"));
    assert.ok(dispatches.every((dispatch) => dispatch.modelBase === ZAP_MOCK_MODEL_ID));
    assert.equal(
      dispatches.some(
        (dispatch) =>
          dispatch.emulatedModelId === scenario.expected.imageControllerModelId &&
          dispatch.artifactRefs.includes(scenario.expected.imageArtifactRef),
      ),
      true,
    );
    const imagePersona = scenario.inputs.personas.find(
      (persona) => persona.modelId === scenario.expected.imageControllerModelId,
    );
    assert.equal(imagePersona?.imageToolModelId, scenario.expected.imageToolModelId);
    assert.equal(
      candidateReasons(routedSelections.get("architecture"), "configuration.mock.claude").some(
        (reason) => reason.includes("stale"),
      ),
      scenario.expected.staleQuotaNotLow,
    );
    assert.equal(
      routedSelections.get("testing")?.configurationId === "configuration.mock.qwen",
      scenario.expected.unknownQuotaNotZero,
    );

    await verifyRefusals(service, fixture, current, projectId, contextId);
    const imagePinned = pinned.get("image");
    assert.notEqual(imagePinned, undefined);
    if (imagePinned === undefined) return;
    const disabled = await disableImageConfiguration(service, fixture.caller, current);
    assert.equal(disabled.ok, true, disabled.ok ? undefined : disabled.error.message);
    if (!disabled.ok) return;
    const revoked = await service.preview(fixture.caller, {
      projectId,
      contextId,
      request: {
        ...selectionRequest("image_generation", "selection.catalog.revoked"),
        override: {
          configurationId:
            scenario.inputs.routeCases.find((route) => route.specialization === "image_generation")
              ?.expectedConfigurationId ?? "configuration.missing",
          reason: "Verify a disabled synthetic choice cannot start future work.",
        },
      },
    });
    assert.equal(revoked.ok, true);
    if (revoked.ok)
      assert.equal(
        !revoked.value.result.ok && revoked.value.result.error.code === "override_refused",
        scenario.expected.revokedFutureDispatchRefused,
      );

    const stale = opened.value.replaceSnapshot(access, {
      ...seedRequest,
      clientRequestId: ClientRequestIdSchema.parse("request.catalog.seed-stale"),
      sourceEventId: "catalog.seed.stale",
      requestDigest: digest("stale-seed"),
    });
    assert.equal(!stale.ok, scenario.expected.staleRevisionRefused);
    opened.value.close();

    const reopened = openExecutionCatalogStore({
      databasePath,
      hostId: "host.catalog.product",
      clock: () => new Date(clockNow),
    });
    assert.equal(reopened.ok, true);
    if (!reopened.ok) return;
    stores.push(reopened.value);
    const persisted = reopened.value.readSnapshot(access, projectId);
    assert.equal(persisted.ok, scenario.expected.coldReopenStable);
    const selection = reopened.value.readSelection(
      access,
      projectId,
      contextId,
      imagePinned.runId,
      imagePinned.attemptId,
    );
    assert.equal(selection.ok, scenario.expected.pinnedSelectionPreserved);
    if (selection.ok)
      assert.equal(selection.value.selection.modelId, scenario.expected.imageControllerModelId);
    const reopenedService = createExecutionCatalogService({
      store: reopened.value,
      access: { resolve: (caller) => ({ ok: true, value: storeAccess(caller, projectId) }) },
      authority: fixture.authority,
      clock: () => new Date(clockNow),
    });
    const replayedRename = await reopenedService.upsertConfiguration(fixture.caller, renameRequest);
    assert.deepEqual(replayedRename, renamed);
    const conflictingReplay = await reopenedService.upsertConfiguration(fixture.caller, {
      ...renameRequest,
      configuration: { ...renameRequest.configuration, displayName: "Conflicting replay" },
    });
    assert.equal(
      !conflictingReplay.ok && conflictingReplay.error.code === "idempotency_conflict",
      true,
    );
    reopened.value.close();
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed,
        passed: true,
        inputPosition: scenario.inputs.routeCases.length,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    for (const store of stores) store.close();
    await rm(root, { recursive: true, force: true });
  }
});

function seedCatalog(
  store: ExecutionCatalogStore,
  access: ReturnType<typeof storeAccess>,
  snapshot: ReturnType<typeof expandCatalogFixture>["snapshot"],
) {
  const request = {
    clientRequestId: ClientRequestIdSchema.parse("request.catalog.seed"),
    requestDigest: digest(snapshot),
    sourceEventId: "catalog.seed.1",
    expectedCatalogRevision: DecimalSchema.parse("0"),
    expectedPreferencesRevision: DecimalSchema.parse("0"),
    operation: "configuration_upsert" as const,
    subjectId: "catalog.synthetic.fixture",
    snapshot,
  };
  const seeded = store.replaceSnapshot(access, request);
  assert.equal(seeded.ok, true);
  return request;
}

function storeAccess(
  caller: ReturnType<typeof expandCatalogFixture>["caller"],
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
) {
  return ExecutionCatalogStoreAccessSchema.parse({
    ...caller,
    hostId: "host.catalog.product",
    authorizedProjectIds: caller.authorizedProjectIds.includes(projectId)
      ? [projectId]
      : caller.authorizedProjectIds,
    catalogAdministrator: true,
  });
}

function mutationIdentity(
  snapshot: Pick<ExecutionCatalogSnapshot, "catalogRevision" | "preferencesRevision">,
  label: string,
) {
  return {
    clientRequestId: ClientRequestIdSchema.parse(`request.catalog.${label}`),
    sourceEventId: `catalog.${label}`,
    expectedCatalogRevision: snapshot.catalogRevision,
    expectedPreferencesRevision: snapshot.preferencesRevision,
  };
}

function selectionRequest(
  specialization: TaskSpecialization,
  selectionRef: string,
): Omit<ExecutionSelectionRequest, "requestedAt"> {
  return {
    selectionRef,
    specialization:
      scenario.inputs.routeCases.find((candidate) => candidate.specialization === specialization)
        ?.specialization ?? "general",
    purpose: "development_implementation" as const,
    taskClass: "change" as const,
    role: "worker" as const,
    executionMode: "managed" as const,
    invocationScope: "managed_agent" as const,
    productId: "product.zap-mock",
    productVersion: "synthetic-1",
    requiredModalities: specialization === "image_generation" ? ["text", "image_output"] : ["text"],
    effort: { mode: "unspecified" as const },
    context: { mode: "default" as const },
    override: null,
  };
}

interface RecordingDispatch {
  readonly agentProduct: "zap_mock";
  readonly modelBase: typeof ZAP_MOCK_MODEL_ID;
  readonly emulatedModelId: string;
  readonly artifactRefs: readonly string[];
}

function materializeZapMockDispatch(
  seed: string,
  position: number,
  selection: {
    readonly agentProduct: string;
    readonly modelId: string;
    readonly specialization: string;
  },
  imageArtifactRef: string,
): RecordingDispatch {
  assert.equal(selection.agentProduct, "zap_mock");
  const model = createZapMockModel({
    seed: `${seed}.${String(position)}`,
    scenario: {
      scenarioId: `scenario.catalog.${String(position)}`,
      steps: [{ kind: "ready" }, { kind: "echo", prefix: "selected: " }],
    },
  });
  assert.equal(model.ok, true);
  if (!model.ok)
    assert.fail(
      reqMessage("the deterministic ZapMock process did not materialize", "repair the fixture"),
    );
  assert.equal(
    model.value.dispatch({ kind: "start", inputId: `input.start.${String(position)}` }).ok,
    true,
  );
  const turn = model.value.dispatch({
    kind: "message",
    inputId: `input.message.${String(position)}`,
    messageId: `message.catalog.${String(position)}`,
    text: selection.modelId,
  });
  assert.equal(turn.ok, true);
  return {
    agentProduct: "zap_mock",
    modelBase: model.value.state().modelId,
    emulatedModelId: selection.modelId,
    artifactRefs: selection.specialization === "image_generation" ? [imageArtifactRef] : [],
  };
}

async function verifyRefusals(
  service: ReturnType<typeof createExecutionCatalogService>,
  fixture: ReturnType<typeof expandCatalogFixture>,
  snapshot: ReturnType<typeof expandCatalogFixture>["snapshot"],
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
) {
  const foreign = await service.preview(fixture.caller, {
    projectId: ProjectIdSchema.parse(scenario.inputs.foreignProjectId),
    contextId,
    request: selectionRequest("general", "selection.catalog.foreign"),
  });
  assert.equal(!foreign.ok, scenario.expected.foreignProjectRefused);
  const ultra = await service.preview(fixture.caller, {
    projectId,
    contextId,
    request: {
      ...selectionRequest("image_generation", "selection.catalog.effort"),
      effort: { mode: "explicit", value: "ultra" },
    },
  });
  assert.equal(ultra.ok, true);
  if (ultra.ok) assert.equal(!ultra.value.result.ok, scenario.expected.unsupportedEffortRefused);
  const context = await service.preview(fixture.caller, {
    projectId,
    contextId,
    request: {
      ...selectionRequest("image_generation", "selection.catalog.context"),
      context: { mode: "explicit", tokens: 999 },
    },
  });
  assert.equal(context.ok, true);
  if (context.ok)
    assert.equal(!context.value.result.ok, scenario.expected.unsupportedContextRefused);
  assert.ok(snapshot.usage.some((value) => value.status === "unknown"));
  assert.ok(snapshot.usage.some((value) => value.observedAt < scenario.inputs.now));
}

async function disableImageConfiguration(
  service: ReturnType<typeof createExecutionCatalogService>,
  caller: ReturnType<typeof expandCatalogFixture>["caller"],
  snapshot: ReturnType<typeof expandCatalogFixture>["snapshot"],
) {
  const image = snapshot.configurations.find((configuration) =>
    configuration.modalities.includes("image_output"),
  );
  if (image === undefined)
    assert.fail(
      reqMessage(
        "the image route has no image-output configuration",
        "add the Sol controller and its image tool capability to the fixture",
      ),
    );
  return service.upsertConfiguration(caller, {
    ...mutationIdentity(snapshot, "disable-image"),
    configuration: { ...image, enabled: false },
  });
}

function digest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function candidateReasons(
  selection: ExecutionSelection | undefined,
  configurationId: string,
): readonly string[] {
  return (
    selection?.explanations.find((candidate) => candidate.configurationId === configurationId)
      ?.reasons ?? []
  );
}

function reqMessage(why: string, fix: string): string {
  return `violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: ${why}; fix surface: ${fix}`;
}
