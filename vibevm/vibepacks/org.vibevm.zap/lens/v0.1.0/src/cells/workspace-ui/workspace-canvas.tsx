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
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type { ProjectDescriptor, ProjectId } from "../workspace-model/index.ts";
import { CanvasCard } from "./canvas-card.tsx";
import { AnnotationPanel } from "./annotation-panel.tsx";
import { ScopedRequestFence } from "./request-fence.ts";

export const WorkspaceCanvas = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projects: readonly ProjectDescriptor[];
  readonly theme: "light" | "dark";
  readonly refreshEpoch: number;
  readonly onOpenProject$: QRL<(projectId: ProjectId) => void>;
  readonly onOpenQuestions$: QRL<(projectId: ProjectId) => void>;
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
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));

  useVisibleTask$(({ track, cleanup }) => {
    const projectScope = track(() =>
      props.projects.map((project) => `${project.projectId}:${project.revision}`).join("|"),
    );
    const refresh = track(() => refreshEpoch.value);
    track(() => props.refreshEpoch);
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined || props.projects.length === 0) {
      input.value = null;
      loading.value = false;
      return;
    }
    const token = activeFence.begin(`${projectScope}\u0000${String(refresh)}`);
    loading.value = true;
    void readWorkspaceCanvas(
      port,
      props.projects.slice(0, 8).map((project) => ({
        projectId: project.projectId,
        contextId: project.defaultContextId,
        fallbackLabel: project.displayName,
      })),
    ).then((read) => {
      if (!activeFence.isCurrent(token)) return;
      loading.value = false;
      if (!read.ok) {
        error.value = read.error.message;
        return;
      }
      input.value = read.value;
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
            project.revision,
            ...project.managedWorks.map((work) => work.revision),
          ]
        : [project.selection.projectId, project.reason],
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
  const omittedProjects = props.projects.slice(8);

  return (
    <section class="workspace-canvas-shell">
      <header class="workspace-panel canvas-toolbar">
        <div>
          <p class="eyebrow">Unified project map</p>
          <h1>Projects, plans and agents</h1>
          <p>Each region keeps its own project, context and plan authority.</p>
          <small>
            Showing {input.value.projects.length} of {props.projects.length} registered project(s).
          </small>
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
            {[...new Map(questions.map((item) => [item.projectId, item])).values()].map((item) => (
              <button
                key={item.projectId}
                class="button secondary"
                onClick$={() => props.onOpenQuestions$(item.projectId)}
              >
                {item.projectLabel}
              </button>
            ))}
          </div>
        )}
      </header>
      {omittedProjects.length === 0 ? null : (
        <details class="workspace-notice canvas-diagnostics">
          <summary>{omittedProjects.length} project(s) are outside this bounded map</summary>
          <p>{omittedProjects.map((project) => project.displayName).join(", ")}</p>
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
  const title =
    card.kind === "project"
      ? project
      : card.kind === "semantic"
        ? card.object.title
        : card.kind === "agent"
          ? card.agent.displayName
          : card.entity === "task"
            ? card.work.goal
            : `${card.work.provider} run`;
  return `${project} · ${title}`;
}
