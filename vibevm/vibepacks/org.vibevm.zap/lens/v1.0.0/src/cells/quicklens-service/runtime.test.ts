/** @verifies spec://org.vibevm.zap/lens/PROP-002#shells */
import assert from "node:assert/strict";
import test from "node:test";
import { AgentPlanRuntimeConfigSchema, QuicklensRuntimeConfigSchema } from "./runtime.ts";

const agent = {
  protocol: "quicklens/1",
  workspaceId: "workspace.runtime",
  conversationId: "conversation.runtime",
  zap: {
    reader: {
      endpoint: "http://127.0.0.1:32192",
      credentialId: "reader.runtime",
      bearer: "fixture-reader-secret",
    },
    data: {
      endpoint: "http://127.0.0.1:32192",
      credentialId: "data.runtime",
      bearer: "fixture-data-secret",
    },
    coordinator: {
      endpoint: "http://127.0.0.1:32192",
      credentialId: "coordinator.runtime",
      bearer: "fixture-short-secret",
    },
  },
  workflowDatabasePath: "runtime.sqlite",
  specifications: [{ root: ".", include: ["**/*.xml"] }],
};

test("agent runtime accepts backend credentials but rejects human and Owner material", () => {
  assert.equal(AgentPlanRuntimeConfigSchema.safeParse(agent).success, true);
  assert.equal(
    AgentPlanRuntimeConfigSchema.safeParse({
      ...agent,
      broker: {
        endpoint: "http://127.0.0.1:1",
        humanPrincipalToken: "human-secret-token-value-123",
      },
    }).success,
    false,
  );
  assert.equal(
    AgentPlanRuntimeConfigSchema.safeParse({
      ...agent,
      zap: { ...agent.zap, owner: agent.zap.coordinator },
    }).success,
    false,
  );
});

test("UI runtime requires separate reader, Coordinator and human Owner channels", () => {
  const connection = agent.zap.coordinator;
  assert.equal(
    QuicklensRuntimeConfigSchema.safeParse({
      ...agent,
      sourceLabel: "Runtime fixture",
      broker: {
        endpoint: "http://127.0.0.1:32191",
        humanPrincipalToken: "human-principal-token-value-123",
      },
      zap: { reader: connection, data: connection, coordinator: connection, owner: connection },
      gateway: {
        namespace: "runtime-fixture",
        pairingToken: "pairing-token-value-123456789",
        host: "127.0.0.1",
        port: 0,
        allowedHosts: ["127.0.0.1"],
        allowedOrigins: ["http://127.0.0.1:4173"],
      },
    }).success,
    true,
  );
});
