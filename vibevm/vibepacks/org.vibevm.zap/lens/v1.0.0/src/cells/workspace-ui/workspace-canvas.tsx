/** Unified multi-project map and scoped inspector. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import {
  $,
  component$,
  noSerialize,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import {
  DARK_GRAPH_THEME,
  LIGHT_GRAPH_THEME,
  projectPortfolioGraph,
  type PortfolioCard,
} from "../quicklens-graph/index.ts";
import { PortfolioCanvas } from "../quicklens-ui/index.tsx";
import {
  readWorkspaceCanvas,
  type WorkspaceCanvasInput,
  type WorkspaceCanvasSelection,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type { ProjectDescriptor, ProjectId, WorkContextId } from "../workspace-model/index.ts";
import { CanvasCard } from "./canvas-card.tsx";
import { AnnotationPanel } from "./annotation-panel.tsx";
import { ScopedRequestFence } from "./request-fence.ts";

export const WorkspaceCanvas = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projects: readonly ProjectDescriptor[];
  readonly theme: "light" | "dark";
  readonly refreshEpoch: number;
  readonly onOpenProject$: QRL<(projectId: ProjectId, contextId: WorkContextId) => void>;
  readonly onOpenQuestions$: QRL<(projectId: ProjectId, contextId: WorkContextId) => void>;
}>((props) => {
  const input = useSignal<WorkspaceCanvasInput | null>(null);
  const selectedKey = useSignal<string | null>(null);
  const collapsedProjects = useSignal<readonly string[]>([]);
  const collapsedMilestones = useSignal<readonly string[]>([]);
  const fitEpoch = useSignal(0);
  const loading = useSignal(true);
  const error = useSignal<string | null>(null);
  const refreshEpoch = useSignal(0);
  const annotationMode = useSignal<"notes" | "trash" | null>(null);
  const selectedScopes = useSignal<readonly WorkspaceCanvasSelection[]>([]);
  const scopeSelectionTouched = useSignal(false);
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));

  useVisibleTask$(({ track, cleanup }) => {
    const projectScope = track(() =>
      props.projects.map((project) => `${project.projectId}:${project.revision}`).join("|"),
    );
    track(() => selectedScopes.value.map(selectionKey).join("|"));
    const refresh = track(() => refreshEpoch.value);
    track(() => props.refreshEpoch);
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined || props.projects.length === 0) {
      input.value = null;
      loading.value = false;
      return;
    }
    const validProjects = new Set(props.projects.map((project) => project.projectId));
    const retained = selectedScopes.value.filter((scope) => validProjects.has(scope.projectId));
    const selections =
      retained.length === 0
        ? props.projects.slice(0, 8).map((project) => ({
            projectId: project.projectId,
            contextId: project.defaultContextId,
            fallbackLabel: project.displayName,
          }))
        : retained;
    if (selectionSignature(selections) !== selectionSignature(selectedScopes.value)) {
      selectedScopes.value = selections;
      return;
    }
    const token = activeFence.begin(`${projectScope}\u0000${String(refresh)}`);
    loading.value = true;
    void readWorkspaceCanvas(port, selections).then((read) => {
      if (!activeFence.isCurrent(token)) return;
      loading.value = false;
      if (!read.ok) {
        error.value = read.error.message;
        return;
      }
      input.value = read.value;
      if (!scopeSelectionTouched.value) {
        const discovered = canvasScopes(props.projects, read.value).slice(0, 8);
        if (selectionSignature(discovered) !== selectionSignature(selectedScopes.value))
          selectedScopes.value = discovered;
      }
      error.value = null;
    });
    cleanup(() => {
      activeFence.cancel();
    });
  });

  if (props.projects.length === 0) {
    return (
      <section class="workspace-panel workspace-empty">
        <strong>No projects yet</strong>
        <span>Add an existing project directory to place it on the shared map.</span>
      </section>
    );
  }
  if (loading.value || input.value === null) {
    return (
      <section class="workspace-panel workspace-loading">
        {error.value ?? "Building the project map…"}
      </section>
    );
  }
  const theme = props.theme === "dark" ? DARK_GRAPH_THEME : LIGHT_GRAPH_THEME;
  const projection = projectPortfolioGraph(
    input.value,
    {
      collapsedProjects: new Set(collapsedProjects.value),
      collapsedMilestones: new Set(collapsedMilestones.value),
      selectedKey: selectedKey.value,
    },
    theme,
  );
  const selected =
    selectedKey.value === null ? null : (projection.cards.get(selectedKey.value) ?? null);
  const selectedCollapsed =
    selectedKey.value !== null &&
    (collapsedProjects.value.includes(selectedKey.value) ||
      collapsedMilestones.value.includes(selectedKey.value));
  const sceneKey = JSON.stringify({
    nodes: projection.graph.nodes().sort(),
    edges: projection.graph.edges().sort(),
    revisions: input.value.projects.map((project) =>
      project.state === "ready"
        ? [
            project.selection.projectId,
            project.selection.contextId,
            project.revision,
            ...project.managedWorks.map((work) => work.revision),
          ]
        : [project.selection.projectId, project.selection.contextId, project.reason],
    ),
  });
  const questions = input.value.projects.flatMap((project) =>
    project.state === "ready"
      ? project.view.questions
          .filter((question) => question.state === "open")
          .map((question) => ({
            projectId: project.selection.projectId,
            projectLabel: project.view.project.displayName,
            question,
          }))
      : [],
  );
  const availableScopes = canvasScopes(props.projects, input.value);
  const unselectedScopes = availableScopes.filter(
    (scope) =>
      !selectedScopes.value.some((selected) => selectionKey(selected) === selectionKey(scope)),
  );

  return (
    <section class="workspace-canvas-shell">
      <header class="workspace-panel canvas-toolbar">
        <div>
          <p class="eyebrow">Unified project map</p>
          <h1>Projects, plans and agents</h1>
          <p>Each region keeps its own project, context and plan authority.</p>
          <small>
            Selected {selectedScopes.value.length} of {availableScopes.length} visible plan
            context(s).
          </small>
        </div>
        <details class="canvas-scope-selector">
          <summary>Choose plan contexts</summary>
          <div class="canvas-scope-options">
            {availableScopes.map((scope) => {
              const selected = selectedScopes.value.some(
                (candidate) => selectionKey(candidate) === selectionKey(scope),
              );
              return (
                <label key={selectionKey(scope)}>
                  <input
                    type="checkbox"
                    checked={selected}
                    disabled={!selected && selectedScopes.value.length >= 8}
                    onChange$={() => {
                      scopeSelectionTouched.value = true;
                      selectedScopes.value = selected
                        ? selectedScopes.value.filter(
                            (candidate) => selectionKey(candidate) !== selectionKey(scope),
                          )
                        : [...selectedScopes.value, scope];
                    }}
                  />
                  <span>{scope.fallbackLabel}</span>
                </label>
              );
            })}
          </div>
        </details>
        <div class="network-legend repository-legend" aria-label="Workspace map legend">
          <span>Plan context</span>
          <span>Root branch lane</span>
          <span>Isolated worker branch</span>
          <span>Integration and conflict resolution</span>
          <span>Arrows show fork, execution and merge direction</span>
        </div>
        <div class="canvas-toolbar-actions">
          <label class="field-label">
            Find on map
            <select
              aria-label="Canvas object"
              value={selectedKey.value ?? ""}
              onChange$={(_, element) => {
                selectedKey.value = element.value.length === 0 ? null : element.value;
              }}
            >
              <option value="">Select an object</option>
              {[...projection.cards].map(([key, card]) => (
                <option key={key} value={key}>
                  {cardLabel(card)}
                </option>
              ))}
            </select>
          </label>
          <button class="button secondary" onClick$={() => (fitEpoch.value += 1)}>
            Fit all
          </button>
          <button class="button secondary" onClick$={() => (refreshEpoch.value += 1)}>
            Refresh
          </button>
        </div>
        {questions.length === 0 ? (
          <span class="count-chip">No open questions</span>
        ) : (
          <div class="canvas-question-attention">
            <strong>{questions.length} open question group(s)</strong>
            {[
              ...new Map(
                questions.map((item) => [
                  `${item.projectId}\u0000${item.question.contextId}`,
                  item,
                ]),
              ).values(),
            ].map((item) => (
              <button
                key={`${item.projectId}:${item.question.contextId}`}
                class="button secondary"
                onClick$={() => props.onOpenQuestions$(item.projectId, item.question.contextId)}
              >
                {item.projectLabel}
              </button>
            ))}
          </div>
        )}
      </header>
      {unselectedScopes.length === 0 ? null : (
        <details class="workspace-notice canvas-diagnostics">
          <summary>{unselectedScopes.length} plan context(s) are outside this bounded map</summary>
          <p>{unselectedScopes.map((scope) => scope.fallbackLabel).join(", ")}</p>
        </details>
      )}
      {projection.diagnostics.length === 0 ? null : (
        <details class="workspace-notice canvas-diagnostics">
          <summary>Partial map · {projection.diagnostics.length} notice(s)</summary>
          <ul>
            {projection.diagnostics.map((diagnostic) => (
              <li key={diagnostic}>{diagnostic}</li>
            ))}
          </ul>
        </details>
      )}
      <div class={`workspace-canvas-layout ${selected === null ? "" : "has-selection"}`}>
        <PortfolioCanvas
          projection={noSerialize(projection)}
          sceneKey={sceneKey}
          selectedKey={selectedKey.value}
          theme={props.theme}
          fitEpoch={fitEpoch.value}
          onSelect$={$((key) => {
            selectedKey.value = key;
          })}
          onActivate$={$((key) => {
            selectedKey.value = key;
          })}
        />
        {selected === null ? null : (
          <CanvasCard
            card={selected}
            collapsed={selectedCollapsed}
            onOpenProject$={props.onOpenProject$}
            onOpenQuestions$={props.onOpenQuestions$}
            onOpenNotes$={$(() => (annotationMode.value = "notes"))}
            onOpenTrash$={$(() => (annotationMode.value = "trash"))}
            onToggleCollapse$={$(() => {
              const key = selectedKey.value;
              if (key === null) return;
              if (selected.kind === "project") {
                collapsedProjects.value = toggle(collapsedProjects.value, key);
              } else if (selected.kind === "semantic" && selected.object.category === "milestone") {
                collapsedMilestones.value = toggle(collapsedMilestones.value, key);
              }
            })}
          />
        )}
      </div>
      {selected === null || annotationMode.value === null ? null : (
        <AnnotationPanel
          key={`${annotationMode.value}:${selectedKey.value ?? "none"}`}
          port={props.port}
          target={selected.reference}
          sourceBasisRef={`workspace-canvas:${selected.project.state === "ready" ? selected.project.revision : selected.reference.ref}`}
          mode={annotationMode.value}
          onClose$={$(() => (annotationMode.value = null))}
        />
      )}
    </section>
  );
});

function toggle(values: readonly string[], key: string): readonly string[] {
  return values.includes(key) ? values.filter((value) => value !== key) : [...values, key];
}

function cardLabel(card: PortfolioCard): string {
  const project =
    card.project.state === "ready"
      ? card.project.view.project.displayName
      : card.project.selection.fallbackLabel;
  const context =
    card.project.state === "ready"
      ? (card.project.view.contexts.find((item) => item.contextId === card.reference.contextId)
          ?.displayName ?? "Context")
      : "Context";
  const title =
    card.kind === "project"
      ? "Context overview"
      : card.kind === "semantic"
        ? card.object.title
        : card.kind === "agent"
          ? card.agent.displayName
          : card.kind === "managed_work"
            ? card.entity === "task"
              ? compactLabel(card.work.goal, 48)
              : `${card.work.provider} run`
            : card.kind === "plan_workspace"
              ? "Plan workspace"
              : card.kind === "worktree"
                ? worktreeCanvasLabel(card)
                : `Merge result · ${card.integration.state}`;
  return `${project} · ${context} · ${title}`;
}

function worktreeCanvasLabel(card: Extract<PortfolioCard, { kind: "worktree" }>): string {
  if (card.worktree.kind === "registered") return "Original checkout";
  if (card.worktree.kind === "plan_root") return "Plan root";
  if (card.worktree.kind === "worker") return "Worker branch";
  return "Merge workspace";
}

function compactLabel(value: string, maximum: number): string {
  return value.length <= maximum ? value : `${value.slice(0, maximum - 1)}…`;
}

function canvasScopes(
  projects: readonly ProjectDescriptor[],
  input: WorkspaceCanvasInput,
): readonly WorkspaceCanvasSelection[] {
  const byProject = new Map(projects.map((project) => [project.projectId, project]));
  const scopes = projects.map((project) => ({
    projectId: project.projectId,
    contextId: project.defaultContextId,
    fallbackLabel: `${project.displayName} · default`,
  }));
  for (const canvasProject of input.projects) {
    if (canvasProject.state !== "ready") continue;
    const project = byProject.get(canvasProject.selection.projectId);
    for (const context of canvasProject.view.contexts)
      scopes.push({
        projectId: canvasProject.selection.projectId,
        contextId: context.contextId,
        fallbackLabel: `${project?.displayName ?? canvasProject.selection.fallbackLabel} · ${context.displayName}`,
      });
  }
  return [...new Map(scopes.map((scope) => [selectionKey(scope), scope])).values()];
}

function selectionKey(
  selection: Pick<WorkspaceCanvasSelection, "projectId" | "contextId">,
): string {
  return `${selection.projectId}\u0000${selection.contextId}`;
}

function selectionSignature(selections: readonly WorkspaceCanvasSelection[]): string {
  return selections.map(selectionKey).join("|");
}
