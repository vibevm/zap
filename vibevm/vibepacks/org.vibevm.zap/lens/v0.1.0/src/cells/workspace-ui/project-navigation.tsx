/** Project focus and portfolio board. @scope spec://org.vibevm.zap/lens/PROP-005#project-views */
import { component$, type QRL } from "@qwik.dev/core";

import type { ProjectWorkspaceView } from "../workspace-client/index.ts";
import type { AgentDescriptor, ProjectDescriptor, ProjectId } from "../workspace-model/index.ts";

export interface ProjectRailProps {
  readonly projects: readonly ProjectDescriptor[];
  readonly selectedProjectId: ProjectId | null;
  readonly onSelectAll$: QRL<() => void>;
  readonly onSelectProject$: QRL<(projectId: ProjectId) => void>;
  readonly onAddProject$?: QRL<() => void> | undefined;
}

export const ProjectRail = component$<ProjectRailProps>((props) => (
  <aside class="project-rail workspace-panel">
    <div>
      <p class="eyebrow">Projects</p>
      <h2>Zap Quick Lens workspace</h2>
      <p class="workspace-muted">Planning and agent work in one shared view.</p>
    </div>
    <nav aria-label="Project focus">
      <button
        class={`project-link ${props.selectedProjectId === null ? "selected" : ""}`}
        onClick$={props.onSelectAll$}
      >
        <span class="project-monogram">∞</span>
        <span>
          <strong>All projects</strong>
          <small>Unified map and portfolio activity</small>
        </span>
      </button>
      {props.projects.map((project) => (
        <button
          key={project.projectId}
          class={`project-link ${project.projectId === props.selectedProjectId ? "selected" : ""}`}
          onClick$={() => props.onSelectProject$(project.projectId)}
        >
          <span class="project-monogram">{project.displayName.slice(0, 1).toUpperCase()}</span>
          <span>
            <strong>{project.displayName}</strong>
            <small>Revision {project.revision}</small>
          </span>
        </button>
      ))}
    </nav>
    {props.onAddProject$ === undefined ? null : (
      <button class="button secondary project-add" onClick$={props.onAddProject$}>
        Add project
      </button>
    )}
  </aside>
));

export const ProjectBoard = component$<{
  readonly projects: readonly ProjectDescriptor[];
  readonly views: Readonly<Record<string, ProjectWorkspaceView | undefined>>;
  readonly onOpen$: QRL<(projectId: ProjectId) => void>;
}>((props) => (
  <section class="project-board">
    <div class="workspace-section-heading">
      <div>
        <p class="eyebrow">Portfolio board</p>
        <h1>Projects continue independently</h1>
      </div>
      <p>Each card retains its own coordinator, agent network, plan, and event scope.</p>
    </div>
    <div class="project-card-grid">
      {props.projects.map((project) => {
        const view = props.views[project.projectId];
        return (
          <article class="project-card" key={project.projectId}>
            <div class="project-card-heading">
              <span class="project-monogram">{project.displayName.slice(0, 1).toUpperCase()}</span>
              <div>
                <h2>{project.displayName}</h2>
                <p>{view?.contexts[0]?.displayName ?? "Loading project context…"}</p>
              </div>
              <span class={`coordinator-state state-${view?.coordinator?.state ?? "stopped"}`}>
                {view?.coordinator?.state.replaceAll("_", " ") ?? "not started"}
              </span>
            </div>
            {view === undefined ? (
              <div class="workspace-empty compact">Loading project…</div>
            ) : (
              <>
                <MiniAgentGraph agents={view.network.agents} />
                <dl class="project-stats">
                  <div>
                    <dt>Agents</dt>
                    <dd>{view.network.agents.length}</dd>
                  </div>
                  <div>
                    <dt>Questions</dt>
                    <dd>{view.questions.filter((question) => question.state === "open").length}</dd>
                  </div>
                  <div>
                    <dt>Plan</dt>
                    <dd>
                      {view.snapshot.state === "ready"
                        ? view.snapshot.snapshot.phase
                        : "unavailable"}
                    </dd>
                  </div>
                </dl>
              </>
            )}
            <button class="button secondary" onClick$={() => props.onOpen$(project.projectId)}>
              Open project
            </button>
          </article>
        );
      })}
    </div>
  </section>
));

const MiniAgentGraph = component$<{ readonly agents: readonly AgentDescriptor[] }>((props) => {
  const points = miniPoints(props.agents);
  return (
    <svg
      class="mini-agent-graph"
      viewBox="0 0 280 116"
      role="img"
      aria-label="Project agent network"
    >
      {props.agents.map((agent) => {
        const child = points.get(agent.actorId);
        const parent = agent.parentActorId === null ? undefined : points.get(agent.parentActorId);
        return child === undefined || parent === undefined ? null : (
          <line
            key={`line:${agent.actorId}`}
            x1={parent.x}
            y1={parent.y}
            x2={child.x}
            y2={child.y}
          />
        );
      })}
      {props.agents.map((agent) => {
        const point = points.get(agent.actorId) ?? { x: 30, y: 58 };
        return (
          <g key={agent.actorId}>
            <title>{agent.displayName}</title>
            <circle cx={point.x} cy={point.y} r={agent.role === "coordinator" ? 11 : 7} />
          </g>
        );
      })}
    </svg>
  );
});

function miniPoints(
  agents: readonly AgentDescriptor[],
): ReadonlyMap<AgentDescriptor["actorId"], { readonly x: number; readonly y: number }> {
  const coordinators = agents.filter((agent) => agent.role === "coordinator");
  const workers = agents.filter((agent) => agent.role === "worker");
  const result = new Map<AgentDescriptor["actorId"], { readonly x: number; readonly y: number }>();
  coordinators.forEach((agent, index) => result.set(agent.actorId, { x: 24, y: 34 + index * 42 }));
  workers
    .slice(0, 6)
    .forEach((agent, index) =>
      result.set(agent.actorId, { x: 130 + (index % 2) * 78, y: 24 + Math.floor(index / 2) * 36 }),
    );
  return result;
}
