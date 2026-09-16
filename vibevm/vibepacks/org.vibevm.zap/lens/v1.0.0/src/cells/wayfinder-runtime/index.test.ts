import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import type { AgentHost } from "../agent-runtime/index.ts";
import {
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";

test("Wayfinder composes two registered projects behind the authenticated workspace gateway", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-wayfinder-runtime-"));
  const config = {
    version: 1 as const,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "wayfinder",
      pairingToken: "synthetic-wayfinder-pairing-token-0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    profiles: [profile()],
    modelPolicies: [
      { projectId: "project.lens", contextId: "context.lens", policyId: "policy.runtime.lens" },
      { projectId: "project.zap", contextId: "context.zap", policyId: "policy.runtime.zap" },
    ],
    web: {
      enabled: false,
      host: "127.0.0.1",
      port: 0,
      rendererRoot: "C:/placeholder/wayfinder-web",
      passwordVerifierPath: "C:/placeholder/wayfinder-password.json",
      publicOrigin: "https://quicklens.test",
      proxyProofToken: "synthetic-wayfinder-proxy-proof-123456",
      trustedProjectIds: ["project.lens"],
      role: "viewer" as const,
      webOperations: ["read" as const],
      workspaceActions: ["read" as const],
    },
    projects: [project("lens"), project("zap")],
  };
  const fakeHost: AgentHost = {
    hostId: ExecutionHostIdSchema.parse("host.wayfinder.fake"),
    profileIds: ["profile.codex.native"],
    openCoordinator: async () => ({
      ok: false,
      error: { code: "unsupported", message: "fake host: no process is started", retry: "never" },
    }),
  };
  const created = createWayfinderRuntime(config, { hosts: [fakeHost] });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const runtime = created.value;
  try {
    const started = await runtime.start();
    assert.equal(started.ok, true);
    if (!started.ok) return;
    assert.equal(started.value.web, undefined);
    const ticket = runtime.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://quicklens.test",
    });
    assert.notEqual(client, null);
    if (client === null) return;
    const projects = await client.read({ operation: "project.list.v1" });
    assert.equal(projects.ok, true);
    if (projects.ok && projects.value.operation === "project.list.v1") {
      assert.equal(projects.value.projects.length, 2);
    }
    const scoped = await client.read({
      operation: "agent.network.v1",
      projectId: ProjectIdSchema.parse("project.lens"),
      contextId: WorkContextIdSchema.parse("context.lens"),
    });
    assert.equal(scoped.ok, true);
  } finally {
    await runtime.close();
  }
});

function profile() {
  return {
    profileId: "profile.codex.native",
    executablePath: "C:/placeholder/codex.exe",
    requestTimeoutMs: 30_000,
    model: "gpt-5.6-luna",
    effort: "low" as const,
    approvalPolicy: "on-request" as const,
    sandbox: "workspace-write" as const,
    personality: "pragmatic" as const,
    serviceName: "zap-wayfinder",
  };
}

function project(name: "lens" | "zap") {
  const prefix = name === "lens" ? "lens" : "zap";
  return {
    registrationId: `request.project.${prefix}.runtime`,
    projectId: `project.${prefix}`,
    displayName: `${prefix.toUpperCase()} runtime project`,
    repositoryRootRefs: [`repository.${prefix}`],
    actions: { startCoordinator: { state: "available" as const } },
    context: {
      contextId: `context.${prefix}`,
      displayName: `${prefix} context`,
      workspaceRef: `workspace.${prefix}`,
      branchLabel: "main",
      revisionBinding: "synthetic runtime test",
      planning: { state: "unavailable" as const, reason: "ZAP adapter is absent" },
      coordinatorConversationId: `conversation.${prefix}`,
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex.native",
        label: "Codex native coordinator",
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: {
      cwd: `C:/placeholder/${prefix}`,
      launchProfileRef: "profile.codex.native",
    },
  };
}
