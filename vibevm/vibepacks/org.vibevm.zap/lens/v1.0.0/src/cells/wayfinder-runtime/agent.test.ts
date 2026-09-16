/** @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import {
  CoordinatorSessionDescriptorSchema,
  type AgentHost,
  type CoordinatorAdapter,
} from "../agent-runtime/index.ts";
import { openBroker } from "../broker/index.ts";
import { createAgentHttpClient } from "../http/index.ts";
import {
  ClientRequestIdSchema,
  CredentialSchema,
  DecimalSchema,
  EnrollPrincipalInputSchema,
} from "../protocol/index.ts";
import {
  ClientIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  QuestionItemIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";

test("Wayfinder owns broker, durable actor HTTP, rich question, answer, and inbox", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-agent-runtime-"));
  const brokerPath = join(root, "broker.sqlite");
  const seeded = openBroker({ databasePath: brokerPath });
  assert.equal(seeded.ok, true);
  if (!seeded.ok) return;
  const agent = seeded.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: ["workspace.runtime-agent"],
      conversationIds: ["conversation.runtime-agent"],
      capabilities: [
        "message:emit",
        "question:ask",
        "inbox:read",
        "inbox:ack",
        "actor:delegate",
        "actor:expire",
        "inbox:forward",
        "plan:propose",
      ],
    }),
  );
  const human = seeded.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "human_responder",
      workspaceIds: ["workspace.runtime-agent"],
      conversationIds: ["conversation.runtime-agent"],
      capabilities: ["message:emit"],
    }),
  );
  seeded.value.close();
  assert.ok(agent.ok && human.ok);
  if (!agent.ok || !human.ok) return;
  const config = {
    version: 1 as const,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "runtime_agent",
      pairingToken: "synthetic-runtime-agent-pairing-0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    agentGateway: {
      databasePath: brokerPath,
      host: "127.0.0.1" as const,
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: CredentialSchema.parse("status-runtime-agent-000000000001"),
      scopes: [
        {
          workspaceId: "workspace.runtime-agent",
          conversationId: "conversation.runtime-agent",
          humanPrincipalToken: human.value.principalToken,
          agentPrincipalToken: agent.value.principalToken,
        },
      ],
    },
    profiles: [profile()],
    projects: [project()],
    modelPolicies: [],
  };
  const adapter = fakeCoordinator();
  const host: AgentHost = {
    hostId: ExecutionHostIdSchema.parse("host.runtime-agent"),
    profileIds: ["profile.runtime-agent"],
    openCoordinator: () => Promise.resolve({ ok: true, value: adapter }),
  };
  const opened = createWayfinderRuntime(config, { hosts: [host] });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  try {
    const started = await opened.value.start();
    assert.ok(started.ok && started.value.agentGateway !== null);
    if (!started.ok || started.value.agentGateway === null) return;
    const client = createAgentHttpClient({
      baseUrl: new URL(
        `http://${started.value.agentGateway.host}:${String(started.value.agentGateway.port)}`,
      ),
      principalToken: agent.value.principalToken,
    });
    const access = WorkspaceAccessContextSchema.parse({
      principalId: "principal.runtime-ui",
      actorId: null,
      clientId: ClientIdSchema.parse("client.runtime-ui"),
      authorizedProjectIds: [ProjectIdSchema.parse("project.runtime-agent")],
    });
    const ui = opened.value.service.bind({
      access,
      allowedActions: ["read", "session.start.v1", "question.answer.v1"],
    });
    const launching = await ui.command({
      operation: "session.start.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.start"),
      projectId: ProjectIdSchema.parse("project.runtime-agent"),
      contextId: WorkContextIdSchema.parse("context.runtime-agent"),
      interactionKind: "structured",
      profileId: "profile.runtime-agent",
    });
    assert.equal(launching.ok, true);
    const sessions = await ui.read({
      operation: "session.list.v1",
      projectId: ProjectIdSchema.parse("project.runtime-agent"),
      contextId: WorkContextIdSchema.parse("context.runtime-agent"),
    });
    assert.ok(sessions.ok && sessions.value.operation === "session.list.v1");
    if (!sessions.ok || sessions.value.operation !== "session.list.v1") return;
    const session = sessions.value.sessions[0];
    assert.notEqual(session, undefined);
    if (session === undefined || client.askUserQuestion === undefined) return;
    const adapterSessionId = AdapterSessionIdSchema.parse(
      `adapter.owned.${createHash("sha256").update(session.sessionId).digest("hex")}`,
    );
    assert.deepEqual(adapter.bindings, [adapterSessionId]);
    const question = await client.askUserQuestion(adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.question"),
      draft: {
        title: "Runtime question",
        introductionMarkdown: "/ZapAskUserQuestion",
        items: [
          {
            questionItemId: QuestionItemIdSchema.parse("question-item.runtime-agent"),
            header: "Answer",
            promptMarkdown: "Reply",
            contextMarkdown: null,
            artifactRefs: [],
            required: true,
            answerMode: "short_text",
            options: [],
            customAnswer: null,
            recommendation: null,
          },
        ],
        independentWorkAvailable: true,
        deadlineAt: null,
      },
    });
    assert.equal(question.ok, true);
    if (!question.ok) return;
    const item = question.value.items[0];
    assert.notEqual(item, undefined);
    if (item === undefined) return;
    assert.equal(
      (
        await ui.command({
          operation: "question.answer.v1",
          clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.answer"),
          projectId: question.value.projectId,
          contextId: question.value.contextId,
          questionGroupId: question.value.questionGroupId,
          expectedRevision: question.value.revision,
          submission: {
            answers: [
              { questionItemId: item.questionItemId, answer: { kind: "short_text", text: "yes" } },
            ],
            noteMarkdown: null,
          },
        })
      ).ok,
      true,
    );
    const inbox = await client.inbox(adapterSessionId, {
      afterSequence: DecimalSchema.parse("0"),
      limit: 10,
    });
    assert.ok(
      inbox.ok &&
        inbox.value.deliveries[0]?.message.correlationId === question.value.questionGroupId,
    );
    assert.equal(adapter.sent.length, 1);
    assert.match(adapter.sent[0] ?? "", /ANSWER READY/);
    assert.match(adapter.sent[0] ?? "", /"text":"yes"/);
    const child = await client.delegate(adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.child"),
      capabilities: ["question:ask", "inbox:read"],
      host: {
        kind: "codex",
        sessionId: session.sessionId,
        subagentId: "native.child.fixture",
        provenance: "explicit_handle",
      },
      replyPolicy: { kind: "retain" },
    });
    assert.equal(child.ok, true);
    if (!child.ok) return;
    const childQuestion = await client.askUserQuestion(child.value.adapterSessionId, {
      clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.child-question"),
      draft: {
        title: "Child question",
        introductionMarkdown: "/ZapAskUserQuestion",
        items: [
          {
            questionItemId: QuestionItemIdSchema.parse("question-item.runtime-child"),
            header: "Child",
            promptMarkdown: "Reply to child",
            contextMarkdown: null,
            artifactRefs: [],
            required: true,
            answerMode: "short_text",
            options: [],
            customAnswer: null,
            recommendation: null,
          },
        ],
        independentWorkAvailable: false,
        deadlineAt: null,
      },
    });
    assert.equal(childQuestion.ok, true);
    if (!childQuestion.ok) return;
    const childItem = childQuestion.value.items[0];
    assert.notEqual(childItem, undefined);
    if (childItem === undefined) return;
    const childAnswer = await ui.command({
      operation: "question.answer.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.runtime-agent.child-answer"),
      projectId: childQuestion.value.projectId,
      contextId: childQuestion.value.contextId,
      questionGroupId: childQuestion.value.questionGroupId,
      expectedRevision: childQuestion.value.revision,
      submission: {
        answers: [
          {
            questionItemId: childItem.questionItemId,
            answer: { kind: "short_text", text: "child yes" },
          },
        ],
        noteMarkdown: null,
      },
    });
    assert.equal(childAnswer.ok, true);
    assert.equal(adapter.sent.length, 2);
    const forwarded = adapter.sent[1] ?? "";
    assert.match(forwarded, new RegExp(`Origin actor: ${child.value.connection.actor.actorId}`));
    assert.match(forwarded, /addressed to your preprovisioned broker actor/);
    assert.match(forwarded, /forwarded to the owned coordinator/);
    assert.match(forwarded, /child consumption is not claimed/);
  } finally {
    await opened.value.close();
  }
});

function profile() {
  return {
    profileId: "profile.runtime-agent",
    executablePath: "C:/placeholder/codex.exe",
    requestTimeoutMs: 30_000,
    model: "gpt-5.6-luna",
    effort: "low" as const,
    approvalPolicy: "on-request" as const,
    sandbox: "workspace-write" as const,
    personality: "pragmatic" as const,
    serviceName: "zap-wayfinder",
  };
}

function fakeCoordinator(): CoordinatorAdapter & {
  readonly sent: string[];
  readonly bindings: string[];
} {
  const sent: string[] = [];
  const bindings: string[] = [];
  const unsupported = async () => ({
    ok: false as const,
    error: { code: "unsupported" as const, message: "fixture", retry: "never" as const },
  });
  const capabilities = {
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
  };
  return {
    sent,
    bindings,
    capabilities,
    async start(input) {
      if (input.agentBinding !== undefined && input.agentBinding !== null)
        bindings.push(input.agentBinding.adapterSessionId);
      return {
        ok: true,
        value: CoordinatorSessionDescriptorSchema.parse({
          coordinatorSessionId: input.coordinatorSessionId,
          projectId: input.projectId,
          contextId: input.contextId,
          conversationId: input.conversationId,
          coordinatorActorId: input.coordinatorActorId,
          hostId: input.hostId,
          profileId: input.profileId,
          productId: "codex",
          role: "coordinator",
          launchOrigin: "lens",
          interactionKind: "structured",
          state: "ready",
          nativeThreadRef: {
            namespace: "native.thread",
            value: "thread.runtime-agent",
            incarnation: "1",
          },
          nativeSessionId: "native.runtime-agent",
          cwd: input.cwd,
          processEpoch: "epoch.runtime-agent",
          bootstrap: "submitted",
          instructionSources: [],
          capabilities,
        }),
      };
    },
    resume: unsupported,
    readHistory: unsupported,
    readNativeChildHistory: unsupported,
    startTurn: unsupported,
    steer: unsupported,
    async send(input) {
      sent.push(input.text);
      return {
        ok: true,
        value: {
          coordinatorSessionId: input.coordinatorSessionId,
          nativeThreadId: "thread.runtime-agent",
          nativeTurnId: `turn.${sent.length}`,
          observation: "host_accepted",
          processEpoch: "epoch.runtime-agent",
        },
      };
    },
    interrupt: unsupported,
    respondToRequest: unsupported,
    restart: unsupported,
    subscribe: () => () => undefined,
    close: () => undefined,
  };
}

function project() {
  return {
    registrationId: "request.runtime-agent.register",
    projectId: "project.runtime-agent",
    displayName: "Runtime agent",
    repositoryRootRefs: ["repo.runtime-agent"],
    actions: {},
    context: {
      contextId: "context.runtime-agent",
      displayName: "Runtime agent",
      workspaceRef: "workspace.runtime-agent",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable" as const, reason: "fixture" },
      coordinatorConversationId: "conversation.runtime-agent",
      brokerScope: {
        workspaceId: "workspace.runtime-agent",
        conversationId: "conversation.runtime-agent",
      },
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.runtime-agent",
        label: "Runtime agent",
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: { cwd: process.cwd(), launchProfileRef: "profile.runtime-agent" },
  };
}
