/** Normal managed-worker policy selection. @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createDefaultCodexModelPolicy, ModelPolicySchema } from "../model-policy/index.ts";
import { openModelPolicyStore, ModelPolicyStoreAccessSchema } from "../model-policy-store/index.ts";
import type { ManagedActorBindingPort, ManagedAgentProfile } from "../managed-work/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { ClientRequestIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { openConfiguredManagedWorkRuntime } from "./managed-work.ts";
import { projectRegistration, scriptedTerminalService } from "./managed-work.fixture.ts";

test("Sol high coordinator can launch Luna low policy worker and pin it across policy changes", async () => {
  const root = await mkdtemp(join(tmpdir(), "managed-policy-"));
  const workspace = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  const policies = openModelPolicyStore({ databasePath: join(root, "policy.sqlite") });
  assert.equal(workspace.ok, true);
  assert.equal(policies.ok, true);
  if (!workspace.ok || !policies.ok) return;
  assert.equal(workspace.value.registerProject(projectRegistration()).ok, true);
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.managed.policy"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.managed.policy"),
    authorizedProjectIds: [ProjectIdSchema.parse("project.managed")],
  });
  const policyAccess = ModelPolicyStoreAccessSchema.parse(access);
  const initialized = policies.value.initializeDefaultPolicy(policyAccess, {
    projectId: ProjectIdSchema.parse("project.managed"),
    contextId: WorkContextIdSchema.parse("context.managed"),
    clientRequestId: ClientRequestIdSchema.parse("request.managed.policy.init"),
    sourceEventId: "managed.policy.init",
    policyId: "policy.managed.worker",
  });
  assert.equal(initialized.ok, true);
  if (!initialized.ok) return;
  assert.equal(
    policies.value.updatePolicy(policyAccess, {
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
      clientRequestId: ClientRequestIdSchema.parse("request.managed.policy.small"),
      sourceEventId: "managed.policy.small",
      expectedRevision: DecimalSchema.parse("1"),
      policy: workerPolicy("2", "small", "low"),
    }).ok,
    true,
  );
  const terminals = scriptedTerminalService();
  const opened = openConfiguredManagedWorkRuntime({
    profiles: [
      workerProfile(root, "small", "gpt-5.6-luna", "low"),
      workerProfile(root, "big", "gpt-5.6-sol", "high"),
    ],
    databasePath: join(root, "managed.sqlite"),
    terminals: terminals.service,
    bindings: bindings(),
    policyStore: policies.value,
    routingProvider: undefined,
    proxyPolicy: { mode: "direct" },
    environment: { resolve: () => Promise.resolve({ ok: true, value: {} }) },
    attachments: {
      prepareBeforeWork: () =>
        Promise.resolve({ ok: true, value: { state: "ready", instructions: [] } }),
      acknowledge: () => Promise.resolve({ ok: true, value: null }),
    },
    workspaceStore: workspace.value,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  try {
    const first = await opened.value.backend.prepare(access, request("first"));
    assert.equal(first.ok, true);
    if (!first.ok) return;
    assert.equal(first.value.profileId, "profile.managed.small");
    assert.equal(first.value.modelSelection.modelId, "gpt-5.6-luna");
    assert.notEqual(first.value.modelSelection.modelId, "gpt-5.6-sol");
    assert.deepEqual(first.value.modelSelection.effectiveEffort, {
      state: "explicit",
      value: "low",
    });
    assert.equal(
      policies.value.updatePolicy(policyAccess, {
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        clientRequestId: ClientRequestIdSchema.parse("request.managed.policy.big"),
        sourceEventId: "managed.policy.big",
        expectedRevision: DecimalSchema.parse("2"),
        policy: workerPolicy("3", "big", "high"),
      }).ok,
      true,
    );
    assert.equal((await opened.value.backend.start(access, first.value.runId, "1")).ok, true);
    const second = await opened.value.backend.prepare(access, request("second"));
    assert.equal(second.ok, true);
    if (!second.ok) return;
    assert.equal(second.value.profileId, "profile.managed.big");
    assert.equal(second.value.modelSelection.modelId, "gpt-5.6-sol");
    assert.deepEqual(second.value.modelSelection.effectiveEffort, {
      state: "explicit",
      value: "high",
    });
    assert.equal((await opened.value.backend.start(access, second.value.runId, "1")).ok, true);
    const retained = opened.value.backend.get(access, first.value.runId);
    assert.equal(retained.ok && retained.value.modelSelection.policyRevision, "2");
    assert.equal(retained.ok && retained.value.modelSelection.modelId, "gpt-5.6-luna");
    assert.equal(terminals.launches[0]?.includes("gpt-5.6-luna"), true);
    assert.equal(terminals.launches[0]?.includes('model_reasoning_effort="low"'), true);
    assert.equal(terminals.launches[1]?.includes("gpt-5.6-sol"), true);
    assert.equal(terminals.launches[1]?.includes('model_reasoning_effort="high"'), true);
    const overridden = await opened.value.backend.prepare(access, {
      ...request("override"),
      selection: {
        mode: "profile_override",
        profileId: "profile.managed.small",
        reasonMarkdown: "This bounded task intentionally uses the cheaper fixture profile.",
      },
    });
    assert.equal(overridden.ok, true);
    if (overridden.ok) {
      assert.equal(overridden.value.profileId, "profile.managed.small");
      assert.match(overridden.value.modelSelection.overrideRef ?? "", /^override\./);
      assert.equal(
        overridden.value.modelSelection.overrideReason,
        "This bounded task intentionally uses the cheaper fixture profile.",
      );
    }
  } finally {
    opened.value.close();
    policies.value.close();
    workspace.value.close();
    await rm(root, { recursive: true, force: true });
  }
});

function workerPolicy(revision: string, tier: "small" | "big", effort: "low" | "high") {
  const base = createDefaultCodexModelPolicy({ policyId: "policy.managed.worker", revision });
  return ModelPolicySchema.parse({
    ...base,
    taskRules: base.taskRules.map((rule) =>
      rule.ruleId === "codex.development.big-high"
        ? { ...rule, tier, effort: { mode: "explicit", value: effort } }
        : rule,
    ),
  });
}

function workerProfile(
  root: string,
  tier: "small" | "big",
  modelId: string,
  effort: "low" | "high",
): ManagedAgentProfile {
  return {
    profileId: `profile.managed.${tier}`,
    tier,
    projectId: "project.managed",
    contextId: "context.managed",
    provider: "codex",
    executablePath: "C:/fixture/codex.exe",
    argumentPrefix: [],
    cwd: "C:/fixture",
    modelId,
    effort,
    effortSupported: true,
    environmentRef: null,
    mcpConfigPath: join(root, `mcp-${tier}.json`),
    proxy: { mode: "direct" },
    capabilities: {
      provider: "codex",
      observedVersion: "0.152.1",
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
      evidence: ["Synthetic normal managed-worker profile"],
    },
  };
}

function request(name: string) {
  return {
    clientRequestId: `request.managed.policy.${name}`,
    projectId: ProjectIdSchema.parse("project.managed"),
    contextId: WorkContextIdSchema.parse("context.managed"),
    selection: { mode: "project_policy" as const },
    goal: `Run ${name} worker`,
    expectedResult: "Policy-selected worker report",
    targetRefs: [],
    contextRefs: [],
    parentTaskId: null,
    parentRunId: null,
    projectedParentActorId: null,
    sourceBasisRef: "plan.managed.policy",
    planRevision: null,
    depth: 0,
    budgets: { maximumTurns: 2, wallTimeMs: 10_000 },
  };
}

function bindings(): ManagedActorBindingPort {
  return {
    prepare: (input) =>
      Promise.resolve({
        ok: true,
        value: {
          actorId: `actor.${input.runId}`,
          adapterSessionId: `adapter.${input.runId}`,
          mcpConfigPath: input.mcpConfigPath,
          environment: {},
        },
      }),
    activate: (input) =>
      Promise.resolve({
        ok: true,
        value: { mcpConfigPath: input.mcpConfigPath, environment: {} },
      }),
  };
}
