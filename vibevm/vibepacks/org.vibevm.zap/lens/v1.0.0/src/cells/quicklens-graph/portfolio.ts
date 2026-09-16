/** Pure deterministic multi-project scene projection. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import { MultiDirectedGraph } from "graphology";
import type {
  AgentDescriptor,
  ManagedWorkView,
  ProjectObjectReference,
} from "../workspace-model/index.ts";
import type {
  IntegrationAttempt,
  ProjectPlanRecord,
  RepositoryWorktreeRecord,
} from "../repository-model/index.ts";
import {
  ProjectObjectReferenceSchema,
  projectObjectReferenceKey,
} from "../workspace-model/index.ts";
import type { WorkspaceCanvasInput, WorkspaceCanvasProject } from "../workspace-client/index.ts";
import type {
  QuicklensRef,
  QuicklensSnapshot,
  SemanticObject,
  SemanticRelationship,
} from "../quicklens-model/index.ts";
import type { GraphTheme } from "./index.ts";
import { layoutQuicklensGraph } from "./layout.ts";
import { addManagedWorkNodes } from "./portfolio-managed.ts";
import {
  addRepositoryWorkspaceNodes,
  createRepositoryGraphContext,
  flushRepositoryEdges,
  compactPortfolioLabel,
  repositoryContextLabel,
  type RepositoryGraphContext,
} from "./portfolio-repository.ts";

export type PortfolioNodeKind =
  | "project"
  | "goal"
  | "milestone"
  | "task"
  | "resource"
  | "agent"
  | "work_task"
  | "work_run"
  | "plan_workspace"
  | "worktree"
  | "integration"
  | "context";

export interface PortfolioNodeAttributes extends Record<string, unknown> {
  readonly label: string;
  readonly x: number;
  readonly y: number;
  readonly size: number;
  readonly color: string;
  readonly nodeKind: PortfolioNodeKind;
  readonly projectId: string;
  readonly statusLabel: string;
  readonly forceLabel: boolean;
  readonly highlighted: boolean;
  readonly selectable: boolean;
}

export interface PortfolioEdgeAttributes extends Record<string, unknown> {
  readonly label: string;
  readonly color: string;
  readonly size: number;
  readonly type: "arrow" | "line";
  readonly sourceKind: "semantic" | "agent" | "managed_work" | "repository" | "presentation";
}

export type PortfolioGraph = MultiDirectedGraph<
  PortfolioNodeAttributes,
  PortfolioEdgeAttributes,
  Record<string, unknown>
>;

export type PortfolioCard =
  | {
      readonly kind: "project";
      readonly reference: ProjectObjectReference;
      readonly project: WorkspaceCanvasProject;
    }
  | {
      readonly kind: "semantic";
      readonly reference: ProjectObjectReference;
      readonly object: SemanticObject;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    }
  | {
      readonly kind: "agent";
      readonly reference: ProjectObjectReference;
      readonly agent: AgentDescriptor;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    }
  | {
      readonly kind: "managed_work";
      readonly reference: ProjectObjectReference;
      readonly entity: "task" | "run";
      readonly work: ManagedWorkView;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    }
  | {
      readonly kind: "plan_workspace";
      readonly reference: ProjectObjectReference;
      readonly plan: ProjectPlanRecord;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    }
  | {
      readonly kind: "worktree";
      readonly reference: ProjectObjectReference;
      readonly worktree: RepositoryWorktreeRecord;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    }
  | {
      readonly kind: "integration";
      readonly reference: ProjectObjectReference;
      readonly integration: IntegrationAttempt;
      readonly project: Extract<WorkspaceCanvasProject, { state: "ready" }>;
    };

export interface PortfolioViewState {
  readonly collapsedProjects: ReadonlySet<string>;
  readonly collapsedMilestones: ReadonlySet<string>;
  readonly selectedKey: string | null;
}

export interface PortfolioProjection {
  readonly graph: PortfolioGraph;
  readonly cards: ReadonlyMap<string, PortfolioCard>;
  readonly diagnostics: readonly string[];
}

const MAX_OBJECTS_PER_PROJECT = 240;
const MAX_AGENTS_PER_PROJECT = 32;

export function projectPortfolioGraph(
  input: WorkspaceCanvasInput,
  state: PortfolioViewState,
  theme: GraphTheme,
): PortfolioProjection {
  const graph: PortfolioGraph = new MultiDirectedGraph({ allowSelfLoops: true });
  const cards = new Map<string, PortfolioCard>();
  const diagnostics: string[] = [];
  const columns = Math.min(3, input.projects.length);
  const repositoryContext = createRepositoryGraphContext(input.projects);
  input.projects.forEach((project, index) => {
    const origin = { x: (index % columns) * 13.5, y: Math.floor(index / columns) * 16 };
    addProject(graph, cards, diagnostics, project, origin, repositoryContext, state, theme);
  });
  flushRepositoryEdges(graph, repositoryContext, theme);
  return { graph, cards, diagnostics };
}

function addProject(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  diagnostics: string[],
  project: WorkspaceCanvasProject,
  origin: { readonly x: number; readonly y: number },
  repositoryContext: RepositoryGraphContext,
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  const contextId = project.state === "ready" ? project.contextId : project.selection.contextId;
  const projectReference = reference(
    project.selection.projectId,
    contextId,
    "project",
    project.selection.projectId,
  );
  const projectKey = projectObjectReferenceKey(projectReference);
  const projectLabel =
    project.state === "ready"
      ? repositoryContextLabel(project)
      : compactPortfolioLabel(project.selection.fallbackLabel, 36);
  const projectStatus = project.state === "ready" ? project.view.execution.state : "unavailable";
  addProjectRegion(graph, project.selection.projectId, contextId, origin, theme);
  graph.addNode(projectKey, {
    label: projectLabel,
    x: origin.x + 4.7,
    y: origin.y,
    size: 14,
    color:
      state.selectedKey === projectKey
        ? theme.selectedRing
        : project.state === "ready"
          ? projectColor(project.view.execution.state, theme)
          : theme.tones.warning,
    nodeKind: "project",
    projectId: project.selection.projectId,
    statusLabel: projectStatus,
    forceLabel: true,
    highlighted: state.selectedKey === projectKey,
    selectable: true,
  });
  cards.set(projectKey, { kind: "project", reference: projectReference, project });
  if (project.state === "unavailable") {
    diagnostics.push(`${projectLabel}: ${project.reason}`);
    return;
  }
  if (state.collapsedProjects.has(projectKey)) return;
  diagnostics.push(
    ...project.coverageReasons.map((reason) => `${project.view.project.displayName}: ${reason}`),
  );
  if (project.view.snapshot.state === "unavailable") {
    diagnostics.push(`${projectLabel}: ${project.view.snapshot.reason}`);
  } else {
    addSemanticProject(graph, cards, diagnostics, project, projectKey, origin, state, theme);
  }
  addAgents(graph, cards, diagnostics, project, projectKey, origin, state, theme);
  addManagedWorkNodes(graph, cards, project, projectKey, origin, state, theme);
  addRepositoryWorkspaceNodes(
    graph,
    cards,
    diagnostics,
    project,
    projectKey,
    origin,
    repositoryContext,
    state,
    theme,
  );
}

function addSemanticProject(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  diagnostics: string[],
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  projectKey: string,
  origin: { readonly x: number; readonly y: number },
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  if (project.view.snapshot.state !== "ready") return;
  const snapshot = project.view.snapshot.snapshot;
  const objects = prioritizedObjects(snapshot);
  if (objects.length > MAX_OBJECTS_PER_PROJECT) {
    diagnostics.push(
      `${project.view.project.displayName}: ${String(objects.length - MAX_OBJECTS_PER_PROJECT)} objects are outside the canvas bound.`,
    );
  }
  const bounded = objects.slice(0, MAX_OBJECTS_PER_PROJECT);
  const byRef = new Map(bounded.map((object) => [object.ref, object]));
  const goal = snapshot.navigation?.activeOutcomeRef ?? null;
  const milestoneRefs = orderedMilestones(
    snapshot.navigation?.members.map((member) => member.ref) ?? [],
    bounded,
  );
  const milestoneIndex = new Map(milestoneRefs.map((ref, index) => [ref, index]));
  const taskParents = contributionParents(snapshot.relationships, milestoneIndex);
  const workLayout = layoutQuicklensGraph(snapshot, "work_order");
  const maximumStage = Math.max(1, ...workLayout.stages.values());
  const hiddenByMilestone = new Set<string>();
  for (const [taskRef, parents] of taskParents) {
    if (parents.length === 1) {
      const parentKey = semanticKey(project, parents[0] ?? "");
      if (state.collapsedMilestones.has(parentKey)) hiddenByMilestone.add(taskRef);
    }
  }
  for (const object of bounded) {
    if (hiddenByMilestone.has(object.ref)) continue;
    const key = semanticKey(project, object.ref);
    const kind = nodeKind(object, object.ref === goal);
    const position = semanticPosition(
      object,
      kind,
      milestoneIndex,
      taskParents,
      workLayout.stages,
      maximumStage,
      origin,
    );
    graph.addNode(key, {
      label: object.title,
      x: position.x,
      y: position.y,
      size: kind === "goal" ? 15 : kind === "milestone" ? 11 : kind === "task" ? 8 : 6,
      color: state.selectedKey === key ? theme.selectedRing : theme.tones[object.status.tone],
      nodeKind: kind,
      projectId: project.selection.projectId,
      statusLabel: object.status.label,
      forceLabel:
        kind === "goal" ||
        (kind === "milestone" && object.status.tone === "active") ||
        state.selectedKey === key,
      highlighted: state.selectedKey === key,
      selectable: true,
    });
    cards.set(key, {
      kind: "semantic",
      reference: reference(
        project.selection.projectId,
        project.contextId,
        "semantic_object",
        object.ref,
      ),
      object,
      project,
    });
  }
  if (goal !== null) addPresentationEdge(graph, projectKey, semanticKey(project, goal), theme);
  addSemanticEdges(graph, project, snapshot.relationships, byRef, theme);
}

function prioritizedObjects(snapshot: QuicklensSnapshot): SemanticObject[] {
  const priority = [
    snapshot.navigation?.activeOutcomeRef,
    snapshot.navigation?.currentStrategyRef,
    ...(snapshot.navigation?.members.map((member) => member.ref) ?? []),
    snapshot.navigation?.focusRef,
    ...(snapshot.navigation?.currentWorkRefs ?? []),
  ].filter((ref): ref is QuicklensRef => ref !== null && ref !== undefined);
  const byRef = new Map(snapshot.objects.map((object) => [object.ref, object]));
  const reserved = [...new Set(priority)].flatMap((ref) => {
    const object = byRef.get(ref);
    return object === undefined ? [] : [object];
  });
  const reservedRefs = new Set(reserved.map((object) => object.ref));
  const remaining = snapshot.objects
    .filter((object) => !reservedRefs.has(object.ref))
    .sort((left, right) => left.ref.localeCompare(right.ref));
  return [...reserved, ...remaining];
}

function addAgents(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  diagnostics: string[],
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  projectKey: string,
  origin: { readonly x: number; readonly y: number },
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  const agents = [...project.view.network.agents]
    .sort((left, right) => left.actorId.localeCompare(right.actorId))
    .slice(0, MAX_AGENTS_PER_PROJECT);
  if (project.view.network.agents.length > agents.length) {
    diagnostics.push(
      `${project.view.project.displayName}: ${String(project.view.network.agents.length - agents.length)} agents are outside the canvas bound.`,
    );
  }
  const agentKeys = new Map<string, string>();
  agents.forEach((agent, index) => {
    const referenceValue = reference(
      project.selection.projectId,
      project.contextId,
      "agent",
      agent.actorId,
    );
    const key = projectObjectReferenceKey(referenceValue);
    agentKeys.set(agent.actorId, key);
    graph.addNode(key, {
      label: agent.displayName,
      x: origin.x + 8.2,
      y: origin.y + 0.8 + spread(index, agents.length, 4.8),
      size: agent.role === "coordinator" ? 10 : 7,
      color: state.selectedKey === key ? theme.selectedRing : agentColor(agent.state, theme),
      nodeKind: "agent",
      projectId: project.selection.projectId,
      statusLabel: agent.state.replaceAll("_", " "),
      forceLabel: state.selectedKey === key,
      highlighted: state.selectedKey === key,
      selectable: true,
    });
    cards.set(key, { kind: "agent", reference: referenceValue, agent, project });
    if (agent.parentActorId === null) addPresentationEdge(graph, projectKey, key, theme);
  });
  for (const relationship of project.view.network.relationships) {
    const source = agentKeys.get(relationship.fromActorId);
    const target = agentKeys.get(relationship.toActorId);
    if (source === undefined || target === undefined) continue;
    graph.addDirectedEdgeWithKey(
      `agent:${project.selection.projectId}:${project.contextId}:${relationship.fromActorId}:${relationship.toActorId}:${relationship.kind}`,
      source,
      target,
      {
        label: relationship.kind,
        color: theme.edgeClasses.reference,
        size: 1.5,
        type: relationship.kind === "message" ? "arrow" : "line",
        sourceKind: "agent",
      },
    );
  }
}

function addProjectRegion(
  graph: PortfolioGraph,
  projectId: string,
  contextId: string,
  origin: { readonly x: number; readonly y: number },
  theme: GraphTheme,
): void {
  const points = [
    { x: origin.x - 0.35, y: origin.y - 0.35 },
    { x: origin.x + 11.75, y: origin.y - 0.35 },
    { x: origin.x + 11.75, y: origin.y + 11.75 },
    { x: origin.x - 0.35, y: origin.y + 11.75 },
  ];
  const keys = points.map((point, index) => {
    const key = `region:${projectId}:${contextId}:${String(index)}`;
    graph.addNode(key, {
      label: "",
      x: point.x,
      y: point.y,
      size: 0.1,
      color: theme.mutedEdge,
      nodeKind: "context",
      projectId,
      statusLabel: "project region",
      forceLabel: false,
      highlighted: false,
      selectable: false,
    });
    return key;
  });
  keys.forEach((source, index) => {
    const target = keys[(index + 1) % keys.length];
    if (target === undefined) return;
    graph.addDirectedEdgeWithKey(
      `region-edge:${projectId}:${contextId}:${String(index)}`,
      source,
      target,
      {
        label: "",
        color: theme.mutedEdge,
        size: 1.2,
        type: "line",
        sourceKind: "presentation",
      },
    );
  });
}

function addSemanticEdges(
  graph: PortfolioGraph,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  relationships: readonly SemanticRelationship[],
  objects: ReadonlyMap<string, SemanticObject>,
  theme: GraphTheme,
): void {
  for (const relationship of relationships) {
    if (!objects.has(relationship.source) || !objects.has(relationship.target)) continue;
    const source = semanticKey(project, relationship.source);
    const target = semanticKey(project, relationship.target);
    if (!graph.hasNode(source) || !graph.hasNode(target)) continue;
    graph.addDirectedEdgeWithKey(
      projectObjectReferenceKey(
        reference(
          project.selection.projectId,
          project.contextId,
          "semantic_relationship",
          relationship.ref,
        ),
      ),
      source,
      target,
      {
        label: relationship.label,
        color: theme.edge,
        size: 1.4,
        type: "arrow",
        sourceKind: "semantic",
      },
    );
  }
}

function addPresentationEdge(
  graph: PortfolioGraph,
  source: string,
  target: string,
  theme: GraphTheme,
): void {
  if (!graph.hasNode(source) || !graph.hasNode(target)) return;
  graph.addDirectedEdgeWithKey(`scope:${source}:${target}`, source, target, {
    label: "project scope",
    color: theme.mutedEdge,
    size: 1,
    type: "line",
    sourceKind: "presentation",
  });
}

function orderedMilestones(
  navigation: readonly string[],
  objects: readonly SemanticObject[],
): string[] {
  const result = [...new Set(navigation)];
  const remaining = objects
    .filter((object) => object.category === "milestone" && !result.includes(object.ref))
    .map((object) => object.ref)
    .sort();
  return [...result, ...remaining];
}

function contributionParents(
  relationships: readonly SemanticRelationship[],
  milestones: ReadonlyMap<string, number>,
): ReadonlyMap<string, readonly string[]> {
  const parents = new Map<string, string[]>();
  for (const relationship of relationships) {
    if (
      normalize(relationship.semanticType) !== "contributes_to" ||
      !milestones.has(relationship.target)
    )
      continue;
    const current = parents.get(relationship.source) ?? [];
    parents.set(relationship.source, [...current, relationship.target].sort());
  }
  return parents;
}

function semanticPosition(
  object: SemanticObject,
  kind: PortfolioNodeKind,
  milestones: ReadonlyMap<string, number>,
  taskParents: ReadonlyMap<string, readonly string[]>,
  stages: ReadonlyMap<string, number>,
  maximumStage: number,
  origin: { readonly x: number; readonly y: number },
): { readonly x: number; readonly y: number } {
  if (kind === "goal") return { x: origin.x + 3.6, y: origin.y + 0.9 };
  if (kind === "milestone") {
    const index = milestones.get(object.ref) ?? 0;
    return { x: origin.x + spread(index, milestones.size, 6) + 0.3, y: origin.y + 1.8 };
  }
  if (kind === "task") {
    const parents = taskParents.get(object.ref) ?? [];
    const lane =
      parents.length === 1
        ? (milestones.get(parents[0] ?? "") ?? milestones.size)
        : milestones.size;
    const lanes = Math.max(1, milestones.size + 1);
    const stage = stages.get(object.ref);
    return {
      x: origin.x + 0.3 + spread(lane, lanes, 7.2),
      y: origin.y + 2.6 + ((stage ?? maximumStage) / maximumStage) * 2.1,
    };
  }
  if (kind === "resource") return { x: origin.x + 1.6, y: origin.y + 5.3 };
  return { x: origin.x + 5.7, y: origin.y + 5.3 };
}

function nodeKind(object: SemanticObject, goal: boolean): PortfolioNodeKind {
  if (goal) return "goal";
  if (object.category === "milestone") return "milestone";
  if (object.category === "task") return "task";
  if (object.category === "resource") return "resource";
  return "context";
}

function semanticKey(
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  ref: string,
): string {
  return projectObjectReferenceKey(
    reference(project.selection.projectId, project.contextId, "semantic_object", ref),
  );
}

function reference(
  projectId: string,
  contextId: string,
  domain: ProjectObjectReference["domain"],
  ref: string,
): ProjectObjectReference {
  return ProjectObjectReferenceSchema.parse({ projectId, contextId, domain, ref });
}

function spread(index: number, count: number, width: number): number {
  return count <= 1 ? width / 2 : (index / (count - 1)) * width;
}

function agentColor(state: AgentDescriptor["state"], theme: GraphTheme): string {
  if (state === "active") return theme.tones.success;
  if (state === "failed") return theme.tones.danger;
  if (state === "waiting_for_user" || state === "starting") return theme.tones.warning;
  return theme.tones.neutral;
}

function projectColor(state: string, theme: GraphTheme): string {
  if (state === "running") return theme.tones.success;
  if (state === "uncertain") return theme.tones.danger;
  if (state === "pausing" || state === "stopping" || state === "continuing") {
    return theme.tones.warning;
  }
  return theme.tones.neutral;
}

function normalize(value: string): string {
  return value.trim().toLowerCase().replaceAll("-", "_");
}
