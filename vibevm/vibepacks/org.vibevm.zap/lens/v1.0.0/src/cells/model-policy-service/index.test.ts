import assert from "node:assert/strict";
import test from "node:test";
import {
  ActorIdSchema,
  PrincipalIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
} from "../protocol/index.ts";
import {
  ClientIdSchema as WorkspaceClientIdSchema,
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type ProjectId,
} from "../workspace-model/index.ts";
import {
  ModelCapabilityProfileSchema,
  ModelSelectionRequestSchema,
  TrustedModelContextSchema,
  createDefaultCodexModelPolicy,
} from "../model-policy/index.ts";
import { openModelPolicyStore, type ModelPolicyStoreAccess } from "../model-policy-store/index.ts";
import { createModelPolicyService } from "./index.ts";

test("authenticated policy get/update/history enforces human project scope and CAS", () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T13:00:00.000Z"),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.service");
  const contextId = WorkContextIdSchema.parse("context.service");
  const access = accessFor("service", [projectId]);
  assert.equal(
    opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.service.init"),
      sourceEventId: "service.policy.1",
      policyId: "policy.service",
    }).ok,
    true,
  );
  const service = createModelPolicyService({
    store: opened.value,
    trustedContext: {
      resolve: () => ({
        ok: true,
        value: TrustedModelContextSchema.parse({
          capabilities: [],
          allowedProfileIds: [],
          parentSelection: null,
          actualObservation: null,
        }),
      }),
    },
  });
  const got = service.get(access, { projectId, contextId });
  assert.equal(got.ok, true);
  if (!got.ok) return;
  const update = service.update(access, {
    projectId,
    contextId,
    clientRequestId: ClientRequestIdSchema.parse("request.service.update"),
    sourceEventId: "service.policy.2",
    expectedRevision: DecimalSchema.parse("1"),
    policy: createDefaultCodexModelPolicy({ policyId: "policy.service", revision: "2" }),
  });
  assert.equal(update.ok, true);
  if (update.ok) assert.equal(update.value.change.toRevision, "2");
  const stale = service.update(access, {
    projectId,
    contextId,
    clientRequestId: ClientRequestIdSchema.parse("request.service.stale"),
    sourceEventId: "service.policy.stale",
    expectedRevision: DecimalSchema.parse("1"),
    policy: createDefaultCodexModelPolicy({ policyId: "policy.service", revision: "2" }),
  });
  assert.equal(stale.ok, false);
  const history = service.history(access, { projectId, contextId });
  assert.equal(history.ok, true);
  if (history.ok)
    assert.deepEqual(
      history.value.changes.map((change) => change.toRevision),
      ["1", "2"],
    );
  const denied = service.get(accessFor("other", []), { projectId, contextId });
  assert.equal(denied.ok, false);
  service.get(access, { projectId, contextId });
  opened.value.close();
});

test("preview keeps trusted evidence server-side and reports missing or unsupported effort", async () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T13:00:00.000Z"),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.preview");
  const contextId = WorkContextIdSchema.parse("context.preview");
  const access = accessFor("preview", [projectId]);
  assert.equal(
    opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.preview.init"),
      sourceEventId: "preview.policy.1",
      policyId: "policy.preview",
    }).ok,
    true,
  );
  const request = ModelSelectionRequestSchema.parse({
    selectionRef: "selection.preview",
    purpose: "development_implementation",
    taskClass: "change",
    role: "coordinator",
    executionMode: "native",
    invocationScope: "coordinator",
    productId: "codex",
    productVersion: "1.0",
    override: null,
  });
  const missing = createModelPolicyService({
    store: opened.value,
    trustedContext: {
      resolve: () => ({
        ok: true,
        value: TrustedModelContextSchema.parse({
          capabilities: [],
          allowedProfileIds: ["codex.big"],
          parentSelection: null,
          actualObservation: null,
        }),
      }),
    },
  });
  const missingPreview = await missing.preview(access, { projectId, contextId, request });
  assert.equal(missingPreview.ok, true);
  if (missingPreview.ok) assert.equal(missingPreview.value.result.ok, false);
  const unsupportedContext = TrustedModelContextSchema.parse({
    allowedProfileIds: ["codex.big"],
    parentSelection: null,
    actualObservation: null,
    capabilities: [
      ModelCapabilityProfileSchema.parse({
        capabilityId: "cap.unsupported",
        productId: "codex",
        productVersion: "1.0",
        executionMode: "native",
        invocationScope: "coordinator",
        modelId: "gpt-5.6-sol",
        effort: { mode: "unsupported" },
        extendedThinking: "unsupported",
        evidence: { source: "fake", observedAt: "2026-09-15T13:00:00.000Z" },
      }),
    ],
  });
  const unsupported = createModelPolicyService({
    store: opened.value,
    trustedContext: { resolve: () => ({ ok: true, value: unsupportedContext }) },
  });
  const unsupportedPreview = await unsupported.preview(access, { projectId, contextId, request });
  assert.equal(unsupportedPreview.ok, true);
  if (unsupportedPreview.ok && !unsupportedPreview.value.result.ok)
    assert.equal(unsupportedPreview.value.result.error.code, "effort_unsupported");
  opened.value.close();
});

test("service reads an immutable pinned selection", () => {
  const opened = openModelPolicyStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.pinned");
  const contextId = WorkContextIdSchema.parse("context.pinned");
  const access = accessFor("pinned", [projectId]);
  assert.equal(
    opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.pinned.init"),
      sourceEventId: "pinned.policy.1",
      policyId: "policy.pinned",
    }).ok,
    true,
  );
  const policy = createDefaultCodexModelPolicy({ policyId: "policy.pinned", revision: "1" });
  const selection = policy.taskRules[1];
  assert.notEqual(selection, undefined);
  if (selection === undefined) return;
  const resolved = opened.value.resolvePreview(access, {
    projectId,
    contextId,
    request: {
      selectionRef: "selection.pinned",
      purpose: "development_implementation",
      taskClass: "change",
      role: "coordinator",
      executionMode: "native",
      invocationScope: "coordinator",
      productId: "codex",
      productVersion: "1.0",
      override: null,
    },
    trustedContext: {
      capabilities: [
        {
          capabilityId: "cap.pinned",
          productId: "codex",
          productVersion: "1.0",
          executionMode: "native",
          invocationScope: "coordinator",
          modelId: "gpt-5.6-sol",
          effort: { mode: "configurable", allowedValues: ["high"], defaultValue: "high" },
          extendedThinking: "configurable",
          evidence: { source: "fake", observedAt: "2026-09-15T13:00:00.000Z" },
        },
      ],
      allowedProfileIds: ["codex.big"],
      parentSelection: null,
      actualObservation: null,
    },
  });
  assert.equal(resolved.ok && resolved.value.ok, true);
  if (!resolved.ok || !resolved.value.ok) return;
  const runId = RunIdSchema.parse("run.pinned");
  const attemptId = AttemptIdSchema.parse("attempt.pinned");
  assert.equal(
    opened.value.storeSelection(access, {
      projectId,
      contextId,
      runId,
      attemptId,
      clientRequestId: ClientRequestIdSchema.parse("request.pinned.store"),
      sourceEventId: "pinned.selection.1",
      selection: resolved.value.value,
    }).ok,
    true,
  );
  const service = createModelPolicyService({
    store: opened.value,
    trustedContext: {
      resolve: () => ({
        ok: true,
        value: TrustedModelContextSchema.parse({
          capabilities: [],
          allowedProfileIds: [],
          parentSelection: null,
          actualObservation: null,
        }),
      }),
    },
  });
  const read = service.selection(access, { projectId, contextId, runId, attemptId });
  assert.equal(read.ok, true);
  if (read.ok) assert.equal(read.value.selection.selection.selectionRef, "selection.pinned");
  opened.value.close();
});

function accessFor(name: string, projects: readonly ProjectId[]): ModelPolicyStoreAccess {
  return {
    principalId: PrincipalIdSchema.parse(`principal.${name}`),
    actorId: ActorIdSchema.parse(`actor.${name}`),
    clientId: WorkspaceClientIdSchema.parse(`client.${name}`),
    authorizedProjectIds: [...projects],
  };
}
