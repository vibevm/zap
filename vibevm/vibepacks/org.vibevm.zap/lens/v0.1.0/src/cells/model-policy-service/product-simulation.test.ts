/** Public-service model policy scenario. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  createDefaultCodexModelPolicy,
  ModelCapabilityProfileSchema,
  ModelSelectionRequestSchema,
  TrustedModelContextSchema,
} from "../model-policy/index.ts";
import { ModelPolicyProductSimulationSchema } from "../mock-simulation/index.ts";
import { openModelPolicyStore, type ModelPolicyStoreAccess } from "../model-policy-store/index.ts";
import { ActorIdSchema, ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type ProjectId,
} from "../workspace-model/index.ts";
import { createModelPolicyService } from "./index.ts";

const scenario = ModelPolicyProductSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./selection-history-isolation.simulation.json", import.meta.url), "utf8"),
  ),
);

test("model policy service preserves selection history and project isolation", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
  const root = await mkdtemp(join(tmpdir(), "zap-model-policy-product-"));
  try {
    const opened = openModelPolicyStore({
      databasePath: join(root, "policy.sqlite"),
      clock: () => new Date("2026-09-16T12:00:00.000Z"),
      idFactory: (() => {
        let next = 0;
        return (kind) => `${kind}.product-${String(++next)}`;
      })(),
    });
    assert.equal(opened.ok, true);
    if (!opened.ok) return;
    const input = scenario.inputs;
    const projectId = ProjectIdSchema.parse(input.projectId);
    const contextId = WorkContextIdSchema.parse(input.contextId);
    const otherProjectId = ProjectIdSchema.parse(input.otherProjectId);
    const otherContextId = WorkContextIdSchema.parse(input.otherContextId);
    const access = accessFor("product", [projectId]);
    const foreignAccess = accessFor("other", [otherProjectId]);
    const initialized = opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.policy.product.init"),
      sourceEventId: "policy.product.1",
      policyId: input.policyId,
    });
    assert.equal(initialized.ok, true);
    const trusted = TrustedModelContextSchema.parse({
      allowedProfileIds: [input.profileId],
      parentSelection: null,
      actualObservation: null,
      capabilities: [
        ModelCapabilityProfileSchema.parse({
          capabilityId: "cap.policy.product",
          productId: "codex",
          productVersion: "1.0",
          executionMode: "native",
          invocationScope: "coordinator",
          modelId: input.modelId,
          effort: {
            mode: "configurable",
            allowedValues: [input.effort],
            defaultValue: input.effort,
          },
          extendedThinking: "configurable",
          evidence: {
            source: "deterministic-product-scenario",
            observedAt: "2026-09-16T12:00:00.000Z",
          },
        }),
      ],
    });
    const service = createModelPolicyService({
      store: opened.value,
      trustedContext: { resolve: () => ({ ok: true, value: trusted }) },
    });
    const updated = service.update(access, {
      projectId,
      contextId,
      clientRequestId: "request.policy.product.update-2",
      sourceEventId: "policy.product.2",
      expectedRevision: "1",
      policy: createDefaultCodexModelPolicy({ policyId: input.policyId, revision: "2" }),
    });
    assert.equal(updated.ok, true);
    const request = ModelSelectionRequestSchema.parse({
      selectionRef: input.selectionRef,
      purpose: "test_agent",
      taskClass: "verification",
      role: "coordinator",
      executionMode: "native",
      invocationScope: "coordinator",
      productId: "codex",
      productVersion: "1.0",
      override: null,
    });
    const preview = await service.preview(access, { projectId, contextId, request });
    assert.equal(preview.ok, true);
    if (!preview.ok || !preview.value.result.ok) return;
    assert.equal(preview.value.result.value.modelId, scenario.expected.selectedModelId);
    assert.equal(
      preview.value.result.value.policyRevision,
      scenario.expected.selectedPolicyRevision,
    );
    const runId = RunIdSchema.parse(input.runId);
    const attemptId = AttemptIdSchema.parse(input.attemptId);
    const stored = opened.value.storeSelection(access, {
      projectId,
      contextId,
      runId,
      attemptId,
      clientRequestId: ClientRequestIdSchema.parse("request.policy.product.selection"),
      sourceEventId: "selection.product.1",
      selection: preview.value.result.value,
    });
    assert.equal(stored.ok, true);
    const updatedAgain = service.update(access, {
      projectId,
      contextId,
      clientRequestId: "request.policy.product.update-3",
      sourceEventId: "policy.product.3",
      expectedRevision: "2",
      policy: createDefaultCodexModelPolicy({ policyId: input.policyId, revision: "3" }),
    });
    assert.equal(updatedAgain.ok, true);
    const history = service.history(access, { projectId, contextId });
    assert.equal(history.ok, true);
    if (history.ok)
      assert.deepEqual(
        history.value.versions.map((version) => version.revision),
        scenario.expected.historyRevisions,
      );
    const selection = service.selection(access, { projectId, contextId, runId, attemptId });
    assert.equal(selection.ok, true);
    if (selection.ok)
      assert.equal(
        selection.value.selection.selection.policyRevision ===
          scenario.expected.selectedPolicyRevision,
        scenario.expected.selectionRemainsPinned,
      );
    const initializedOther = opened.value.initializeDefaultPolicy(foreignAccess, {
      projectId: otherProjectId,
      contextId: otherContextId,
      clientRequestId: ClientRequestIdSchema.parse("request.policy.other.init"),
      sourceEventId: "policy.other.1",
      policyId: "policy.other",
    });
    assert.equal(initializedOther.ok, true);
    const refused = service.get(access, {
      projectId: otherProjectId,
      contextId: otherContextId,
    });
    assert.equal(!refused.ok, scenario.expected.foreignProjectRefused);
    if (!refused.ok) assert.equal(refused.error.code, "forbidden");
    const own = service.get(foreignAccess, {
      projectId: otherProjectId,
      contextId: otherContextId,
    });
    assert.equal(own.ok, true);
    opened.value.close();
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed,
        passed: true,
        inputPosition: 7,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function accessFor(name: string, projects: readonly ProjectId[]): ModelPolicyStoreAccess {
  return {
    principalId: PrincipalIdSchema.parse(`principal.${name}`),
    actorId: ActorIdSchema.parse(`actor.${name}`),
    clientId: ClientIdSchema.parse(`client.${name}`),
    authorizedProjectIds: [...projects],
  };
}
