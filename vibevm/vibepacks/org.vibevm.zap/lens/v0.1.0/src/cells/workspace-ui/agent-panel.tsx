/** Agent network and sourced public output. @scope spec://org.vibevm.zap/lens/PROP-006#shared-clients */
import { component$, type QRL } from "@qwik.dev/core";

import type { AgentDescriptor, AgentNetwork, AgentOutputItem } from "../workspace-model/index.ts";
import { groupAgentOutput } from "./agent-output.ts";

export interface AgentPanelProps {
  readonly network: AgentNetwork;
  readonly selectedActorId: AgentDescriptor["actorId"] | null;
  readonly output: readonly AgentOutputItem[];
  readonly outputLoading: boolean;
  readonly outputError: string | null;
  readonly onSelect$: QRL<(actorId: AgentDescriptor["actorId"]) => void>;
}

export const AgentPanel = component$<AgentPanelProps>((props) => {
  const positions = networkPositions(props.network.agents);
  const selected = props.network.agents.find((agent) => agent.actorId === props.selectedActorId);
  return (
    <div class="agent-workspace">
      <section class="workspace-panel agent-network-panel">
        <div class="workspace-panel-heading">
          <div>
            <p class="eyebrow">Agent administration</p>
            <h2>Coordinator and agents</h2>
          </div>
          <span class="count-chip">{props.network.agents.length} agents</span>
        </div>
        <p class="workspace-muted">
          This network shows sourced host and broker relationships. Native children do not expose a
          terminal unless a separate managed capability exists.
        </p>
        <svg
          class="agent-network-graph"
          viewBox="0 0 420 240"
          role="img"
          aria-label="Agent network"
        >
          {props.network.relationships.map((relationship) => {
            const from = positions.get(relationship.fromActorId);
            const to = positions.get(relationship.toActorId);
            return from === undefined || to === undefined ? null : (
              <line
                key={`${relationship.fromActorId}:${relationship.toActorId}:${relationship.kind}`}
                x1={from.x}
                y1={from.y}
                x2={to.x}
                y2={to.y}
                class={`agent-network-edge agent-network-edge-${relationship.kind}`}
              />
            );
          })}
          {props.network.agents.map((agent) => {
            const point = positions.get(agent.actorId) ?? { x: 80, y: 80 };
            return (
              <g
                key={agent.actorId}
                class={`agent-node agent-node-${agent.state}`}
                role="button"
                tabindex={0}
                aria-label={`Inspect ${agent.displayName}`}
                onClick$={() => props.onSelect$(agent.actorId)}
                onKeyDown$={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void props.onSelect$(agent.actorId);
                  }
                }}
              >
                <circle
                  cx={point.x}
                  cy={point.y}
                  r={agent.role === "coordinator" ? 17 : 12}
                  class={agent.actorId === props.selectedActorId ? "agent-node-selected" : ""}
                />
                <text x={point.x} y={point.y + 28} text-anchor="middle">
                  {shortLabel(agent.displayName)}
                </text>
              </g>
            );
          })}
        </svg>
        <div class="network-legend" aria-label="Network relationship legend">
          <span class="network-legend-item network-legend-delegated">Delegated / parent</span>
          <span class="network-legend-item network-legend-message">Message</span>
        </div>
        {props.network.coverage.state === "partial" ? (
          <p class="workspace-notice">Partial network · {props.network.coverage.reason}</p>
        ) : null}
        <div class="agent-list" aria-label="Agents">
          {props.network.agents.map((agent) => (
            <button
              key={agent.actorId}
              class={`agent-list-item ${agent.actorId === props.selectedActorId ? "selected" : ""}`}
              onClick$={() => props.onSelect$(agent.actorId)}
            >
              <span class={`agent-state-dot agent-state-${agent.state}`} />
              <span>
                <strong>{agent.displayName}</strong>
                <small>
                  {agent.role} · {agent.executionMode} · {agent.state.replaceAll("_", " ")}
                </small>
              </span>
            </button>
          ))}
        </div>
      </section>

      <section class="workspace-panel agent-output-panel">
        <div class="workspace-panel-heading">
          <div>
            <p class="eyebrow">Public output</p>
            <h2>{selected?.displayName ?? "Select an agent"}</h2>
          </div>
          {selected === undefined ? null : (
            <span class="agent-mode-chip">{selected.executionMode}</span>
          )}
        </div>
        {selected === undefined ? (
          <div class="workspace-empty">
            <strong>No agent selected</strong>
            <span>Choose an agent to inspect its sourced public messages and tool status.</span>
          </div>
        ) : props.outputLoading ? (
          <p class="workspace-muted">Loading observed output…</p>
        ) : props.outputError !== null ? (
          <p class="workspace-notice">{props.outputError}</p>
        ) : props.output.length === 0 ? (
          <div class="workspace-empty">
            <strong>No captured public output</strong>
            <span>The server has not supplied public messages or observable tool status.</span>
          </div>
        ) : (
          <ol class="agent-output-list">
            {groupAgentOutput(props.output).map((item) => (
              <li key={item.key}>
                <div class="output-meta">
                  <span>{item.kind}</span>
                  <time dateTime={item.occurredAt}>{formatTime(item.occurredAt)}</time>
                </div>
                <p>{item.bodyMarkdown}</p>
                {item.deltaCount > 1 ? <small>{item.deltaCount} streamed fragments</small> : null}
                {item.artifactCount === 0 ? null : (
                  <small>{item.artifactCount} linked artifact(s)</small>
                )}
                {item.rawMetadata.length === 0 ? null : (
                  <details class="output-metadata">
                    <summary>Raw metadata</summary>
                    <pre>{item.rawMetadata.join("\n")}</pre>
                  </details>
                )}
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  );
});

function networkPositions(
  agents: readonly AgentDescriptor[],
): ReadonlyMap<AgentDescriptor["actorId"], { readonly x: number; readonly y: number }> {
  const byId = new Map(agents.map((agent) => [agent.actorId, agent]));
  const depths = new Map<AgentDescriptor["actorId"], number>();
  for (const agent of agents) {
    const seen = new Set<AgentDescriptor["actorId"]>();
    let current: AgentDescriptor | undefined = agent;
    let depth = 0;
    while (current.parentActorId !== null) {
      if (seen.has(current.actorId)) break;
      seen.add(current.actorId);
      current = byId.get(current.parentActorId);
      if (current === undefined) break;
      depth += 1;
    }
    depths.set(agent.actorId, Math.min(depth, 3));
  }
  const groups = new Map<number, AgentDescriptor[]>();
  for (const agent of [...agents].sort((left, right) =>
    left.displayName.localeCompare(right.displayName),
  )) {
    const depth = depths.get(agent.actorId) ?? 0;
    const group = groups.get(depth) ?? [];
    group.push(agent);
    groups.set(depth, group);
  }
  const result = new Map<AgentDescriptor["actorId"], { readonly x: number; readonly y: number }>();
  for (const [depth, group] of groups) {
    group.forEach((agent, index) => {
      result.set(agent.actorId, {
        x: 55 + depth * 100,
        y: group.length === 1 ? 120 : 45 + (index / Math.max(group.length - 1, 1)) * 160,
      });
    });
  }
  return result;
}

function shortLabel(value: string): string {
  return value.length <= 15 ? value : `${value.slice(0, 13)}…`;
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(
    new Date(value),
  );
}
