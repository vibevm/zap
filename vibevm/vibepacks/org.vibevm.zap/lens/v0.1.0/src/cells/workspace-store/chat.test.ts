/** Durable chat correlation and restart proof. @scope spec://org.vibevm.zap/lens/PROP-012#delivery */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  ChatDispatchClaimSchema,
  ObservedChatReplySchema,
  TrustedProjectRegistrationSchema,
  openWorkspaceStore,
} from "./index.ts";

test("transport-correlated replies resolve durable chat after reopen and deduplicate", () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "lens-chat-"));
  const databasePath = join(directory, "workspace.sqlite");
  const now = () => new Date("2026-09-16T10:00:00.000Z");
  const registration = TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.chat-correlation",
    projectId: "project.chat-correlation",
    displayName: "Chat correlation",
    repositoryRootRefs: ["repository.chat-correlation"],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: "context.chat-correlation",
      displayName: "Chat context",
      workspaceRef: "workspace.chat-correlation",
      branchLabel: "main",
      revisionBinding: "revision.chat-correlation",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: "conversation.chat-correlation",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.chat-correlation",
        label: "Chat correlation profile",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: {
      cwd: "C:\\fixtures\\chat-correlation",
      launchProfileRef: "profile.chat-correlation",
    },
  });
  const access = {
    principalId: PrincipalIdSchema.parse("principal.chat-correlation"),
    clientId: ClientIdSchema.parse("client.chat-correlation"),
    actorId: ActorIdSchema.parse("actor.chat-correlation"),
    authorizedProjectIds: [ProjectIdSchema.parse(registration.projectId)],
  };
  const correlation = {
    provenance: "transport_correlation" as const,
    clientMessageId: "message.chat-correlation",
    processEpoch: "epoch.chat-correlation",
  };
  const projectId = ProjectIdSchema.parse(registration.projectId);
  const contextId = WorkContextIdSchema.parse(registration.context.contextId);
  const sessionId = AgentSessionIdSchema.parse("session.chat-correlation");
  const open = () => openWorkspaceStore({ databasePath, clock: now });
  try {
    const first = open();
    assert.equal(first.ok, true);
    if (!first.ok) return;
    const store = first.value;
    assert.equal(store.registerProject(registration).ok, true);
    const posted = store.command(
      access,
      WorkspaceCommandRequestSchema.parse({
        operation: "chat.post.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.chat-correlation"),
        projectId,
        contextId,
        conversationId: registration.context.coordinatorConversationId,
        bodyMarkdown: "Wake the coordinator",
        artifactRefs: [],
        correlationId: null,
        causationMessageId: null,
      }),
    );
    assert.equal(posted.ok, true, posted.ok ? "" : posted.error.message);
    if (!posted.ok || posted.value.operation !== "chat.post.v1") return;
    assert.equal(store.queueChat(posted.value.message.messageId).ok, true);
    const claim = store.claimChatDispatch(
      ChatDispatchClaimSchema.parse({
        projectId,
        contextId,
        messageId: posted.value.message.messageId,
        sessionId,
        processEpoch: correlation.processEpoch,
      }),
    );
    assert.equal(claim.ok, true);
    assert.equal(
      store.settleChatDispatch({
        projectId,
        contextId,
        messageId: posted.value.message.messageId,
        sessionId,
        processEpoch: correlation.processEpoch,
        observation: "host_accepted",
        nativeTurnId: null,
        transportCorrelation: correlation,
        updatedAt: now().toISOString(),
      }).ok,
      true,
    );
    const replyInput = ObservedChatReplySchema.parse({
      sourceEventId: "chat-reply:stable-item",
      projectId,
      contextId,
      sessionId,
      processEpoch: correlation.processEpoch,
      nativeTurnId: null,
      transportCorrelation: correlation,
      actorId: ActorIdSchema.parse("actor.coordinator"),
      bodyMarkdown: "The coordinator received it.",
      occurredAt: now().toISOString(),
    });
    const reply = store.appendObservedChatReply(replyInput);
    assert.equal(reply.ok, true);
    if (reply.ok) assert.notEqual(reply.value, null);
    store.close();

    const reopened = open();
    assert.equal(reopened.ok, true);
    if (!reopened.ok) return;
    const replay = reopened.value.appendObservedChatReply(replyInput);
    assert.deepEqual(replay, { ok: true, value: null });
    reopened.value.close();
  } finally {
    try {
      rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    } catch {
      // Windows may hold SQLite sidecars briefly after a close; the test owns
      // this isolated directory and leaves no running process behind.
    }
  }
});

test("observed reply requires a native turn or accepted transport correlation", () => {
  const parsed = ObservedChatReplySchema.safeParse({
    sourceEventId: "chat-reply:invalid",
    projectId: "project.invalid",
    contextId: "context.invalid",
    sessionId: "session.invalid",
    processEpoch: "epoch.invalid",
    nativeTurnId: null,
    actorId: "actor.invalid",
    bodyMarkdown: "Missing correlation",
    occurredAt: "2026-09-16T10:00:00.000Z",
  });
  assert.equal(parsed.success, false);
});
