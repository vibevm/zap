/** Project-scoped human question controller and friendly context. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
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
  createQuestionCancelRequest,
  readQuestionWorkspace,
  type ProjectWorkspaceView,
  type QuestionWorkspaceView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type {
  QuestionGroupId,
  QuestionSubmission,
  WorkContextId,
} from "../workspace-model/index.ts";
import { RichQuestionsPanel } from "./rich-questions.tsx";
import { submitQuestion } from "./workspace-helpers.ts";
import { ScopedRequestFence } from "./request-fence.ts";
import { selectedQuestionChanged } from "./workspace-event-refresh.ts";

export const WorkspaceQuestions = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly view: ProjectWorkspaceView;
  readonly contextId: WorkContextId;
  readonly onRefresh$: QRL<() => Promise<void>>;
}>((props) => {
  const selected = useSignal<QuestionWorkspaceView | null>(null);
  const loading = useSignal(false);
  const error = useSignal<string | null>(null);
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));

  const load = $(async (questionGroupId: QuestionGroupId): Promise<void> => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return;
    const projectId = props.view.project.projectId;
    const contextId = props.contextId;
    const token = activeFence.begin(`${projectId}\u0000${contextId}\u0000${questionGroupId}`);
    loading.value = true;
    const result = await readQuestionWorkspace(port, {
      projectId,
      contextId,
      questionGroupId,
    });
    if (
      !activeFence.isCurrent(token) ||
      props.view.project.projectId !== projectId ||
      props.contextId !== contextId
    )
      return;
    loading.value = false;
    if (result.ok) {
      selected.value = result.value;
      error.value = null;
    } else error.value = result.error.message;
  });

  useVisibleTask$(({ track, cleanup }) => {
    const projectId = track(() => props.view.project.projectId);
    const contextId = track(() => props.contextId);
    fence.value?.begin(`${projectId}\u0000${contextId}`);
    selected.value = null;
    loading.value = false;
    cleanup(() => fence.value?.cancel());
  });

  useVisibleTask$(({ track }) => {
    track(() =>
      props.view.questions
        .map((question) => `${question.questionGroupId}:${question.revision}`)
        .join("|"),
    );
    const questionGroupId = selected.value?.question.questionGroupId ?? null;
    if (questionGroupId === null) return;
    if (!props.view.questions.some((question) => question.questionGroupId === questionGroupId)) {
      selected.value = null;
      return;
    }
    if (selectedQuestionChanged(props.view.questions, selected.value)) void load(questionGroupId);
  });

  const selectedQuestion = selected.value?.question ?? null;
  const requestingAgent =
    selectedQuestion === null
      ? undefined
      : props.view.network.agents.find((agent) => agent.actorId === selectedQuestion.originActorId);
  const context = props.view.contexts.find((item) => item.contextId === props.contextId);
  return (
    <RichQuestionsPanel
      groups={props.view.questions}
      selected={selected.value}
      loading={loading.value}
      error={error.value}
      projectLabel={props.view.project.displayName}
      {...(context === undefined ? {} : { contextLabel: context.displayName })}
      {...(requestingAgent === undefined
        ? {}
        : { requestingAgentLabel: requestingAgent.displayName })}
      onSelect$={load}
      onSubmit$={$((question, submission: QuestionSubmission, amendmentReason) =>
        submitQuestion(props.port, question, submission, amendmentReason, async () => {
          await props.onRefresh$();
          await load(question.questionGroupId);
        }),
      )}
      onCancel$={$(async (question, reason) => {
        const port = props.port;
        if (port === undefined) return false;
        const result = await port.command(
          createQuestionCancelRequest({
            projectId: question.projectId,
            contextId: question.contextId,
            questionGroupId: question.questionGroupId,
            expectedRevision: question.revision,
            reasonMarkdown: reason,
          }),
        );
        if (!result.ok) {
          error.value = result.error.message;
          return false;
        }
        await props.onRefresh$();
        selected.value = null;
        return true;
      })}
    />
  );
});
