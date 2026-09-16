/** Public HTTP/MCP-shaped mock question, inbox and acknowledgement gate. @scope spec://org.vibevm.zap/lens/PROP-013#verification */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import test from "node:test";
import { z } from "zod";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  CredentialSchema,
  DecimalSchema,
  DeliveryIdSchema,
  EnrollPrincipalInputSchema,
} from "../protocol/index.ts";
import { openBroker } from "../broker/index.ts";
import { createAgentHttpClient } from "../http/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import { createZapMockAgent, type ZapMockQuestionBridge } from "./index.ts";
import { createZapMockModel, type ZapMockSnapshot } from "../mock-model/index.ts";
import { questionHttpBehavior, questionWakeScenario } from "./question-scenario.fixture.ts";
import { createWayfinderRuntime } from "../wayfinder-runtime/index.ts";
import {
  ProjectIdSchema,
  QuestionItemIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";

test("ZapMock question bridge uses authenticated HTTP inbox and ack", async () => {
  const root = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "zap-mock-question-"));
  const brokerPath = join(root, "agent.sqlite");
  const broker = openBroker({ databasePath: brokerPath });
  assert.equal(broker.ok, true);
  if (!broker.ok) return;
  const agentEnrollment = broker.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: ["workspace.mock.question"],
      conversationIds: ["conversation.mock.question"],
      capabilities: [
        "message:emit",
        "question:ask",
        "question:cancel",
        "inbox:read",
        "inbox:ack",
        "inbox:forward",
        "actor:delegate",
        "actor:expire",
        "plan:propose",
      ],
    }),
  );
  const humanEnrollment = broker.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "human_responder",
      workspaceIds: ["workspace.mock.question"],
      conversationIds: ["conversation.mock.question"],
      capabilities: ["message:emit"],
    }),
  );
  assert.equal(agentEnrollment.ok, true);
  assert.equal(humanEnrollment.ok, true);
  if (!agentEnrollment.ok || !humanEnrollment.ok) return;
  const agentToken = agentEnrollment.value.principalToken;
  const humanToken = humanEnrollment.value.principalToken;
  broker.value.close();
  let agentClient: ReturnType<typeof createAgentHttpClient> | null = null;
  let adapterSessionId: string | null = null;
  let acknowledgements = 0;
  let inboxReads = 0;
  let inboxError: string | null = null;
  const bridge: ZapMockQuestionBridge = {
    async publishQuestion(input) {
      adapterSessionId = input.adapterSessionId;
      if (agentClient === null)
        return { ok: false, error: { message: "agent HTTP client is not ready" } };
      if (agentClient.askUserQuestion === undefined)
        return { ok: false, error: { message: "question HTTP tool is unavailable" } };
      const result = await agentClient.askUserQuestion(
        AdapterSessionIdSchema.parse(input.adapterSessionId),
        {
          clientRequestId: ClientRequestIdSchema.parse(`request.${input.questionId}`),
          draft: {
            title: input.questionId,
            introductionMarkdown: input.prompt,
            independentWorkAvailable: false,
            deadlineAt: null,
            items: [
              {
                questionItemId: QuestionItemIdSchema.parse(`${input.questionId}.item`),
                header: "Signal",
                promptMarkdown: input.prompt,
                contextMarkdown: null,
                artifactRefs: [],
                required: true,
                answerMode: "short_text",
                options: [],
                customAnswer: null,
                recommendation: null,
              },
            ],
          },
        },
      );
      return result.ok
        ? { ok: true, value: { questionGroupId: result.value.questionGroupId } }
        : { ok: false, error: { message: result.error.message } };
    },
    async readAnswer(session) {
      inboxReads += 1;
      if (agentClient === null)
        return { ok: false, error: { message: "agent HTTP client is not ready" } };
      const page = await agentClient.waitInbox(AdapterSessionIdSchema.parse(session), {
        afterSequence: "0",
        limit: 50,
        timeoutMilliseconds: 5_000,
      });
      if (!page.ok) {
        inboxError = page.error.message;
        return { ok: false, error: { message: page.error.message } };
      }
      const deliveries = page.value.deliveries.flatMap((delivery) => {
        const payload = z
          .looseObject({
            type: z.literal("question.answer"),
            questionGroupId: z.string(),
            answerVersionId: z.string(),
            submission: z.unknown(),
          })
          .safeParse(delivery.message.payload);
        return payload.success
          ? [
              {
                deliveryId: delivery.deliveryId,
                questionId: payload.data.questionGroupId,
                answerVersion: payload.data.answerVersionId,
                text: JSON.stringify(payload.data.submission),
              },
            ]
          : [];
      });
      return { ok: true, value: deliveries };
    },
    async acknowledge(input) {
      if (agentClient === null)
        return { ok: false, error: { message: "agent HTTP client is not ready" } };
      const result = await agentClient.ack(AdapterSessionIdSchema.parse(input.adapterSessionId), {
        clientRequestId: ClientRequestIdSchema.parse("request.mock.ack"),
        deliveryIds: [DeliveryIdSchema.parse(input.deliveryId)],
      });
      if (result.ok) acknowledgements += 1;
      return result.ok
        ? { ok: true, value: null }
        : { ok: false, error: { message: result.error.message } };
    },
  };
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? questionHttpBehavior.scenarioFile.seed;
  assert.equal(
    process.env["ZAP_MOCK_SIMULATION_ID"] ?? questionHttpBehavior.scenarioId,
    questionHttpBehavior.scenarioId,
  );
  const modelFactory = (snapshot?: ZapMockSnapshot) => {
    const model = createZapMockModel({
      seed,
      scenario: questionWakeScenario,
      ...(snapshot === undefined ? {} : { snapshot }),
    });
    if (!model.ok) throw new Error();
    return model.value;
  };
  const agent = createZapMockAgent({ modelFactory, questionBridge: bridge });
  const config = runtimeConfig(root, brokerPath, agentToken, humanToken);
  const created = createWayfinderRuntime(config, { hosts: [agent.host] });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  try {
    const started = await created.value.start();
    assert.equal(started.ok, true);
    if (!started.ok || started.value.agentGateway === null) return;
    agentClient = createAgentHttpClient({
      baseUrl: new URL(`http://127.0.0.1:${String(started.value.agentGateway.port)}`),
      principalToken: agentToken,
    });
    const ticket = created.value.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const workspace = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://mock.test",
    });
    assert.notEqual(workspace, null);
    if (workspace === null) return;
    const startedSession = await workspace.command({
      operation: "session.start.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.question.session"),
      projectId: ProjectIdSchema.parse("project.mock.question"),
      contextId: WorkContextIdSchema.parse("context.mock.question"),
      interactionKind: "structured",
      profileId: "profile.zap-mock",
    });
    assert.equal(startedSession.ok, true, startedSession.ok ? "" : startedSession.error.message);
    const sent = await workspace.command({
      operation: "chat.post.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.question.turn"),
      projectId: ProjectIdSchema.parse("project.mock.question"),
      contextId: WorkContextIdSchema.parse("context.mock.question"),
      conversationId: ConversationIdSchema.parse("conversation.mock.question"),
      bodyMarkdown: questionHttpBehavior.inputs.initialMessage,
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    });
    assert.equal(sent.ok, true);
    assert.notEqual(adapterSessionId, null);
    const initialExecution = await execution(workspace);
    const beforeSessionId = initialExecution.sessionId;
    assert.notEqual(beforeSessionId, null);
    const questions = await workspace.read({
      operation: "question.list.v1",
      projectId: ProjectIdSchema.parse("project.mock.question"),
      contextId: WorkContextIdSchema.parse("context.mock.question"),
      state: "open",
      limit: 10,
    });
    assert.equal(questions.ok, true);
    if (questions.ok && questions.value.operation === "question.list.v1") {
      const question = questions.value.questions[0];
      assert.ok(question !== undefined);
      if (question !== undefined) {
        const item = question.items[0];
        assert.equal(item?.promptMarkdown, questionHttpBehavior.expected.questionPrompt);
        assert.ok(item !== undefined);
        if (item !== undefined) {
          const beforePause = await execution(workspace);
          assert.notEqual(beforePause.sessionId, null);
          const paused = await workspace.command({
            operation: "project.pause.v1",
            clientRequestId: ClientRequestIdSchema.parse("request.mock.pause"),
            projectId: ProjectIdSchema.parse("project.mock.question"),
            contextId: WorkContextIdSchema.parse("context.mock.question"),
            sessionId: beforePause.sessionId,
            expectedRevision: beforePause.revision,
            reasonMarkdown: questionHttpBehavior.inputs.pauseReason,
          });
          assert.equal(paused.ok, true, JSON.stringify(paused));
          if (!paused.ok || paused.value.operation !== "project.pause.v1") return;
          assert.equal(
            paused.value.execution.state,
            questionHttpBehavior.expected.pausedExecutionState,
          );
          const answered = await workspace.command({
            operation: "question.answer.v1",
            clientRequestId: ClientRequestIdSchema.parse("request.mock.answer"),
            projectId: ProjectIdSchema.parse("project.mock.question"),
            contextId: WorkContextIdSchema.parse("context.mock.question"),
            questionGroupId: question.questionGroupId,
            expectedRevision: DecimalSchema.parse(question.revision),
            submission: {
              answers: [
                {
                  questionItemId: QuestionItemIdSchema.parse(item.questionItemId),
                  answer: { kind: "short_text", text: questionHttpBehavior.inputs.answerText },
                },
              ],
              noteMarkdown: null,
            },
          });
          assert.equal(answered.ok, true);
          assert.equal(
            acknowledgements,
            questionHttpBehavior.expected.acknowledgementsBeforeContinue,
          );
          const beforeContinue = await execution(workspace);
          const continued = await workspace.command({
            operation: "project.continue.v1",
            clientRequestId: ClientRequestIdSchema.parse("request.mock.continue"),
            projectId: ProjectIdSchema.parse("project.mock.question"),
            contextId: WorkContextIdSchema.parse("context.mock.question"),
            sessionId: beforePause.sessionId,
            expectedRevision: beforeContinue.revision,
            reasonMarkdown: questionHttpBehavior.inputs.continueReason,
          });
          assert.equal(continued.ok, true, JSON.stringify(continued));
          await until(() =>
            acknowledgements === questionHttpBehavior.expected.acknowledgementsAfterContinue
              ? true
              : undefined,
          );
          const noticePage = await workspace.read({
            operation: "chat.page.v1",
            projectId: ProjectIdSchema.parse("project.mock.question"),
            contextId: WorkContextIdSchema.parse("context.mock.question"),
            conversationId: ConversationIdSchema.parse("conversation.mock.question"),
            afterSequence: DecimalSchema.parse("0"),
            limit: 50,
          });
          assert.equal(noticePage.ok, true, `answer=${JSON.stringify(answered)}`);
          if (noticePage.ok && noticePage.value.operation === "chat.page.v1")
            assert.equal(
              noticePage.value.page.messages.some((message) =>
                message.bodyMarkdown.includes("ANSWER READY"),
              ),
              true,
              `inboxReads=${inboxReads}`,
            );
        }
      }
    }
    assert.equal(
      acknowledgements,
      questionHttpBehavior.expected.acknowledgementsAfterContinue,
      `inboxReads=${inboxReads}, inboxError=${inboxError ?? "none"}, observations=${JSON.stringify(agent.driver.observations())}`,
    );
    assert.equal(agent.driver.observations().at(-1)?.accepted, true);
    const afterContinue = await execution(workspace);
    assert.equal(
      afterContinue.sessionId === null || beforeSessionId === null
        ? false
        : afterContinue.sessionId === beforeSessionId,
      questionHttpBehavior.expected.sameSession,
    );
    assert.equal(
      beforeSessionId === null ? null : agent.driver.state(beforeSessionId)?.status,
      questionHttpBehavior.expected.finalModelStatus,
    );
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: questionHttpBehavior.scenarioId,
        seed,
        passed: true,
        inputPosition: 5,
        zeroLlmInference: questionHttpBehavior.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await created.value.close();
    rmSync(root, { recursive: true, force: true });
  }
});

type MockWorkspaceClient = NonNullable<ReturnType<typeof createWorkspaceHttpClient>>;

async function execution(client: MockWorkspaceClient) {
  const result = await client.read({
    operation: "project.execution.get.v1",
    projectId: ProjectIdSchema.parse("project.mock.question"),
    contextId: WorkContextIdSchema.parse("context.mock.question"),
  });
  assert.equal(result.ok, true);
  if (!result.ok || result.value.operation !== "project.execution.get.v1") throw new Error();
  return result.value.execution;
}

async function until(read: () => boolean | undefined): Promise<void> {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    if (read() === true) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error();
}

function runtimeConfig(root: string, brokerPath: string, agentToken: string, humanToken: string) {
  return {
    version: 1 as const,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "mock-question",
      pairingToken: randomBytes(24).toString("base64url"),
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://mock.test"],
    },
    agentGateway: {
      databasePath: brokerPath,
      host: "127.0.0.1" as const,
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: CredentialSchema.parse("status.mock.question.0000000001"),
      scopes: [
        {
          workspaceId: "workspace.mock.question",
          conversationId: "conversation.mock.question",
          humanPrincipalToken: humanToken,
          agentPrincipalToken: agentToken,
        },
      ],
    },
    profiles: [
      {
        profileId: "profile.zap-mock",
        executablePath: "C:\\synthetic\\zap-mock.exe",
        requestTimeoutMs: 5000,
        model: "zap-mock/deterministic-v1",
        effort: "low" as const,
        approvalPolicy: "on-request" as const,
        sandbox: "workspace-write" as const,
        personality: "pragmatic" as const,
        serviceName: "zap-mock",
      },
    ],
    projects: [
      {
        registrationId: "request.project.mock.question",
        projectId: "project.mock.question",
        displayName: "Mock question",
        repositoryRootRefs: ["repository.mock.question"],
        actions: { startCoordinator: { state: "available" as const } },
        context: {
          contextId: "context.mock.question",
          displayName: "Question",
          workspaceRef: "workspace.mock.question",
          branchLabel: "main",
          revisionBinding: "mock",
          planning: { state: "unavailable" as const, reason: "mock" },
          coordinatorConversationId: "conversation.mock.question",
          brokerScope: {
            workspaceId: "workspace.mock.question",
            conversationId: "conversation.mock.question",
          },
        },
        coordinatorLaunchOptions: [
          {
            profileId: "profile.zap-mock",
            label: "ZapMock",
            interactionKind: "structured" as const,
            availability: { state: "available" as const },
          },
        ],
        protected: { cwd: "C:\\synthetic\\zap-mock", launchProfileRef: "profile.zap-mock" },
      },
    ],
  };
}
