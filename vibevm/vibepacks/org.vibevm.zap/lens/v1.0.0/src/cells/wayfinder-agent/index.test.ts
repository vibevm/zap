/** @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import assert from "node:assert/strict";
import test from "node:test";
import { openBroker } from "../broker/index.ts";
import { createAgentHttpClient, createLensHttpGateway } from "../http/index.ts";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  CredentialSchema,
  PrincipalIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import { createLocalPrincipalTransport } from "../transport/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  QuestionItemIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
  type QuestionGroup,
} from "../workspace-model/index.ts";
import { createWorkspaceInteractionFeature } from "../workspace-interaction/index.ts";
import {
  createCoordinatorAdapterRegistry,
  createWorkspaceService,
} from "../workspace-service/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { createBrokerAgentAnswerDelivery, createWayfinderAgentPublisher } from "./index.ts";

test("two broker actors publish rich questions and two clients answer without cross-project delivery", async () => {
  const broker = openBroker({ databasePath: ":memory:" });
  const store = openWorkspaceStore({ databasePath: ":memory:" });
  assert.ok(broker.ok && store.ok);
  if (!broker.ok || !store.ok) return;
  const a = ids("a");
  const b = ids("b");
  assert.equal(store.value.registerProject(registration(a)).ok, true);
  assert.equal(store.value.registerProject(registration(a)).ok, true);
  assert.equal(store.value.registerProject(registration(b)).ok, true);
  const agentPrincipal = broker.value.enrollPrincipal({
    kind: "agent",
    workspaceIds: [a.workspaceId],
    conversationIds: [a.conversationId],
    capabilities: ["question:ask", "inbox:read", "actor:delegate"],
  });
  const humanPrincipal = broker.value.enrollPrincipal({
    kind: "human_responder",
    workspaceIds: [a.workspaceId],
    conversationIds: [a.conversationId],
    capabilities: ["message:emit"],
  });
  assert.ok(agentPrincipal.ok && humanPrincipal.ok);
  if (!agentPrincipal.ok || !humanPrincipal.ok) return;
  const interactions = createWorkspaceInteractionFeature({
    store: store.value,
    agentAnswers: createBrokerAgentAnswerDelivery(
      createLocalPrincipalTransport(broker.value, humanPrincipal.value.principalToken),
    ),
  });
  const service = createWorkspaceService({
    store: store.value,
    adapters: createCoordinatorAdapterRegistry([]),
    interactions,
  });
  const publisher = createWayfinderAgentPublisher({ store: store.value });
  let sessionSequence = 0;
  const gateway = createLensHttpGateway({
    broker: broker.value,
    agentQuestions: publisher,
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [],
    statusToken: CredentialSchema.parse("status-wayfinder-agent-000000000001"),
    adapterSessionIdFactory: () => `adapter.wayfinder.${++sessionSequence}.000000000000`,
  });
  const started = await gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  try {
    const client = createAgentHttpClient({
      baseUrl: new URL(`http://127.0.0.1:${String(started.value.port)}`),
      principalToken: agentPrincipal.value.principalToken,
    });
    const parent = await client.connect({
      clientRequestId: ClientRequestIdSchema.parse("request.agent.parent.connect"),
      workspaceId: a.workspaceId,
      conversationId: a.conversationId,
      capabilities: ["question:ask", "inbox:read", "actor:delegate"],
      host: { kind: "codex", sessionId: "thread.parent", provenance: "explicit_handle" },
      replyPolicy: { kind: "retain" },
    });
    assert.equal(parent.ok, true);
    if (!parent.ok) return;
    const child = await client.delegate(parent.value.adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.agent.child.delegate"),
      capabilities: ["question:ask", "inbox:read"],
      host: {
        kind: "codex",
        sessionId: "thread.parent",
        subagentId: "thread.child",
        provenance: "explicit_handle",
      },
      replyPolicy: { kind: "retain" },
    });
    assert.equal(child.ok, true);
    if (!child.ok || client.askUserQuestion === undefined) return;
    const parentQuestion = await client.askUserQuestion(parent.value.adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.agent.parent.question"),
      draft: questionDraft("parent", "Parent question"),
    });
    const childQuestion = await client.askUserQuestion(child.value.adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.agent.child.question"),
      draft: questionDraft("child", "Child question"),
    });
    assert.ok(parentQuestion.ok && childQuestion.ok);
    if (!parentQuestion.ok || !childQuestion.ok) return;
    const uiOne = service.bind({
      access: uiAccess(a, "one"),
      allowedActions: ["read", "question.answer.v1"],
    });
    const uiTwo = service.bind({
      access: uiAccess(a, "two"),
      allowedActions: ["read", "question.answer.v1"],
    });
    const childAnswer = await uiOne.command(
      answerRequest(childQuestion.value, "child-answer", "one"),
    );
    const parentAnswer = await uiTwo.command(
      answerRequest(parentQuestion.value, "parent-answer", "two"),
    );
    assert.ok(childAnswer.ok && parentAnswer.ok);
    const parentInbox = await client.inbox(parent.value.adapterSessionId, {
      afterSequence: "0",
      limit: 10,
    });
    const childInbox = await client.inbox(child.value.adapterSessionId, {
      afterSequence: "0",
      limit: 10,
    });
    assert.ok(parentInbox.ok && childInbox.ok);
    if (parentInbox.ok && childInbox.ok) {
      assert.equal(parentInbox.value.deliveries.length, 1);
      assert.equal(childInbox.value.deliveries.length, 1);
      assert.equal(
        parentInbox.value.deliveries[0]?.message.correlationId,
        parentQuestion.value.questionGroupId,
      );
      assert.equal(
        childInbox.value.deliveries[0]?.message.correlationId,
        childQuestion.value.questionGroupId,
      );
    }
    const projectB = service.bind({ access: uiAccess(b, "reader"), allowedActions: ["read"] });
    const foreign = await projectB.read({
      operation: "question.list.v1",
      projectId: b.projectId,
      contextId: b.contextId,
      state: null,
      limit: 10,
    });
    assert.ok(foreign.ok && foreign.value.operation === "question.list.v1");
    if (foreign.ok && foreign.value.operation === "question.list.v1")
      assert.equal(foreign.value.questions.length, 0);
  } finally {
    await gateway.close();
    service.close();
    store.value.close();
    broker.value.close();
  }
});

function questionDraft(suffix: string, title: string) {
  return {
    title,
    introductionMarkdown: "Sent through /ZapAskUserQuestion.",
    items: [
      {
        questionItemId: QuestionItemIdSchema.parse(`question-item.${suffix}`),
        header: "Reply",
        promptMarkdown: "What should happen?",
        contextMarkdown: null,
        artifactRefs: [],
        required: true,
        answerMode: "short_text" as const,
        options: [],
        customAnswer: null,
        recommendation: null,
      },
    ],
    independentWorkAvailable: true,
    deadlineAt: null,
  };
}

function answerRequest(question: QuestionGroup, text: string, suffix: string) {
  const questionItemId =
    question.items[0]?.questionItemId ??
    QuestionItemIdSchema.parse("question-item.fixture-missing");
  return {
    operation: "question.answer.v1" as const,
    clientRequestId: ClientRequestIdSchema.parse(`request.agent.answer.${suffix}`),
    projectId: question.projectId,
    contextId: question.contextId,
    questionGroupId: question.questionGroupId,
    expectedRevision: question.revision,
    submission: {
      answers: [{ questionItemId, answer: { kind: "short_text" as const, text } }],
      noteMarkdown: null,
    },
  };
}

function ids(suffix: string) {
  return {
    projectId: ProjectIdSchema.parse(`project.${suffix}`),
    contextId: WorkContextIdSchema.parse(`context.${suffix}`),
    workspaceId: WorkspaceIdSchema.parse(`workspace.${suffix}`),
    conversationId: ConversationIdSchema.parse(`conversation.${suffix}`),
  };
}

function registration(scope: ReturnType<typeof ids>) {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: `request.register.${scope.projectId}`,
    projectId: scope.projectId,
    displayName: scope.projectId,
    repositoryRootRefs: ["repo.fixture"],
    actions: {},
    context: {
      contextId: scope.contextId,
      displayName: scope.contextId,
      workspaceRef: "workspace.fixture",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable", reason: "fixture" },
      coordinatorConversationId: scope.conversationId,
      brokerScope: { workspaceId: scope.workspaceId, conversationId: scope.conversationId },
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

function uiAccess(scope: ReturnType<typeof ids>, suffix: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(`principal.ui.${suffix}`),
    actorId: null,
    clientId: ClientIdSchema.parse(`client.ui.${suffix}`),
    authorizedProjectIds: [scope.projectId],
  });
}
