import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { ConversationIdSchema, DecimalSchema, WorkspaceIdSchema } from "../protocol/index.ts";
import { openBroker } from "./index.ts";
import {
  AGENT_CAPABILITIES,
  CONVERSATION,
  WORKSPACE,
  auth,
  connect,
  enroll,
  expectError,
  memoryBroker,
  request,
  take,
} from "./test-support.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("interleaved nested actors receive out-of-order answers only in their own inbox", () => {
  const broker = memoryBroker();
  const agent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const responderA = enroll(broker, "human_responder", [
    "question:answer",
    "question:amend",
    "events:read",
  ]);
  const responderB = enroll(broker, "human_responder", [
    "question:answer",
    "question:amend",
    "events:read",
  ]);
  const parent = connect(broker, agent, "parent");
  const parentAuth = auth(agent, parent);
  const childA = take(
    broker.delegate(parentAuth, {
      clientRequestId: request("child_a"),
      capabilities: [...AGENT_CAPABILITIES],
      host: { kind: "test", subagentId: "a", provenance: "attested" },
      replyPolicy: { kind: "retain" },
    }),
  );
  const childB = take(
    broker.delegate(parentAuth, {
      clientRequestId: request("child_b"),
      capabilities: [...AGENT_CAPABILITIES],
      host: { kind: "test", subagentId: "b", provenance: "attested" },
      replyPolicy: { kind: "retain" },
    }),
  );
  const grandchild = take(
    broker.delegate(auth(agent, childA), {
      clientRequestId: request("grandchild"),
      capabilities: ["question:ask", "inbox:read", "inbox:ack"],
      host: { kind: "test", subagentId: "a1", provenance: "attested" },
      replyPolicy: { kind: "retain" },
    }),
  );
  const questionA = take(
    broker.ask(auth(agent, childA), {
      clientRequestId: request("question_a"),
      prompt: "A?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  const questionB = take(
    broker.ask(auth(agent, childB), {
      clientRequestId: request("question_b"),
      prompt: "B?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  const questionNested = take(
    broker.ask(auth(agent, grandchild), {
      clientRequestId: request("question_nested"),
      prompt: "Nested?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );

  take(
    broker.answer(
      { principalToken: responderB.principalToken },
      {
        clientRequestId: request("answer_b"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: questionB.questionId,
        expectedRevision: DecimalSchema.parse("1"),
        answer: "answer B",
      },
    ),
  );
  take(
    broker.answer(
      { principalToken: responderA.principalToken },
      {
        clientRequestId: request("answer_nested"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: questionNested.questionId,
        expectedRevision: DecimalSchema.parse("1"),
        answer: "answer nested",
      },
    ),
  );
  take(
    broker.answer(
      { principalToken: responderA.principalToken },
      {
        clientRequestId: request("answer_a"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: questionA.questionId,
        expectedRevision: DecimalSchema.parse("1"),
        answer: "answer A",
      },
    ),
  );

  const inboxA = take(broker.inbox(auth(agent, childA), {}));
  const inboxB = take(broker.inbox(auth(agent, childB), {}));
  const inboxNested = take(broker.inbox(auth(agent, grandchild), {}));
  assert.deepEqual(
    inboxA.deliveries.map((item) => item.message.correlationId),
    [questionA.questionId],
  );
  assert.deepEqual(
    inboxB.deliveries.map((item) => item.message.correlationId),
    [questionB.questionId],
  );
  assert.deepEqual(
    inboxNested.deliveries.map((item) => item.message.correlationId),
    [questionNested.questionId],
  );
  take(broker.close());
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("restart preserves idempotency and resume fences the old binding generation", () => {
  const directory = mkdtempSync(join(tmpdir(), "codlens-broker-"));
  const databasePath = join(directory, "broker.sqlite");
  const first = take(openBroker({ databasePath }));
  const agent = enroll(first, "agent", AGENT_CAPABILITIES);
  const connection = connect(first, agent, "restart_actor");
  const asked = take(
    first.ask(auth(agent, connection), {
      clientRequestId: request("restart_question"),
      prompt: "Persist?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  take(first.close());

  const second = take(openBroker({ databasePath }));
  const duplicate = take(
    second.ask(auth(agent, connection), {
      clientRequestId: request("restart_question"),
      prompt: "Persist?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  assert.equal(duplicate.questionId, asked.questionId);
  const resumed = take(
    second.resume({
      principalToken: agent.principalToken,
      clientRequestId: request("resume"),
      actorId: connection.actor.actorId,
      resumeCredential: connection.credentials.resumeCredential,
      host: { kind: "test", sessionId: "new", provenance: "attested" },
    }),
  );
  const stale = second.ask(auth(agent, connection), {
    clientRequestId: request("stale_question"),
    prompt: "Stale?",
    answerMode: "free_text",
    choices: [],
    independentWorkAvailable: true,
  });
  expectError(stale, "stale_binding");
  assert.equal(resumed.handle.generation, "2");
  take(second.close());
  rmSync(directory, { recursive: true, force: true });
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("duplicate writes, answer races, cancellation and wrong scope have explicit outcomes", () => {
  const broker = memoryBroker();
  const agent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const responder = enroll(broker, "human_responder", [
    "question:answer",
    "question:amend",
    "events:read",
  ]);
  const outsider = enroll(
    broker,
    "human_responder",
    ["question:answer", "events:read"],
    WorkspaceIdSchema.parse("workspace_other"),
    ConversationIdSchema.parse("conversation_other"),
  );
  const connection = connect(broker, agent, "race_actor");
  const binding = auth(agent, connection);
  const duplicateChoices = broker.ask(binding, {
    clientRequestId: request("duplicate_choices"),
    prompt: "Duplicate?",
    answerMode: "single_choice",
    choices: [
      { id: "same_choice", label: "First" },
      { id: "same_choice", label: "Second" },
    ],
    independentWorkAvailable: true,
  });
  expectError(duplicateChoices, "invalid_input");
  const question = take(
    broker.ask(binding, {
      clientRequestId: request("race_question"),
      prompt: "Route?",
      answerMode: "single_choice",
      choices: [
        { id: "choice_a", label: "A" },
        { id: "choice_b", label: "B" },
      ],
      independentWorkAvailable: true,
    }),
  );
  const changedDuplicate = broker.ask(binding, {
    clientRequestId: request("race_question"),
    prompt: "Changed?",
    answerMode: "free_text",
    choices: [],
    independentWorkAvailable: true,
  });
  expectError(changedDuplicate, "idempotency_conflict");
  const wrongScope = broker.answer(
    { principalToken: outsider.principalToken },
    {
      clientRequestId: request("wrong_scope"),
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      questionId: question.questionId,
      expectedRevision: DecimalSchema.parse("1"),
      answer: "choice_a",
    },
  );
  expectError(wrongScope, "forbidden");
  take(
    broker.answer(
      { principalToken: responder.principalToken },
      {
        clientRequestId: request("race_answer"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: question.questionId,
        expectedRevision: DecimalSchema.parse("1"),
        answer: "choice_a",
      },
    ),
  );
  const losingCancel = broker.cancelQuestion(binding, {
    clientRequestId: request("race_cancel"),
    questionId: question.questionId,
    expectedRevision: DecimalSchema.parse("1"),
  });
  expectError(losingCancel, "conflict");
  const secondAnswer = broker.answer(
    { principalToken: responder.principalToken },
    {
      clientRequestId: request("second_answer"),
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      questionId: question.questionId,
      expectedRevision: DecimalSchema.parse("2"),
      answer: "choice_b",
    },
  );
  expectError(secondAnswer, "already_answered");
  const invalidAmendment = broker.amendAnswer(
    { principalToken: responder.principalToken },
    {
      clientRequestId: request("invalid_amendment"),
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      questionId: question.questionId,
      expectedRevision: DecimalSchema.parse("2"),
      answer: "missing_choice",
    },
  );
  expectError(invalidAmendment, "invalid_input");
  const amended = take(
    broker.amendAnswer(
      { principalToken: responder.principalToken },
      {
        clientRequestId: request("valid_amendment"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: question.questionId,
        expectedRevision: DecimalSchema.parse("2"),
        answer: "choice_b",
      },
    ),
  );
  assert.equal(amended.revision, "3");
  assert.equal(amended.answer, "choice_b");
  take(broker.close());
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("sparse acknowledgement, backpressure and completed-child forwarding remain explicit", () => {
  const broker = memoryBroker();
  const agent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const responder = enroll(broker, "human_responder", ["question:answer", "events:read"]);
  const parent = connect(broker, agent, "delivery_parent");
  const parentAuth = auth(agent, parent);
  const child = take(
    broker.delegate(parentAuth, {
      clientRequestId: request("delivery_child"),
      capabilities: ["question:ask", "inbox:read", "inbox:ack"],
      host: { kind: "test", provenance: "explicit_handle" },
      replyPolicy: { kind: "forward_parent" },
    }),
  );
  const oversized = broker.emit(parentAuth, {
    clientRequestId: request("oversized"),
    toActorId: child.actor.actorId,
    payload: "x".repeat(70_000),
  });
  expectError(oversized, "backpressure");
  const canonicalFirst = take(
    broker.emit(parentAuth, {
      clientRequestId: request("canonical_nested"),
      toActorId: parent.actor.actorId,
      payload: { outer: { alpha: 1, beta: 2 } },
    }),
  );
  const canonicalRetry = take(
    broker.emit(parentAuth, {
      clientRequestId: request("canonical_nested"),
      toActorId: parent.actor.actorId,
      payload: { outer: { beta: 2, alpha: 1 } },
    }),
  );
  assert.equal(canonicalRetry.messageId, canonicalFirst.messageId);
  take(
    broker.emit(parentAuth, {
      clientRequestId: request("message_one"),
      toActorId: child.actor.actorId,
      payload: { order: 1 },
    }),
  );
  take(
    broker.emit(parentAuth, {
      clientRequestId: request("message_two"),
      toActorId: child.actor.actorId,
      payload: { order: 2 },
    }),
  );
  const childInbox = take(broker.inbox(auth(agent, child), {}));
  assert.equal(childInbox.deliveries.length, 2);
  const secondDelivery = childInbox.deliveries.at(1);
  assert.notEqual(secondDelivery, undefined);
  if (secondDelivery === undefined) assert.fail("second delivery missing");
  take(
    broker.ack(auth(agent, child), {
      clientRequestId: request("ack_second"),
      deliveryIds: [secondDelivery.deliveryId],
    }),
  );
  assert.deepEqual(
    take(broker.inbox(auth(agent, child), {})).deliveries.map((item) => item.message.payload),
    [{ order: 1 }],
  );

  const question = take(
    broker.ask(auth(agent, child), {
      clientRequestId: request("forward_question"),
      prompt: "Late?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  take(
    broker.expireActor(parentAuth, {
      clientRequestId: request("expire_child"),
      actorId: child.actor.actorId,
    }),
  );
  take(
    broker.answer(
      { principalToken: responder.principalToken },
      {
        clientRequestId: request("late_answer"),
        workspaceId: WORKSPACE,
        conversationId: CONVERSATION,
        questionId: question.questionId,
        expectedRevision: DecimalSchema.parse("1"),
        answer: "late",
      },
    ),
  );
  const parentInbox = take(broker.inbox(parentAuth, {}));
  const forwarded = parentInbox.deliveries.find(
    (item) => item.message.correlationId === question.questionId,
  );
  if (forwarded === undefined) assert.fail("forwarded delivery missing");
  assert.equal(forwarded.logicalRecipientActorId, child.actor.actorId);
  assert.equal(forwarded.recipientActorId, parent.actor.actorId);
  const unsupported = broker.executePlan(parentAuth, { operation: "apply" });
  expectError(unsupported, "unsupported_operation");
  take(broker.close());
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("the same conversation label remains isolated by workspace", () => {
  const broker = memoryBroker();
  const otherWorkspace = WorkspaceIdSchema.parse("workspace_isolated");
  const firstAgent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const secondAgent = enroll(broker, "agent", AGENT_CAPABILITIES, otherWorkspace);
  const firstViewer = enroll(broker, "viewer", ["events:read"]);
  const secondViewer = enroll(broker, "viewer", ["events:read"], otherWorkspace);
  const first = connect(broker, firstAgent, "workspace_first");
  const second = take(
    broker.connect({
      principalToken: secondAgent.principalToken,
      clientRequestId: request("workspace_second"),
      workspaceId: otherWorkspace,
      conversationId: CONVERSATION,
      capabilities: [...AGENT_CAPABILITIES],
      host: { kind: "test", provenance: "attested" },
      replyPolicy: { kind: "retain" },
    }),
  );
  take(
    broker.ask(auth(firstAgent, first), {
      clientRequestId: request("workspace_question_first"),
      prompt: "First workspace?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  take(
    broker.ask(auth(secondAgent, second), {
      clientRequestId: request("workspace_question_second"),
      prompt: "Second workspace?",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    }),
  );
  const firstEvents = take(
    broker.events(
      { principalToken: firstViewer.principalToken },
      { workspaceId: WORKSPACE, conversationId: CONVERSATION },
    ),
  );
  const secondEvents = take(
    broker.events(
      { principalToken: secondViewer.principalToken },
      { workspaceId: otherWorkspace, conversationId: CONVERSATION },
    ),
  );
  assert.deepEqual(
    firstEvents.events.map((event) => event.payload),
    [
      {
        questionId: firstEvents.events[0]?.correlationId,
        prompt: "First workspace?",
        answerMode: "free_text",
        choices: [],
        independentWorkAvailable: true,
      },
    ],
  );
  assert.equal(secondEvents.events.length, 1);
  const firstEvent = firstEvents.events.at(0);
  const secondEvent = secondEvents.events.at(0);
  if (firstEvent === undefined || secondEvent === undefined) assert.fail("workspace event missing");
  assert.equal(secondEvent.sequence, "1");
  assert.notEqual(firstEvent.messageId, secondEvent.messageId);
  const invalidCursor = broker.events(
    { principalToken: firstViewer.principalToken },
    {
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      afterSequence: DecimalSchema.parse("999"),
    },
  );
  expectError(invalidCursor, "resync_required");
  take(broker.close());
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("expiry and forwarding maintenance operations enforce bounded batches", () => {
  let instant = new Date("2026-09-15T00:00:00.000Z");
  const broker = take(openBroker({ databasePath: ":memory:", clock: () => instant }));
  const agent = enroll(broker, "agent", AGENT_CAPABILITIES);
  const parent = connect(broker, agent, "bounded_parent");
  const parentAuth = auth(agent, parent);
  const child = take(
    broker.delegate(parentAuth, {
      clientRequestId: request("bounded_child"),
      capabilities: ["question:ask", "inbox:read", "inbox:ack"],
      host: { kind: "test", provenance: "explicit_handle" },
      replyPolicy: { kind: "forward_parent" },
    }),
  );
  for (const suffix of ["one", "two", "three"]) {
    take(
      broker.ask(auth(agent, child), {
        clientRequestId: request(`expires_${suffix}`),
        prompt: `Expires ${suffix}?`,
        answerMode: "free_text",
        choices: [],
        independentWorkAvailable: true,
        deadlineAt: "2026-09-15T00:00:01.000Z",
      }),
    );
    take(
      broker.emit(parentAuth, {
        clientRequestId: request(`pending_${suffix}`),
        toActorId: child.actor.actorId,
        payload: { suffix },
      }),
    );
  }
  instant = new Date("2026-09-15T00:00:02.000Z");
  assert.equal(take(broker.expireQuestions(2)).length, 2);
  assert.equal(take(broker.expireQuestions(2)).length, 1);
  take(
    broker.expireActor(parentAuth, {
      clientRequestId: request("bounded_expire_actor"),
      actorId: child.actor.actorId,
    }),
  );
  const firstBatch = take(
    broker.forwardInbox(parentAuth, {
      clientRequestId: request("forward_batch_one"),
      fromActorId: child.actor.actorId,
      toActorId: parent.actor.actorId,
      limit: 2,
    }),
  );
  const secondBatch = take(
    broker.forwardInbox(parentAuth, {
      clientRequestId: request("forward_batch_two"),
      fromActorId: child.actor.actorId,
      toActorId: parent.actor.actorId,
      limit: 2,
    }),
  );
  const thirdBatch = take(
    broker.forwardInbox(parentAuth, {
      clientRequestId: request("forward_batch_three"),
      fromActorId: child.actor.actorId,
      toActorId: parent.actor.actorId,
      limit: 2,
    }),
  );
  assert.equal(firstBatch.forwardedDeliveryIds.length, 2);
  assert.equal(secondBatch.forwardedDeliveryIds.length, 2);
  assert.equal(thirdBatch.forwardedDeliveryIds.length, 2);
  take(broker.close());
});

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("public validation and storage errors redact submitted credential material", () => {
  const broker = memoryBroker();
  const secret = "credential_material_that_must_not_escape";
  const result = broker.connect({
    principalToken: secret,
    clientRequestId: request("redacted"),
    workspaceId: WORKSPACE,
    conversationId: CONVERSATION,
    capabilities: [],
    host: { kind: "test", provenance: "unverified" },
    replyPolicy: { kind: "retain" },
  });
  if (result.ok) assert.fail("unknown credential unexpectedly connected");
  assert.equal(result.error.message.includes(secret), false);
  assert.equal(result.error.details, undefined);
  take(broker.close());
});
