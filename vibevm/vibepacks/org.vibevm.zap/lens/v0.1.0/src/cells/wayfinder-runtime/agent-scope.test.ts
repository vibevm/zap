/** @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { openBroker, type LensBroker } from "../broker/index.ts";
import { createAgentHttpClient } from "../http/index.ts";
import {
  ClientRequestIdSchema,
  ConnectInputSchema,
  CredentialSchema,
  EnrollPrincipalInputSchema,
} from "../protocol/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import { openWayfinderAgentFoundation } from "./agent.ts";

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
