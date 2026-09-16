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
  publicConnection,
  type ActorId,
  type BindingAuth,
  type ConnectInput,
  type Connection,
  type Question,
  type Result,
} from "../protocol/index.ts";
import { createLocalAgentTransport, type TransportBrokerPort } from "../transport/index.ts";
import {
  createCodlensMcpServer,
  type AgentPlanProposalPort,
  type ManagedWorkAgentPort,
} from "./index.ts";

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
  const context = z
    .looseObject({
      actor: ActorDescriptorSchema,
      handle: z.looseObject({ workspaceId: z.string() }),
    })
    .parse(
      parseSuccess(
        await client.callTool({
          name: "codlens_context",
          arguments: { adapterSessionId: parentSession },
        }),
      ),
    );
  assert.equal(context.actor.workspaceId, "workspace.test");
  assert.equal(context.handle.workspaceId, "workspace.test");
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

test("MCP plan tools preserve authenticated adapter session and caller context", async () => {
  const calls: string[] = [];
  const record =
    (name: string): AgentPlanProposalPort["submit"] =>
    async (actor, session) => {
      calls.push(`${name}:${actor.actor.actorId}:${session}`);
      return { ok: true, value: { state: name } };
    };
  const plan: AgentPlanProposalPort = {
    register: record("register"),
    submit: record("submit"),
    preview: record("preview"),
    apply: record("apply"),
    reconcile: record("reconcile"),
    prepare: async (actor, session, kind) => {
      calls.push(`prepare.${kind}:${actor.actor.actorId}:${session}`);
      return { ok: true, value: { state: "prepared" } };
    },
    discover: async (actor, session) => {
      calls.push(`discover:${actor.actor.actorId}:${session}`);
      return { ok: true, value: { state: "discover" } };
    },
    author: record("author"),
    prepareComposite: record("prepare-composite"),
    authorComposite: record("author-composite"),
  };
  const server = createCodlensMcpServer({
    agent: createLocalAgentTransport({
      broker: fakeBroker(),
      principalToken,
      adapterSessionIdFactory: () => "adapter.session.plan.0001",
    }),
    planProposal: plan,
  });
  const client = new Client({ name: "codlens-plan-test", version: "1.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await server.connect(serverTransport);
  await client.connect(clientTransport);
  const connected = publicResultSchema.parse(
    parseSuccess(
      await client.callTool({
        name: "codlens_connect",
        arguments: connectArguments("request.plan"),
      }),
    ),
  );
  for (const [name, key] of [
    ["codlens_plan_intent", "request"],
    ["codlens_plan_proposal", "proposal"],
    ["codlens_plan_preview", "request"],
    ["codlens_plan_apply", "request"],
    ["codlens_plan_reconcile", "request"],
  ] as const) {
    await client.callTool({
      name,
      arguments: { adapterSessionId: connected.adapterSessionId, [key]: {} },
    });
  }
  await client.callTool({
    name: "codlens_plan_prepare",
    arguments: {
      adapterSessionId: connected.adapterSessionId,
      kind: "comparison",
      request: {},
    },
  });
  await client.callTool({
    name: "codlens_plan_discover",
    arguments: { adapterSessionId: connected.adapterSessionId },
  });
  await client.callTool({
    name: "codlens_plan_author",
    arguments: { adapterSessionId: connected.adapterSessionId, request: {} },
  });
  await client.callTool({
    name: "codlens_plan_prepare_composite",
    arguments: { adapterSessionId: connected.adapterSessionId, request: {} },
  });
  await client.callTool({
    name: "codlens_plan_author_composite",
    arguments: { adapterSessionId: connected.adapterSessionId, request: {} },
  });
  assert.deepEqual(
    calls.map((value) => value.split(":", 1)[0]),
    [
      "register",
      "submit",
      "preview",
      "apply",
      "reconcile",
      "prepare.comparison",
      "discover",
      "author",
      "prepare-composite",
      "author-composite",
    ],
  );
  assert.ok(calls.every((value) => value.includes("actor.1:adapter.session.plan.0001")));
  await client.close();
  await server.close();
});

test("official SDK discovers typed managed tools and preserves the assigned session", async () => {
  const calls: { session: string; input: unknown }[] = [];
  const managed: ManagedWorkAgentPort = {
    profiles: async (session) => {
      calls.push({ session, input: null });
      return {
        ok: true,
        value: [{ profileId: "profile.codex.managed", label: "Codex managed" }],
      };
    },
    create: async (session, input) => {
      calls.push({ session, input });
      return { ok: true, value: { runId: "run.managed.child", state: "prepared" } };
    },
    start: async () => ({ ok: true, value: null }),
    read: async () => ({ ok: true, value: null }),
    report: async () => ({ ok: true, value: null }),
    acknowledgeAttachment: async () => ({ ok: true, value: null }),
  };
  const server = createCodlensMcpServer({
    agent: createLocalAgentTransport({
      broker: fakeBroker(),
      principalToken,
      adapterSessionIdFactory: () => "adapter.session.managed.0001",
    }),
    managedWork: managed,
  });
  const client = new Client({ name: "codlens-managed-test", version: "1.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await server.connect(serverTransport);
  await client.connect(clientTransport);
  const connected = publicResultSchema.parse(
    parseSuccess(
      await client.callTool({
        name: "codlens_connect",
        arguments: connectArguments("request.managed"),
      }),
    ),
  );
  const tools = await client.listTools();
  const managedToolNames = tools.tools
    .map((tool) => tool.name)
    .filter((name) => name.startsWith("codlens_managed_work_"));
  assert.deepEqual(managedToolNames, [
    "codlens_managed_work_profiles",
    "codlens_managed_work_create",
    "codlens_managed_work_start",
    "codlens_managed_work_read",
    "codlens_managed_work_report",
    "codlens_managed_work_attachment_ack",
  ]);
  parseSuccess(
    await client.callTool({
      name: "codlens_managed_work_profiles",
      arguments: { adapterSessionId: connected.adapterSessionId },
    }),
  );
  const created = z.object({ runId: z.string(), state: z.literal("prepared") }).parse(
    parseSuccess(
      await client.callTool({
        name: "codlens_managed_work_create",
        arguments: {
          adapterSessionId: connected.adapterSessionId,
          input: {
            clientRequestId: "request.managed.create",
            selection: {
              mode: "profile_override",
              profileId: "profile.codex.managed",
              reasonMarkdown: "Exercise the exact managed MCP fixture profile.",
            },
            goal: "Inspect the explicit work target",
            expectedResult: "A typed report",
            targetRefs: [
              {
                projectId: "project.managed",
                contextId: "context.managed",
                domain: "work_task",
                ref: "task.managed",
              },
            ],
          },
        },
      }),
    ),
  );
  assert.equal(created.runId, "run.managed.child");
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.session === connected.adapterSessionId));
  assert.deepEqual(calls[1]?.input, {
    clientRequestId: "request.managed.create",
    selection: {
      mode: "profile_override",
      profileId: "profile.codex.managed",
      reasonMarkdown: "Exercise the exact managed MCP fixture profile.",
    },
    goal: "Inspect the explicit work target",
    expectedResult: "A typed report",
    targetRefs: [
      {
        projectId: "project.managed",
        contextId: "context.managed",
        domain: "work_task",
        ref: "task.managed",
      },
    ],
    contextRefs: [],
    planRevision: null,
    budgets: { maximumTurns: 64, wallTimeMs: 3_600_000 },
  });
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
    capabilities: [
      "message:emit",
      "question:ask",
      "inbox:read",
      "inbox:ack",
      "actor:delegate",
      "plan:propose",
    ],
    host: { kind: "test", sessionId: "session.test", provenance: "explicit_handle" },
    replyPolicy: { kind: "retain" },
  };
}

function fakeBroker(): TransportBrokerPort {
  let actorNumber = 0;
  let questionNumber = 0;
  const actors = new Map<string, ActorId>();
  const connections = new Map<string, Connection>();
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
        capabilities: [
          "message:emit",
          "question:ask",
          "inbox:read",
          "inbox:ack",
          "actor:delegate",
          "plan:propose",
        ],
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
    connections.set(value.credentials.bindingToken, value);
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
    context: (auth) => {
      const value = connections.get(auth.bindingToken);
      return value === undefined ? unsupported() : { ok: true, value: publicConnection(value) };
    },
    delegate: (auth) => ({ ok: true, value: connection(actor(auth, actors)) }),
    emit: unsupported,
    ask: (auth, input) => ({
      ok: true,
      value: question(actor(auth, actors), input.prompt, ++questionNumber),
    }),
    inbox: unsupported,
    planIntent: unsupported,
    ack: unsupported,
    events: unsupported,
    answer: unsupported,
    listActors: unsupported,
    listQuestions: unsupported,
    emitPrincipal: unsupported,
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
