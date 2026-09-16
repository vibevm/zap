/** Human managed-work lifecycle UI. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import {
  $,
  component$,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import {
  readRepositoryWorkspace,
  workspaceRequestId,
  type RepositoryWorkspaceView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
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
import { ManagedWorkDetail } from "./managed-work-detail.tsx";

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
  const repository = useSignal<RepositoryWorkspaceView | null>(null);
  const workspaceMode = useSignal<"inherit" | "isolated_child">("inherit");
  const workspacePlanId = useSignal("");
  const parentWorktreeId = useSignal("");
  const report = useSignal("");
  const review = useSignal("");
  const busy = useSignal(false);
  const error = useSignal<string | null>(null);
  const notice = useSignal<string | null>(null);
  const refresh = $(async () => {
    const port = props.port;
    if (port === undefined) return;
    const [profileResult, workResult, repositoryResult] = await Promise.all([
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
      readRepositoryWorkspace(port, props.projectId, props.contextId),
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
    repository.value = repositoryResult.ok ? repositoryResult.value : null;
    const plan =
      repository.value?.plans.find((candidate) => candidate.planId === workspacePlanId.value) ??
      repository.value?.plans[0];
    workspacePlanId.value = plan?.planId ?? "";
    const parent =
      repository.value?.worktrees.find(
        (worktree) =>
          worktree.planId === plan?.planId && worktree.worktreeId === parentWorktreeId.value,
      ) ?? repository.value?.worktrees.find((worktree) => worktree.planId === plan?.planId);
    parentWorktreeId.value = parent?.worktreeId ?? "";
    if (repository.value === null) workspaceMode.value = "inherit";
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
          <label class="field-label" for="managed-work-workspace-mode">
            Worker workspace
          </label>
          <select
            id="managed-work-workspace-mode"
            value={workspaceMode.value}
            onChange$={(_, element) =>
              (workspaceMode.value =
                element.value === "isolated_child" ? "isolated_child" : "inherit")
            }
          >
            <option value="inherit">Inherit selected plan workspace</option>
            <option
              value="isolated_child"
              disabled={repository.value === null || repository.value.plans.length === 0}
            >
              Isolated child worktree
            </option>
          </select>
          {repository.value === null ? (
            <p class="workspace-muted">
              Repository workspace capability is unavailable. This task will use the registered
              context workspace.
            </p>
          ) : repository.value.plans.length === 0 ? (
            <p class="workspace-muted">
              Prepare an independent plan workspace before requesting isolation.
            </p>
          ) : (
            <>
              <label class="field-label" for="managed-work-plan-workspace">
                Plan workspace
              </label>
              <select
                id="managed-work-plan-workspace"
                value={workspacePlanId.value}
                onChange$={(_, element) => {
                  workspacePlanId.value = element.value;
                  parentWorktreeId.value =
                    repository.value?.worktrees.find(
                      (worktree) => worktree.planId === element.value,
                    )?.worktreeId ?? "";
                }}
              >
                {repository.value.plans.map((plan) => (
                  <option key={plan.planId} value={plan.planId}>
                    {`${plan.displayName} · ${plan.state}`}
                  </option>
                ))}
              </select>
              {workspaceMode.value !== "isolated_child" ? null : (
                <label class="field-label" for="managed-work-parent-worktree">
                  Parent branch and committed basis
                  <select
                    id="managed-work-parent-worktree"
                    value={parentWorktreeId.value}
                    onChange$={(_, element) => (parentWorktreeId.value = element.value)}
                  >
                    {repository.value.worktrees
                      .filter((worktree) => worktree.planId === workspacePlanId.value)
                      .map((worktree) => (
                        <option key={worktree.worktreeId} value={worktree.worktreeId}>
                          {`${branchName(worktree.branchRef)} · ${worktree.headCommit.slice(0, 10)}`}
                        </option>
                      ))}
                  </select>
                </label>
              )}
              <p class="workspace-muted">
                Isolation forks the recorded committed basis. Unsaved edits stay in their current
                worktree.
              </p>
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
                planId: workspacePlanId.value === "" ? null : workspacePlanId.value,
                workspaceRequest:
                  workspaceMode.value === "isolated_child"
                    ? isolatedWorkspace(repository.value, parentWorktreeId.value)
                    : { mode: "inherit" },
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
            if (
              action === "start" ||
              action === "continue" ||
              action === "interrupt" ||
              action === "stop"
            ) {
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

function isolatedWorkspace(view: RepositoryWorkspaceView | null, worktreeId: string) {
  const parent = view?.worktrees.find((worktree) => worktree.worktreeId === worktreeId);
  return parent === undefined
    ? { mode: "inherit" as const }
    : {
        mode: "isolated_child" as const,
        parentWorktreeId: parent.worktreeId,
        expectedParentHead: parent.headCommit,
      };
}

function branchName(value: string): string {
  return value.split("/").at(-1) ?? value;
}
