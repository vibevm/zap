/** @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { z } from "zod";
import { openBroker, type LensBroker } from "../broker/index.ts";
import { createAgentHttpClient, createPrincipalHttpClient } from "../http/index.ts";
import {
  ClientRequestIdSchema,
  ConnectInputSchema,
  CredentialSchema,
  EnrollPrincipalInputSchema,
  AnswerQuestionInputSchema,
  ActorIdSchema,
} from "../protocol/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { AgentSessionIdSchema } from "../workspace-model/index.ts";
import { openWayfinderAgentFoundation } from "./agent.ts";
import { WayfinderAgentScopeInputSchema } from "./agent-scope.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";

test("one gateway keeps two project credentials and adapter sessions in exact scopes", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-two-scopes-"));
  const brokerPath = join(root, "broker.sqlite");
  const seeded = openBroker({ databasePath: brokerPath });
  assert.equal(seeded.ok, true);
  if (!seeded.ok) return;
  const a = enroll(seeded.value, "a");
  const b = enroll(seeded.value, "b");
  const humanA = enrollHuman(seeded.value, "a");
  const humanB = enrollHuman(seeded.value, "b");
  seeded.value.close();
  assert.ok(a.ok && b.ok && humanA.ok && humanB.ok);
  if (!a.ok || !b.ok || !humanA.ok || !humanB.ok) return;
  const workspace = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(workspace.ok, true);
  if (!workspace.ok) return;
  for (const suffix of ["a", "b"] as const) {
    assert.equal(
      workspace.value.registerProject(TrustedProjectRegistrationSchema.parse(project(suffix))).ok,
      true,
    );
  }
  const opened = openWayfinderAgentFoundation(
    {
      databasePath: brokerPath,
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: CredentialSchema.parse("status-two-scope-fixture-000001"),
      scopes: [
        scope("a", humanA.value.principalToken, a.value.principalToken),
        scope("b", humanB.value.principalToken, b.value.principalToken),
      ],
    },
    workspace.value,
  );
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  try {
    const address = await opened.value.start();
    assert.equal(address.ok, true);
    if (!address.ok) return;
    const baseUrl = new URL(`http://${address.value.host}:${String(address.value.port)}`);
    const clientA = createAgentHttpClient({ baseUrl, principalToken: a.value.principalToken });
    const clientB = createAgentHttpClient({ baseUrl, principalToken: b.value.principalToken });
    const connectedA = await clientA.connect(connection("a", "request.scope.a"));
    const cross = await clientA.connect(connection("b", "request.scope.cross"));
    const connectedB = await clientB.connect(connection("b", "request.scope.b"));
    assert.ok(connectedA.ok && connectedB.ok);
    assert.equal(cross.ok, false);
    if (connectedA.ok && connectedB.ok) {
      assert.equal(connectedA.value.connection.actor.workspaceId, "workspace.scope-a");
      assert.equal(connectedB.value.connection.actor.workspaceId, "workspace.scope-b");
      assert.notEqual(connectedA.value.adapterSessionId, connectedB.value.adapterSessionId);
    }
  } finally {
    await opened.value.close();
    workspace.value.close();
  }
});

test("empty gateway lazily persists two isolated project scopes and reuses them after restart", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-dynamic-scopes-"));
  const brokerPath = join(root, "broker.sqlite");
  const workspace = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(workspace.ok, true);
  if (!workspace.ok) return;
  for (const suffix of ["a", "b"] as const)
    assert.equal(
      workspace.value.registerProject(TrustedProjectRegistrationSchema.parse(project(suffix))).ok,
      true,
    );
  const config = {
    databasePath: brokerPath,
    host: "127.0.0.1" as const,
    port: 0,
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [],
    statusToken: CredentialSchema.parse("status-dynamic-scope-fixture-0001"),
    scopes: [],
  };
  const opened = openWayfinderAgentFoundation(config, workspace.value);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const inputA = WayfinderAgentScopeInputSchema.parse({
    projectId: "project.scope-a",
    contextId: "context.scope-a",
    workspaceId: "workspace.scope-a",
    conversationId: "conversation.scope-a",
  });
  const inputB = WayfinderAgentScopeInputSchema.parse({
    projectId: "project.scope-b",
    contextId: "context.scope-b",
    workspaceId: "workspace.scope-b",
    conversationId: "conversation.scope-b",
  });
  assert.equal((await opened.value.ensureScope(inputA)).ok, false);
  const started = await opened.value.start();
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const [scopeA, scopeB] = await Promise.all([
    opened.value.ensureScope(inputA),
    opened.value.ensureScope(inputB),
  ]);
  assert.ok(scopeA.ok && scopeB.ok);
  if (!scopeA.ok || !scopeB.ok) return;
  assert.notEqual(scopeA.value.agentCredentialFile, scopeB.value.agentCredentialFile);
  const mismatch = await opened.value.ensureScope(
    WayfinderAgentScopeInputSchema.parse({
      ...inputA,
      workspaceId: inputB.workspaceId,
      conversationId: inputB.conversationId,
    }),
  );
  assert.equal(mismatch.ok, false);
  const agentToken = credential(scopeA.value.agentCredentialFile);
  const humanToken = credential(scopeA.value.humanCredentialFile);
  const baseUrl = new URL(scopeA.value.brokerUrl);
  const agent = createAgentHttpClient({ baseUrl, principalToken: agentToken });
  const connected = await agent.connect(connection("a", "request.dynamic.scope.a"));
  assert.equal(connected.ok, true);
  if (!connected.ok) return;
  const question = await agent.ask(connected.value.adapterSessionId, {
    clientRequestId: "request.dynamic.question",
    prompt: "Choose a bounded answer",
    answerMode: "free_text",
    choices: [],
    independentWorkAvailable: false,
  });
  assert.equal(question.ok, true);
  if (!question.ok) return;
  const human = createPrincipalHttpClient({ baseUrl, principalToken: humanToken });
  const answered = await human.answer(
    AnswerQuestionInputSchema.parse({
      clientRequestId: "request.dynamic.answer",
      workspaceId: "workspace.scope-a",
      conversationId: "conversation.scope-a",
      questionId: question.value.questionId,
      expectedRevision: question.value.revision,
      answer: "accepted",
    }),
  );
  assert.equal(answered.ok && answered.value.state, "answered");
  const coordinatorSessionId = AgentSessionIdSchema.parse("agent-session.scope-a.provider");
  const binding = await opened.value.ownedCoordinators.bind({
    projectId: inputA.projectId,
    contextId: inputA.contextId,
    coordinatorSessionId,
    coordinatorActorId: ActorIdSchema.parse("actor.scope-a.provider"),
    workspaceId: inputA.workspaceId,
    conversationId: inputA.conversationId,
  });
  assert.equal(binding.ok, true);
  if (!binding.ok) return;
  const routed = opened.value.ownedCoordinators.route(binding.value.actorId);
  assert.equal(routed.ok, true);
  if (routed.ok) assert.equal(routed.value?.coordinatorActorId, "actor.scope-a.provider");
  for (const provider of ["claude_code", "opencode", "qwen_code"] as const) {
    const prepared = await opened.value.prepareOwnedCoordinatorLaunch({
      provider,
      projectId: inputA.projectId,
      contextId: inputA.contextId,
      coordinatorSessionId,
      actorId: binding.value.actorId,
      adapterSessionId: AdapterSessionIdSchema.parse(binding.value.adapterSessionId),
      mcpConfigPath: join(root, `${provider}.mcp`),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) continue;
    assert.equal(
      prepared.value.environment["CODLENS_ADAPTER_SESSION_ID"],
      binding.value.adapterSessionId,
    );
    const configText: string = readFileSync(prepared.value.mcpConfigPath, "utf8");
    assert.match(configText, /zap-wayfinder/);
    const configJson: unknown = JSON.parse(configText);
    if (provider === "opencode") {
      const openCodeConfig = z
        .object({
          mcp: z.object({
            "zap-wayfinder": z.object({
              type: z.literal("local"),
              command: z.array(z.string()).min(1),
              environment: z.record(z.string(), z.string()),
              enabled: z.literal(true),
            }),
          }),
          permission: z.record(z.string(), z.enum(["allow", "ask"])),
        })
        .parse(configJson);
      const local = openCodeConfig.mcp["zap-wayfinder"];
      assert.equal(local.environment["CODLENS_ADAPTER_SESSION_ID"], binding.value.adapterSessionId);
      assert.equal(openCodeConfig.permission["zap-wayfinder_codlens_inbox_wait"], "allow");
      assert.equal(openCodeConfig.permission["zap_wayfinder_codlens_inbox_wait"], "allow");
      assert.equal(openCodeConfig.permission["zap-wayfinder_codlens_plan_apply"], undefined);
      assert.equal(openCodeConfig.permission["*"], "ask");
    } else {
      z.object({
        mcpServers: z.object({ "zap-wayfinder": z.object({ command: z.string() }) }),
      }).parse(configJson);
    }
    assert.equal(configText.includes(agentToken), false);
  }
  const crossed = await opened.value.prepareOwnedCoordinatorLaunch({
    provider: "claude_code",
    projectId: inputB.projectId,
    contextId: inputB.contextId,
    coordinatorSessionId,
    actorId: binding.value.actorId,
    adapterSessionId: AdapterSessionIdSchema.parse(binding.value.adapterSessionId),
    mcpConfigPath: join(root, "crossed.mcp"),
  });
  assert.equal(crossed.ok, false);
  await opened.value.close();

  const reopened = openWayfinderAgentFoundation(config, workspace.value);
  assert.equal(reopened.ok, true);
  if (!reopened.ok) return;
  try {
    const restarted = await reopened.value.start();
    assert.equal(restarted.ok, true);
    const reused = await reopened.value.ensureScope(inputA);
    assert.equal(reused.ok, true);
    if (reused.ok) assert.equal(reused.value.agentCredentialFile, scopeA.value.agentCredentialFile);
  } finally {
    await reopened.value.close();
    workspace.value.close();
  }
});

function credential(path: string) {
  const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
  return CredentialSchema.parse(
    z.object({ protocol: z.literal("lens/1"), principalToken: CredentialSchema }).parse(raw)
      .principalToken,
  );
}

function enroll(broker: LensBroker, suffix: string) {
  return broker.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: [`workspace.scope-${suffix}`],
      conversationIds: [`conversation.scope-${suffix}`],
      capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "plan:propose"],
    }),
  );
}

function enrollHuman(broker: Parameters<typeof enroll>[0], suffix: string) {
  return broker.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "human_responder",
      workspaceIds: [`workspace.scope-${suffix}`],
      conversationIds: [`conversation.scope-${suffix}`],
      capabilities: ["message:emit"],
    }),
  );
}

function scope(suffix: string, humanPrincipalToken: string, agentPrincipalToken: string) {
  return {
    workspaceId: `workspace.scope-${suffix}`,
    conversationId: `conversation.scope-${suffix}`,
    humanPrincipalToken,
    agentPrincipalToken,
  };
}

function connection(suffix: string, request: string) {
  return ConnectInputSchema.omit({ principalToken: true }).parse({
    clientRequestId: ClientRequestIdSchema.parse(request),
    workspaceId: `workspace.scope-${suffix}`,
    conversationId: `conversation.scope-${suffix}`,
    capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "plan:propose"],
    host: {
      kind: "test" as const,
      sessionId: `scope-${suffix}`,
      provenance: "explicit_handle" as const,
    },
    replyPolicy: { kind: "retain" as const },
  });
}

function project(suffix: string) {
  return {
    registrationId: `request.scope.${suffix}.register`,
    projectId: `project.scope-${suffix}`,
    displayName: `Scope ${suffix}`,
    repositoryRootRefs: [`repo.scope-${suffix}`],
    actions: {},
    context: {
      contextId: `context.scope-${suffix}`,
      displayName: `Scope ${suffix}`,
      workspaceRef: `workspace.scope-${suffix}`,
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable" as const, reason: "fixture" },
      coordinatorConversationId: `conversation.scope-${suffix}`,
      brokerScope: {
        workspaceId: `workspace.scope-${suffix}`,
        conversationId: `conversation.scope-${suffix}`,
      },
    },
    coordinatorLaunchOptions: [
      {
        profileId: `profile.scope-${suffix}`,
        label: `Scope ${suffix}`,
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: { cwd: process.cwd(), launchProfileRef: `profile.scope-${suffix}` },
  };
}
