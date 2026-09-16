/** Normal empty Wayfinder startup proof. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import assert from "node:assert/strict";
import { mkdtemp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { randomBytes } from "node:crypto";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createWorkspaceHttpConnection } from "../workspace-client/index.ts";
import { createWayfinderRuntime } from "./index.ts";

test("normal start registers tiered workers and two clients observe the project", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-product-start-"));
  const project = await mkdtemp(join(root, "project-"));
  const origin = "http://127.0.0.1:4174";
  const created = createWayfinderRuntime({
    version: 1,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "productstart",
      pairingToken: randomBytes(32).toString("base64url"),
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [origin],
    },
    agentGateway: {
      databasePath: join(root, "agent.sqlite"),
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.product.start.synthetic.0001",
      scopes: [],
    },
    managedTerminals: {
      enabled: true,
      databasePath: join(root, "terminals.sqlite"),
      profiles: [],
      outputHistoryLimit: 200,
    },
    profiles: [
      {
        profileId: "profile.product.fixture",
        executablePath: "C:/fixture/no-provider.exe",
        requestTimeoutMs: 30_000,
        model: "gpt-5.6-sol",
        effort: "high",
        approvalPolicy: "on-request",
        sandbox: "workspace-write",
        personality: "pragmatic",
        serviceName: "product-start-test",
      },
    ],
    providerCoordinatorProfiles: [
      {
        profileId: "profile.qwen.fixture",
        provider: "qwen_code",
        executablePath: process.execPath,
        argumentPrefix: [],
        cwd: root,
        modelId: "qwen-fixture-model",
        effort: null,
        endpoint: null,
        environmentRef: null,
        proxy: { mode: "direct" },
      },
    ],
    productProviders: [
      {
        profileId: "profile.product.fixture",
        provider: "codex",
        displayName: "Fixture profile",
        modelId: "gpt-5.6-sol",
        effort: "high",
        interactionKind: "structured",
        installed: true,
        configured: true,
        authenticated: "not_observed",
        launchable: true,
        evidence: ["No provider process is opened by this registration proof"],
      },
      {
        profileId: "profile.qwen.fixture",
        provider: "qwen_code",
        displayName: "Qwen fixture",
        modelId: "qwen-fixture-model",
        effort: null,
        interactionKind: "structured",
        installed: true,
        configured: true,
        authenticated: "not_observed",
        launchable: true,
        evidence: ["No provider process is opened by this registration proof"],
      },
    ],
    managedWorkerProfiles: [
      {
        profileId: "worker.codex.small",
        sourceProfileId: "profile.product.fixture",
        tier: "small",
        modelId: "gpt-5.6-luna",
        effort: "low",
      },
      {
        profileId: "worker.codex.big",
        sourceProfileId: "profile.product.fixture",
        tier: "big",
        modelId: "gpt-5.6-sol",
        effort: "high",
      },
      {
        profileId: "worker.qwen.medium",
        sourceProfileId: "profile.qwen.fixture",
        tier: "medium",
        modelId: "qwen-fixture-model",
        effort: null,
      },
    ],
    projects: [],
    modelPolicies: [],
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  t.after(() => created.value.close());
  const started = await created.value.start();
  assert.equal(started.ok, true);
  if (!started.ok) return;
  assert.deepEqual(started.value.projectIds, []);
  const firstTicket = created.value.issuePairingTicket();
  const secondTicket = created.value.issuePairingTicket();
  assert.equal(firstTicket.ok, true);
  assert.equal(secondTicket.ok, true);
  if (!firstTicket.ok || !secondTicket.ok) return;
  const gateway = `http://${started.value.host}:${String(started.value.port)}${started.value.basePath}`;
  const first = createWorkspaceHttpConnection({
    baseUrl: gateway,
    origin,
    pairingToken: firstTicket.value.ticket,
  });
  const second = createWorkspaceHttpConnection({
    baseUrl: gateway,
    origin,
    pairingToken: secondTicket.value.ticket,
  });
  assert.notEqual(first, null);
  assert.notEqual(second, null);
  if (first === null || second === null) return;
  const empty = await first.product.request({ operation: "product.setup.get.v1" });
  assert.equal(empty.ok && empty.value.operation === "product.setup.get.v1", true);
  const registered = await first.product.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.runtime.product.register"),
    directoryPath: project,
    displayName: "Runtime project",
    profileId: "profile.product.fixture",
  });
  assert.equal(registered.ok, true);
  assert.equal((await readdir(join(root, "agent.sqlite.agent-scopes"))).length, 2);
  if (!registered.ok || registered.value.operation !== "product.project.register.v1") return;
  const policy = await first.workspace.read({
    operation: "model-policy.get.v1",
    projectId: registered.value.project.projectId,
    contextId: registered.value.project.contextId,
  });
  assert.equal(policy.ok && policy.value.operation === "model-policy.get.v1", true);
  if (policy.ok && policy.value.operation === "model-policy.get.v1") {
    const medium = policy.value.policy.policy.tierBindings.find(
      (binding) => binding.tier === "medium",
    );
    assert.equal(medium?.productId, "qwen_code");
    assert.equal(medium?.providerId, "qwen_code");
    assert.equal(medium?.modelId, "qwen-fixture-model");
  }
  const managedProfiles = await first.workspace.read({
    operation: "managed-work.profile.list.v1",
    projectId: registered.value.project.projectId,
    contextId: registered.value.project.contextId,
  });
  assert.equal(
    managedProfiles.ok &&
      managedProfiles.value.operation === "managed-work.profile.list.v1" &&
      managedProfiles.value.profiles.some(
        (profile) =>
          profile.tier === "small" &&
          profile.modelId === "gpt-5.6-luna" &&
          profile.effort === "low",
      ) &&
      managedProfiles.value.profiles.some(
        (profile) =>
          profile.tier === "big" && profile.modelId === "gpt-5.6-sol" && profile.effort === "high",
      ),
    true,
    JSON.stringify(managedProfiles),
  );
  const prepared = await first.workspace.command({
    operation: "managed-work.create.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.runtime.managed.policy"),
    projectId: registered.value.project.projectId,
    contextId: registered.value.project.contextId,
    selection: { mode: "project_policy" },
    goal: "Prepare a no-model policy-selected worker",
    expectedResult: "Pinned managed selection",
    targetRefs: [],
  });
  assert.equal(
    prepared.ok &&
      prepared.value.operation === "managed-work.create.v1" &&
      prepared.value.work.modelSelection.modelId === "gpt-5.6-luna" &&
      prepared.value.work.modelSelection.effectiveEffort.state === "explicit" &&
      prepared.value.work.modelSelection.effectiveEffort.value === "low",
    true,
    JSON.stringify(prepared),
  );
  const discovered = await second.workspace.read({ operation: "project.list.v1" });
  assert.equal(
    discovered.ok &&
      discovered.value.operation === "project.list.v1" &&
      discovered.value.projects[0]?.displayName === "Runtime project",
    true,
  );
});
