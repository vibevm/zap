/** Managed task/run projection for the unified canvas. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type { ManagedWorkView } from "../workspace-model/index.ts";
import {
  ProjectObjectReferenceSchema,
  projectObjectReferenceKey,
} from "../workspace-model/index.ts";
import type { WorkspaceCanvasProject } from "../workspace-client/index.ts";
import type { GraphTheme, PortfolioCard, PortfolioGraph, PortfolioViewState } from "./index.ts";

export function addManagedWorkNodes(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  projectKey: string,
  origin: { readonly x: number; readonly y: number },
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  const agentKeys = new Map(
    [...cards]
      .filter(
        ([, card]) =>
          card.kind === "agent" &&
          card.reference.projectId === project.selection.projectId &&
          card.reference.contextId === project.contextId,
      )
      .map(([key, card]) => [card.kind === "agent" ? card.agent.actorId : "", key]),
  );
  project.managedWorks.forEach((work, index) => {
    const taskReference = ProjectObjectReferenceSchema.parse({
      projectId: project.selection.projectId,
      contextId: project.contextId,
      domain: "work_task",
      ref: work.taskId,
    });
    const runReference = ProjectObjectReferenceSchema.parse({
      projectId: project.selection.projectId,
      contextId: project.contextId,
      domain: "work_run",
      ref: work.runId,
    });
    const taskKey = projectObjectReferenceKey(taskReference);
    const runKey = projectObjectReferenceKey(runReference);
    const y = origin.y + 3 + spread(index, Math.max(1, project.managedWorks.length), 2.2);
    graph.addNode(taskKey, {
      label: firstLine(work.goal),
      x: origin.x + 6.5,
      y,
      size: 9,
      color: state.selectedKey === taskKey ? theme.selectedRing : workColor(work.state, theme),
      nodeKind: "work_task",
      projectId: project.selection.projectId,
      statusLabel: work.state.replaceAll("_", " "),
      forceLabel: state.selectedKey === taskKey,
      highlighted: state.selectedKey === taskKey,
      selectable: true,
    });
    graph.addNode(runKey, {
      label: `${work.provider} run`,
      x: origin.x + 7.8,
      y,
      size: 7,
      color: state.selectedKey === runKey ? theme.selectedRing : workColor(work.state, theme),
      nodeKind: "work_run",
      projectId: project.selection.projectId,
      statusLabel: work.state.replaceAll("_", " "),
      forceLabel: state.selectedKey === runKey,
      highlighted: state.selectedKey === runKey,
      selectable: true,
    });
    cards.set(taskKey, {
      kind: "managed_work",
      reference: taskReference,
      entity: "task",
      work,
      project,
    });
    cards.set(runKey, {
      kind: "managed_work",
      reference: runReference,
      entity: "run",
      work,
      project,
    });
    addEdge(
      graph,
      `managed-task-run:${project.selection.projectId}:${project.contextId}:${work.runId}`,
      taskKey,
      runKey,
      "run",
      theme,
    );
    const agentKey = agentKeys.get(work.actorId);
    if (agentKey !== undefined) {
      addEdge(
        graph,
        `managed-run-agent:${project.selection.projectId}:${project.contextId}:${work.runId}`,
        runKey,
        agentKey,
        "actor",
        theme,
      );
    }
    for (const target of work.targetRefs) {
      if (
        target.projectId !== project.selection.projectId ||
        target.contextId !== project.contextId ||
        target.domain !== "semantic_object"
      )
        continue;
      const targetKey = projectObjectReferenceKey(target);
      if (graph.hasNode(targetKey)) {
        addEdge(
          graph,
          `managed-target:${project.selection.projectId}:${project.contextId}:${work.runId}:${target.ref}`,
          taskKey,
          targetKey,
          "target",
          theme,
        );
      }
    }
    if (!graph.hasEdge(projectKey, taskKey)) {
      graph.addDirectedEdgeWithKey(`scope:${projectKey}:${taskKey}`, projectKey, taskKey, {
        label: "project scope",
        color: theme.mutedEdge,
        size: 1,
        type: "line",
        sourceKind: "presentation",
      });
    }
  });
}

function addEdge(
  graph: PortfolioGraph,
  key: string,
  source: string,
  target: string,
  label: string,
  theme: GraphTheme,
): void {
  graph.addDirectedEdgeWithKey(key, source, target, {
    label,
    color: theme.edgeClasses.execution,
    size: 1.8,
    type: "arrow",
    sourceKind: "managed_work",
  });
}

function workColor(state: ManagedWorkView["state"], theme: GraphTheme): string {
  if (state === "running" || state === "accepted") return theme.tones.success;
  if (state === "failed") return theme.tones.danger;
  if (state === "waiting_for_user" || state === "follow_up_required") return theme.tones.warning;
  return theme.tones.neutral;
}

function firstLine(value: string): string {
  const line = value.split(/\r?\n/, 1)[0]?.trim() ?? "Managed work";
  return line.length <= 80 ? line : `${line.slice(0, 77)}…`;
}

function spread(index: number, count: number, width: number): number {
  return count <= 1 ? width / 2 : (index / (count - 1)) * width;
}
