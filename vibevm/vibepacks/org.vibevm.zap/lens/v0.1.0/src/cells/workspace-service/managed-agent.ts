/** Managed terminal actor projection. @scope spec://org.vibevm.zap/lens/PROP-007#agent-network */
import { createHash } from "node:crypto";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  AgentDescriptorSchema,
  AgentNetworkSchema,
  ExecutionHostIdSchema,
  type AgentNetwork,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { workspaceFailure } from "./errors.ts";
import type { Launch } from "./launch.ts";

export function reconcileManagedAgentNetwork(
  network: AgentNetwork,
  visibility:
    | {
        readonly state: "available";
        readonly terminals: readonly { readonly terminalId: string; readonly state: string }[];
      }
    | { readonly state: "unavailable" },
): AgentNetwork {
  const states = new Map(
    visibility.state === "available"
      ? visibility.terminals.map((terminal) => [terminal.terminalId, terminal.state])
      : [],
  );
  const uncertain = network.agents.some((agent) => {
    if (
      agent.executionMode !== "managed" ||
      agent.terminalId === null ||
      agent.terminalId === undefined
    )
      return false;
    const state = states.get(agent.terminalId);
    return state === undefined || state === "unknown" || state === "stopping";
  });
  return AgentNetworkSchema.parse({
    ...network,
    agents: network.agents.map((agent) => {
      if (
        agent.executionMode !== "managed" ||
        agent.terminalId === null ||
        agent.terminalId === undefined
      )
        return agent;
      const terminalState = states.get(agent.terminalId);
      return {
        ...agent,
        state:
          terminalState === "running"
            ? "active"
            : terminalState === "exited" || terminalState === "stopped"
              ? "stopped"
              : "unknown",
      };
    }),
    coverage:
      uncertain && network.coverage.state === "complete"
        ? {
            state: "partial",
            reason: "Managed terminal status requires recovery or a current runtime observation.",
          }
        : network.coverage,
  });
}

export function registerManagedTerminalAgent(input: {
  readonly store: WorkspaceStore;
  readonly request: Extract<WorkspaceCommandRequest, { operation: "terminal.start.v1" }>;
  readonly response: Extract<WorkspaceCommandResponse, { operation: "terminal.start.v1" }>;
  readonly coordinator: Launch | undefined;
  readonly now: string;
}): WorkspaceResult<null> {
  const terminal = input.response.terminal;
  if (
    terminal.projectId !== input.request.projectId ||
    terminal.contextId !== input.request.contextId ||
    terminal.terminalId !== input.request.terminalId ||
    terminal.sessionId !== input.request.sessionId ||
    terminal.runId !== input.request.runId
  ) {
    return workspaceFailure("conflict", "managed terminal returned an out-of-scope identity");
  }
  const parent =
    input.coordinator?.projectId === terminal.projectId &&
    input.coordinator.contextId === terminal.contextId
      ? ActorIdSchema.parse(input.coordinator.coordinatorActorId)
      : null;
  const actor = AgentDescriptorSchema.parse({
    actorId: ActorIdSchema.parse(`actor.managed.${digest(terminal.terminalId)}`),
    sessionId: terminal.sessionId,
    projectId: terminal.projectId,
    contextId: terminal.contextId,
    role: "worker",
    parentActorId: parent,
    displayName: input.request.profileId,
    executionMode: "managed",
    hostId: ExecutionHostIdSchema.parse("host.wayfinder.managed"),
    nativeRef: null,
    terminalId: terminal.terminalId,
    state:
      terminal.state === "running"
        ? "active"
        : terminal.state === "exited" || terminal.state === "stopped"
          ? "stopped"
          : "unknown",
    revision: DecimalSchema.parse("1"),
  });
  const saved = input.store.upsertAgent(actor);
  if (!saved.ok) return saved;
  if (parent !== null) {
    const linked = input.store.recordAgentRelationship({
      projectId: terminal.projectId,
      contextId: terminal.contextId,
      fromActorId: parent,
      toActorId: actor.actorId,
      kind: "parent",
      provenance: "server",
      sourceEventId: `managed-terminal:${terminal.terminalId}`,
      observedAt: input.now,
    });
    if (!linked.ok) return linked;
  }
  return { ok: true, value: null };
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
