import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import type { AgentHost } from "../agent-runtime/index.ts";
import { ModelSelectionRequestSchema } from "../model-policy/index.ts";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";

test("configured Wayfinder trusted routing serves model-policy preview and denies another project", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-wayfinder-policy-runtime-"));
  const projectId = ProjectIdSchema.parse("project.policy.runtime");
  const contextId = WorkContextIdSchema.parse("context.policy.runtime");
  const otherProjectId = ProjectIdSchema.parse("project.policy.other");
  const otherContextId = WorkContextIdSchema.parse("context.policy.other");
  const config = {
    version: 1,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "wayfinder",
      pairingToken: "synthetic-policy-pairing-token-0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    profiles: [profile()],
    projects: [project(projectId, contextId), project(otherProjectId, otherContextId)],
    routing: {
      profiles: [
        {
          scope: { projectId, contextId },
          profile: {
            profileId: "codex.small",
            productId: "codex",
            productVersion: "1.0",
            providerId: "openai",
            modelId: "gpt-5.6-luna",
            effort: "low",
          },
        },
      ],
      capabilities: [
        {
          profileId: "codex.small",
          capability: {
            capabilityId: "cap.policy.runtime",
            productId: "codex",
            productVersion: "1.0",
            executionMode: "native",
            invocationScope: "coordinator",
            modelId: "gpt-5.6-luna",
            effort: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
            extendedThinking: "configurable",
            evidence: { source: "configured-runtime-test", observedAt: "2026-09-15T17:00:00.000Z" },
          },
        },
      ],
      policies: [
        {
          scope: { projectId, contextId },
          policyId: "policy.runtime.configured",
          clientRequestId: ClientRequestIdSchema.parse("request.policy.runtime.configured"),
          sourceEventId: "policy.runtime.configured.1",
        },
      ],
    },
  };
  const host: AgentHost = {
    hostId: ExecutionHostIdSchema.parse("host.policy.runtime"),
    profileIds: ["profile.codex.native"],
    openCoordinator: async () => ({
      ok: false,
      error: {
        code: "unsupported",
        message: "policy proof does not launch a coordinator",
        retry: "never",
      },
    }),
  };
  const created = createWayfinderRuntime(config, { hosts: [host] });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const runtime = created.value;
  try {
    const client = runtime.service.bind({
      access: WorkspaceAccessContextSchema.parse({
        principalId: PrincipalIdSchema.parse("principal.policy.runtime"),
        actorId: null,
        clientId: ClientIdSchema.parse("client.policy.runtime"),
        authorizedProjectIds: [projectId],
      }),
      allowedActions: ["read"],
    });
    const preview = await client.read({
      operation: "model-policy.preview.v1",
      projectId,
      contextId,
      request: ModelSelectionRequestSchema.parse({
        selectionRef: "selection.policy.runtime",
        purpose: "test_agent",
        taskClass: "verification",
        role: "coordinator",
        executionMode: "native",
        invocationScope: "coordinator",
        productId: "codex",
        productVersion: "1.0",
        override: null,
      }),
    });
    assert.equal(preview.ok, true, preview.ok ? "" : preview.error.message);
    if (preview.ok && preview.value.operation === "model-policy.preview.v1") {
      assert.equal(
        preview.value.result.ok,
        true,
        preview.value.result.ok ? "" : preview.value.result.error.message,
      );
      if (preview.value.result.ok) {
        assert.equal(preview.value.result.value.modelId, "gpt-5.6-luna");
        assert.equal(preview.value.result.value.effectiveEffort.state, "explicit");
      }
    }
    const wrongProject = await client.read({
      operation: "model-policy.preview.v1",
      projectId: otherProjectId,
      contextId: otherContextId,
      request: ModelSelectionRequestSchema.parse({
        selectionRef: "selection.policy.other",
        purpose: "test_agent",
        taskClass: "verification",
        role: "coordinator",
        executionMode: "native",
        invocationScope: "coordinator",
        productId: "codex",
        productVersion: "1.0",
        override: null,
      }),
    });
    assert.equal(wrongProject.ok, false);
    if (!wrongProject.ok) assert.equal(wrongProject.error.code, "forbidden");
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
    effort: "low",
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "zap-wayfinder-policy-test",
  };
}

function project(
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
) {
  return {
    registrationId: `request.${projectId}.runtime`,
    projectId,
    displayName: projectId,
    repositoryRootRefs: [`repository.${projectId}`],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId,
      displayName: contextId,
      workspaceRef: `workspace.${projectId}`,
      branchLabel: "main",
      revisionBinding: "synthetic policy runtime test",
      planning: { state: "unavailable", reason: "policy proof does not launch an agent" },
      coordinatorConversationId: `conversation.${projectId}`,
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex.native",
        label: "Codex policy proof coordinator",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: `C:/placeholder/${projectId}`, launchProfileRef: "profile.codex.native" },
  };
}
