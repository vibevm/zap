/** @scope spec://org.vibevm.zap/lens/PROP-002#shared-client */
import {
  $,
  component$,
  useSignal,
  useStore,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";

import {
  filterQuicklensObjects,
  layoutQuicklensGraph,
  relationshipsShownInGraph,
  type GraphFilters,
  type GraphView,
} from "../quicklens-graph/index.ts";
import type {
  ObjectCategory,
  QuicklensDataSource,
  QuicklensError,
  QuicklensRef,
  QuicklensSnapshot,
} from "../quicklens-model/index.ts";
import { GraphCanvas } from "./graph-canvas.tsx";
import { ObjectCard } from "./object-card.tsx";
import { PlanPanel } from "./plan-panel.tsx";
import { QuestionPanel } from "./question-panel.tsx";

export interface QuicklensAppProps {
  readonly source: NoSerialize<QuicklensDataSource>;
  readonly initialSelectedRef?: QuicklensRef | undefined;
  readonly headerActionLabel?: string | undefined;
  readonly onHeaderAction$?: QRL<() => void | Promise<void>> | undefined;
}

export const QuicklensApp = component$<QuicklensAppProps>((props) => {
  const snapshot = useSignal<QuicklensSnapshot | null>(null);
  const error = useSignal<QuicklensError | null>(null);
  const loading = useSignal(true);
  const selectedRef = useSignal<QuicklensRef | null>(props.initialSelectedRef ?? null);
  const theme = useSignal<"light" | "dark">("light");
  const graphView = useSignal<GraphView>("goal");
  const filters = useStore<{
    search: string;
    category: "all" | ObjectCategory;
    status: string;
  }>({ search: "", category: "all", status: "all" });

  const refresh = $(async () => {
    const source = props.source;
    if (source === undefined) return;
    loading.value = snapshot.value === null;
    const controller = new AbortController();
    const result = await source.read({ signal: controller.signal });
    loading.value = false;
    if (result.ok) {
      snapshot.value = result.value;
      error.value = null;
      if (
        selectedRef.value !== null &&
        !result.value.objects.some((object) => object.ref === selectedRef.value)
      ) {
        selectedRef.value = null;
      }
    } else {
      error.value = result.error;
    }
  });

  useVisibleTask$(({ cleanup }) => {
    const saved = window.localStorage.getItem("quicklens.theme");
    theme.value = saved === "dark" ? "dark" : "light";
    void refresh();
    const source = props.source;
    if (source === undefined || source.subscribe === undefined) return;
    const unsubscribe = source.subscribe(() => void refresh());
    cleanup(unsubscribe);
  });

  const current = snapshot.value;
  const statusOptions = [
    ...new Map(current?.objects.map((object) => [object.status.code, object.status.label]) ?? []),
  ];
  const graphFilters: GraphFilters = {
    search: filters.search,
    categories: filters.category === "all" ? [] : [filters.category],
    statuses: filters.status === "all" ? [] : [filters.status],
  };
  const graphLayout = current === null ? null : layoutQuicklensGraph(current, graphView.value);
  const visibleObjects = current === null ? [] : filterQuicklensObjects(current, graphFilters);
  const visibleRefs = new Set(visibleObjects.map((object) => object.ref));
  const visibleRelationshipCount = (
    current === null ? [] : relationshipsShownInGraph(current, graphView.value)
  ).filter(
    (relationship) => visibleRefs.has(relationship.source) && visibleRefs.has(relationship.target),
  ).length;
  const selected =
    selectedRef.value === null
      ? null
      : (current?.objects.find((object) => object.ref === selectedRef.value) ?? null);
  const selectedHidden = selected !== null && !visibleRefs.has(selected.ref);
  const relationships =
    selected === null || current === null
      ? []
      : current.relationships.filter(
          (relationship) =>
            relationship.source === selected.ref || relationship.target === selected.ref,
        );
  const titles = Object.fromEntries(
    current?.objects.map((object) => [object.ref, object.title]) ?? [],
  );
  const pendingQuestions =
    current?.questions.filter((question) => question.state === "pending").length ?? 0;

  return (
    <div class={`quicklens theme-${theme.value}`}>
      <header class="topbar">
        <div class="brand-lockup">
          <span class="brand-mark">Q</span>
          <div>
            <strong>Quicklens</strong>
            <span>Plan workspace</span>
          </div>
        </div>
        <div class="source-summary">
          {current === null ? (
            <span>Connecting…</span>
          ) : (
            <>
              <span class={`source-mode source-${current.sourceMode}`}>{current.sourceMode}</span>
              <strong>{current.sourceLabel}</strong>
              <span>Revision {current.revision}</span>
            </>
          )}
        </div>
        <div class="header-actions">
          {props.onHeaderAction$ === undefined || props.headerActionLabel === undefined ? null : (
            <button class="header-session-action" onClick$={props.onHeaderAction$}>
              {props.headerActionLabel}
            </button>
          )}
          <button
            class="question-jump"
            disabled={pendingQuestions === 0}
            onClick$={() => {
              document.getElementById("questions")?.scrollIntoView({ behavior: "smooth" });
            }}
          >
            <span>{pendingQuestions}</span>
            {pendingQuestions === 1 ? "question" : "questions"}
          </button>
          <button
            class="theme-toggle"
            aria-label={`Switch to ${theme.value === "light" ? "dark" : "light"} theme`}
            onClick$={() => {
              theme.value = theme.value === "light" ? "dark" : "light";
              window.localStorage.setItem("quicklens.theme", theme.value);
            }}
          >
            {theme.value === "light" ? "◐" : "◑"}
          </button>
        </div>
      </header>

      {current?.sourceMode === "demo" ? (
        <div class="demo-banner">Demonstration mode · synthetic data · no live actions</div>
      ) : null}
      {current?.phase === "partial" || current?.phase === "stale" ? (
        <div class={`phase-banner phase-${current.phase}`}>
          <strong>{current.phase === "partial" ? "Partial data" : "Stale snapshot"}</strong>
          <span>{current.phaseDetail ?? "Refresh before making a decision."}</span>
        </div>
      ) : null}

      <main class="workspace">
        <aside class="controls panel">
          <p class="eyebrow">Explore</p>
          <label class="field-label" for="search">
            Search
          </label>
          <input
            id="search"
            type="search"
            placeholder="Name, purpose, type, status"
            value={filters.search}
            onInput$={(_, element) => {
              filters.search = element.value;
            }}
          />
          <label class="field-label" for="graph-view">
            Graph view
          </label>
          <select
            id="graph-view"
            value={graphView.value}
            onChange$={(_, element) => {
              graphView.value = element.value === "work_order" ? "work_order" : "goal";
            }}
          >
            <option value="goal">Goal structure</option>
            <option value="work_order">Work order</option>
          </select>
          <label class="field-label" for="category">
            Object type
          </label>
          <select
            id="category"
            value={filters.category}
            onChange$={(_, element) => {
              const value = element.value;
              filters.category =
                value === "task" ||
                value === "milestone" ||
                value === "resource" ||
                value === "other"
                  ? value
                  : "all";
            }}
          >
            <option value="all">All types</option>
            <option value="task">Tasks</option>
            <option value="milestone">Milestones</option>
            <option value="resource">Resources</option>
            <option value="other">Other semantic types</option>
          </select>
          <label class="field-label" for="status">
            Status
          </label>
          <select
            id="status"
            value={filters.status}
            onChange$={(_, element) => {
              filters.status = element.value;
            }}
          >
            <option value="all">All statuses</option>
            {statusOptions.map(([code, label]) => (
              <option value={code} key={code}>
                {label}
              </option>
            ))}
          </select>
          <button class="button quiet" onClick$={refresh}>
            Refresh data
          </button>
          {current === null ? null : (
            <div class="legend">
              <strong>
                {graphView.value === "goal" ? "Goal coordinates" : "Work-order coordinates"}
              </strong>
              <p>
                {graphView.value === "goal"
                  ? "The identified goal is central. Explicit strategy, plan membership and decomposition extend outward; context has no work order."
                  : "Prerequisites point left to right. A shared stage means these links establish no order; resources and holds remain separate."}
              </p>
              <span>
                <i class="legend-dot tone-active" /> Active
              </span>
              <span>
                <i class="legend-dot tone-success" /> Complete
              </span>
              <span>
                <i class="legend-dot tone-warning" /> Waiting
              </span>
              <span>
                <i class="legend-dot tone-neutral" /> Neutral
              </span>
              <span>
                <i class="legend-line edge-execution" /> Prerequisite
              </span>
              <span>
                <i class="legend-line edge-hierarchy" /> Hierarchy
              </span>
              <span>
                <i class="legend-line edge-reference" /> Resource / reference
              </span>
            </div>
          )}
        </aside>

        <section class="graph-shell">
          {loading.value ? (
            <StateCard title="Loading workspace" detail="Reading the active semantic snapshot…" />
          ) : error.value !== null && current === null ? (
            <StateCard
              title="Workspace unavailable"
              detail={error.value.message}
              recovery={error.value.recovery}
            />
          ) : current === null ? (
            <StateCard title="No data" detail="The data source returned no usable snapshot." />
          ) : current.objects.length === 0 ? (
            <StateCard
              title="Empty workspace"
              detail="No semantic objects are available in this context."
            />
          ) : visibleObjects.length === 0 ? (
            <StateCard
              title="No matches"
              detail={`0 of ${String(current.objects.length)} objects match the current filters.`}
              recovery="Adjust search, object type or status to restore the graph."
            />
          ) : (
            <GraphCanvas
              snapshot={current}
              filters={graphFilters}
              selectedRef={selectedRef.value}
              theme={theme.value}
              view={graphView.value}
              onSelect$={$((ref) => {
                selectedRef.value = ref;
              })}
            />
          )}
          {current === null || current.objects.length === 0 ? null : (
            <div class="graph-caption" aria-live="polite">
              <strong>
                {visibleObjects.length} of {current.objects.length} objects visible
              </strong>
              <span>
                {visibleRelationshipCount} of {current.relationships.length} relationships shown
              </span>
            </div>
          )}
          {graphLayout === null ? null : (
            <div class="graph-guide" aria-live="polite">
              <strong>
                {graphView.value === "goal" ? "Goal → decomposition" : "Start → next step"}
              </strong>
              <span>
                {graphView.value === "goal"
                  ? "Distance shows known decomposition, never effort or priority. Other links remain in the selected card."
                  : "Columns show prerequisite stages, never duration or readiness. Other links remain in the selected card."}
              </span>
              {graphLayout.diagnostics.length === 0 ? null : (
                <ul>
                  {graphLayout.diagnostics.map((diagnostic) => (
                    <li key={diagnostic.code}>{diagnostic.message}</li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>

        <aside class="inspector">
          {selectedHidden ? (
            <p class="selection-note">
              Selected card kept open · current filters hide this object from the graph.
            </p>
          ) : null}
          <ObjectCard object={selected} relationships={relationships} titles={titles} />
        </aside>

        <section class="lower-grid">
          <QuestionPanel
            questions={current?.questions ?? []}
            answerAvailability={
              current?.questionAnswer ?? { enabled: false, reason: "Question data is loading." }
            }
            source={props.source}
            onRefresh$={refresh}
          />
          <PlanPanel
            plan={current?.plan ?? null}
            targets={current?.agentTargets ?? []}
            source={props.source}
            onRefresh$={refresh}
          />
        </section>
      </main>
    </div>
  );
});

const StateCard = component$<{
  readonly title: string;
  readonly detail: string;
  readonly recovery?: string;
}>(({ title, detail, recovery }) => (
  <div class="state-card">
    <span class="state-orbit" />
    <h2>{title}</h2>
    <p>{detail}</p>
    {recovery === undefined ? null : <small>{recovery}</small>}
  </div>
));
