import { expect, test } from "vitest";

import {
  initialHistoryCursor,
  listWorkspaceProjects,
  readAgentOutput,
  readProjectWorkspace,
  readQuestionWorkspace,
  readWorkspaceHistory,
} from "./index.ts";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import {
  ProjectIdSchema,
  QuestionGroupIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";

const lensProject = ProjectIdSchema.parse("project.lens");
const zapProject = ProjectIdSchema.parse("project.zap");
const lensContext = WorkContextIdSchema.parse("context.lens.main");
const runtimeActor = ActorIdSchema.parse("act.lens.runtime");
const questionGroup = QuestionGroupIdSchema.parse("question-group.workspace");

test("workspace client keeps project and agent scopes distinct", async () => {
  const port = createWorkspaceDemoPort();
  const projects = await listWorkspaceProjects(port);
  expect(projects.ok).toBe(true);
  if (!projects.ok) return;
  expect(projects.value.map((project) => project.projectId)).toEqual([
    "project.lens",
    "project.zap",
  ]);

  const lens = await readProjectWorkspace(port, lensProject);
  const zap = await readProjectWorkspace(port, zapProject);
  expect(lens.ok).toBe(true);
  expect(zap.ok).toBe(true);
  if (!lens.ok || !zap.ok) return;
  expect(lens.value.network.agents).toHaveLength(4);
  expect(zap.value.network.agents).toHaveLength(0);

  const output = await readAgentOutput(port, {
    projectId: lensProject,
    contextId: lensContext,
    actorId: runtimeActor,
    afterSequence: DecimalSchema.parse("0"),
  });
  expect(output.ok).toBe(true);
  if (output.ok) expect(output.value.items[0]?.bodyMarkdown).toContain("user decision");
});

test("workspace client exposes question history and scoped activity", async () => {
  const port = createWorkspaceDemoPort();
  const question = await readQuestionWorkspace(port, {
    projectId: lensProject,
    contextId: lensContext,
    questionGroupId: questionGroup,
  });
  expect(question.ok).toBe(true);
  if (question.ok) expect(question.value.question.items).toHaveLength(3);

  const history = await readWorkspaceHistory(
    port,
    initialHistoryCursor({ kind: "project", projectId: lensProject }),
  );
  expect(history.ok).toBe(true);
  if (history.ok) {
    expect(history.value.events.every((event) => event.projectId === lensProject)).toBe(true);
    expect(history.value.coverage.state).toBe("complete");
  }
});
