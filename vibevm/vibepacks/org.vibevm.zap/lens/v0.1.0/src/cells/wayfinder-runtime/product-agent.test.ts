/** Protected non-Codex launch preparation proof. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { CoordinatorStartInputSchema } from "../agent-runtime/index.ts";
import { ActorIdSchema } from "../protocol/index.ts";
import { AgentSessionIdSchema } from "../workspace-model/index.ts";
import { createProtectedEnvironmentResolver } from "../product-app/index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { trustedRegistration } from "./annotations.test-support.ts";
import { openWayfinderAgentFoundation } from "./agent.ts";
import { createProviderLaunchPreparation } from "./product-agent.ts";

test("protected environment and exact actor scope prepare a non-Codex MCP launch", async () => {
  const root = await mkdtemp(join(tmpdir(), "provider-preparation-"));
  const store = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(store.ok, true);
  if (!store.ok) return;
  const registration = trustedRegistration();
  assert.equal(store.value.registerProject(registration).ok, true);
  const foundation = openWayfinderAgentFoundation(
    {
      databasePath: join(root, "agent.sqlite"),
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.provider.preparation.synthetic.0001",
      scopes: [],
    },
    store.value,
  );
  assert.equal(foundation.ok, true);
  if (!foundation.ok) return;
  try {
    const started = await foundation.value.start();
    assert.equal(started.ok, true);
    if (!started.ok || registration.context.brokerScope === undefined) return;
    const binding = await foundation.value.ownedCoordinators.bind({
      projectId: registration.projectId,
      contextId: registration.context.contextId,
      coordinatorSessionId: AgentSessionIdSchema.parse("session.provider.preparation"),
      coordinatorActorId: ActorIdSchema.parse("actor.provider.preparation"),
      workspaceId: registration.context.brokerScope.workspaceId,
      conversationId: registration.context.brokerScope.conversationId,
    });
    assert.equal(binding.ok, true);
    if (!binding.ok) return;
    const environmentPath = join(root, "provider-environment.json");
    await writeFile(
      environmentPath,
      JSON.stringify({ OPENROUTER_API_KEY: "synthetic-not-a-real-key", PROVIDER_BASE: "local" }),
    );
    const profile = ProviderCoordinatorProfileSchema.parse({
      profileId: "profile.qwen.preparation",
      provider: "qwen_code",
      executablePath: "C:/fixture/node.exe",
      argumentPrefix: ["C:/fixture/qwen-cli.js"],
      cwd: root,
      modelId: "synthetic-model",
      effort: null,
      endpoint: null,
      environmentRef: "environment.qwen.synthetic",
      mcpConfigPath: join(root, "mcp", "qwen.json"),
      mcpCommandPath: process.execPath,
      mcpArgs: ["dist/mcp.js"],
    });
    const preparation = createProviderLaunchPreparation({
      foundation: foundation.value,
      environment: createProtectedEnvironmentResolver({
        "environment.qwen.synthetic": environmentPath,
      }),
      mcpRoot: join(root, "fallback-mcp"),
    });
    const prepared = await preparation.prepare({
      profile,
      scope: CoordinatorStartInputSchema.parse({
        coordinatorSessionId: "session.provider.preparation",
        projectId: registration.projectId,
        contextId: registration.context.contextId,
        conversationId: registration.context.coordinatorConversationId,
        coordinatorActorId: binding.value.actorId,
        hostId: "host.provider.qwen",
        profileId: profile.profileId,
        modelId: profile.modelId,
        cwd: root,
        bootstrapText: "Synthetic provider preparation only.",
        bootstrapBasis: "lens.test.provider-preparation",
        agentScope: registration.context.brokerScope,
        agentBinding: binding.value,
      }),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    assert.equal(prepared.value.environment["OPENROUTER_API_KEY"], "synthetic-not-a-real-key");
    assert.match(prepared.value.environment["CODLENS_URL"] ?? "", /^http:\/\/127\.0\.0\.1:/);
    const mcp = await readFile(prepared.value.mcpConfigPath, "utf8");
    assert.match(mcp, /zap-wayfinder/);
    assert.doesNotMatch(mcp, /synthetic-not-a-real-key/);
  } finally {
    await foundation.value.close();
    store.value.close();
    await rm(root, { recursive: true, force: true });
  }
});
