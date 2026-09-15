import assert from "node:assert/strict";
import test from "node:test";
import {
  ConnectionSchema,
  CredentialSchema,
  EventPageSchema,
  QuestionSchema,
  type Result,
} from "../protocol/index.ts";
import type { TransportBrokerPort } from "../transport/index.ts";
import { createLensHttpGateway } from "./index.ts";

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
    connect: () => ({
      ok: true,
      value: ConnectionSchema.parse({
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
      }),
    }),
    resume: unsupported,
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
    expireActor: unsupported,
    forwardInbox: unsupported,
  };
}
