/** Coalesced visible-project refresh after semantic workspace events.
 * @scope spec://org.vibevm.zap/lens/PROP-010#human-questions
 */
import type {
  HistoryEvent,
  ProjectId,
  QuestionGroup,
  WorkContextId,
} from "../workspace-model/index.ts";
import type { QuestionWorkspaceView } from "../workspace-client/index.ts";

type RefreshProject = (
  projectId: ProjectId,
  contextId: WorkContextId,
  preserveInspection: boolean,
) => Promise<void>;

export function createProjectEventRefresh(
  refresh: RefreshProject,
  refreshVisibleProjects?: () => void,
  delay = 80,
) {
  let pending: ReturnType<typeof setTimeout> | undefined;
  let visiblePending: ReturnType<typeof setTimeout> | undefined;
  return {
    accept(
      event: HistoryEvent,
      projectId: ProjectId | null,
      contextId: WorkContextId | null,
    ): boolean {
      const relevant = isProjectRefreshEvent(event);
      if (relevant && refreshVisibleProjects !== undefined) {
        if (visiblePending !== undefined) clearTimeout(visiblePending);
        visiblePending = setTimeout(() => {
          visiblePending = undefined;
          refreshVisibleProjects();
        }, delay);
      }
      if (
        !eventRefreshesProject(event, projectId, contextId) ||
        projectId === null ||
        contextId === null
      )
        return relevant;
      if (pending !== undefined) clearTimeout(pending);
      pending = setTimeout(() => {
        pending = undefined;
        void refresh(projectId, contextId, true);
      }, delay);
      return relevant;
    },
    close(): void {
      if (pending !== undefined) clearTimeout(pending);
      if (visiblePending !== undefined) clearTimeout(visiblePending);
      pending = undefined;
      visiblePending = undefined;
    },
  };
}

export function eventRefreshesProject(
  event: HistoryEvent,
  projectId: ProjectId | null,
  contextId: WorkContextId | null,
): boolean {
  if (
    projectId === null ||
    contextId === null ||
    event.projectId !== projectId ||
    (event.contextId !== null && event.contextId !== contextId)
  )
    return false;
  return isProjectRefreshEvent(event);
}

export function isProjectRefreshEvent(event: HistoryEvent): boolean {
  return (
    /^(question\.|chat\.message\.|managed-work\.|project\.|agent\.|session\.)/.test(event.kind) ||
    /^host\.(native_child_observed|turn_started|turn_completed|item_started|item_completed|session_started|session_resumed|session_exited)$/.test(
      event.kind,
    ) ||
    /^terminal\.(started|control_changed|exited)$/.test(event.kind)
  );
}

export function selectedQuestionChanged(
  groups: readonly QuestionGroup[],
  selected: QuestionWorkspaceView | null,
): boolean {
  if (selected === null) return false;
  const current = groups.find(
    (question) => question.questionGroupId === selected.question.questionGroupId,
  );
  return current !== undefined && current.revision !== selected.question.revision;
}
