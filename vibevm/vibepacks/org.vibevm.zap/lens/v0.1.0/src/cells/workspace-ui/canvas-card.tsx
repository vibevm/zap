/** Scoped inspector for unified-canvas selections. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import { component$, type QRL } from "@qwik.dev/core";
import type { PortfolioCard } from "../quicklens-graph/index.ts";
import type { ProjectId, ProjectObjectReference, WorkContextId } from "../workspace-model/index.ts";

export interface CanvasAnnotationSummary {
  readonly noteCount: number;
  readonly deferredCount: number;
  readonly trashCount: number;
}

export const CanvasCard = component$<{
  readonly card: PortfolioCard | null;
  readonly collapsed: boolean;
  readonly annotations?: CanvasAnnotationSummary | undefined;
  readonly onOpenProject$: QRL<(projectId: ProjectId, contextId: WorkContextId) => void>;
  readonly onOpenQuestions$: QRL<(projectId: ProjectId, contextId: WorkContextId) => void>;
  readonly onToggleCollapse$: QRL<() => void>;
  readonly onOpenNotes$?: QRL<(reference: ProjectObjectReference) => void> | undefined;
  readonly onOpenTrash$?: QRL<(projectId: ProjectId) => void> | undefined;
}>((props) => {
  const card = props.card;
  if (card === null) {
    return (
      <aside class="workspace-panel canvas-inspector workspace-empty">
        <strong>Select an object</strong>
        <span>Choose a project, goal, milestone, task, resource or agent on the map.</span>
      </aside>
    );
  }
  const projectId = card.reference.projectId;
  return (
    <aside class="workspace-panel canvas-inspector">
      <p class="eyebrow">{card.reference.domain.replaceAll("_", " ")}</p>
      {card.kind === "project" ? (
        <ProjectCard card={card} />
      ) : card.kind === "semantic" ? (
        <SemanticCard card={card} />
      ) : card.kind === "managed_work" ? (
        <ManagedWorkCard card={card} />
      ) : card.kind === "plan_workspace" ? (
        <PlanWorkspaceCard card={card} />
      ) : card.kind === "worktree" ? (
        <WorktreeCard card={card} />
      ) : card.kind === "integration" ? (
        <IntegrationCard card={card} />
      ) : (
        <AgentCard card={card} />
      )}
      <p class="canvas-scope">
        <strong>Scope</strong>
        <span>{card.reference.projectId}</span>
        <span>{card.reference.contextId}</span>
      </p>
      <div class="execution-actions">
        <button
          class="button secondary"
          onClick$={() => props.onOpenProject$(projectId, card.reference.contextId)}
        >
          Open project
        </button>
        {card.kind === "project" ? (
          <button
            class="button secondary"
            onClick$={() => props.onOpenQuestions$(projectId, card.reference.contextId)}
          >
            Open questions
          </button>
        ) : null}
        {card.kind === "project" ||
        (card.kind === "semantic" && card.object.category === "milestone") ? (
          <button class="button secondary" onClick$={props.onToggleCollapse$}>
            {props.collapsed ? "Expand" : "Collapse"}
          </button>
        ) : null}
      </div>
      <section class="canvas-extension">
        <div>
          <strong>Notes</strong>
          <span>
            {props.annotations === undefined
              ? props.onOpenNotes$ === undefined
                ? "Note service not configured"
                : "Open notes and deferred instructions"
              : `${String(props.annotations.noteCount)} notes · ${String(props.annotations.deferredCount)} deferred`}
          </span>
        </div>
        {props.onOpenNotes$ === undefined ? null : (
          <button class="button secondary" onClick$={() => props.onOpenNotes$?.(card.reference)}>
            Open notes
          </button>
        )}
      </section>
      {card.kind !== "project" ? null : (
        <section class="canvas-extension">
          <div>
            <strong>Trash</strong>
            <span>
              {props.annotations === undefined
                ? props.onOpenTrash$ === undefined
                  ? "Trash service not configured"
                  : "Open recoverable project Trash"
                : `${String(props.annotations.trashCount)} recoverable items`}
            </span>
          </div>
          {props.onOpenTrash$ === undefined ? null : (
            <button class="button secondary" onClick$={() => props.onOpenTrash$?.(projectId)}>
              Open Trash
            </button>
          )}
        </section>
      )}
    </aside>
  );
});

const ProjectCard = component$<{ card: Extract<PortfolioCard, { kind: "project" }> }>((props) => {
  const project = props.card.project;
  return project.state === "unavailable" ? (
    <>
      <h2>{project.selection.fallbackLabel}</h2>
      <p class="workspace-notice">{project.reason}</p>
    </>
  ) : (
    <>
      <h2>{project.view.project.displayName}</h2>
      <p>
        {project.view.contexts.find((item) => item.contextId === project.contextId)?.displayName}
      </p>
      <dl class="canvas-facts">
        <div>
          <dt>Execution</dt>
          <dd>{project.view.execution.state.replaceAll("_", " ")}</dd>
        </div>
        <div>
          <dt>Agents</dt>
          <dd>{project.view.network.agents.length}</dd>
        </div>
        <div>
          <dt>Open questions</dt>
          <dd>{project.view.questions.filter((question) => question.state === "open").length}</dd>
        </div>
      </dl>
    </>
  );
});

const SemanticCard = component$<{ card: Extract<PortfolioCard, { kind: "semantic" }> }>((props) => {
  const object = props.card.object;
  return (
    <>
      <h2>{object.title}</h2>
      <span class={`coordinator-state state-${object.status.tone}`}>{object.status.label}</span>
      {object.purpose === null ? null : <p>{object.purpose}</p>}
      {object.expectedResult === null ? null : (
        <p>
          <strong>Expected result</strong>
          <br />
          {object.expectedResult}
        </p>
      )}
      {object.acceptance === null ? null : (
        <p>
          <strong>Acceptance</strong>
          <br />
          {object.acceptance}
        </p>
      )}
      {object.blockerSummary === null ? null : (
        <p class="workspace-notice">{object.blockerSummary}</p>
      )}
    </>
  );
});

const AgentCard = component$<{ card: Extract<PortfolioCard, { kind: "agent" }> }>((props) => {
  const agent = props.card.agent;
  return (
    <>
      <h2>{agent.displayName}</h2>
      <span class={`coordinator-state state-${agent.state}`}>
        {agent.state.replaceAll("_", " ")}
      </span>
      <dl class="canvas-facts">
        <div>
          <dt>Role</dt>
          <dd>{agent.role}</dd>
        </div>
        <div>
          <dt>Execution</dt>
          <dd>{agent.executionMode}</dd>
        </div>
        <div>
          <dt>Terminal</dt>
          <dd>{agent.terminalId ?? "none"}</dd>
        </div>
      </dl>
    </>
  );
});

const ManagedWorkCard = component$<{
  card: Extract<PortfolioCard, { kind: "managed_work" }>;
}>((props) => {
  const work = props.card.work;
  return (
    <>
      <h2>{work.goal}</h2>
      <span class={`coordinator-state state-${work.state}`}>{work.state.replaceAll("_", " ")}</span>
      <p>{work.expectedResult}</p>
      <dl class="canvas-facts">
        <div>
          <dt>Object</dt>
          <dd>{props.card.entity}</dd>
        </div>
        <div>
          <dt>Provider</dt>
          <dd>{work.provider.replaceAll("_", " ")}</dd>
        </div>
        <div>
          <dt>Profile</dt>
          <dd>{work.profileId}</dd>
        </div>
      </dl>
      {work.report === null ? null : <p>{work.report.summaryMarkdown}</p>}
    </>
  );
});

const PlanWorkspaceCard = component$<{
  card: Extract<PortfolioCard, { kind: "plan_workspace" }>;
}>((props) => {
  const plan = props.card.plan;
  return (
    <>
      <h2>{plan.displayName}</h2>
      <span class={`coordinator-state state-${plan.state}`}>{plan.state}</span>
      <p>
        {plan.algorithmBinding.state === "bound"
          ? "Algorithm plan connected"
          : "Ready to create or connect an algorithm plan"}
      </p>
      <dl class="canvas-facts">
        <div>
          <dt>Context</dt>
          <dd>{contextName(props.card)}</dd>
        </div>
        <div>
          <dt>Workspace</dt>
          <dd>{plan.state}</dd>
        </div>
        <div>
          <dt>Execution host</dt>
          <dd>{friendlyIdentity(plan.executionHostId)}</dd>
        </div>
        <div>
          <dt>Updated by</dt>
          <dd>{friendlyIdentity(plan.lastUpdatedByPrincipalId)}</dd>
        </div>
      </dl>
    </>
  );
});

const WorktreeCard = component$<{ card: Extract<PortfolioCard, { kind: "worktree" }> }>((props) => {
  const worktree = props.card.worktree;
  const plan =
    props.card.project.repository.state === "ready"
      ? props.card.project.repository.value.plans.find((item) => item.planId === worktree.planId)
      : undefined;
  const activeAssignments = worktree.assignments.filter(
    (assignment) => assignment.releasedAt === null,
  );
  return (
    <>
      <h2>{branchName(worktree.branchRef)}</h2>
      <span class={`coordinator-state state-${worktree.state}`}>{worktree.state}</span>
      <p>
        {plan === undefined ? "" : `${plan.displayName} · `}
        {worktree.kind.replaceAll("_", " ")} workspace
      </p>
      <dl class="canvas-facts">
        <div>
          <dt>Basis</dt>
          <dd>{shortCommit(worktree.basisCommit)}</dd>
        </div>
        <div>
          <dt>Current head</dt>
          <dd>{shortCommit(worktree.headCommit)}</dd>
        </div>
        <div>
          <dt>Responsibility</dt>
          <dd>{assignmentSummary(activeAssignments)}</dd>
        </div>
        <div>
          <dt>Execution host</dt>
          <dd>{friendlyIdentity(worktree.executionHostId)}</dd>
        </div>
      </dl>
    </>
  );
});

const IntegrationCard = component$<{
  card: Extract<PortfolioCard, { kind: "integration" }>;
}>((props) => {
  const integration = props.card.integration;
  return (
    <>
      <h2>
        {integration.conflictPaths.length > 0 ? "Conflict resolution" : "Integration candidate"}
      </h2>
      <span class={`coordinator-state state-${integration.state}`}>{integration.state}</span>
      <p>
        {integration.conflictPaths.length > 0
          ? `${String(integration.conflictPaths.length)} path(s) need an explicit resolution task.`
          : integration.candidateCommit === null
            ? "Candidate is still preparing."
            : `Candidate ${shortCommit(integration.candidateCommit)}`}
      </p>
      <dl class="canvas-facts">
        <div>
          <dt>Configured check</dt>
          <dd>
            {integration.testEvidence === null
              ? "Not run"
              : integration.testEvidence.passed
                ? "Passed"
                : "Failed"}
          </dd>
        </div>
        <div>
          <dt>Semantic review</dt>
          <dd>
            {integration.review === null
              ? "Awaiting review"
              : integration.review.accepted
                ? "Accepted"
                : "Rejected"}
          </dd>
        </div>
        <div>
          <dt>Reviewer</dt>
          <dd>
            {integration.review === null
              ? "Unassigned"
              : friendlyIdentity(integration.review.reviewerPrincipalId)}
          </dd>
        </div>
        <div>
          <dt>Check runner</dt>
          <dd>
            {integration.testEvidence === null
              ? "Unassigned"
              : friendlyIdentity(integration.testEvidence.runnerAuthorityId)}
          </dd>
        </div>
      </dl>
      {integration.conflictPaths.length === 0 ? null : (
        <details class="workspace-notice">
          <summary>Conflicting paths</summary>
          <ul>
            {integration.conflictPaths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
});

function contextName(card: Extract<PortfolioCard, { kind: "plan_workspace" }>): string {
  return (
    card.project.view.contexts.find((context) => context.contextId === card.reference.contextId)
      ?.displayName ?? "Plan context"
  );
}
function branchName(value: string): string {
  return value.split("/").at(-1) ?? value;
}
function shortCommit(value: string): string {
  return value.slice(0, 10);
}
function friendlyIdentity(value: string): string {
  return value.split(/[.:/]/).at(-1) ?? value;
}
function assignmentSummary(
  assignments: readonly Extract<
    PortfolioCard,
    { kind: "worktree" }
  >["worktree"]["assignments"][number][],
): string {
  if (assignments.length === 0) return "Unassigned";
  const labels = assignments.map((assignment) =>
    assignment.actorId === null
      ? assignment.taskId === null
        ? "Unassigned attempt"
        : `Task ${friendlyIdentity(assignment.taskId)}`
      : `Agent ${friendlyIdentity(assignment.actorId)}`,
  );
  return labels.join(", ");
}
