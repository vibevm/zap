import assert from "node:assert/strict";
import test from "node:test";
import {
  ClientRequestIdSchema,
  ConnectionSchema,
  publicConnection,
  ConversationIdSchema,
  CredentialSchema,
  EventPageSchema,
  QuestionSchema,
  WorkspaceIdSchema,
  type Result,
} from "../protocol/index.ts";
import { openBroker } from "../broker/index.ts";
import { AdapterSessionIdSchema, type TransportBrokerPort } from "../transport/index.ts";
import {
  createAgentHttpClient,
  createLensHttpGateway,
  createPrincipalHttpClient,
} from "./index.ts";

const principalToken = CredentialSchema.parse("principal-token-http-00000000001");
const statusToken = CredentialSchema.parse("status-token-http-0000000000001");

test("HTTP gateway authenticates origin and redacts connection credentials", async () => {
  const gateway = createLensHttpGateway({
    broker: fixtureBroker(),
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: ["http://quicklens.test"],
    statusToken,
    adapterSessionIdFactory: () => "adapter.http.session.00000001",
  });
  const started = await gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const url = `http://127.0.0.1:${started.value.port}`;
  try {
    const denied = await fetch(`${url}/v1/status`, {
      headers: {
        Authorization: `Bearer ${statusToken}`,
        Origin: "http://foreign.test",
      },
    });
    assert.equal(denied.status, 403);

    const connected = await fetch(`${url}/v1/connect`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${principalToken}`,
        "Content-Type": "application/json",
        Origin: "http://quicklens.test",
      },
      body: JSON.stringify({
        clientRequestId: "request.http.connect",
        workspaceId: "workspace.test",
        conversationId: "conversation.test",
        capabilities: ["question:ask", "inbox:read"],
        host: { kind: "test", provenance: "explicit_handle" },
        replyPolicy: { kind: "retain" },
      }),
    });
    assert.equal(connected.status, 200);
    const connectedText = await connected.text();
    assert.equal(connectedText.includes("bindingToken"), false);
    assert.equal(connectedText.includes("resumeCredential"), false);
    const publicSession = PublicSessionResponse.parse(JSON.parse(connectedText));
    const self = await createAgentHttpClient({
      baseUrl: new URL(url),
      principalToken,
    }).context(AdapterSessionIdSchema.parse(publicSession.value.adapterSessionId));
    assert.equal(self.ok && self.value.actor.workspaceId, "workspace.test");

    const handleOnly = await fetch(`${url}/v1/inbox`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${publicSession.value.adapterSessionId}`,
        "Content-Type": "application/json",
      },
      body: "{}",
    });
    assert.equal(handleOnly.status, 401);
    const wrongPrincipal = await fetch(`${url}/v1/inbox`, {
      method: "POST",
      headers: {
        Authorization: "Bearer wrong-principal-token-000000001",
        "X-Codlens-Adapter-Session": publicSession.value.adapterSessionId,
        "Content-Type": "application/json",
      },
      body: "{}",
    });
    assert.equal(wrongPrincipal.status, 401);
    assert.equal((await wrongPrincipal.text()).includes("bindingToken"), false);

    const asked = await Promise.race([
      fetch(`${url}/v1/ask`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${principalToken}`,
          "X-Codlens-Adapter-Session": publicSession.value.adapterSessionId,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          clientRequestId: "request.http.ask",
          prompt: "Answer later",
          answerMode: "free_text",
        }),
      }),
      new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), 100)),
    ]);
    assert.notEqual(asked, undefined);
    if (asked !== undefined) assert.equal(asked.status, 200);
  } finally {
    assert.equal((await gateway.close()).ok, true);
  }
});

test("gateway close aborts an idle authenticated SSE stream", async () => {
  const gateway = createLensHttpGateway({
    broker: fixtureBroker(),
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [],
    statusToken,
    adapterSessionIdFactory: () => "adapter.http.stream.00000001",
    streamPollMilliseconds: 5_000,
  });
  const started = await gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const response = await fetch(
    `http://127.0.0.1:${started.value.port}/v1/events?workspaceId=workspace.test&conversationId=conversation.test`,
    { headers: { Authorization: `Bearer ${principalToken}` } },
  );
  assert.equal(response.status, 200);
  const closed = await Promise.race([
    gateway.close().then((result) => result.ok),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 200)),
  ]);
  assert.equal(closed, true);
});

test("principal HTTP views and notice preserve scope and human authority", async () => {
  const opened = openBroker({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const workspaceId = WorkspaceIdSchema.parse("workspace.principal-http");
  const conversationId = ConversationIdSchema.parse("conversation.principal-http");
  const agent = opened.value.enrollPrincipal({
    kind: "agent",
    workspaceIds: [workspaceId],
    conversationIds: [conversationId],
    capabilities: ["message:emit", "question:ask", "inbox:read", "plan:propose"],
  });
  const human = opened.value.enrollPrincipal({
    kind: "human_responder",
    workspaceIds: [workspaceId],
    conversationIds: [conversationId],
    capabilities: ["message:emit", "question:answer", "question:amend", "events:read"],
  });
  assert.ok(agent.ok && human.ok);
  if (!agent.ok || !human.ok) return;
  const connected = opened.value.connect({
    principalToken: agent.value.principalToken,
    clientRequestId: ClientRequestIdSchema.parse("request.principal-http.connect"),
    workspaceId,
    conversationId,
    capabilities: ["message:emit", "question:ask", "inbox:read", "plan:propose"],
    host: { kind: "test", sessionId: "principal-http", provenance: "attested" },
    replyPolicy: { kind: "retain" },
  });
  assert.equal(connected.ok, true);
  if (!connected.ok) return;
  opened.value.ask(
    {
      principalToken: agent.value.principalToken,
      bindingToken: connected.value.credentials.bindingToken,
    },
    {
      clientRequestId: ClientRequestIdSchema.parse("request.principal-http.ask"),
      prompt: "HTTP answer?",
      answerMode: "free_text",
    },
  );
  const gateway = createLensHttpGateway({
    broker: opened.value,
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [],
    statusToken,
    adapterSessionIdFactory: () => "adapter.principal.http.00001",
  });
  const started = await gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const client = createPrincipalHttpClient({
    baseUrl: new URL(`http://127.0.0.1:${String(started.value.port)}`),
    principalToken: human.value.principalToken,
  });
  const agentClient = createAgentHttpClient({
    baseUrl: new URL(`http://127.0.0.1:${String(started.value.port)}`),
    principalToken: agent.value.principalToken,
  });
  const httpActor = await agentClient.connect({
    clientRequestId: ClientRequestIdSchema.parse("request.principal-http.http-connect"),
    workspaceId,
    conversationId,
    capabilities: ["message:emit", "inbox:read", "plan:propose"],
    host: { kind: "test", sessionId: "principal-http-remote", provenance: "explicit_handle" },
    replyPolicy: { kind: "retain" },
  });
  assert.equal(httpActor.ok, true);
  if (!httpActor.ok) return;
  const scope = { workspaceId, conversationId, limit: 10 };
  const actors = await client.listActors(scope);
  assert.equal(actors.ok && actors.value.actors[0]?.eligiblePlanTarget, true);
  const questions = await client.listQuestions(scope);
  assert.equal(questions.ok && questions.value.questions.length, 1);
  const notice = await client.emit({
    clientRequestId: ClientRequestIdSchema.parse("request.principal-http.notice"),
    workspaceId,
    conversationId,
    toActorId: httpActor.value.connection.actor.actorId,
    payload: { type: "plan_intent", intentRef: "intent.http", text: "Revise plan", basis: {} },
  });
  assert.equal(notice.ok, true);
  if (notice.ok) {
    const proof = await agentClient.planIntent(
      httpActor.value.adapterSessionId,
      notice.value.messageId,
    );
    assert.equal(proof.ok && proof.value.toActorId, httpActor.value.connection.actor.actorId);
  }
  const foreign = await client.listActors({
    workspaceId: WorkspaceIdSchema.parse("workspace.foreign"),
    conversationId,
    limit: 10,
  });
  assert.equal(foreign.ok, false);
  if (!foreign.ok) assert.equal(foreign.error.code, "forbidden");
  await gateway.close();
  opened.value.close();
});

import { z } from "zod";

const PublicSessionResponse = z.object({
  protocol: z.literal("lens/1"),
  ok: z.literal(true),
  value: z.object({ adapterSessionId: z.string() }).passthrough(),
});

function fixtureBroker(): TransportBrokerPort {
  const unsupported = (): Result<never> => ({
    ok: false,
    error: {
      code: "unsupported_operation",
      message:
        "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: operation is outside this fixture; fix surface: call connect or ask",
    },
  });
  return {
    connect: () => ({ ok: true, value: httpConnection() }),
    resume: unsupported,
    context: () => ({ ok: true, value: publicConnection(httpConnection()) }),
    delegate: unsupported,
    emit: unsupported,
    ask: (_auth, input) => ({
      ok: true,
      value: QuestionSchema.parse({
        questionId: "question.http",
        workspaceId: "workspace.test",
        conversationId: "conversation.test",
        originActorId: "actor.http",
        prompt: input.prompt,
        answerMode: input.answerMode,
        choices: input.choices ?? [],
        independentWorkAvailable: input.independentWorkAvailable ?? true,
        replyPolicy: { kind: "retain" },
        state: "open",
        revision: "1",
        deadlineAt: input.deadlineAt ?? null,
        answer: null,
      }),
    }),
    inbox: unsupported,
    planIntent: unsupported,
    ack: unsupported,
    events: () => ({
      ok: true,
      value: EventPageSchema.parse({
        events: [],
        observationCursor: "0",
        hasMore: false,
      }),
    }),
    answer: unsupported,
    listActors: unsupported,
    listQuestions: unsupported,
    emitPrincipal: unsupported,
    expireActor: unsupported,
    forwardInbox: unsupported,
  };
}

function httpConnection() {
  return ConnectionSchema.parse({
    actor: {
      principalId: "principal.test",
      actorId: "actor.http",
      workspaceId: "workspace.test",
      conversationId: "conversation.test",
      parentActorId: null,
      state: "active",
      capabilities: ["question:ask", "inbox:read"],
      hostKind: "test",
      hostProvenance: "attested",
    },
    handle: {
      actorId: "actor.http",
      bindingId: "binding.http",
      workspaceId: "workspace.test",
      conversationId: "conversation.test",
      generation: "1",
    },
    credentials: {
      bindingToken: "binding-token-http-000000000001",
      resumeCredential: "resume-token-http-0000000000001",
    },
  });
}
