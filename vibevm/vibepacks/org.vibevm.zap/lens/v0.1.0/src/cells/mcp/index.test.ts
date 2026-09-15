import assert from "node:assert/strict";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { z } from "zod";
import {
  ActorDescriptorSchema,
  ConnectionSchema,
  CredentialSchema,
  QuestionSchema,
  type ActorId,
  type BindingAuth,
  type ConnectInput,
  type Connection,
  type Question,
  type Result,
} from "../protocol/index.ts";
import { createLocalAgentTransport, type TransportBrokerPort } from "../transport/index.ts";
import { createCodlensMcpServer } from "./index.ts";

const principalToken = CredentialSchema.parse("principal-token-0000000000000001");

test("official SDK round trip keeps concurrent child credentials private", async () => {
  const broker = fakeBroker();
  let sessionNumber = 0;
  const server = createCodlensMcpServer({
    agent: createLocalAgentTransport({
      broker,
      principalToken,
      adapterSessionIdFactory: () => `adapter.session.${String(++sessionNumber).padStart(12, "0")}`,
    }),
  });
  const client = new Client({ name: "codlens-test", version: "1.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await server.connect(serverTransport);
  await client.connect(clientTransport);

  const parent = parseSuccess(
    await client.callTool({
      name: "codlens_connect",
      arguments: connectArguments("request.parent"),
    }),
  );
  const parentSession = publicResultSchema.parse(parent).adapterSessionId;
  const children = await Promise.all(
    ["a", "b", "c"].map((suffix) =>
      client.callTool({
        name: "codlens_delegate",
        arguments: {
          adapterSessionId: parentSession,
          input: {
            clientRequestId: `request.delegate.${suffix}`,
            capabilities: ["question:ask", "inbox:read"],
            host: { kind: "test", subagentId: suffix, provenance: "explicit_handle" },
            replyPolicy: { kind: "forward_parent" },
          },
        },
      }),
    ),
  );
  const childSessions = children.map(
    (result) => publicResultSchema.parse(parseSuccess(result)).adapterSessionId,
  );
  const questions = await Promise.all(
    childSessions.map((adapterSessionId, index) =>
      client.callTool({
        name: "codlens_ask",
        arguments: {
          adapterSessionId,
          input: {
            clientRequestId: `request.ask.${index}`,
            prompt: `Question ${index}?`,
            answerMode: "free_text",
            choices: [],
            independentWorkAvailable: true,
          },
        },
      }),
    ),
  );
  const origins = questions.map(
    (result) => QuestionSchema.parse(parseSuccess(result)).originActorId,
  );
  assert.equal(new Set(origins).size, 3);
  const serialized = JSON.stringify(children);
  assert.equal(serialized.includes("bindingToken"), false);
  assert.equal(serialized.includes("resumeCredential"), false);

  await client.close();
  await server.close();
});

test("codlens_ask resolves without keeping a human wait in the MCP call", async () => {
  const server = createCodlensMcpServer({
    agent: createLocalAgentTransport({
      broker: fakeBroker(),
      principalToken,
      adapterSessionIdFactory: () => "adapter.session.nonblocking.0001",
    }),
  });
  const client = new Client({ name: "codlens-test", version: "1.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await server.connect(serverTransport);
  await client.connect(clientTransport);
  const connected = publicResultSchema.parse(
    parseSuccess(
      await client.callTool({
        name: "codlens_connect",
        arguments: connectArguments("request.nonblocking"),
      }),
    ),
  );
  const completed = await Promise.race([
    client
      .callTool({
        name: "codlens_ask",
        arguments: {
          adapterSessionId: connected.adapterSessionId,
          input: {
            clientRequestId: "request.ask.nonblocking",
            prompt: "Choose later",
            answerMode: "free_text",
          },
        },
      })
      .then(() => true),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 100)),
  ]);
  assert.equal(completed, true);
  await client.close();
  await server.close();
});

test("model-facing connect cannot self-assert attested host provenance", async () => {
  const server = createCodlensMcpServer({
    agent: createLocalAgentTransport({
      broker: fakeBroker(),
      principalToken,
      adapterSessionIdFactory: () => "adapter.session.untrusted.0001",
    }),
  });
  const client = new Client({ name: "codlens-test", version: "1.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await server.connect(serverTransport);
  await client.connect(clientTransport);
  const attempted = connectArguments("request.attested");
  const rejected = await client.callTool({
    name: "codlens_connect",
    arguments: {
      ...attempted,
      host: { ...attempted.host, provenance: "attested" },
    },
  });
  assert.equal(rejected.isError, true);
  await client.close();
  await server.close();
});

const publicResultSchema = z.object({
  adapterSessionId: z.string(),
  connection: z.object({
    actor: ActorDescriptorSchema,
    handle: z.object({
      actorId: z.string(),
      bindingId: z.string(),
      workspaceId: z.string(),
      conversationId: z.string(),
      generation: z.string(),
    }),
  }),
});

function parseSuccess(result: Awaited<ReturnType<Client["callTool"]>>): unknown {
  const envelope = z
    .object({
      protocol: z.literal("lens/1"),
      ok: z.literal(true),
      value: z.unknown(),
    })
    .parse(result.structuredContent);
  return envelope.value;
}

function connectArguments(clientRequestId: string): Omit<ConnectInput, "principalToken"> {
  return {
    clientRequestId,
    workspaceId: "workspace.test",
    conversationId: "conversation.test",
    capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "actor:delegate"],
    host: { kind: "test", sessionId: "session.test", provenance: "explicit_handle" },
    replyPolicy: { kind: "retain" },
  };
}

function fakeBroker(): TransportBrokerPort {
  let actorNumber = 0;
  let questionNumber = 0;
  const actors = new Map<string, ActorId>();
  const connection = (parentActorId: ActorId | null): Connection => {
    const number = ++actorNumber;
    const actorId = `actor.${number}`;
    const value = ConnectionSchema.parse({
      actor: {
        principalId: "principal.test",
        actorId,
        workspaceId: "workspace.test",
        conversationId: "conversation.test",
        parentActorId,
        state: "active",
        capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "actor:delegate"],
        hostKind: "test",
        hostProvenance: "attested",
      },
      handle: {
        actorId,
        bindingId: `binding.${number}`,
        workspaceId: "workspace.test",
        conversationId: "conversation.test",
        generation: "1",
      },
      credentials: {
        bindingToken: `binding-token-${String(number).padStart(20, "0")}`,
        resumeCredential: `resume-token-${String(number).padStart(21, "0")}`,
      },
    });
    actors.set(value.credentials.bindingToken, value.actor.actorId);
    return value;
  };
  const unsupported = (): Result<never> => ({
    ok: false,
    error: {
      code: "unsupported_operation",
      message:
        "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: unused fake operation; fix surface: call a covered operation",
    },
  });
  return {
    connect: () => ({ ok: true, value: connection(null) }),
    resume: unsupported,
    delegate: (auth) => ({ ok: true, value: connection(actor(auth, actors)) }),
    emit: unsupported,
    ask: (auth, input) => ({
      ok: true,
      value: question(actor(auth, actors), input.prompt, ++questionNumber),
    }),
    inbox: unsupported,
    ack: unsupported,
    events: unsupported,
    answer: unsupported,
    expireActor: unsupported,
    forwardInbox: unsupported,
  };
}

function actor(auth: BindingAuth, actors: Map<string, ActorId>): ActorId {
  return ActorDescriptorSchema.shape.actorId.parse(actors.get(auth.bindingToken));
}

function question(originActorId: ActorId, prompt: string, number: number): Question {
  return QuestionSchema.parse({
    questionId: `question.${number}`,
    workspaceId: "workspace.test",
    conversationId: "conversation.test",
    originActorId,
    prompt,
    answerMode: "free_text",
    choices: [],
    independentWorkAvailable: true,
    replyPolicy: { kind: "retain" },
    state: "open",
    revision: "1",
    deadlineAt: null,
    answer: null,
  });
}
