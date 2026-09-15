/** Durable scoped activity queue. @scope spec://org.vibevm.zap/lens/PROP-005#history */
import { component$, useSignal, type QRL } from "@qwik.dev/core";

import type { HistoryEvent } from "../workspace-model/index.ts";
import { isSemanticEvent } from "./workspace-helpers.ts";

export type ActivityScope = "all" | "project" | "agent";

export interface ActivityPanelProps {
  readonly scope: ActivityScope;
  readonly projectLabel: string | null;
  readonly agentLabel: string | null;
  readonly projectNames: ReadonlyMap<string, string>;
  readonly actorNames: ReadonlyMap<string, string>;
  readonly events: readonly HistoryEvent[];
  readonly loading: boolean;
  readonly error: string | null;
  readonly hasMore: boolean;
  readonly coverage: string | null;
  readonly onScope$: QRL<(scope: ActivityScope) => void>;
  readonly onLoadMore$: QRL<() => void>;
}

export const ActivityPanel = component$<ActivityPanelProps>((props) => {
  const showAll = useSignal(false);
  const events = showAll.value ? props.events : props.events.filter(isSemanticEvent);
  return (
    <section class="workspace-panel activity-panel">
      <div class="workspace-panel-heading">
        <div>
          <p class="eyebrow">Event queue</p>
          <h2>Activity</h2>
        </div>
        <div class="scope-switch" role="group" aria-label="Activity scope">
          <button
            class={showAll.value ? "selected" : ""}
            onClick$={() => {
              showAll.value = !showAll.value;
            }}
          >
            {showAll.value ? "Semantic events" : "Show all events"}
          </button>
          <ScopeButton label="All" value="all" current={props.scope} onSelect$={props.onScope$} />
          <ScopeButton
            label={props.projectLabel ?? "Project"}
            value="project"
            current={props.scope}
            disabled={props.projectLabel === null}
            onSelect$={props.onScope$}
          />
          <ScopeButton
            label={props.agentLabel ?? "Agent"}
            value="agent"
            current={props.scope}
            disabled={props.agentLabel === null}
            onSelect$={props.onScope$}
          />
        </div>
      </div>
      {props.coverage === null ? null : <p class="workspace-notice">{props.coverage}</p>}
      {props.loading && events.length === 0 ? (
        <p class="workspace-muted">Loading durable activity…</p>
      ) : props.error !== null ? (
        <p class="workspace-notice">{props.error}</p>
      ) : events.length === 0 ? (
        <div class="workspace-empty">
          <strong>{props.events.length === 0 ? "No captured events" : "No semantic events"}</strong>
          <span>
            {props.events.length === 0
              ? "The selected scope has no retained public history."
              : "Raw token and unmapped diagnostics remain available in Show all events."}
          </span>
        </div>
      ) : (
        <ol class="activity-list">
          {events.map((event) => (
            <li key={event.historyEventId}>
              <div class="activity-marker" data-source={event.source} />
              <div>
                <div class="activity-meta">
                  <strong>{humanKind(event.kind)}</strong>
                  <span>{event.source}</span>
                  <span title={event.projectId}>
                    {props.projectNames.get(event.projectId) ?? event.projectId}
                  </span>
                  {event.actorId === null ? null : (
                    <span title={event.actorId}>
                      {props.actorNames.get(event.actorId) ?? event.actorId}
                    </span>
                  )}
                  <time dateTime={event.occurrenceAt}>{formatDate(event.occurrenceAt)}</time>
                </div>
                {event.planProvenance === null ? null : (
                  <p class="activity-detail">{planSummary(event)}</p>
                )}
                {event.sourceEventId === null ? null : (
                  <small>Source event retained · sequence {event.projectSequence}</small>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
      {props.hasMore ? (
        <button
          class="button secondary activity-more"
          disabled={props.loading}
          onClick$={props.onLoadMore$}
        >
          {props.loading ? "Loading…" : "Load earlier activity"}
        </button>
      ) : null}
    </section>
  );
});

const ScopeButton = component$<{
  readonly label: string;
  readonly value: ActivityScope;
  readonly current: ActivityScope;
  readonly disabled?: boolean;
  readonly onSelect$: QRL<(scope: ActivityScope) => void>;
}>((props) => (
  <button
    class={props.current === props.value ? "selected" : ""}
    disabled={props.disabled ?? false}
    onClick$={() => props.onSelect$(props.value)}
  >
    {props.label}
  </button>
));

function humanKind(value: string): string {
  return value.replaceAll(/[._-]+/g, " ").replace(/^./, (letter) => letter.toUpperCase());
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function planSummary(event: HistoryEvent): string {
  const plan = event.planProvenance;
  if (plan === null) return "";
  if (plan.appliedRevision !== null) return `Applied plan revision ${plan.appliedRevision}.`;
  if (plan.proposedPlanId !== null)
    return "A proposed plan revision is recorded with its source basis.";
  if (plan.previousPlanId !== null) return "The event retains its previous plan identity.";
  return "Plan source basis recorded.";
}
