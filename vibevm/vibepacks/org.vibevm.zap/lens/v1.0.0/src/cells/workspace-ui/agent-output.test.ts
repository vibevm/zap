/** Native output readability proof. @scope spec://org.vibevm.zap/lens/PROP-005#project-views */
import assert from "node:assert/strict";
import test from "node:test";
import {
  AgentOutputItemSchema,
  HistoryEventSchema,
  ProjectIdSchema,
  WorkspaceCommandRequestSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import { readProjectWorkspace } from "../workspace-client/index.ts";
import { groupAgentOutput } from "./agent-output.ts";
import {
  isSemanticEvent,
  preservesInspection,
  refreshInspectedQuestion,
} from "./workspace-helpers.ts";

test("native user content is readable and adjacent deltas form one stream block", () => {
  const groups = groupAgentOutput([
    item(
      "1",
      JSON.stringify({
        item: { type: "userMessage", content: [{ type: "text", text: "Run it" }] },
      }),
    ),
    item("2", JSON.stringify({ event: { delta: "Hel" } })),
    item("3", JSON.stringify({ event: { delta: "lo" } })),
    item("4", "Separate status", "status"),
  ]);
  assert.equal(groups.length, 3);
  assert.equal(groups[0]?.bodyMarkdown, "Run it");
  assert.equal(groups[1]?.bodyMarkdown, "Hello");
  assert.equal(groups[1]?.deltaCount, 2);
  assert.equal(groups[1]?.rawMetadata.length, 2);
  assert.equal(groups[2]?.bodyMarkdown, "Separate status");
});

test("activity defaults to semantic events while retaining raw diagnostics", () => {
  assert.equal(isSemanticEvent(history("chat.message.answered")), true);
  assert.equal(isSemanticEvent(history("message_delta")), false);
  assert.equal(isSemanticEvent(history("host.unmapped.diagnostic")), false);
});

test("same-context refresh preserves focus and reloads the selected question detail", async () => {
  const port = createWorkspaceDemoPort();
  const projectId = ProjectIdSchema.parse("project.lens");
  const contextId = WorkContextIdSchema.parse("context.lens.main");
  const view = await readProjectWorkspace(port, projectId, contextId);
  assert.equal(view.ok, true);
  if (!view.ok) return;
  const selected = view.value.questions[0];
  assert.notEqual(selected, undefined);
  if (selected === undefined) return;
  const item = selected.items[0];
  const option = item?.options[0];
  assert.notEqual(item, undefined);
  assert.notEqual(option, undefined);
  if (item === undefined || option === undefined) return;
  const answered = await port.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "question.answer.v1",
      clientRequestId: "request.refresh.question",
      projectId,
      contextId,
      questionGroupId: selected.questionGroupId,
      expectedRevision: selected.revision,
      submission: {
        answers: [
          {
            questionItemId: item.questionItemId,
            answer: { kind: "single_choice", optionId: option.optionId },
          },
        ],
        noteMarkdown: null,
      },
    }),
  );
  assert.equal(answered.ok, true);
  assert.equal(preservesInspection(true, true, true), true);
  assert.equal(preservesInspection(true, true, false), false);
  const detail = await refreshInspectedQuestion(
    port,
    view.value,
    selected.questionGroupId,
    view.value.project.projectId,
    view.value.project.defaultContextId,
  );
  assert.equal(detail?.question.questionGroupId, selected.questionGroupId);
  assert.ok((detail?.answerVersions.length ?? 0) > 0);
});

function item(sequence: string, bodyMarkdown: string, kind: "text" | "status" = "text") {
  return AgentOutputItemSchema.parse({
    projectId: "project.output",
    contextId: "context.output",
    actorId: "actor.output",
    sessionId: "session.output",
    runId: "run.output",
    sequence,
    kind,
    bodyMarkdown,
    artifactRefs: [],
    nativeRef: null,
    occurredAt: "2026-09-15T00:00:00.000Z",
  });
}

function history(kind: string) {
  return HistoryEventSchema.parse({
    historyEventId: `history.${kind}`,
    projectId: "project.output",
    contextId: "context.output",
    globalSequence: "1",
    projectSequence: "1",
    sourceSequence: "1",
    kind,
    source: "host",
    actorId: "actor.output",
    occurrenceAt: "2026-09-15T00:00:00.000Z",
    ingestedAt: "2026-09-15T00:00:00.000Z",
    sourceEventId: `source.${kind}`,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  });
}
