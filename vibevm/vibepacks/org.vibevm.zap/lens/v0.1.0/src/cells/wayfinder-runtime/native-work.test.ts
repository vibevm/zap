/** @scope spec://org.vibevm.zap/lens/PROP-011#deferred-instructions */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { z } from "zod";
import { createAgentHttpClient, createNativeWorkHttpClient } from "../http/index.ts";
import { createCodlensMcpServer } from "../mcp/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import {
  ClientRequestIdSchema,
  ActorIdSchema,
  ConversationIdSchema,
  CredentialSchema,
  PrincipalIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import {
  createAnnotationWorkAttachmentPort,
  openAnnotationStore,
} from "../workspace-annotations/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { openWayfinderAgentFoundation } from "./agent.ts";

test("native MCP calls real HTTP declared-target annotation prepare, read and ack", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-native-work-"));
  const workspace = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(workspace.ok, true);
  if (!workspace.ok) return;
  const projectId = ProjectIdSchema.parse("project.native-work");
  const contextId = WorkContextIdSchema.parse("context.native-work");
  const workspaceId = WorkspaceIdSchema.parse("workspace.native-work");
  const conversationId = ConversationIdSchema.parse("conversation.native-work");
  assert.equal(workspace.value.registerProject(registration()).ok, true);
  const foundation = openWayfinderAgentFoundation(
    {
      databasePath: join(root, "broker.sqlite"),
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.native-work.synthetic.0001",
      scopes: [],
    },
    workspace.value,
  );
  assert.equal(foundation.ok, true);
  if (!foundation.ok) return;
  const annotations = openAnnotationStore({
    databasePath: join(root, "annotations.sqlite"),
    idFactory: (kind) => `${kind}.native-test`,
  });
  assert.equal(annotations.ok, true);
  if (!annotations.ok) return;
  try {
    const started = await foundation.value.start();
    assert.equal(started.ok, true);
    if (!started.ok) return;
    const ensured = await foundation.value.ensureScope({
      projectId,
      contextId,
      workspaceId,
      conversationId,
    });
    assert.equal(ensured.ok, true);
    if (!ensured.ok) return;
    const binding = await foundation.value.ownedCoordinators.bind({
      projectId,
      contextId,
      coordinatorSessionId: AgentSessionIdSchema.parse("agent-session.native-work"),
      coordinatorActorId: ActorIdSchema.parse("actor.native-work.coordinator"),
      workspaceId,
      conversationId,
    });
    assert.equal(binding.ok, true);
    if (!binding.ok) return;
    const target = ProjectObjectReferenceSchema.parse({
      projectId,
      contextId,
      domain: "semantic_object",
      ref: "work.native.target",
    });
    const annotationAccess = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.native-annotation"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.native-annotation"),
      authorizedProjectIds: [projectId],
    });
    const created = annotations.value.command(annotationAccess, {
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.native-annotation.create"),
      projectId,
      contextId,
      target,
      kind: "deferred",
      title: "Native exact target",
      bodyMarkdown: "Read this instruction before touching the declared object.",
      sourceBasisRef: "basis.native-work.1",
      targetSnapshot: {
        basisRef: "basis.native-work.1",
        capturedAt: "2026-09-16T00:00:00.000Z",
        value: { title: "Native target" },
      },
    });
    assert.equal(created.ok, true);
    const attachments = createAnnotationWorkAttachmentPort({
      store: annotations.value,
      resolver: {
        resolve: () =>
          Promise.resolve({
            state: "present" as const,
            snapshot: {
              basisRef: "basis.native-work.1",
              capturedAt: "2026-09-16T00:00:00.000Z",
              value: { title: "Native target" },
            },
          }),
      },
      idFactory: (kind) => `${kind}.native-test`,
      clock: () => new Date("2026-09-16T00:01:00.000Z"),
    });
    assert.equal(foundation.value.bindNativeWork(attachments).ok, true);
    const principalToken = readCredential(ensured.value.agentCredentialFile);
    const baseUrl = new URL(ensured.value.brokerUrl);
    const agent = createAgentHttpClient({ baseUrl, principalToken });
    const server = createCodlensMcpServer({
      agent,
      assignedSession: AdapterSessionIdSchema.parse(binding.value.adapterSessionId),
      nativeWork: createNativeWorkHttpClient({ baseUrl, principalToken }),
    });
    const client = new Client({ name: "native-work-test", version: "1.0.0" });
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    await client.connect(clientTransport);
    const assigned = z
      .object({ adapterSessionId: AdapterSessionIdSchema })
      .parse(
        parseSuccess(await client.callTool({ name: "codlens_assigned_context", arguments: {} })),
      );
    assert.equal(assigned.adapterSessionId, binding.value.adapterSessionId);
    const undeclared = z
      .object({ state: z.literal("waiting_for_target"), instructions: z.array(z.never()) })
      .parse(
        parseSuccess(
          await client.callTool({
            name: "codlens_native_work_before",
            arguments: {
              adapterSessionId: binding.value.adapterSessionId,
              input: {
                attemptId: "attempt.native-work.undeclared",
                targetRefs: [],
                sourceBasisRef: "basis.native-work.1",
                planRevision: "1",
              },
            },
          }),
        ),
      );
    assert.equal(undeclared.instructions.length, 0);
    const attemptId = AttemptIdSchema.parse("attempt.native-work.exact");
    const prepared = parseSuccess(
      await client.callTool({
        name: "codlens_native_work_before",
        arguments: {
          adapterSessionId: binding.value.adapterSessionId,
          input: {
            attemptId,
            targetRefs: [target],
            sourceBasisRef: "basis.native-work.1",
            planRevision: "1",
          },
        },
      }),
    );
    const receipt = z
      .object({
        attemptId: AttemptIdSchema,
        state: z.literal("ready"),
        instructions: z.array(
          z.object({ attachmentId: z.string(), version: z.string(), bodyMarkdown: z.string() }),
        ),
      })
      .parse(prepared);
    assert.equal(receipt.instructions.length, 1);
    const read = parseSuccess(
      await client.callTool({
        name: "codlens_native_work_read",
        arguments: { adapterSessionId: binding.value.adapterSessionId, input: { attemptId } },
      }),
    );
    assert.deepEqual(read, receipt);
    const instruction = receipt.instructions[0];
    assert.ok(instruction !== undefined);
    if (instruction === undefined) return;
    parseSuccess(
      await client.callTool({
        name: "codlens_native_work_attachment_ack",
        arguments: {
          adapterSessionId: binding.value.adapterSessionId,
          input: {
            attemptId,
            attachmentId: instruction.attachmentId,
            version: instruction.version,
          },
        },
      }),
    );
    await client.close();
    await server.close();
  } finally {
    annotations.value.close();
    await foundation.value.close();
    workspace.value.close();
    await rm(root, { recursive: true, force: true });
  }
});

function readCredential(path: string) {
  const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
  return CredentialSchema.parse(
    z.object({ protocol: z.literal("lens/1"), principalToken: CredentialSchema }).parse(raw)
      .principalToken,
  );
}

function parseSuccess(result: Awaited<ReturnType<Client["callTool"]>>): unknown {
  return z
    .object({ protocol: z.literal("lens/1"), ok: z.literal(true), value: z.unknown() })
    .parse(result.structuredContent).value;
}

function registration() {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: "request.native-work.register",
    projectId: "project.native-work",
    displayName: "Native work",
    repositoryRootRefs: ["repository.native-work"],
    actions: {},
    context: {
      contextId: "context.native-work",
      displayName: "Native work",
      workspaceRef: "workspace.native-work",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable", reason: "fixture" },
      coordinatorConversationId: "conversation.native-work",
      brokerScope: {
        workspaceId: "workspace.native-work",
        conversationId: "conversation.native-work",
      },
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.native-work",
        label: "Native work",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: process.cwd(), launchProfileRef: "profile.native-work" },
  });
}
