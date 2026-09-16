import assert from "node:assert/strict";
import test from "node:test";
import { ModelSelectionSchema } from "../model-policy/index.ts";
import {
  ManagedWorkClaimSchema,
  type ManagedAgentBackend,
  type ManagedWorkClaim,
} from "../managed-work/index.ts";
import {
  AttemptIdSchema,
  RunIdSchema,
  TaskIdSchema,
  TerminalIdSchema,
  AgentSessionIdSchema,
  WorkspaceCommandRequestSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { createWorkspaceService } from "./index.ts";
import { access, registration } from "./index.test-support.ts";

test("authenticated workspace client owns the managed work lifecycle and scope", async () => {
  const opened = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const project = registration("managed");
  assert.equal(opened.value.registerProject(project).ok, true);
  const backend = fakeBackend(project.projectId, project.context.contextId);
  const service = createWorkspaceService({
    store: opened.value,
    adapters: emptyAdapters(),
    managedWork: backend,
  });
  const client = service.bind({
    access: access("managed", [project.projectId], "managed-client"),
    allowedActions: [
      "read",
      "managed-work.create.v1",
      "managed-work.start.v1",
      "managed-work.report.v1",
      "managed-work.review.v1",
    ],
  });
  const create = WorkspaceCommandRequestSchema.parse({
    operation: "managed-work.create.v1",
    clientRequestId: "request.managed.create",
    projectId: project.projectId,
    contextId: project.context.contextId,
    selection: {
      mode: "profile_override",
      profileId: "profile.codex.managed",
      reasonMarkdown: "Exercise the exact synthetic managed profile.",
    },
    goal: "Run the bounded scripted worker",
    expectedResult: "A typed report",
    targetRefs: [
      {
        projectId: project.projectId,
        contextId: project.context.contextId,
        domain: "work_task",
        ref: "task.synthetic",
      },
    ],
  });
  const prepared = await client.command(create);
  assert.equal(prepared.ok, true);
  if (!prepared.ok || prepared.value.operation !== "managed-work.create.v1") return;
  assert.equal(prepared.value.work.state, "prepared");
  const managedActorId = prepared.value.work.actorId;
  const network = await client.read({
    operation: "agent.network.v1",
    projectId: project.projectId,
    contextId: project.context.contextId,
  });
  assert.equal(network.ok, true);
  if (network.ok && network.value.operation === "agent.network.v1")
    assert.equal(
      network.value.network.agents.some((agent) => agent.actorId === managedActorId),
      true,
    );
  const started = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "managed-work.start.v1",
      clientRequestId: "request.managed.start",
      projectId: project.projectId,
      contextId: project.context.contextId,
      runId: prepared.value.work.runId,
      expectedRevision: prepared.value.work.revision,
    }),
  );
  assert.equal(started.ok, true);
  if (!started.ok || started.value.operation !== "managed-work.start.v1") return;
  const reported = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "managed-work.report.v1",
      clientRequestId: "request.managed.report",
      projectId: project.projectId,
      contextId: project.context.contextId,
      runId: started.value.work.runId,
      expectedRevision: started.value.work.revision,
      summaryMarkdown: "The scripted worker completed.",
      artifactRefs: [],
    }),
  );
  assert.equal(reported.ok, true);
  if (!reported.ok || reported.value.operation !== "managed-work.report.v1") return;
  const reviewed = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "managed-work.review.v1",
      clientRequestId: "request.managed.review",
      projectId: project.projectId,
      contextId: project.context.contextId,
      runId: reported.value.work.runId,
      expectedRevision: reported.value.work.revision,
      disposition: "accepted",
      commentMarkdown: "Reviewed the typed report.",
    }),
  );
  assert.equal(reviewed.ok, true);
  const listed = await client.read({
    operation: "managed-work.list.v1",
    projectId: project.projectId,
    contextId: project.context.contextId,
  });
  assert.equal(listed.ok, true);
  if (listed.ok && listed.value.operation === "managed-work.list.v1")
    assert.equal(listed.value.works[0]?.state, "accepted");
  const denied = service.bind({
    access: access("other", [], "other-client"),
    allowedActions: ["read", "managed-work.create.v1"],
  });
  const deniedResult = await denied.command(create);
  assert.equal(deniedResult.ok, false);
  service.close();
  opened.value.close();
});

function fakeBackend(projectId: ProjectId, contextId: WorkContextId): ManagedAgentBackend {
  const claims = new Map<string, ManagedWorkClaim>();
  return {
    capabilities: [],
    registerProfile: (profile) => ({ ok: true, value: profile }),
    async prepare(_access, request) {
      assert.deepEqual(request.contextRefs, []);
      assert.equal(request.parentTaskId, null);
      assert.equal(request.parentRunId, null);
      assert.equal(request.sourceBasisRef, `lens.user.${request.clientRequestId}`);
      assert.equal(request.planRevision, null);
      assert.equal(request.depth, 0);
      assert.deepEqual(request.budgets, { maximumTurns: 64, wallTimeMs: 3_600_000 });
      const claim = claimFor(projectId, contextId, request.clientRequestId);
      claims.set(claim.runId, claim);
      return { ok: true, value: claim };
    },
    get(access, runId) {
      const claim = claims.get(runId);
      return claim !== undefined && access.authorizedProjectIds.includes(projectId)
        ? { ok: true, value: claim }
        : { ok: false, error: { code: "forbidden", message: "outside scope" } };
    },
    list(access) {
      return access.authorizedProjectIds.includes(projectId)
        ? { ok: true, value: [...claims.values()] }
        : { ok: false, error: { code: "forbidden", message: "outside scope" } };
    },
    profiles() {
      return { ok: true, value: [] };
    },
    async start(_access, runId, expectedRevision) {
      return move(claims, runId, expectedRevision, "running");
    },
    async interrupt(_access, runId, expectedRevision) {
      return move(claims, runId, expectedRevision, "uncertain");
    },
    async stop(_access, runId, expectedRevision) {
      return move(claims, runId, expectedRevision, "stopped");
    },
    async reconcile(_access, runId) {
      const claim = claims.get(runId);
      return claim === undefined
        ? { ok: false, error: { code: "unavailable", message: "missing claim" } }
        : { ok: true, value: claim };
    },
    async report(_access, runId, expectedRevision, report) {
      const current = claims.get(runId);
      if (current === undefined || current.revision !== expectedRevision)
        return { ok: false, error: { code: "conflict", message: "stale claim" } };
      const next = ManagedWorkClaimSchema.parse({
        ...current,
        state: "reported",
        revision: String(Number(current.revision) + 1),
        report: { ...report, reportedAt: new Date().toISOString() },
      });
      claims.set(runId, next);
      return { ok: true, value: next };
    },
    async review(_access, runId, expectedRevision, review) {
      const current = claims.get(runId);
      if (current === undefined || current.revision !== expectedRevision)
        return { ok: false, error: { code: "conflict", message: "stale claim" } };
      const next = ManagedWorkClaimSchema.parse({
        ...current,
        state: review.disposition,
        revision: String(Number(current.revision) + 1),
        review: {
          ...review,
          reviewerActorId: "actor.human.reviewer",
          reviewedAt: new Date().toISOString(),
        },
      });
      claims.set(runId, next);
      return { ok: true, value: next };
    },
    async acknowledgeAttachment() {
      return { ok: true, value: null };
    },
  };
}

function claimFor(projectId: string, contextId: string, requestId: string): ManagedWorkClaim {
  const modelSelection = ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: "selection.managed.test",
    policyId: "policy.managed.test",
    policyRevision: "1",
    ruleId: "rule.managed.test",
    overrideRef: null,
    selectionReason: "scripted integration test",
    overrideReason: null,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: "small",
    requestedEffort: { mode: "explicit", value: "low" },
    profileId: "profile.codex.managed",
    productId: "codex",
    productVersion: "synthetic",
    providerId: "codex",
    modelId: "synthetic-script",
    capabilityId: null,
    effortCapability: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
    extendedThinking: "unsupported",
    effectiveEffort: { state: "explicit", value: "low" },
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
  return ManagedWorkClaimSchema.parse({
    taskId: TaskIdSchema.parse(`task.${requestId}`),
    runId: RunIdSchema.parse(`run.${requestId}`),
    attemptId: AttemptIdSchema.parse(`attempt.${requestId}`),
    actorId: "actor.managed.test",
    adapterSessionId: "adapter.managed.test",
    sessionId: AgentSessionIdSchema.parse(`session.${requestId}`),
    terminalId: TerminalIdSchema.parse(`terminal.${requestId}`),
    provider: "codex",
    profileId: "profile.codex.managed",
    packet: {
      taskId: TaskIdSchema.parse(`task.${requestId}`),
      parentTaskId: null,
      projectId,
      contextId,
      goal: "Run the bounded scripted worker",
      contextRefs: [],
      expectedResult: "A typed report",
      sourceBasisRef: "plan.synthetic",
      planRevision: null,
      capabilities: [],
      depth: 0,
      budgets: { maximumTurns: 2, wallTimeMs: 10_000 },
      routing: { preferredProduct: "codex", executionMode: "managed" },
    },
    targetRefs: [{ projectId, contextId, domain: "work_task", ref: "task.synthetic" }],
    modelSelection,
    state: "prepared",
    processExit: null,
    report: null,
    review: null,
    revision: "1",
  });
}

function move(
  claims: Map<string, ManagedWorkClaim>,
  runId: string,
  expectedRevision: string,
  state: ManagedWorkClaim["state"],
) {
  const current = claims.get(runId);
  if (current === undefined || current.revision !== expectedRevision)
    return Promise.resolve({
      ok: false as const,
      error: { code: "conflict" as const, message: "stale claim" },
    });
  const next = ManagedWorkClaimSchema.parse({
    ...current,
    state,
    revision: String(Number(current.revision) + 1),
  });
  claims.set(runId, next);
  return Promise.resolve({ ok: true as const, value: next });
}

function emptyAdapters() {
  return {
    resolve: async () => ({
      ok: false as const,
      error: { code: "not_found" as const, message: "unused", retry: "never" as const },
    }),
  };
}
