/** @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
/** Pure workspace UI event and command helpers. */
import {
  initialHistoryCursor,
  readProjectWorkspace,
  readQuestionWorkspace,
  workspaceRequestId,
  type ProjectWorkspaceView,
  type QuestionWorkspaceView,
} from "../workspace-client/index.ts";
import type {
  AgentDescriptor,
  CoordinatorLaunchOption,
  HistoryPage,
  HistoryCursor,
  HistoryEvent,
  ProjectDescriptor,
  ProjectId,
  QuestionGroup,
  QuestionSubmission,
  WorkContextId,
  WorkspaceClientPort,
} from "../workspace-model/index.ts";
type ActivityScope = "all" | "project" | "agent";

export function historyScope(
  scope: ActivityScope,
  projectId: ProjectId | null,
  contextId: WorkContextId | null,
  actorId: AgentDescriptor["actorId"] | null,
): HistoryCursor["scope"] | null {
  if (scope === "all") return { kind: "all_authorized" };
  if (projectId === null) return null;
  if (scope === "project") return { kind: "project", projectId };
  return contextId === null || actorId === null
    ? null
    : { kind: "actor", projectId, contextId, actorId };
}

export async function observeEvents(
  port: WorkspaceClientPort,
  signal: AbortSignal,
  accept: (event: HistoryEvent) => void,
): Promise<void> {
  for await (const result of port.subscribe({
    cursor: initialHistoryCursor({ kind: "all_authorized" }),
    signal,
  })) {
    if (result.ok) accept(result.value);
    if (signal.aborted) return;
  }
}

export function eventMatchesScope(
  event: HistoryEvent,
  scope: ActivityScope,
  projectId: ProjectId | null,
  actorId: AgentDescriptor["actorId"] | null,
): boolean {
  if (scope === "all") return true;
  if (event.projectId !== projectId) return false;
  return scope === "project" || event.actorId === actorId;
}

export function uniqueEvents(events: readonly HistoryEvent[]): readonly HistoryEvent[] {
  return [...new Map(events.map((event) => [event.historyEventId, event])).values()].sort(
    (left, right) => (BigInt(left.globalSequence) < BigInt(right.globalSequence) ? -1 : 1),
  );
}

export function isSemanticEvent(event: HistoryEvent): boolean {
  return !/(^|[._-])(delta|token|unmapped|raw)([._-]|$)/i.test(event.kind);
}

export function historyCoverageLabel(coverage: HistoryPage["coverage"]): string | null {
  return coverage.state === "gap"
    ? `History gap · available from sequence ${coverage.firstAvailableSequence}. ${coverage.reason}`
    : null;
}

export function selectedAgent(
  view: ProjectWorkspaceView | null,
  actorId: AgentDescriptor["actorId"] | null,
): AgentDescriptor | undefined {
  return view?.network.agents.find((agent) => agent.actorId === actorId);
}

export function preservesInspection(
  requested: boolean,
  sameProject: boolean,
  sameContext: boolean,
): boolean {
  return requested && sameProject && sameContext;
}

export async function refreshInspectedQuestion(
  port: WorkspaceClientPort,
  view: ProjectWorkspaceView,
  questionGroupId: QuestionGroup["questionGroupId"] | null,
  projectId: ProjectId,
  contextId: WorkContextId,
): Promise<QuestionWorkspaceView | null> {
  if (
    questionGroupId === null ||
    !view.questions.some((question) => question.questionGroupId === questionGroupId)
  ) {
    return null;
  }
  const detail = await readQuestionWorkspace(port, { projectId, contextId, questionGroupId });
  return detail.ok ? detail.value : null;
}

export function ownedTerminalOptions(
  view: ProjectWorkspaceView,
): readonly CoordinatorLaunchOption[] {
  return view.coordinatorLaunchOptions.filter(
    (option) => option.interactionKind === "owned_terminal",
  );
}

export function projectNames(projects: readonly ProjectDescriptor[]): ReadonlyMap<string, string> {
  return new Map(projects.map((project) => [project.projectId, project.displayName]));
}

export function agentNames(view: ProjectWorkspaceView | null): ReadonlyMap<string, string> {
  return new Map((view?.network.agents ?? []).map((agent) => [agent.actorId, agent.displayName]));
}

export async function readProjectBoardViews(
  port: WorkspaceClientPort,
  projects: readonly ProjectDescriptor[],
): Promise<Record<string, ProjectWorkspaceView>> {
  const entries = await Promise.all(
    projects.slice(0, 8).map(async (project) => {
      const view = await readProjectWorkspace(port, project.projectId);
      return view.ok ? ([project.projectId, view.value] as const) : null;
    }),
  );
  return Object.fromEntries(entries.filter((entry) => entry !== null));
}

export async function submitQuestion(
  port: WorkspaceClientPort | undefined,
  question: QuestionGroup,
  submission: QuestionSubmission,
  amendmentReason: string | null,
  refresh: () => Promise<void>,
): Promise<boolean> {
  if (port === undefined) return false;
  const common = {
    clientRequestId: workspaceRequestId(amendmentReason === null ? "answer" : "amend"),
    projectId: question.projectId,
    contextId: question.contextId,
    questionGroupId: question.questionGroupId,
    expectedRevision: question.revision,
    submission,
  };
  const result = await port.command(
    amendmentReason === null
      ? { operation: "question.answer.v1", ...common }
      : { operation: "question.amend.v1", ...common, amendmentReasonMarkdown: amendmentReason },
  );
  if (!result.ok) return false;
  await refresh();
  return true;
}
