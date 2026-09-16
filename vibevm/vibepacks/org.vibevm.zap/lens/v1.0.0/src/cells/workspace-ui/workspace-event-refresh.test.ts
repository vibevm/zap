/** Visible workspace event refresh proof. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import assert from "node:assert/strict";
import test from "node:test";
import {
  HistoryEventSchema,
  ProjectIdSchema,
  QuestionGroupSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import {
  createProjectEventRefresh,
  eventRefreshesProject,
  selectedQuestionChanged,
} from "./workspace-event-refresh.ts";

const projectId = ProjectIdSchema.parse("project.refresh");
const contextId = WorkContextIdSchema.parse("context.refresh");

test("question events coalesce a scoped refresh while terminal chunks do not", async () => {
  const refreshed: string[] = [];
  let canvasRefreshes = 0;
  const controller = createProjectEventRefresh(
    async (project, context, preserve) => {
      refreshed.push(`${project}:${context}:${String(preserve)}`);
    },
    () => (canvasRefreshes += 1),
    5,
  );
  const question = event("question.created");
  controller.accept(question, projectId, contextId);
  controller.accept(question, projectId, contextId);
  controller.accept(event("host.message_delta"), projectId, contextId);
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.deepEqual(refreshed, ["project.refresh:context.refresh:true"]);
  assert.equal(canvasRefreshes, 1);
  assert.equal(eventRefreshesProject(event("terminal.output"), projectId, contextId), false);
  controller.close();
});

test("an unrelated new group does not reload the selected question draft", () => {
  const selectedQuestion = question("question.selected", "1");
  const selected = {
    question: selectedQuestion,
    answerVersions: [],
  };
  assert.equal(
    selectedQuestionChanged([selectedQuestion, question("question.new", "1")], selected),
    false,
  );
  assert.equal(selectedQuestionChanged([question("question.selected", "2")], selected), true);
});

function event(kind: string) {
  return HistoryEventSchema.parse({
    historyEventId: `history.${kind.replaceAll(".", "-")}`,
    projectId,
    contextId,
    globalSequence: "1",
    projectSequence: "1",
    sourceSequence: null,
    kind,
    source: "lens",
    actorId: null,
    occurrenceAt: "2026-09-16T00:00:00.000Z",
    ingestedAt: "2026-09-16T00:00:00.000Z",
    sourceEventId: null,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  });
}

function question(questionGroupId: string, revision: string) {
  return QuestionGroupSchema.parse({
    questionGroupId,
    projectId,
    contextId,
    conversationId: "conversation.refresh",
    originActorId: "actor.refresh",
    messageId: `message.${questionGroupId}`,
    title: "Refresh question",
    introductionMarkdown: "Choose safely.",
    items: [
      {
        questionItemId: `${questionGroupId}.item`,
        header: "Signal",
        promptMarkdown: "Signal",
        contextMarkdown: null,
        artifactRefs: [],
        answerMode: "short_text",
        options: [],
        customAnswer: null,
        recommendation: null,
        required: true,
      },
    ],
    independentWorkAvailable: false,
    deadlineAt: null,
    state: "open",
    revision,
    createdAt: "2026-09-16T00:00:00.000Z",
    updatedAt: "2026-09-16T00:00:00.000Z",
  });
}
