/** Repository plan/worktree/integration canvas projection. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import type {
  IntegrationAttempt,
  ProjectPlanRecord,
  RepositoryWorktreeRecord,
  WorktreeAssignment,
} from "../repository-model/index.ts";
import {
  ProjectObjectReferenceSchema,
  projectObjectReferenceKey,
} from "../workspace-model/index.ts";
import type { WorkspaceCanvasInput, WorkspaceCanvasProject } from "../workspace-client/index.ts";
import type { GraphTheme, PortfolioCard, PortfolioGraph, PortfolioViewState } from "./index.ts";

interface DeferredRepositoryEdge {
  readonly key: string;
  readonly source: string | undefined;
  readonly target: string | undefined;
  readonly label: string;
}
export interface RepositoryGraphContext {
  readonly worktreeKeys: ReadonlyMap<string, string>;
  readonly worktrees: ReadonlyMap<string, RepositoryWorktreeRecord>;
  readonly edges: DeferredRepositoryEdge[];
}

export function createRepositoryGraphContext(
  projects: WorkspaceCanvasInput["projects"],
): RepositoryGraphContext {
  const worktreeKeys = new Map<string, string>();
  const worktrees = new Map<string, RepositoryWorktreeRecord>();
  for (const project of projects) {
    if (project.state !== "ready" || project.repository.state !== "ready") continue;
    for (const worktree of project.repository.value.worktrees) {
      worktrees.set(worktree.worktreeId, worktree);
      worktreeKeys.set(
        worktree.worktreeId,
        projectObjectReferenceKey(
          ProjectObjectReferenceSchema.parse({
            projectId: worktree.projectId,
            contextId: worktree.contextId,
            domain: "worktree",
            ref: worktree.worktreeId,
          }),
        ),
      );
    }
  }
  return { worktreeKeys, worktrees, edges: [] };
}

export function flushRepositoryEdges(
  graph: PortfolioGraph,
  context: RepositoryGraphContext,
  theme: GraphTheme,
): void {
  for (const edge of context.edges)
    addEdge(graph, edge.key, edge.source, edge.target, edge.label, theme);
}

export function addRepositoryWorkspaceNodes(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  diagnostics: string[],
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  projectKey: string,
  origin: { readonly x: number; readonly y: number },
  graphContext: RepositoryGraphContext,
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  if (project.repository.state === "unavailable") {
    diagnostics.push(
      `${project.view.project.displayName} · ${repositoryContextLabel(project)}: repository workspace unavailable. ${project.repository.reason}`,
    );
    return;
  }
  const repository = project.repository.value;
  const repositoryOnly =
    project.view.snapshot.state === "unavailable" ||
    project.view.snapshot.snapshot.objects.length === 0;
  const plans = repository.plans
    .filter((plan) => plan.contextId === project.contextId)
    .toSorted((left, right) => left.displayName.localeCompare(right.displayName));
  const planIds = new Set(plans.map((plan) => plan.planId));
  plans.forEach((plan, index) => {
    addPlan(
      graph,
      cards,
      project,
      projectKey,
      plan,
      index,
      plans.length,
      origin,
      repositoryOnly,
      state,
      theme,
    );
  });
  const worktrees = repository.worktrees
    .filter((worktree) => worktree.contextId === project.contextId)
    .toSorted(
      (left, right) =>
        worktreeOrder(left.kind) - worktreeOrder(right.kind) ||
        left.branchRef.localeCompare(right.branchRef),
    );
  worktrees.forEach((worktree) => {
    const lane = worktrees.filter((candidate) => candidate.kind === worktree.kind);
    addWorktree(
      graph,
      cards,
      project,
      worktree,
      lane.findIndex((candidate) => candidate.worktreeId === worktree.worktreeId),
      lane.length,
      origin,
      repositoryOnly,
      state,
      theme,
    );
  });
  for (const worktree of worktrees) {
    const key = graphContext.worktreeKeys.get(worktree.worktreeId);
    if (key === undefined) continue;
    if (worktree.parentWorktreeId !== null) {
      addEdge(
        graph,
        `forked_from:${key}`,
        graphContext.worktreeKeys.get(worktree.parentWorktreeId),
        key,
        "forked from",
        theme,
      );
    }
    for (const assignment of worktree.assignments)
      addAssignment(graph, key, worktree, assignment, theme);
  }
  const integrations = repository.integrations.filter((integration) =>
    planIds.has(integration.planId),
  );
  integrations.forEach((integration, index) => {
    addIntegration(
      graph,
      cards,
      project,
      integration,
      index,
      integrations.length,
      graphContext,
      origin,
      repositoryOnly,
      state,
      theme,
    );
  });
}

function addPlan(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  projectKey: string,
  plan: ProjectPlanRecord,
  index: number,
  count: number,
  origin: { readonly x: number; readonly y: number },
  repositoryOnly: boolean,
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  if (plan.projectId !== project.selection.projectId || plan.contextId !== project.contextId)
    return;
  const reference = ProjectObjectReferenceSchema.parse({
    projectId: plan.projectId,
    contextId: plan.contextId,
    domain: "plan_workspace",
    ref: plan.planId,
  });
  const key = projectObjectReferenceKey(reference);
  if (graph.hasNode(key)) return;
  graph.addNode(key, {
    label: "Plan",
    x: repositoryOnly ? origin.x + 5.5 : origin.x + spread(index, count, 9) + 0.8,
    y: origin.y + (repositoryOnly ? 2 : 5.2),
    size: 11,
    color: state.selectedKey === key ? theme.selectedRing : stateColor(plan.state, theme),
    nodeKind: "plan_workspace",
    projectId: plan.projectId,
    statusLabel: `${plan.state} · ${plan.algorithmBinding.state} plan`,
    forceLabel: state.selectedKey === key,
    highlighted: state.selectedKey === key,
    selectable: true,
  });
  cards.set(key, { kind: "plan_workspace", reference, plan, project });
  addEdge(graph, `plan-scope:${key}`, projectKey, key, "plan context", theme);
}

function addWorktree(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  worktree: RepositoryWorktreeRecord,
  index: number,
  count: number,
  origin: { readonly x: number; readonly y: number },
  repositoryOnly: boolean,
  state: PortfolioViewState,
  theme: GraphTheme,
): string {
  const reference = ProjectObjectReferenceSchema.parse({
    projectId: worktree.projectId,
    contextId: worktree.contextId,
    domain: "worktree",
    ref: worktree.worktreeId,
  });
  const key = projectObjectReferenceKey(reference);
  if (!graph.hasNode(key))
    graph.addNode(key, {
      label: worktreeLabel(worktree, project),
      x: repositoryOnly
        ? origin.x + spread(index, count, 7) + 2.2
        : origin.x + spread(index, count, 9.8) + 0.5,
      y: repositoryOnly
        ? origin.y + repositoryLane(worktree.kind)
        : origin.y + 6.9 + worktreeLane(worktree.kind),
      size: worktree.kind === "integration" ? 9 : 8,
      color: state.selectedKey === key ? theme.selectedRing : stateColor(worktree.state, theme),
      nodeKind: "worktree",
      projectId: worktree.projectId,
      statusLabel: `${worktree.kind.replaceAll("_", " ")} · ${worktree.state} · ${String(worktree.assignments.length)} assignment(s)`,
      forceLabel:
        worktree.kind !== "integration" ||
        worktree.state === "conflicted" ||
        state.selectedKey === key,
      highlighted: state.selectedKey === key,
      selectable: true,
    });
  if (!cards.has(key)) cards.set(key, { kind: "worktree", reference, worktree, project });
  const planKey =
    worktree.planId === null
      ? undefined
      : repositoryKey(
          project.selection.projectId,
          project.contextId,
          "plan_workspace",
          worktree.planId,
        );
  addEdge(graph, `executes_in:${key}`, planKey, key, "executes in", theme);
  return key;
}

function addAssignment(
  graph: PortfolioGraph,
  worktreeKey: string,
  worktree: RepositoryWorktreeRecord,
  assignment: WorktreeAssignment,
  theme: GraphTheme,
): void {
  const sources = [
    assignment.actorId === null
      ? null
      : repositoryKey(worktree.projectId, worktree.contextId, "agent", assignment.actorId),
    assignment.taskId === null
      ? null
      : repositoryKey(worktree.projectId, worktree.contextId, "work_task", assignment.taskId),
    assignment.runId === null
      ? null
      : repositoryKey(worktree.projectId, worktree.contextId, "work_run", assignment.runId),
  ].filter((value): value is string => value !== null && graph.hasNode(value));
  for (const source of sources)
    addEdge(
      graph,
      `assignment:${assignment.assignmentId}:${source}`,
      source,
      worktreeKey,
      "executes in",
      theme,
    );
}

function addIntegration(
  graph: PortfolioGraph,
  cards: Map<string, PortfolioCard>,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
  integration: IntegrationAttempt,
  index: number,
  count: number,
  graphContext: RepositoryGraphContext,
  origin: { readonly x: number; readonly y: number },
  repositoryOnly: boolean,
  state: PortfolioViewState,
  theme: GraphTheme,
): void {
  const reference = ProjectObjectReferenceSchema.parse({
    projectId: project.selection.projectId,
    contextId: project.contextId,
    domain: "integration",
    ref: integration.integrationId,
  });
  const key = projectObjectReferenceKey(reference);
  graph.addNode(key, {
    label: integrationLabel(integration, graphContext.worktrees),
    x: repositoryOnly
      ? origin.x + spread(index, count, 7) + 2.2
      : origin.x + spread(index, count, 9) + 0.8,
    y: origin.y + (repositoryOnly ? 10.5 : 13),
    size: 10,
    color: state.selectedKey === key ? theme.selectedRing : stateColor(integration.state, theme),
    nodeKind: "integration",
    projectId: project.selection.projectId,
    statusLabel: integration.state,
    forceLabel: true,
    highlighted: state.selectedKey === key,
    selectable: true,
  });
  cards.set(key, { kind: "integration", reference, integration, project });
  graphContext.edges.push({
    key: `merge_source:${key}`,
    source: graphContext.worktreeKeys.get(integration.sourceWorktreeId),
    target: key,
    label: "merge source",
  });
  graphContext.edges.push({
    key: `merges_to:${key}`,
    source: key,
    target: graphContext.worktreeKeys.get(integration.targetWorktreeId),
    label: "merges to",
  });
  if (integration.state === "conflicted")
    graphContext.edges.push({
      key: `resolves_conflict:${key}`,
      source: key,
      target: graphContext.worktreeKeys.get(integration.integrationWorktreeId),
      label: "resolves conflict",
    });
}

function addEdge(
  graph: PortfolioGraph,
  key: string,
  source: string | undefined,
  target: string | undefined,
  label: string,
  theme: GraphTheme,
): void {
  if (
    source === undefined ||
    target === undefined ||
    graph.hasEdge(key) ||
    !graph.hasNode(source) ||
    !graph.hasNode(target)
  )
    return;
  graph.addDirectedEdgeWithKey(key, source, target, {
    label,
    color: theme.edgeClasses.execution,
    size: 2.2,
    type: "arrow",
    sourceKind: "repository",
  });
}

function repositoryKey(
  projectId: string,
  contextId: string,
  domain: "plan_workspace" | "agent" | "work_task" | "work_run",
  ref: string,
): string {
  return projectObjectReferenceKey(
    ProjectObjectReferenceSchema.parse({
      projectId,
      contextId,
      domain,
      ref,
    }),
  );
}

function stateColor(state: string, theme: GraphTheme): string {
  if (["ready", "tested", "accepted", "promoted"].includes(state)) return theme.tones.success;
  if (["conflicted", "stale", "unavailable", "rejected"].includes(state)) return theme.tones.danger;
  if (["preparing", "candidate"].includes(state)) return theme.tones.warning;
  return theme.tones.neutral;
}

function worktreeLabel(
  worktree: RepositoryWorktreeRecord,
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
): string {
  const assignment = worktree.assignments.find((item) => item.releasedAt === null);
  const assignedWork = project.managedWorks.find(
    (work) => work.runId === assignment?.runId || work.taskId === assignment?.taskId,
  );
  if (assignedWork !== undefined)
    return compactPortfolioLabel(assignedWork.goal.split(/\r?\n/, 1)[0] ?? "Worker task", 32);
  if (worktree.kind === "registered") {
    const branch = worktree.branchRef.replace(/^refs\/heads\//, "");
    return branch === "" ? "Original checkout" : `Original · ${compactPortfolioLabel(branch, 18)}`;
  }
  if (worktree.kind === "plan_root") return "Root workspace";
  if (worktree.kind === "worker") return "Worker workspace";
  return "Integration workspace";
}
function integrationLabel(
  integration: IntegrationAttempt,
  worktrees: ReadonlyMap<string, RepositoryWorktreeRecord>,
): string {
  const source = endpointLabel(worktrees.get(integration.sourceWorktreeId));
  const target = endpointLabel(worktrees.get(integration.targetWorktreeId));
  return integration.conflictPaths.length > 0
    ? `Resolve ${String(integration.conflictPaths.length)} → ${target}`
    : `Merge ${source} → ${target}`;
}
export function repositoryContextLabel(
  project: Extract<WorkspaceCanvasProject, { state: "ready" }>,
): string {
  return (
    project.view.contexts.find((context) => context.contextId === project.contextId)?.displayName ??
    "Context"
  );
}
function worktreeLane(kind: RepositoryWorktreeRecord["kind"]): number {
  return kind === "registered" ? 0 : kind === "plan_root" ? 1.4 : kind === "worker" ? 2.8 : 4.2;
}
function repositoryLane(kind: RepositoryWorktreeRecord["kind"]): number {
  return kind === "registered" || kind === "plan_root" ? 4 : kind === "worker" ? 6 : 8;
}
function worktreeOrder(kind: RepositoryWorktreeRecord["kind"]): number {
  return kind === "registered" ? 0 : kind === "plan_root" ? 1 : kind === "worker" ? 2 : 3;
}
function spread(index: number, count: number, width: number): number {
  return count <= 1 ? width / 2 : (index / (count - 1)) * width;
}
export function compactPortfolioLabel(value: string, maximum: number): string {
  return value.length <= maximum ? value : `${value.slice(0, maximum - 1)}…`;
}
function endpointLabel(worktree: RepositoryWorktreeRecord | undefined): string {
  if (worktree?.kind === "registered") return "original";
  if (worktree?.kind === "plan_root") return "plan";
  if (worktree?.kind === "worker") return "worker";
  return worktree?.kind === "integration" ? "candidate" : "target";
}
