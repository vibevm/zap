import assert from "node:assert/strict";
import { test } from "node:test";

import { DecimalSchema } from "../protocol/index.ts";
import { openBroker } from "./index.ts";
import {
  AGENT_CAPABILITIES,
  CONVERSATION,
  WORKSPACE,
  auth,
  connect,
  enroll,
  expectError,
  request,
  take,
} from "./test-support.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("answer checks the deadline inside its transaction without waiting for maintenance", () => {
  let instant = new Date("2026-09-15T00:00:00.000Z");
  const broker = take(openBroker({ databasePath: ":memory:", clock: () => instant }));
  const agent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const responder = enroll(broker, "human_responder", ["question:answer", "events:read"]);
  const connection = connect(broker, agent, "deadline_actor");
  const question = take(
    broker.ask(auth(agent, connection), {
      clientRequestId: request("deadline_question"),
      prompt: "Before deadline?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
      deadlineAt: "2026-09-15T00:00:01.000Z",
    }),
  );
  instant = new Date("2026-09-15T00:00:02.000Z");
  const late = broker.answer(
    { principalToken: responder.principalToken },
    {
      clientRequestId: request("late_deadline_answer"),
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      questionId: question.questionId,
      expectedRevision: DecimalSchema.parse("1"),
      answer: "too late",
    },
  );
  expectError(late, "conflict");
  assert.equal(take(broker.expireQuestions()).length, 0);
  const events = take(
    broker.events(
      { principalToken: responder.principalToken },
      { workspaceId: WORKSPACE, conversationId: CONVERSATION },
    ),
  );
  const lastEvent = events.events.at(-1);
  if (lastEvent === undefined) assert.fail("expiry event missing");
  assert.equal(lastEvent.kind, "question.expired");
  take(broker.close());
});
