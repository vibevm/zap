/** No-model public Wayfinder composition gate for ZapMockAgent. @scope spec://org.vibevm.zap/lens/PROP-013#verification */
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import { ClientRequestIdSchema, ConversationIdSchema, DecimalSchema } from "../protocol/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import { createZapMockModel } from "../mock-model/index.ts";
import { createZapMockAgent } from "./index.ts";
import { createWayfinderRuntime } from "../wayfinder-runtime/index.ts";

test("ZapMockAgent drives the public runtime and durable chat without inference", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "zap-mock-runtime-"));
  const profileId = "profile.zap-mock";
  const mockModel = createZapMockModel({
    seed: "runtime-gate",
    scenario: {
      scenarioId: "scenario.runtime",
      steps: [{ kind: "ready" }, { kind: "echo", prefix: "MOCK:" }],
    },
  });
  assert.equal(mockModel.ok, true);
  if (!mockModel.ok) return;
  const agent = createZapMockAgent({ profileId, modelFactory: () => mockModel.value });
  const config = {
    version: 1 as const,
    state: { databasePath: join(directory, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "mockgate",
      pairingToken: randomBytes(24).toString("base64url"),
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://mock.test"],
    },
    profiles: [
      {
        profileId,
        executablePath: "C:\\synthetic\\zap-mock.exe",
        requestTimeoutMs: 5_000,
        model: "zap-mock/deterministic-v1",
        effort: "low" as const,
        approvalPolicy: "on-request" as const,
        sandbox: "workspace-write" as const,
        personality: "pragmatic" as const,
        serviceName: "zap-mock",
      },
    ],
    projects: [project(profileId)],
  };
  const created = createWayfinderRuntime(config, { hosts: [agent.host] });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const runtime = created.value;
  try {
    const started = await runtime.start();
    assert.equal(started.ok, true);
    if (!started.ok) return;
    const ticket = runtime.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://mock.test",
    });
    assert.notEqual(client, null);
    if (client === null) return;
    const projectId = "project.mock.runtime";
    const contextId = "context.mock.runtime";
    const conversationId = ConversationIdSchema.parse("conversation.mock.runtime");
    const session = await client.command({
      operation: "session.start.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.mock.start"),
      projectId: ProjectIdSchema.parse(projectId),
      contextId: WorkContextIdSchema.parse(contextId),
      interactionKind: "structured",
      profileId,
    });
    assert.equal(session.ok, true);
    const chat = await client.command({
      operation: "chat.post.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.mock.chat"),
      projectId: ProjectIdSchema.parse(projectId),
      contextId: WorkContextIdSchema.parse(contextId),
      conversationId,
      bodyMarkdown: "hello",
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    });
    assert.equal(chat.ok, true);
    const page = await client.read({
      operation: "chat.page.v1",
      projectId: ProjectIdSchema.parse(projectId),
      contextId: WorkContextIdSchema.parse(contextId),
      conversationId,
      afterSequence: DecimalSchema.parse("0"),
      limit: 20,
    });
    assert.equal(page.ok, true);
    if (page.ok && page.value.operation === "chat.page.v1")
      assert.equal(
        page.value.page.messages.some((message) => message.bodyMarkdown === "MOCK:hello"),
        true,
      );
  } finally {
    await runtime.close();
    rmSync(directory, { recursive: true, force: true });
  }
});

function project(profileId: string) {
  return {
    registrationId: "request.project.mock.runtime",
    projectId: "project.mock.runtime",
    displayName: "ZapMock runtime",
    repositoryRootRefs: ["repository.mock.runtime"],
    actions: { startCoordinator: { state: "available" as const } },
    context: {
      contextId: "context.mock.runtime",
      displayName: "Mock context",
      workspaceRef: "workspace.mock.runtime",
      branchLabel: "main",
      revisionBinding: "synthetic",
      planning: { state: "unavailable" as const, reason: "mock gate" },
      coordinatorConversationId: "conversation.mock.runtime",
    },
    coordinatorLaunchOptions: [
      {
        profileId,
        label: "ZapMock",
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: { cwd: "C:\\synthetic\\zap-mock", launchProfileRef: profileId },
  };
}
