import assert from "node:assert/strict";
import test from "node:test";

import {
  ProjectIdSchema,
  QuestionGroupIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createQuestionCancelRequest } from "./questions.ts";

test("question cancel helper preserves project/context/group/revision and reason", () => {
  const request = createQuestionCancelRequest({
    projectId: ProjectIdSchema.parse("project.cancel"),
    contextId: WorkContextIdSchema.parse("context.cancel"),
    questionGroupId: QuestionGroupIdSchema.parse("question-group.cancel"),
    expectedRevision: "4",
    reasonMarkdown: "No longer needed by this task.",
  });
  assert.equal(request.operation, "question.cancel.v1");
  assert.equal(request.projectId, "project.cancel");
  assert.equal(request.contextId, "context.cancel");
  assert.equal(request.questionGroupId, "question-group.cancel");
  assert.equal(request.expectedRevision, "4");
  assert.equal(request.reasonMarkdown, "No longer needed by this task.");
});
