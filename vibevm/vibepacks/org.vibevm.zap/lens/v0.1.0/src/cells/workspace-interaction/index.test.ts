/** @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import assert from "node:assert/strict";
import test from "node:test";
import type { CoordinatorAdapter, CoordinatorEvent } from "../agent-runtime/index.ts";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  ConversationIdSchema,
  PrincipalIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
  ProjectIdSchema,
} from "../workspace-model/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { createWorkspaceInteractionFeature } from "./index.ts";

test("native answers persist while paused, dispatch exactly, and fence an old process epoch", async () => {
  let sequence = 0;
  const store = openWorkspaceStore({
    databasePath: ":memory:",
    idFactory: (kind) => `${kind}.${String(++sequence).padStart(8, "0")}`,
  });
  assert.equal(store.ok, true);
  if (!store.ok) return;
  const ids = scopeIds("native");
  assert.equal(store.value.registerProject(registration(ids)).ok, true);
  const feature = createWorkspaceInteractionFeature({ store: store.value });
  const event = nativeQuestionEvent(ids.sessionId, "epoch.1", 41);
  const observed = feature.observeNativeRequest({
    projectId: ids.projectId,
    contextId: ids.contextId,
    conversationId: ids.conversationId,
    originActorId: ids.actorId,
    event,
  });
  assert.equal(observed.ok, true);
  if (!observed.ok || observed.value === null || !("questionGroupId" in observed.value)) return;
  const firstItem = observed.value.items[0];
  const selectedOption = firstItem?.options[1] ?? firstItem?.options[0];
  assert.ok(firstItem !== undefined && selectedOption !== undefined);
  if (firstItem === undefined || selectedOption === undefined) return;
  const answered = store.value.command(humanAccess(ids, "one"), {
    operation: "question.answer.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.native.answer.1"),
    projectId: ids.projectId,
    contextId: ids.contextId,
    questionGroupId: observed.value.questionGroupId,
    expectedRevision: observed.value.revision,
    submission: {
      answers: [
        {
          questionItemId: firstItem.questionItemId,
          answer: { kind: "single_choice", optionId: selectedOption.optionId },
        },
      ],
      noteMarkdown: null,
    },
  });
  assert.equal(answered.ok, true);
  if (!answered.ok) return;
  const calls: unknown[] = [];
  const adapter = fakeAdapter(calls);
  const retained = await feature.afterQuestionCommand(
    humanAccess(ids, "one"),
    {
      operation: "question.answer.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.native.answer.1"),
      projectId: ids.projectId,
      contextId: ids.contextId,
      questionGroupId: observed.value.questionGroupId,
      expectedRevision: observed.value.revision,
      submission:
        answered.value.operation === "question.answer.v1"
          ? answered.value.answerVersion.submission
          : { answers: [], noteMarkdown: null },
    },
    answered.value,
    dispatch(ids, adapter, "epoch.1", false),
  );
  assert.equal(retained.ok, true);
  assert.equal(calls.length, 0);
  const drained = await feature.drain(dispatch(ids, adapter, "epoch.1", true));
  assert.equal(drained.ok, true);
  assert.deepEqual(calls, [
    {
      coordinatorSessionId: ids.sessionId,
      requestId: 41,
      processEpoch: "epoch.1",
      answer: { answers: { choice: { answers: ["Second"] } } },
    },
  ]);

  const staleEvent = nativeQuestionEvent(ids.sessionId, "epoch.1", "old-request");
  const staleQuestion = feature.observeNativeRequest({
    projectId: ids.projectId,
    contextId: ids.contextId,
    conversationId: ids.conversationId,
    originActorId: ids.actorId,
    event: staleEvent,
  });
  assert.ok(
    staleQuestion.ok && staleQuestion.value !== null && "questionGroupId" in staleQuestion.value,
  );
  if (
    !staleQuestion.ok ||
    staleQuestion.value === null ||
    !("questionGroupId" in staleQuestion.value)
  )
    return;
  const staleItem = staleQuestion.value.items[0];
  assert.notEqual(staleItem, undefined);
  if (staleItem === undefined) return;
  const staleAnswer = store.value.command(humanAccess(ids, "two"), {
    operation: "question.answer.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.native.answer.2"),
    projectId: ids.projectId,
    contextId: ids.contextId,
    questionGroupId: staleQuestion.value.questionGroupId,
    expectedRevision: staleQuestion.value.revision,
    submission: {
      answers: [
        { questionItemId: staleItem.questionItemId, answer: { kind: "short_text", text: "later" } },
      ],
      noteMarkdown: null,
    },
  });
  assert.equal(staleAnswer.ok, true);
  const staleDrain = await feature.drain(dispatch(ids, adapter, "epoch.2", true));
  assert.equal(staleDrain.ok, false);
  if (!staleDrain.ok) assert.equal(staleDrain.error.code, "stale_revision");
  assert.equal(calls.length, 1);
  store.value.close();
});

test("native approvals remain separate from rich question answers", async () => {
  let sequence = 100;
  const store = openWorkspaceStore({
    databasePath: ":memory:",
    idFactory: (kind) => `${kind}.${++sequence}`,
  });
  assert.equal(store.ok, true);
  if (!store.ok) return;
  const ids = scopeIds("approval");
  store.value.registerProject(registration(ids));
  const feature = createWorkspaceInteractionFeature({ store: store.value });
  const event = nativeApprovalEvent(ids.sessionId, "epoch.approval", "approval.1");
  const observed = feature.observeNativeRequest({ ...ids, originActorId: ids.actorId, event });
  assert.ok(
    observed.ok && observed.value !== null && "nativeApprovalId" in observed.value,
    JSON.stringify(observed),
  );
  if (!observed.ok || observed.value === null || !("nativeApprovalId" in observed.value)) return;
  const questionList = store.value.read(humanAccess(ids, "reader"), {
    operation: "question.list.v1",
    projectId: ids.projectId,
    contextId: ids.contextId,
    state: null,
    limit: 10,
  });
  assert.ok(questionList.ok && questionList.value.operation === "question.list.v1");
  if (questionList.ok && questionList.value.operation === "question.list.v1")
    assert.equal(questionList.value.questions.length, 0);
  const calls: unknown[] = [];
  const responded = await feature.respondToApproval(
    humanAccess(ids, "approver"),
    {
      operation: "native-approval.respond.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.native.approval.1"),
      projectId: ids.projectId,
      contextId: ids.contextId,
      nativeApprovalId: observed.value.nativeApprovalId,
      expectedRevision: observed.value.revision,
      response: { decision: "decline" },
    },
    dispatch(ids, fakeAdapter(calls), "epoch.approval", true),
  );
  assert.equal(responded.ok, true);
  assert.equal(calls.length, 1);
  store.value.close();
});

test("host resolution closes an unanswered native question", () => {
  const store = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(store.ok, true);
  if (!store.ok) return;
  const ids = scopeIds("resolved");
  store.value.registerProject(registration(ids));
  const feature = createWorkspaceInteractionFeature({ store: store.value });
  const event = nativeQuestionEvent(ids.sessionId, "epoch.resolved", 9);
  const question = feature.observeNativeRequest({
    projectId: ids.projectId,
    contextId: ids.contextId,
    conversationId: ids.conversationId,
    originActorId: ids.actorId,
    event,
  });
  assert.ok(question.ok && question.value !== null && "questionGroupId" in question.value);
  if (!question.ok || question.value === null || !("questionGroupId" in question.value)) return;
  const resolved: CoordinatorEvent = {
    ...event,
    kind: "host_request_resolved",
    sourceEventId: "source.resolved",
    data: { requestId: 9 },
  };
  assert.equal(
    feature.observeResolved({
      projectId: ids.projectId,
      contextId: ids.contextId,
      conversationId: ids.conversationId,
      originActorId: ids.actorId,
      event: resolved,
    }).ok,
    true,
  );
  const detail = store.value.read(humanAccess(ids, "resolved"), {
    operation: "question.get.v1",
    projectId: ids.projectId,
    contextId: ids.contextId,
    questionGroupId: question.value.questionGroupId,
  });
  assert.ok(detail.ok && detail.value.operation === "question.get.v1");
  if (detail.ok && detail.value.operation === "question.get.v1")
    assert.equal(detail.value.detail.question.state, "cancelled");
  store.value.close();
});

function nativeQuestionEvent(
  sessionId: ReturnType<typeof AgentSessionIdSchema.parse>,
  epoch: string,
  requestId: string | number,
): CoordinatorEvent {
  return {
    coordinatorSessionId: sessionId,
    processEpoch: epoch,
    nativeThreadId: "thread.native",
    nativeTurnId: "turn.native",
    nativeItemId: "item.native",
    kind: "host_request_pending",
    sourceEventId: `source.${String(requestId)}`,
    data: {
      coordinatorSessionId: sessionId,
      nativeThreadId: "thread.native",
      nativeTurnId: "turn.native",
      nativeItemId: "item.native",
      requestId,
      processEpoch: epoch,
      kind: "user_input",
      body: {
        threadId: "thread.native",
        turnId: "turn.native",
        itemId: "item.native",
        questions:
          requestId === "old-request"
            ? [{ id: "text", header: "Text", question: "Say more" }]
            : [
                {
                  id: "choice",
                  header: "Choice",
                  question: "Pick",
                  options: [
                    { label: "First", description: "One" },
                    { label: "Second", description: "Two" },
                  ],
                },
              ],
        isBlocking: true,
      },
    },
  };
}

function nativeApprovalEvent(
  sessionId: ReturnType<typeof AgentSessionIdSchema.parse>,
  epoch: string,
  requestId: string,
): CoordinatorEvent {
  return {
    coordinatorSessionId: sessionId,
    processEpoch: epoch,
    nativeThreadId: "thread.native",
    nativeTurnId: "turn.native",
    nativeItemId: "item.command",
    kind: "host_request_pending",
    sourceEventId: "source.approval",
    data: {
      coordinatorSessionId: sessionId,
      nativeThreadId: "thread.native",
      nativeTurnId: "turn.native",
      nativeItemId: "item.command",
      requestId,
      processEpoch: epoch,
      kind: "command_approval",
      body: {
        threadId: "thread.native",
        turnId: "turn.native",
        itemId: "item.command",
        command: "test",
      },
    },
  };
}

function scopeIds(suffix: string) {
  return {
    projectId: ProjectIdSchema.parse(`project.${suffix}`),
    contextId: WorkContextIdSchema.parse(`context.${suffix}`),
    workspaceId: WorkspaceIdSchema.parse(`workspace.${suffix}`),
    conversationId: ConversationIdSchema.parse(`conversation.${suffix}`),
    actorId: ActorIdSchema.parse(`actor.${suffix}`),
    sessionId: AgentSessionIdSchema.parse(`session.${suffix}`),
  };
}

function registration(ids: ReturnType<typeof scopeIds>) {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: `request.register.${ids.projectId}`,
    projectId: ids.projectId,
    displayName: ids.projectId,
    repositoryRootRefs: ["repo.fixture"],
    actions: {},
    context: {
      contextId: ids.contextId,
      displayName: ids.contextId,
      workspaceRef: "workspace.fixture",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable", reason: "fixture" },
      coordinatorConversationId: ids.conversationId,
      brokerScope: { workspaceId: ids.workspaceId, conversationId: ids.conversationId },
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.test",
        label: "Test",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: process.cwd(), launchProfileRef: "profile.test" },
  });
}

function humanAccess(ids: ReturnType<typeof scopeIds>, suffix: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(`principal.human.${suffix}`),
    actorId: null,
    clientId: ClientIdSchema.parse(`client.human.${suffix}`),
    authorizedProjectIds: [ids.projectId],
  });
}

function dispatch(
  ids: ReturnType<typeof scopeIds>,
  adapter: CoordinatorAdapter,
  epoch: string,
  executionEnabled: boolean,
) {
  return {
    projectId: ids.projectId,
    contextId: ids.contextId,
    coordinatorSessionId: ids.sessionId,
    adapter,
    processEpoch: epoch,
    executionEnabled,
  };
}

function fakeAdapter(calls: unknown[]): CoordinatorAdapter {
  const unavailable = async () => ({
    ok: false as const,
    error: { code: "unsupported" as const, message: "fixture", retry: "never" as const },
  });
  return {
    capabilities: {
      persistentThreads: true,
      turnStart: true,
      activeTurnSteer: true,
      turnInterrupt: true,
      historyRead: true,
      nativeChildObservation: true,
      nativeChildDirectInput: false,
      structuredUserInput: true,
      commandApproval: true,
      managedTerminal: false,
    },
    start: unavailable,
    resume: unavailable,
    readHistory: unavailable,
    readNativeChildHistory: unavailable,
    startTurn: unavailable,
    steer: unavailable,
    send: unavailable,
    interrupt: unavailable,
    async respondToRequest(input) {
      calls.push(input);
      return { ok: true, value: undefined };
    },
    restart: unavailable,
    subscribe: () => () => undefined,
    close: () => undefined,
  };
}
