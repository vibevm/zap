/** Human managed-work lifecycle UI. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import {
  $,
  component$,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import { workspaceRequestId, type WorkspaceClientPort } from "../workspace-client/index.ts";
import { ActorIdSchema } from "../protocol/index.ts";
import type {
  AgentDescriptor,
  ManagedWorkView,
  ProjectId,
  WorkContextId,
  WorkspaceReadResponse,
} from "../workspace-model/index.ts";
import { createProjectEventRefresh } from "./workspace-event-refresh.ts";
import { observeEvents } from "./workspace-helpers.ts";

type ProfileList = Extract<
  WorkspaceReadResponse,
  { operation: "managed-work.profile.list.v1" }
>["profiles"];

export const ManagedWorkPanel = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly onSelectActor$: QRL<(actorId: AgentDescriptor["actorId"]) => void>;
  readonly onChanged$: QRL<() => void>;
}>((props) => {
  const profiles = useSignal<ProfileList>([]);
  const works = useSignal<readonly ManagedWorkView[]>([]);
  const selectedRunId = useSignal<string | null>(null);
  const goal = useSignal("");
  const expectedResult = useSignal("");
  const selectionMode = useSignal("project_policy");
  const overrideReason = useSignal("");
  const targetDomain = useSignal<"semantic_object" | "work_task">("work_task");
  const targetRefs = useSignal("");
  const report = useSignal("");
  const review = useSignal("");
  const busy = useSignal(false);
  const error = useSignal<string | null>(null);
  const notice = useSignal<string | null>(null);
  const refresh = $(async () => {
    const port = props.port;
    if (port === undefined) return;
    const [profileResult, workResult] = await Promise.all([
      port.read({
        operation: "managed-work.profile.list.v1",
        projectId: props.projectId,
        contextId: props.contextId,
      }),
      port.read({
        operation: "managed-work.list.v1",
        projectId: props.projectId,
        contextId: props.contextId,
      }),
    ]);
    if (
      !profileResult.ok ||
      profileResult.value.operation !== "managed-work.profile.list.v1" ||
      !workResult.ok ||
      workResult.value.operation !== "managed-work.list.v1"
    ) {
      error.value = !profileResult.ok
        ? profileResult.error.message
        : !workResult.ok
          ? workResult.error.message
          : "Managed work returned an unexpected response.";
      return;
    }
    profiles.value = profileResult.value.profiles;
    works.value = workResult.value.works;
    if (
      selectedRunId.value === null ||
      !workResult.value.works.some((work) => work.runId === selectedRunId.value)
    )
      selectedRunId.value = workResult.value.works[0]?.runId ?? null;
    error.value = null;
  });
  useVisibleTask$(({ track, cleanup }) => {
    track(() => `${props.projectId}:${props.contextId}`);
    void refresh();
    const port = props.port;
    if (port === undefined) return;
    const controller = new AbortController();
    const events = createProjectEventRefresh(async () => refresh());
    void observeEvents(port, controller.signal, (event) => {
      events.accept(event, props.projectId, props.contextId);
    });
    cleanup(() => {
      controller.abort();
      events.close();
    });
  });
  const selected = (): ManagedWorkView | undefined =>
    works.value.find((work) => work.runId === selectedRunId.value);
  const command = $(
    async (request: Parameters<WorkspaceClientPort["command"]>[0], text: string) => {
      const port = props.port;
      if (port === undefined) return false;
      busy.value = true;
      const result = await port.command(request);
      busy.value = false;
      if (!result.ok) {
        error.value = result.error.message;
        return false;
      }
      notice.value = text;
      error.value = null;
      await refresh();
      await props.onChanged$();
      return true;
    },
  );
  return (
    <section class="workspace-panel managed-work-panel">
      <div class="workspace-section-heading">
        <div>
          <p class="eyebrow">Managed work</p>
          <h2>Delegate a bounded task</h2>
        </div>
        <button class="button secondary" onClick$={refresh}>
          Refresh
        </button>
      </div>
      <p>
        Preparing a task records its scope and model selection. The agent starts only after the
        separate Start action.
      </p>
      {profiles.value.length === 0 ? (
        <p class="workspace-notice">No managed agent profile is available for this project.</p>
      ) : (
        <div class="managed-work-create">
          <label class="field-label" for="managed-work-profile">
            Worker model selection
          </label>
          <select
            id="managed-work-profile"
            value={selectionMode.value}
            onChange$={(_, element) => (selectionMode.value = element.value)}
          >
            <option value="project_policy">Project policy · recommended</option>
            {profiles.value.map((profile) => (
              <option
                key={profile.profileId}
                value={`profile:${profile.profileId}`}
                disabled={!profile.installed || !profile.launchable}
              >
                {`${profile.tier ?? "custom"} · ${profile.provider.replaceAll("_", " ")} · ${profile.modelId}${profile.effort === null ? "" : ` · ${profile.effort}`}`}
              </option>
            ))}
          </select>
          {selectionMode.value === "project_policy" ? (
            <p class="workspace-muted">
              The current saved project policy selects the model when this new run is prepared.
            </p>
          ) : (
            <>
              <label class="field-label" for="managed-work-override-reason">
                Why override project policy?
              </label>
              <textarea
                id="managed-work-override-reason"
                rows={2}
                value={overrideReason.value}
                onInput$={(_, element) => (overrideReason.value = element.value)}
              />
            </>
          )}
          <label class="field-label" for="managed-work-goal">
            Goal
          </label>
          <textarea
            id="managed-work-goal"
            rows={3}
            value={goal.value}
            onInput$={(_, element) => (goal.value = element.value)}
          />
          <label class="field-label" for="managed-work-result">
            Expected result
          </label>
          <textarea
            id="managed-work-result"
            rows={2}
            value={expectedResult.value}
            onInput$={(_, element) => (expectedResult.value = element.value)}
          />
          <label class="field-label" for="managed-work-target-domain">
            Target type
          </label>
          <select
            id="managed-work-target-domain"
            value={targetDomain.value}
            onChange$={(_, element) =>
              (targetDomain.value =
                element.value === "semantic_object" ? "semantic_object" : "work_task")
            }
          >
            <option value="work_task">Work task</option>
            <option value="semantic_object">Plan object</option>
          </select>
          <label class="field-label" for="managed-work-targets">
            Exact target references (optional, one per line)
          </label>
          <textarea
            id="managed-work-targets"
            rows={2}
            value={targetRefs.value}
            onInput$={(_, element) => (targetRefs.value = element.value)}
          />
          <button
            class="button primary"
            disabled={
              busy.value ||
              goal.value.trim() === "" ||
              expectedResult.value.trim() === "" ||
              (selectionMode.value !== "project_policy" && overrideReason.value.trim() === "")
            }
            onClick$={async () => {
              const port = props.port;
              if (port === undefined) return;
              busy.value = true;
              const result = await port.command({
                operation: "managed-work.create.v1",
                clientRequestId: workspaceRequestId("managed-work-create"),
                projectId: props.projectId,
                contextId: props.contextId,
                selection:
                  selectionMode.value === "project_policy"
                    ? { mode: "project_policy" }
                    : {
                        mode: "profile_override",
                        profileId: selectionMode.value.slice("profile:".length),
                        reasonMarkdown: overrideReason.value.trim(),
                      },
                goal: goal.value.trim(),
                expectedResult: expectedResult.value.trim(),
                targetRefs: targetRefs.value
                  .split(/\r?\n/)
                  .map((value) => value.trim())
                  .filter((value) => value !== "")
                  .map((ref) => ({
                    projectId: props.projectId,
                    contextId: props.contextId,
                    domain: targetDomain.value,
                    ref,
                  })),
              });
              busy.value = false;
              if (!result.ok || result.value.operation !== "managed-work.create.v1") {
                error.value = result.ok
                  ? "Managed task creation returned an unexpected response."
                  : result.error.message;
                return;
              }
              selectedRunId.value = result.value.work.runId;
              goal.value = "";
              expectedResult.value = "";
              targetRefs.value = "";
              overrideReason.value = "";
              notice.value = `Task prepared with ${result.value.work.modelSelection.modelId} (${result.value.work.modelSelection.selectionReason}). The agent has not started.`;
              await refresh();
              await props.onChanged$();
            }}
          >
            Prepare task
          </button>
        </div>
      )}
      {works.value.length === 0 ? null : (
        <label class="field-label" for="managed-work-select">
          Task run
          <select
            id="managed-work-select"
            value={selectedRunId.value ?? ""}
            onChange$={(_, element) => (selectedRunId.value = element.value)}
          >
            {works.value.map((work) => (
              <option key={work.runId} value={work.runId}>{`${work.goal} · ${work.state}`}</option>
            ))}
          </select>
        </label>
      )}
      {selected() === undefined ? null : (
        <ManagedWorkDetail
          work={selected()}
          busy={busy.value}
          report={report.value}
          review={review.value}
          onReportInput$={$((value) => (report.value = value))}
          onReviewInput$={$((value) => (review.value = value))}
          onTerminal$={$(() => {
            const work = selected();
            if (work !== undefined) void props.onSelectActor$(ActorIdSchema.parse(work.actorId));
          })}
          onAction$={$(async (action) => {
            const work = selected();
            if (work === undefined) return;
            if (action === "start" || action === "interrupt" || action === "stop") {
              await command(
                {
                  operation: `managed-work.${action}.v1`,
                  clientRequestId: workspaceRequestId(`managed-work-${action}`),
                  projectId: work.projectId,
                  contextId: work.contextId,
                  runId: work.runId,
                  expectedRevision: work.revision,
                },
                action === "start"
                  ? "Managed agent start requested."
                  : `Managed task ${action} requested.`,
              );
              return;
            }
            if (action === "report") {
              if (report.value.trim() === "") return;
              if (
                await command(
                  {
                    operation: "managed-work.report.v1",
                    clientRequestId: workspaceRequestId("managed-work-report"),
                    projectId: work.projectId,
                    contextId: work.contextId,
                    runId: work.runId,
                    expectedRevision: work.revision,
                    summaryMarkdown: report.value.trim(),
                    artifactRefs: [],
                  },
                  "Worker report recorded for review.",
                )
              )
                report.value = "";
              return;
            }
            const disposition = action === "accept" ? "accepted" : "follow_up_required";
            if (
              await command(
                {
                  operation: "managed-work.review.v1",
                  clientRequestId: workspaceRequestId(`managed-work-${disposition}`),
                  projectId: work.projectId,
                  contextId: work.contextId,
                  runId: work.runId,
                  expectedRevision: work.revision,
                  disposition,
                  commentMarkdown: review.value.trim(),
                },
                disposition === "accepted" ? "Worker report accepted." : "Changes requested.",
              )
            )
              review.value = "";
          })}
        />
      )}
      {notice.value === null ? null : (
        <p class="workspace-notice" role="status">
          {notice.value}
        </p>
      )}
      {error.value === null ? null : (
        <p class="workspace-error" role="alert">
          {error.value}
        </p>
      )}
    </section>
  );
});

type ManagedAction = "start" | "interrupt" | "stop" | "report" | "accept" | "follow_up";
const ManagedWorkDetail = component$<{
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
      <div>
        <strong>{work.goal}</strong>
        <span class={`coordinator-state state-${work.state}`}>
          {work.state.replaceAll("_", " ")}
        </span>
      </div>
      <p>{work.expectedResult}</p>
      <dl class="canvas-facts">
        <div>
          <dt>Provider</dt>
          <dd>{work.provider.replaceAll("_", " ")}</dd>
        </div>
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
          <dt>Terminal</dt>
          <dd>{work.terminalId}</dd>
        </div>
      </dl>
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

function effortLabel(effort: ManagedWorkView["modelSelection"]["effectiveEffort"]): string {
  return effort.state === "explicit" ||
    effort.state === "configured_default" ||
    effort.state === "inherited"
    ? (effort.value ?? "provider default")
    : effort.state;
}
