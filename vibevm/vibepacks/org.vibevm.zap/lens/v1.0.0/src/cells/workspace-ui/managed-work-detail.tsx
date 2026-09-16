/** Managed run inspection and review controls. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { component$, type QRL } from "@qwik.dev/core";
import type { ManagedWorkView } from "../workspace-model/index.ts";

export type ManagedAction =
  | "start"
  | "continue"
  | "interrupt"
  | "stop"
  | "report"
  | "accept"
  | "follow_up";

export const ManagedWorkDetail = component$<{
  readonly work: ManagedWorkView | undefined;
  readonly busy: boolean;
  readonly report: string;
  readonly review: string;
  readonly onReportInput$: QRL<(value: string) => void>;
  readonly onReviewInput$: QRL<(value: string) => void>;
  readonly onTerminal$: QRL<() => void>;
  readonly onAction$: QRL<(action: ManagedAction) => void>;
}>((props) => {
  const work = props.work;
  if (work === undefined) return null;
  const active = ["launching", "running", "waiting_for_user", "uncertain"].includes(work.state);
  return (
    <article class="managed-work-detail">
      <p class="eyebrow">
        {work.executionSelection?.configurationName ?? "Legacy execution profile"}
      </p>
      <div>
        <strong>{work.goal}</strong>
        <span class={`coordinator-state state-${work.state}`}>
          {work.state.replaceAll("_", " ")}
        </span>
      </div>
      <p>{work.expectedResult}</p>
      <dl class="canvas-facts">
        {work.executionSelection === null ? null : (
          <div>
            <dt>Execution choice</dt>
            <dd>{work.executionSelection.configurationName}</dd>
          </div>
        )}
        <div>
          <dt>Task type</dt>
          <dd>{work.specialization.replaceAll("_", " ")}</dd>
        </div>
        <div>
          <dt>Provider</dt>
          <dd>{work.provider.replaceAll("_", " ")}</dd>
        </div>
        {work.managedControl === null ? null : (
          <div>
            <dt>Provider readiness</dt>
            <dd>{work.managedControl.readiness.replaceAll("_", " ")}</dd>
          </div>
        )}
        <div>
          <dt>Model</dt>
          <dd>{work.modelSelection.modelId}</dd>
        </div>
        <div>
          <dt>Tier / effort</dt>
          <dd>
            {work.modelSelection.requestedTier} · {effortLabel(work.modelSelection.effectiveEffort)}
          </dd>
        </div>
        <div>
          <dt>Workspace</dt>
          <dd>{workspaceLabel(work)}</dd>
        </div>
      </dl>
      {work.managedControl?.continuation === "restart_from_saved_session" ? (
        <p class="workspace-muted">
          Continue restarts the owned terminal from the saved provider conversation without
          replaying the task bootstrap.
        </p>
      ) : null}
      <p class="workspace-muted">{work.modelSelection.selectionReason}</p>
      {work.modelSelection.overrideReason === null ? null : (
        <p class="workspace-notice">
          <strong>Override rationale</strong>
          <br />
          {work.modelSelection.overrideReason}
        </p>
      )}
      <div class="execution-actions">
        <button class="button secondary" onClick$={props.onTerminal$}>
          Open terminal
        </button>
        {work.state === "prepared" ? (
          <button
            class="button primary"
            disabled={props.busy}
            onClick$={() => props.onAction$("start")}
          >
            Start
          </button>
        ) : null}
        {["paused", "stopped"].includes(work.state) &&
        work.managedControl?.continuation !== "unavailable" ? (
          <button
            class="button primary"
            disabled={props.busy || work.managedControl?.providerSessionId === null}
            onClick$={() => props.onAction$("continue")}
          >
            Continue saved conversation
          </button>
        ) : null}
        {!active ? null : (
          <button
            class="button secondary"
            disabled={props.busy}
            onClick$={() => props.onAction$("interrupt")}
          >
            Interrupt
          </button>
        )}
        {!active ? null : (
          <button
            class="button danger"
            disabled={props.busy}
            onClick$={() => props.onAction$("stop")}
          >
            Stop
          </button>
        )}
      </div>
      {work.report === null ? (
        <div class="managed-work-report">
          <label class="field-label" for="managed-work-report">
            Worker report
          </label>
          <textarea
            id="managed-work-report"
            rows={3}
            value={props.report}
            onInput$={(_, element) => props.onReportInput$(element.value)}
          />
          <button
            class="button secondary"
            disabled={props.busy || props.report.trim() === ""}
            onClick$={() => props.onAction$("report")}
          >
            Record report
          </button>
        </div>
      ) : (
        <div class="managed-work-report">
          <strong>Worker report</strong>
          <p>{work.report.summaryMarkdown}</p>
          {work.review === null ? (
            <>
              <label class="field-label" for="managed-work-review">
                Review comment
              </label>
              <textarea
                id="managed-work-review"
                rows={2}
                value={props.review}
                onInput$={(_, element) => props.onReviewInput$(element.value)}
              />
              <div class="execution-actions">
                <button
                  class="button primary"
                  disabled={props.busy}
                  onClick$={() => props.onAction$("accept")}
                >
                  Accept report
                </button>
                <button
                  class="button secondary"
                  disabled={props.busy || props.review.trim() === ""}
                  onClick$={() => props.onAction$("follow_up")}
                >
                  Request changes
                </button>
              </div>
            </>
          ) : (
            <p>
              <strong>{work.review.disposition.replaceAll("_", " ")}</strong>
              {work.review.commentMarkdown === "" ? null : ` · ${work.review.commentMarkdown}`}
            </p>
          )}
        </div>
      )}
    </article>
  );
});

function workspaceLabel(work: ManagedWorkView): string {
  const assignment = work.workspaceAssignment;
  return assignment === null
    ? "Unavailable"
    : assignment.kind === "legacy_registered"
      ? "Registered context"
      : `${assignment.kind.replaceAll("_", " ")} · ${assignment.basisCommit?.slice(0, 10) ?? "unknown basis"}`;
}

function effortLabel(effort: ManagedWorkView["modelSelection"]["effectiveEffort"]): string {
  return effort.state === "explicit" ||
    effort.state === "configured_default" ||
    effort.state === "inherited"
    ? (effort.value ?? "provider default")
    : effort.state;
}
