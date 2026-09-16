/** Managed-agent terminal status projection proof. @scope spec://org.vibevm.zap/lens/PROP-007#agent-network */
import assert from "node:assert/strict";
import test from "node:test";
import { AgentNetworkSchema } from "../workspace-model/index.ts";
import { reconcileManagedAgentNetwork } from "./managed-agent.ts";

test("managed agent is active only while its exact terminal is currently running", () => {
  const network = AgentNetworkSchema.parse({
    projectId: "project.managed",
    contextId: "context.managed",
    agents: [
      {
        actorId: "actor.managed",
        sessionId: "session.managed",
        projectId: "project.managed",
        contextId: "context.managed",
        role: "worker",
        parentActorId: null,
        displayName: "Managed worker",
        executionMode: "managed",
        hostId: "host.managed",
        nativeRef: null,
        terminalId: "terminal.managed",
        state: "active",
        revision: "1",
      },
    ],
    relationships: [],
    coverage: { state: "complete" },
  });
  const absent = reconcileManagedAgentNetwork(network, { state: "available", terminals: [] });
  assert.equal(absent.agents[0]?.state, "unknown");
  assert.equal(absent.coverage.state, "partial");
  const unavailable = reconcileManagedAgentNetwork(network, { state: "unavailable" });
  assert.equal(unavailable.agents[0]?.state, "unknown");
  const running = reconcileManagedAgentNetwork(network, {
    state: "available",
    terminals: [{ terminalId: "terminal.managed", state: "running" }],
  });
  assert.equal(running.agents[0]?.state, "active");
  const nativeOnly = AgentNetworkSchema.parse({
    ...network,
    agents: network.agents.map((agent) => ({
      ...agent,
      executionMode: "native",
      terminalId: null,
    })),
  });
  assert.equal(
    reconcileManagedAgentNetwork(nativeOnly, { state: "unavailable" }).coverage.state,
    "complete",
  );
  const exited = reconcileManagedAgentNetwork(network, {
    state: "available",
    terminals: [{ terminalId: "terminal.managed", state: "exited" }],
  });
  assert.equal(exited.agents[0]?.state, "stopped");
});
