/** Plan/worktree/integration operations UI. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
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
  commandRepositoryWorkspace,
  readIntegrationDiff,
  readRepositoryWorkspace,
  workspaceRequestId,
  type RepositoryWorkspaceView,
  type IntegrationDiffView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type { IntegrationAttempt, RepositoryWorktreeRecord } from "../repository-model/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import { ScopedRequestFence } from "./request-fence.ts";
import {
  WorktreeSelect,
  TestEvidenceSummary,
  PlanSummary,
  branchName,
  failed,
  integrationName,
  planWorktrees,
  responsibility,
  selectedOrFirst,
  shortCommit,
  worktreeDisplayName,
} from "./repository-workspace-helpers.tsx";

export const RepositoryWorkspacePanel = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly onChanged$: QRL<() => void>;
  readonly onOpenContext$: QRL<(projectId: ProjectId, contextId: WorkContextId) => void>;
}>((props) => {
  const view = useSignal<RepositoryWorkspaceView | null>(null);
  const selectedPlanId = useSignal("");
  const selectedIntegrationId = useSignal("");
  const planName = useSignal("");
  const sourceWorktreeId = useSignal("");
  const targetWorktreeId = useSignal("");
  const testProfileId = useSignal("");
  const reviewRationale = useSignal("");
  const busy = useSignal(false);
  const message = useSignal<string | null>(null);
  const error = useSignal<string | null>(null);
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));

  const refresh = $(async () => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return;
    const token = activeFence.begin(`${props.projectId}\u0000${props.contextId}`);
    const read = await readRepositoryWorkspace(port, props.projectId, props.contextId);
    if (!activeFence.isCurrent(token)) return;
    if (!read.ok) {
      view.value = null;
      error.value = read.error.message;
      return;
    }
    view.value = read.value;
    const plan =
      read.value.plans.find((candidate) => candidate.planId === selectedPlanId.value) ??
      read.value.plans[0];
    selectedPlanId.value = plan?.planId ?? "";
    const worktrees = planWorktrees(read.value, plan?.planId ?? null);
    sourceWorktreeId.value =
      selectedOrFirst(sourceWorktreeId.value, worktrees, plan?.rootWorktreeId)?.worktreeId ?? "";
    targetWorktreeId.value =
      selectedOrFirst(targetWorktreeId.value, worktrees, plan?.integrationTargetWorktreeId)
        ?.worktreeId ?? "";
    const integration =
      read.value.integrations.find(
        (candidate) => candidate.integrationId === selectedIntegrationId.value,
      ) ?? read.value.integrations.find((candidate) => candidate.planId === plan?.planId);
    selectedIntegrationId.value = integration?.integrationId ?? "";
    testProfileId.value =
      read.value.testProfiles.find((profile) => profile.profileId === testProfileId.value)
        ?.profileId ??
      read.value.testProfiles[0]?.profileId ??
      "";
    error.value = null;
  });

  useVisibleTask$(({ track, cleanup }) => {
    track(() => `${props.projectId}:${props.contextId}`);
    void refresh();
    cleanup(() => fence.value?.cancel());
  });

  const run = $(async (action: () => Promise<boolean>) => {
    busy.value = true;
    try {
      if (await action()) {
        await refresh();
        await props.onChanged$();
      }
    } finally {
      busy.value = false;
    }
  });

  if (view.value === null) {
    return (
      <section class="workspace-panel workspace-empty">
        <strong>Repository workspace unavailable</strong>
        <span>{error.value ?? "This context has no repository workspace feature yet."}</span>
        <button class="button secondary" onClick$={refresh}>
          Retry
        </button>
      </section>
    );
  }
  const current = view.value;
  const selectedPlan = current.plans.find((plan) => plan.planId === selectedPlanId.value) ?? null;
  const contextPlan =
    current.plans.find((plan) => plan.contextId === props.contextId) ?? selectedPlan;
  const worktrees = planWorktrees(current, selectedPlan?.planId ?? null);
  const planIntegrations = current.integrations.filter(
    (integration) => integration.planId === selectedPlan?.planId,
  );
  const selectedIntegration =
    planIntegrations.find(
      (integration) => integration.integrationId === selectedIntegrationId.value,
    ) ??
    planIntegrations[0] ??
    null;

  return (
    <section class="workspace-panel repository-workspace-panel">
      <div class="workspace-section-heading">
        <div>
          <p class="eyebrow">Plan workspaces</p>
          <h2>Branches, assignments and integration</h2>
        </div>
        <button class="button secondary" onClick$={refresh}>
          Refresh
        </button>
      </div>
      <p>
        Current context: <strong>{contextPlan?.displayName ?? "Original checkout"}</strong>. Branch{" "}
        {branchName(current.contextWorktree.branchRef)} at commit{" "}
        {shortCommit(current.observedContextHead)}. Original checkout:{" "}
        {branchName(current.registeredWorktree.branchRef)}.
      </p>
      {current.workingTreeState === "clean" ? (
        <p class="workspace-muted">Clean working tree · committed basis shown above.</p>
      ) : (
        <p class="workspace-notice">
          {current.workingTreeState === "dirty"
            ? "Uncommitted changes stay in this worktree. A new plan forks from the observed committed HEAD; Zap does not stash or copy those changes."
            : "Working-tree cleanliness is unknown. Refresh before preparing a new workspace."}
        </p>
      )}
      <div class="repository-create-plan">
        <label class="field-label">
          New plan name
          <input
            value={planName.value}
            placeholder="Payments reliability"
            onInput$={(_, element) => (planName.value = element.value)}
          />
        </label>
        <button
          class="button primary"
          disabled={busy.value || planName.value.trim() === ""}
          onClick$={() =>
            run(async () => {
              const port = props.port;
              if (port === undefined) return false;
              const result = await commandRepositoryWorkspace(port, {
                operation: "plan.workspace.prepare.v1",
                clientRequestId: workspaceRequestId("plan-workspace-prepare"),
                projectId: props.projectId,
                contextId: props.contextId,
                displayName: planName.value.trim(),
                expectedBaseHead: current.observedContextHead,
              });
              if (!result.ok) return failed(result.error.message, error);
              selectedPlanId.value = result.value.plan.planId;
              planName.value = "";
              message.value = "Plan workspace prepared. Start its coordinator only when ready.";
              await props.onOpenContext$(props.projectId, result.value.context.contextId);
              return true;
            })
          }
        >
          Prepare plan
        </button>
      </div>
      {current.plans.length === 0 ? (
        <p class="workspace-muted">No independent plan workspace has been prepared.</p>
      ) : (
        <label class="field-label">
          Plan context
          <select
            aria-label="Plan context"
            value={selectedPlanId.value}
            onChange$={(_, element) => {
              selectedPlanId.value = element.value;
              selectedIntegrationId.value = "";
            }}
          >
            {current.plans.map((plan) => (
              <option
                key={plan.planId}
                value={plan.planId}
                selected={plan.planId === selectedPlanId.value}
              >
                {`${plan.displayName} · ${plan.state}`}
              </option>
            ))}
          </select>
        </label>
      )}
      {selectedPlan === null ? null : (
        <>
          <PlanSummary plan={selectedPlan} />
          <WorktreeList worktrees={worktrees} planDisplayName={selectedPlan.displayName} />
          <section class="repository-actions">
            <h3>Prepare isolated worker workspace</h3>
            <p>
              Inherited work stays in the selected plan workspace. Isolation creates a child at the
              committed parent basis.
            </p>
            <button
              class="button secondary"
              disabled={busy.value || worktrees.length === 0}
              onClick$={() =>
                run(async () => {
                  const port = props.port;
                  const parent = selectedOrFirst(sourceWorktreeId.value, worktrees);
                  if (port === undefined || parent === undefined) return false;
                  const result = await commandRepositoryWorkspace(port, {
                    operation: "worktree.prepare.v1",
                    clientRequestId: workspaceRequestId("worker-worktree-prepare"),
                    projectId: props.projectId,
                    contextId: props.contextId,
                    planId: selectedPlan.planId,
                    parentWorktreeId: parent.worktreeId,
                    expectedParentHead: parent.headCommit,
                  });
                  if (!result.ok) return failed(result.error.message, error);
                  message.value = `Isolated worker workspace preparing from ${branchName(parent.branchRef)}.`;
                  return true;
                })
              }
            >
              Prepare isolated child
            </button>
          </section>
          <IntegrationControls
            port={props.port}
            projectId={props.projectId}
            contextId={props.contextId}
            planId={selectedPlan.planId}
            planDisplayName={selectedPlan.displayName}
            worktrees={worktrees}
            integrations={planIntegrations}
            selectedIntegration={selectedIntegration}
            sourceWorktreeId={sourceWorktreeId}
            targetWorktreeId={targetWorktreeId}
            selectedIntegrationId={selectedIntegrationId}
            testProfiles={current.testProfiles}
            testProfileId={testProfileId}
            reviewRationale={reviewRationale}
            busy={busy.value}
            run$={run}
            error={error}
            message={message}
          />
        </>
      )}
      {message.value === null ? null : <p class="workspace-notice">{message.value}</p>}
      {error.value === null ? null : <p class="workspace-notice">{error.value}</p>}
    </section>
  );
});

const WorktreeList = component$<{
  worktrees: readonly RepositoryWorktreeRecord[];
  planDisplayName: string;
}>((props) => (
  <div class="repository-worktree-list">
    {props.worktrees.map((worktree) => (
      <article key={worktree.worktreeId} class={`repository-worktree lane-${worktree.kind}`}>
        <strong>{worktreeDisplayName(worktree, props.planDisplayName)}</strong>
        <span>
          Branch {branchName(worktree.branchRef)} · {worktree.kind.replaceAll("_", " ")} ·{" "}
          {worktree.state}
        </span>
        <small>
          Basis {shortCommit(worktree.basisCommit)} · head {shortCommit(worktree.headCommit)}
        </small>
        <small>{responsibility(worktree)}</small>
      </article>
    ))}
  </div>
));

const IntegrationControls = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly planId: string;
  readonly planDisplayName: string;
  readonly worktrees: readonly RepositoryWorktreeRecord[];
  readonly integrations: readonly IntegrationAttempt[];
  readonly selectedIntegration: IntegrationAttempt | null;
  readonly sourceWorktreeId: { value: string };
  readonly targetWorktreeId: { value: string };
  readonly selectedIntegrationId: { value: string };
  readonly testProfiles: RepositoryWorkspaceView["testProfiles"];
  readonly testProfileId: { value: string };
  readonly reviewRationale: { value: string };
  readonly busy: boolean;
  readonly run$: QRL<(action: () => Promise<boolean>) => Promise<void>>;
  readonly error: { value: string | null };
  readonly message: { value: string | null };
}>((props) => {
  const integration = props.selectedIntegration;
  const selectedTestProfile = props.testProfiles.find(
    (profile) => profile.profileId === props.testProfileId.value,
  );
  const diff = useSignal<IntegrationDiffView | null>(null);
  const diffMessage = useSignal<string | null>(null);
  useVisibleTask$(({ track }) => {
    const integrationId = track(() => props.selectedIntegration?.integrationId ?? null);
    const port = props.port;
    diff.value = null;
    if (port === undefined || integrationId === null) {
      diffMessage.value = integrationId === null ? null : "Integration diff is unavailable.";
      return;
    }
    void readIntegrationDiff(port, props.projectId, props.contextId, integrationId).then((read) => {
      if (props.selectedIntegration?.integrationId !== integrationId) return;
      if (!read.ok) {
        diffMessage.value = read.error.message;
        return;
      }
      diff.value = read.value;
      diffMessage.value = read.value.truncated
        ? "Diff is truncated at the configured safe display bound. Review the full candidate before acceptance."
        : null;
    });
  });
  const operation = $(async (kind: "test" | "accept" | "reject" | "resolve" | "promote") => {
    const port = props.port;
    const selected = props.selectedIntegration;
    if (port === undefined || selected === null) return false;
    const common = {
      clientRequestId: workspaceRequestId(`integration-${kind}`),
      projectId: props.projectId,
      contextId: props.contextId,
      integrationId: selected.integrationId,
      expectedRevision: selected.revision,
    };
    const result =
      kind === "test"
        ? await commandRepositoryWorkspace(port, {
            operation: "integration.test.v1",
            ...common,
            profileId: props.testProfileId.value,
          })
        : kind === "accept" || kind === "reject"
          ? await commandRepositoryWorkspace(port, {
              operation: "integration.review.v1",
              ...common,
              accepted: kind === "accept",
              rationale: props.reviewRationale.value.trim(),
            })
          : kind === "resolve"
            ? await commandRepositoryWorkspace(port, {
                operation: "integration.resolution.prepare.v1",
                ...common,
              })
            : await commandRepositoryWorkspace(port, {
                operation: "integration.promote.v1",
                ...common,
              });
    if (!result.ok) return failed(result.error.message, props.error);
    props.message.value =
      kind === "test"
        ? "Configured check completed; semantic review remains separate."
        : kind === "resolve"
          ? "Resolution task workspace prepared."
          : kind === "promote"
            ? "Reviewed candidate promoted to the checked target."
            : `Semantic review ${kind === "accept" ? "accepted" : "rejected"}.`;
    return true;
  });
  return (
    <section class="repository-actions">
      <h3>Integration</h3>
      <div class="repository-integration-inputs">
        <WorktreeSelect
          label="Source branch"
          value={props.sourceWorktreeId}
          worktrees={props.worktrees}
          planDisplayName={props.planDisplayName}
        />
        <WorktreeSelect
          label="Target branch"
          value={props.targetWorktreeId}
          worktrees={props.worktrees}
          planDisplayName={props.planDisplayName}
        />
        <button
          class="button primary"
          disabled={props.busy || props.sourceWorktreeId.value === props.targetWorktreeId.value}
          onClick$={() =>
            props.run$(async () => {
              const port = props.port;
              const source = props.worktrees.find(
                (item) => item.worktreeId === props.sourceWorktreeId.value,
              );
              const target = props.worktrees.find(
                (item) => item.worktreeId === props.targetWorktreeId.value,
              );
              if (port === undefined || source === undefined || target === undefined) return false;
              const result = await commandRepositoryWorkspace(port, {
                operation: "integration.prepare.v1",
                clientRequestId: workspaceRequestId("integration-prepare"),
                projectId: props.projectId,
                contextId: props.contextId,
                planId: props.planId,
                sourceWorktreeId: source.worktreeId,
                targetWorktreeId: target.worktreeId,
                expectedSourceHead: source.headCommit,
                expectedTargetHead: target.headCommit,
              });
              if (!result.ok) return failed(result.error.message, props.error);
              props.selectedIntegrationId.value = result.value.integration.integrationId;
              props.message.value = "Integration candidate prepared in its own workspace.";
              return true;
            })
          }
        >
          Prepare candidate
        </button>
      </div>
      {props.integrations.length === 0 ? null : (
        <label class="field-label">
          Integration history
          <select
            aria-label="Integration history"
            value={props.selectedIntegrationId.value}
            onChange$={(_, element) => (props.selectedIntegrationId.value = element.value)}
          >
            {props.integrations.map((item) => (
              <option
                key={item.integrationId}
                value={item.integrationId}
                selected={item.integrationId === props.selectedIntegrationId.value}
              >
                {integrationName(item, props.worktrees, props.planDisplayName)}
              </option>
            ))}
          </select>
        </label>
      )}
      {integration === null ? null : (
        <div class="repository-integration-card">
          <strong>{integrationName(integration, props.worktrees, props.planDisplayName)}</strong>
          <span class={`coordinator-state state-${integration.state}`}>{integration.state}</span>
          {integration.conflictPaths.length === 0 ? null : (
            <p>
              {integration.conflictPaths.length} conflicting path(s) require an explicit resolution
              task.
            </p>
          )}
          <details open class="repository-diff">
            <summary>Candidate changes</summary>
            {diff.value === null ? (
              <p class="workspace-notice">
                {diffMessage.value ?? "Loading the bounded candidate diff…"}
              </p>
            ) : (
              <>
                <p>{diff.value.changedFiles.length} changed file(s)</p>
                <ul>
                  {diff.value.changedFiles.slice(0, 50).map((path) => (
                    <li key={path}>{path}</li>
                  ))}
                </ul>
                {diff.value.changedFiles.length <= 50 ? null : (
                  <p class="workspace-muted">
                    {String(diff.value.changedFiles.length - 50)} more file(s) are outside the
                    display list.
                  </p>
                )}
                {diffMessage.value === null ? null : (
                  <p class="workspace-notice">{diffMessage.value}</p>
                )}
                {diff.value.unifiedText === "" ? (
                  <p class="workspace-muted">No textual patch is available for this candidate.</p>
                ) : (
                  <pre>
                    <code>{diff.value.unifiedText}</code>
                  </pre>
                )}
              </>
            )}
          </details>
          <label class="field-label">
            Configured check
            <select
              aria-label="Configured check"
              value={props.testProfileId.value}
              onChange$={(_, element) => (props.testProfileId.value = element.value)}
            >
              {props.testProfiles.map((profile) => (
                <option
                  key={profile.profileId}
                  value={profile.profileId}
                  selected={profile.profileId === props.testProfileId.value}
                >
                  {profile.displayName}
                </option>
              ))}
            </select>
          </label>
          <button
            class="button secondary"
            disabled={props.busy || props.testProfileId.value === ""}
            onClick$={() => props.run$(() => operation("test"))}
          >
            Run configured check
          </button>
          <TestEvidenceSummary
            descriptionMarkdown={selectedTestProfile?.descriptionMarkdown}
            evidence={integration.testEvidence}
          />
          <label class="field-label">
            Semantic review rationale
            <textarea
              rows={2}
              value={props.reviewRationale.value}
              onInput$={(_, element) => (props.reviewRationale.value = element.value)}
            />
          </label>
          <div class="execution-actions">
            <button
              class="button secondary"
              disabled={props.busy || integration.state !== "conflicted"}
              onClick$={() => props.run$(() => operation("resolve"))}
            >
              Create resolution task
            </button>
            <button
              class="button secondary"
              disabled={
                props.busy || diff.value === null || props.reviewRationale.value.trim() === ""
              }
              onClick$={() => props.run$(() => operation("accept"))}
            >
              Accept review
            </button>
            <button
              class="button secondary"
              disabled={
                props.busy || diff.value === null || props.reviewRationale.value.trim() === ""
              }
              onClick$={() => props.run$(() => operation("reject"))}
            >
              Reject review
            </button>
            <button
              class="button primary"
              disabled={props.busy || integration.state !== "accepted"}
              onClick$={() => props.run$(() => operation("promote"))}
            >
              Promote reviewed candidate
            </button>
          </div>
        </div>
      )}
    </section>
  );
});
