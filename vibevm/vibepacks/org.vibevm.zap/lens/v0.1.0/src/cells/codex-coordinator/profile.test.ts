/** @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { modelParameters } from "./helpers.ts";
import { CodexCoordinatorProfileSchema } from "./profile.ts";
import { ActorIdSchema, ConversationIdSchema, WorkspaceIdSchema } from "../protocol/index.ts";

test("unconfigured Lens MCP preserves host approval defaults", () => {
  const profile = CodexCoordinatorProfileSchema.parse({
    profileId: "profile.no-preapproval",
    executablePath: resolve("codex.exe"),
    requestTimeoutMs: 5_000,
    model: "gpt-test",
    effort: "low",
    approvalPolicy: "never",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "wayfinder-test",
    lensMcp: {
      serverName: "codlens",
      commandPath: resolve("codlens.exe"),
      brokerUrl: "http://127.0.0.1:32191",
      credentialFile: resolve("fixture-credentials.json"),
    },
  });
  const parameters = modelParameters(
    profile.model,
    profile.effort,
    profile,
    {
      workspaceId: WorkspaceIdSchema.parse("workspace.test"),
      conversationId: ConversationIdSchema.parse("conversation.test"),
    },
    {
      actorId: ActorIdSchema.parse("actor.test"),
      adapterSessionId: "adapter.owned.test.00000001",
    },
  );
  assert.doesNotMatch(JSON.stringify(parameters), /approval_mode|default_tools_approval_mode/);
});

test("one registered profile resolves a distinct credential file for each trusted scope", () => {
  const fileA = resolve("scope-a.json");
  const fileB = resolve("scope-b.json");
  const profile = CodexCoordinatorProfileSchema.parse({
    profileId: "profile.scoped-mcp",
    executablePath: resolve("codex.exe"),
    requestTimeoutMs: 5_000,
    model: "gpt-test",
    approvalPolicy: "never",
    sandbox: "workspace-write",
    lensMcp: {
      serverName: "codlens",
      commandPath: resolve("codlens.exe"),
      brokerUrl: "http://127.0.0.1:32191",
      scopeCredentials: [
        { workspaceId: "workspace.a", conversationId: "conversation.a", credentialFile: fileA },
        { workspaceId: "workspace.b", conversationId: "conversation.b", credentialFile: fileB },
      ],
    },
  });
  const parameters = (workspaceId: string, conversationId: string) =>
    modelParameters(
      profile.model,
      null,
      profile,
      {
        workspaceId: WorkspaceIdSchema.parse(workspaceId),
        conversationId: ConversationIdSchema.parse(conversationId),
      },
      {
        actorId: ActorIdSchema.parse(`actor.${workspaceId}`),
        adapterSessionId: `adapter.${workspaceId}.00000001`,
      },
    );
  const a = JSON.stringify(parameters("workspace.a", "conversation.a"));
  const b = JSON.stringify(parameters("workspace.b", "conversation.b"));
  const encodedA = JSON.stringify(fileA).slice(1, -1);
  const encodedB = JSON.stringify(fileB).slice(1, -1);
  assert.equal(a.includes(encodedA), true);
  assert.equal(a.includes(encodedB), false);
  assert.equal(b.includes(encodedB), true);
  assert.match(a, /disabled_tools.*codlens_connect/);
});

test("documented Codex host context is passed as a host limit", () => {
  const profile = CodexCoordinatorProfileSchema.parse({
    profileId: "profile.context",
    executablePath: resolve("codex.exe"),
    requestTimeoutMs: 5_000,
    accountBindingId: "binding.codex.context",
    model: "gpt-test",
    contextWindowTokens: 200_000,
    approvalPolicy: "never",
    sandbox: "workspace-write",
  });
  const parameters = modelParameters(profile.model, null, profile);
  assert.deepEqual(parameters, {
    model: "gpt-test",
    config: { model_context_window: 200_000 },
  });
});
