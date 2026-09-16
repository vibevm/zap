/** Repository workspace UI labels and selectors. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { component$ } from "@qwik.dev/core";
import type { IntegrationAttempt, RepositoryWorktreeRecord } from "../repository-model/index.ts";
import type { RepositoryWorkspaceView } from "../workspace-client/index.ts";

export const WorktreeSelect = component$<{
  readonly label: string;
  readonly value: { value: string };
  readonly worktrees: readonly RepositoryWorktreeRecord[];
  readonly planDisplayName: string;
}>((props) => (
  <label class="field-label">
    {props.label}
    <select
      aria-label={props.label}
      value={props.value.value}
      onChange$={(_, element) => (props.value.value = element.value)}
    >
      {props.worktrees.map((worktree) => (
        <option
          key={worktree.worktreeId}
          value={worktree.worktreeId}
          selected={worktree.worktreeId === props.value.value}
        >{`${worktreeDisplayName(worktree, props.planDisplayName)} · ${worktree.state}`}</option>
      ))}
    </select>
  </label>
));

export const TestEvidenceSummary = component$<{
  readonly descriptionMarkdown?: string | undefined;
  readonly evidence: IntegrationAttempt["testEvidence"];
}>((props) => (
  <>
    {props.descriptionMarkdown === undefined ? null : (
      <p class="workspace-muted">{props.descriptionMarkdown}</p>
    )}
    {props.evidence === null ? (
      <p class="workspace-muted">No configured check result is recorded for this candidate.</p>
    ) : (
      <p class="workspace-notice">
        <strong>{props.evidence.passed ? "Check passed" : "Check failed"}</strong>
        {` · ${props.evidence.summary} Commit ${shortCommit(props.evidence.commit)}.`}
      </p>
    )}
  </>
));

export const PlanSummary = component$<{
  readonly plan: RepositoryWorkspaceView["plans"][number];
}>((props) => (
  <dl class="repository-plan-summary">
    <div>
      <dt>Workspace</dt>
      <dd>{props.plan.state}</dd>
    </div>
    <div>
      <dt>Algorithm plan</dt>
      <dd>{props.plan.algorithmBinding.state}</dd>
    </div>
    <div>
      <dt>Revision</dt>
      <dd>{props.plan.revision}</dd>
    </div>
  </dl>
));

export function planWorktrees(view: RepositoryWorkspaceView, planId: string | null) {
  return view.worktrees.filter(
    (worktree) => worktree.planId === planId || worktree.kind === "registered",
  );
}
export function selectedOrFirst(
  current: string,
  worktrees: readonly RepositoryWorktreeRecord[],
  preferred?: string,
) {
  return (
    worktrees.find((item) => item.worktreeId === current) ??
    worktrees.find((item) => item.worktreeId === preferred) ??
    worktrees[0]
  );
}
export function branchName(value: string): string {
  return value.split("/").at(-1) ?? value;
}
export function shortCommit(value: string): string {
  return value.slice(0, 10);
}
export function responsibility(worktree: RepositoryWorktreeRecord): string {
  const active = worktree.assignments.filter((assignment) => assignment.releasedAt === null);
  if (active.length === 0) return "Unassigned";
  return active
    .map((assignment) =>
      assignment.actorId === null ? "Task assignment" : `Agent ${friendly(assignment.actorId)}`,
    )
    .join(", ");
}
export function worktreeDisplayName(
  worktree: RepositoryWorktreeRecord,
  planDisplayName: string,
): string {
  if (worktree.kind === "registered") return "Original checkout";
  if (worktree.kind === "plan_root") return `${planDisplayName} workspace`;
  if (worktree.kind === "worker") return `${planDisplayName} worker workspace`;
  return `${planDisplayName} integration workspace`;
}
export function integrationName(
  integration: IntegrationAttempt,
  worktrees: readonly RepositoryWorktreeRecord[],
  planDisplayName: string,
): string {
  const source = worktrees.find((item) => item.worktreeId === integration.sourceWorktreeId);
  const target = worktrees.find((item) => item.worktreeId === integration.targetWorktreeId);
  const sourceLabel =
    source === undefined ? "Source workspace" : worktreeDisplayName(source, planDisplayName);
  const targetLabel =
    target === undefined ? "Target workspace" : worktreeDisplayName(target, planDisplayName);
  return `${sourceLabel} → ${targetLabel} · ${integration.state}`;
}
export function failed(message: string, target: { value: string | null }): false {
  target.value = message;
  return false;
}
function friendly(value: string): string {
  return value.split(/[.:/]/).at(-1) ?? value;
}
