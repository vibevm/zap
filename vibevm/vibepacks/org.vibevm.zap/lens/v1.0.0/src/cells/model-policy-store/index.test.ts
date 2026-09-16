import assert from "node:assert/strict";
import test from "node:test";
import {
  ActorIdSchema,
  PrincipalIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
} from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type ProjectId,
} from "../workspace-model/index.ts";
import {
  createDefaultCodexModelPolicy,
  ModelCapabilityProfileSchema,
  ModelSelectionRequestSchema,
  TrustedModelContextSchema,
} from "../model-policy/index.ts";
import { openModelPolicyStore } from "./index.ts";
import type { ModelPolicyStoreAccess } from "./types.ts";

test("policy CAS, immutable history, idempotency, and project isolation", () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T12:00:00.000Z"),
    idFactory: (() => {
      let next = 0;
      return (kind: string) => `${kind}.test-${++next}`;
    })(),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const store = opened.value;
  const projectA = ProjectIdSchema.parse("project.policy-a");
  const projectB = ProjectIdSchema.parse("project.policy-b");
  const contextA = WorkContextIdSchema.parse("context.policy-a");
  const contextB = WorkContextIdSchema.parse("context.policy-b");
  const accessA = access("policy-a", [projectA]);
  const initialized = store.initializeDefaultPolicy(accessA, {
    projectId: projectA,
    contextId: contextA,
    clientRequestId: ClientRequestIdSchema.parse("request.policy.init"),
    sourceEventId: "policy.source.1",
    policyId: "policy.codex.a",
  });
  assert.equal(initialized.ok, true);
  if (!initialized.ok) return;
  assert.equal(initialized.value.policy.revision, "1");
  const replay = store.initializeDefaultPolicy(accessA, {
    projectId: projectA,
    contextId: contextA,
    clientRequestId: ClientRequestIdSchema.parse("request.policy.init"),
    sourceEventId: "policy.source.1",
    policyId: "policy.codex.a",
  });
  assert.deepEqual(replay, initialized);
  const changedReplay = store.initializeDefaultPolicy(accessA, {
    projectId: projectA,
    contextId: contextA,
    clientRequestId: ClientRequestIdSchema.parse("request.policy.init"),
    sourceEventId: "policy.source.changed",
    policyId: "policy.codex.changed",
  });
  assert.equal(changedReplay.ok, false);
  const policy2 = createDefaultCodexModelPolicy({ policyId: "policy.codex.a", revision: "2" });
  const updated = store.updatePolicy(accessA, {
    projectId: projectA,
    contextId: contextA,
    clientRequestId: ClientRequestIdSchema.parse("request.policy.update"),
    sourceEventId: "policy.source.2",
    expectedRevision: DecimalSchema.parse("1"),
    policy: policy2,
  });
  assert.equal(updated.ok, true);
  const stale = store.updatePolicy(accessA, {
    projectId: projectA,
    contextId: contextA,
    clientRequestId: ClientRequestIdSchema.parse("request.policy.stale"),
    sourceEventId: "policy.source.stale",
    expectedRevision: DecimalSchema.parse("1"),
    policy: createDefaultCodexModelPolicy({ policyId: "policy.codex.a", revision: "2" }),
  });
  assert.equal(stale.ok, false);
  const versions = store.listPolicyVersions(accessA, projectA, contextA);
  assert.equal(versions.ok, true);
  if (versions.ok)
    assert.deepEqual(
      versions.value.map((version) => version.revision),
      ["1", "2"],
    );
  const changes = store.listChanges(accessA, projectA, contextA);
  assert.equal(changes.ok, true);
  if (changes.ok)
    assert.deepEqual(
      changes.value.map((change) => change.sourceEventId),
      ["policy.source.1", "policy.source.2"],
    );
  assert.equal(store.readPolicy(accessA, projectB, contextB).ok, false);
  assert.equal(
    store.initializeDefaultPolicy(access("policy-b", [projectB]), {
      projectId: projectB,
      contextId: contextB,
      clientRequestId: ClientRequestIdSchema.parse("request.policy.b"),
      sourceEventId: "policy.b.1",
      policyId: "policy.codex.b",
    }).ok,
    true,
  );
  assert.equal(store.readPolicy(accessA, projectB, contextB).ok, false);
  store.close();
});

test("trusted resolve preview and immutable running selection pin", () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T12:00:00.000Z"),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const store = opened.value;
  const projectId = ProjectIdSchema.parse("project.selection");
  const contextId = WorkContextIdSchema.parse("context.selection");
  const accessSelection = access("selection", [projectId]);
  assert.equal(
    store.initializeDefaultPolicy(accessSelection, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.selection.init"),
      sourceEventId: "selection.policy.1",
      policyId: "policy.selection",
    }).ok,
    true,
  );
  const capabilities = TrustedModelContextSchema.parse({
    allowedProfileIds: ["codex.big"],
    parentSelection: null,
    actualObservation: null,
    capabilities: [
      ModelCapabilityProfileSchema.parse({
        capabilityId: "cap.codex.big",
        productId: "codex",
        productVersion: "1.0",
        executionMode: "native",
        invocationScope: "coordinator",
        modelId: "gpt-5.6-sol",
        effort: { mode: "configurable", allowedValues: ["high"], defaultValue: "high" },
        extendedThinking: "configurable",
        evidence: { source: "fake-adapter", observedAt: "2026-09-15T12:00:00.000Z" },
      }),
    ],
  });
  const request = ModelSelectionRequestSchema.parse({
    selectionRef: "selection.one",
    purpose: "development_implementation",
    taskClass: "change",
    role: "coordinator",
    executionMode: "native",
    invocationScope: "coordinator",
    productId: "codex",
    productVersion: "1.0",
    override: null,
  });
  const preview = store.resolvePreview(accessSelection, {
    projectId,
    contextId,
    request,
    trustedContext: capabilities,
  });
  assert.equal(preview.ok, true);
  if (!preview.ok || !preview.value.ok) return;
  assert.equal(preview.value.value.policyRevision, "1");
  const runId = RunIdSchema.parse("run.selection");
  const attemptId = AttemptIdSchema.parse("attempt.selection");
  const saved = store.storeSelection(accessSelection, {
    projectId,
    contextId,
    runId,
    attemptId,
    clientRequestId: ClientRequestIdSchema.parse("request.selection.store"),
    sourceEventId: "selection.attempt.1",
    selection: preview.value.value,
  });
  assert.equal(saved.ok, true);
  if (!saved.ok) return;
  const policy2 = createDefaultCodexModelPolicy({ policyId: "policy.selection", revision: "2" });
  assert.equal(
    store.updatePolicy(accessSelection, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.selection.policy2"),
      sourceEventId: "selection.policy.2",
      expectedRevision: DecimalSchema.parse("1"),
      policy: policy2,
    }).ok,
    true,
  );
  const pinned = store.readSelection(accessSelection, projectId, contextId, runId, attemptId);
  assert.equal(pinned.ok, true);
  if (pinned.ok) assert.equal(pinned.value.selection.policyRevision, "1");
  const conflicting = store.storeSelection(accessSelection, {
    projectId,
    contextId,
    runId,
    attemptId,
    clientRequestId: ClientRequestIdSchema.parse("request.selection.conflict"),
    sourceEventId: "selection.attempt.2",
    selection: { ...preview.value.value, selectionRef: "selection.other" },
  });
  assert.equal(conflicting.ok, false);
  store.close();
});

function access(name: string, projects: readonly ProjectId[]): ModelPolicyStoreAccess {
  return {
    principalId: PrincipalIdSchema.parse(`principal.${name}`),
    actorId: ActorIdSchema.parse(`actor.${name}`),
    clientId: ClientIdSchema.parse(`client.${name}`),
    authorizedProjectIds: [...projects],
  };
}
