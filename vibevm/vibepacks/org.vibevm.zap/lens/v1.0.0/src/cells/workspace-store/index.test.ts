import assert from "node:assert/strict";
import test from "node:test";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
  type Result,
} from "../protocol/index.ts";
import {
  AgentDescriptorSchema,
  AgentOutputItemSchema,
  AgentRelationshipSchema,
  AgentSessionIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  WorkspaceCommandRequestSchema,
  type WorkspaceCommandContext,
  type WorkspaceError,
} from "../workspace-model/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "./index.ts";

test("projects are isolated and trusted registration is exactly idempotent", () => {
  const store = value(openWorkspaceStore(fixtureOptions()));
  try {
    const first = registration("a");
    const second = registration("b");
    const created = value(store.registerProject(first));
    assert.equal(created.project.projectId, first.projectId);
    assert.equal(value(store.registerProject(first)).project.projectId, first.projectId);
    const changed = { ...first, displayName: "Changed project" };
    assert.equal(store.registerProject(changed).ok, false);
    value(store.registerProject(second));

    const onlyA = access("a", ["a"]);
    const sharedChat = WorkspaceCommandRequestSchema.parse({
      operation: "chat.post.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.shared-by-actors"),
      projectId: first.projectId,
      contextId: first.context.contextId,
      conversationId: first.context.coordinatorConversationId,
      bodyMarkdown: "Independent actor message",
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    });
    const parentMessage = value(store.command(onlyA, sharedChat));
    const childMessage = value(
      store.command({ ...onlyA, actorId: ActorIdSchema.parse("actor.a-child") }, sharedChat),
    );
    assert.equal(parentMessage.operation, "chat.post.v1");
    assert.equal(childMessage.operation, "chat.post.v1");
    if (parentMessage.operation === "chat.post.v1" && childMessage.operation === "chat.post.v1") {
      assert.notEqual(parentMessage.message.messageId, childMessage.message.messageId);
    }
    const listed = value(store.read(onlyA, { operation: "project.list.v1" }));
    assert.equal(listed.operation, "project.list.v1");
    if (listed.operation === "project.list.v1") {
      assert.deepEqual(
        listed.projects.map((project) => project.projectId),
        [first.projectId],
      );
    }
    assert.equal(
      store.read(onlyA, { operation: "project.get.v1", projectId: second.projectId }).ok,
      false,
    );
    const launch = value(store.resolveProjectLaunch(first.projectId, first.context.contextId));
    assert.equal(launch.cwd, "C:\\fixtures\\project-a");
    assert.equal(JSON.stringify(created).includes(launch.cwd), false);
    assert.equal(JSON.stringify(created).includes(launch.launchProfileRef), false);
  } finally {
    store.close();
  }
});

test("answers use revision CAS and amendments retain immutable versions", () => {
  const store = value(openWorkspaceStore(fixtureOptions()));
  try {
    const project = registration("q");
    value(store.registerProject(project));
    const context = access("q", ["q"]);
    const created = value(
      store.command(context, {
        operation: "question.create.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.question-create"),
        projectId: project.projectId,
        contextId: project.context.contextId,
        conversationId: project.context.coordinatorConversationId,
        draft: {
          title: "Choose a route",
          introductionMarkdown: "A grouped question",
          independentWorkAvailable: true,
          deadlineAt: null,
          items: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.route"),
              header: "Route",
              promptMarkdown: "Which route?",
              contextMarkdown: null,
              artifactRefs: [],
              required: true,
              answerMode: "single_choice",
              options: [
                {
                  optionId: QuestionOptionIdSchema.parse("option.first"),
                  label: "First",
                  description: "Use the first route",
                  previewMarkdown: "Preview",
                  artifactRefs: [],
                },
              ],
              customAnswer: {
                allowed: true,
                label: "Another route",
                multiline: false,
                maximumLength: 200,
              },
              recommendation: {
                optionIds: [QuestionOptionIdSchema.parse("option.first")],
                explanationMarkdown: "Nonbinding recommendation",
              },
            },
          ],
        },
      }),
    );
    assert.equal(created.operation, "question.create.v1");
    if (created.operation !== "question.create.v1") return;
    const groupId = created.question.questionGroupId;
    const answered = value(
      store.command(context, {
        operation: "question.answer.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.question-answer"),
        projectId: project.projectId,
        contextId: project.context.contextId,
        questionGroupId: groupId,
        expectedRevision: created.question.revision,
        submission: {
          answers: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.route"),
              answer: {
                kind: "single_choice",
                optionId: QuestionOptionIdSchema.parse("option.first"),
              },
            },
          ],
          noteMarkdown: null,
        },
      }),
    );
    assert.equal(answered.operation, "question.answer.v1");
    if (answered.operation !== "question.answer.v1") return;
    assert.equal(answered.question.revision, "2");
    const stale = store.command(context, {
      operation: "question.answer.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.question-answer-stale"),
      projectId: project.projectId,
      contextId: project.context.contextId,
      questionGroupId: groupId,
      expectedRevision: DecimalSchema.parse("1"),
      submission: answered.answerVersion.submission,
    });
    assert.equal(stale.ok, false);
    const oversizedCustom = store.command(context, {
      operation: "question.amend.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.question-amend-too-long"),
      projectId: project.projectId,
      contextId: project.context.contextId,
      questionGroupId: groupId,
      expectedRevision: DecimalSchema.parse("2"),
      submission: {
        answers: [
          {
            questionItemId: QuestionItemIdSchema.parse("question-item.route"),
            answer: { kind: "custom", text: "x".repeat(201) },
          },
        ],
        noteMarkdown: null,
      },
      amendmentReasonMarkdown: "Oversized answer must refuse",
    });
    assert.equal(oversizedCustom.ok, false);
    const amended = value(
      store.command(context, {
        operation: "question.amend.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.question-amend"),
        projectId: project.projectId,
        contextId: project.context.contextId,
        questionGroupId: groupId,
        expectedRevision: DecimalSchema.parse("2"),
        submission: {
          answers: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.route"),
              answer: { kind: "custom", text: "A later explicit route" },
            },
          ],
          noteMarkdown: "Changed after review",
        },
        amendmentReasonMarkdown: "New evidence",
      }),
    );
    assert.equal(amended.operation, "question.amend.v1");
    const detail = value(
      store.read(context, {
        operation: "question.get.v1",
        projectId: project.projectId,
        contextId: project.context.contextId,
        questionGroupId: groupId,
      }),
    );
    assert.equal(detail.operation, "question.get.v1");
    if (detail.operation === "question.get.v1") {
      assert.equal(detail.detail.answerVersions.length, 2);
      assert.equal(
        detail.detail.answerVersions[0]?.submission.answers[0]?.answer.kind,
        "single_choice",
      );
      assert.equal(
        detail.detail.answerVersions[1]?.previousVersionId,
        detail.detail.answerVersions[0]?.answerVersionId,
      );
    }

    const expiring = value(
      store.command(context, {
        operation: "question.create.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.question-expiring"),
        projectId: project.projectId,
        contextId: project.context.contextId,
        conversationId: project.context.coordinatorConversationId,
        draft: {
          title: "Expired question",
          introductionMarkdown: "Deadline is enforced at submission",
          independentWorkAvailable: false,
          deadlineAt: "2026-09-15T10:00:00.000Z",
          items: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.expired"),
              header: "Expired",
              promptMarkdown: "This answer arrives too late",
              contextMarkdown: null,
              artifactRefs: [],
              required: true,
              answerMode: "short_text",
              options: [],
              customAnswer: null,
              recommendation: null,
            },
          ],
        },
      }),
    );
    assert.equal(expiring.operation, "question.create.v1");
    if (expiring.operation !== "question.create.v1") return;
    const expiredAnswer = store.command(context, {
      operation: "question.answer.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.question-expired-answer"),
      projectId: project.projectId,
      contextId: project.context.contextId,
      questionGroupId: expiring.question.questionGroupId,
      expectedRevision: DecimalSchema.parse("1"),
      submission: {
        answers: [
          {
            questionItemId: QuestionItemIdSchema.parse("question-item.expired"),
            answer: { kind: "short_text", text: "Too late" },
          },
        ],
        noteMarkdown: null,
      },
    });
    assert.equal(expiredAnswer.ok, false);
    const expiredDetail = value(
      store.read(context, {
        operation: "question.get.v1",
        projectId: project.projectId,
        contextId: project.context.contextId,
        questionGroupId: expiring.question.questionGroupId,
      }),
    );
    if (expiredDetail.operation === "question.get.v1") {
      assert.equal(expiredDetail.detail.question.state, "expired");
      assert.equal(expiredDetail.detail.answerVersions.length, 0);
    }
  } finally {
    store.close();
  }
});

test("source-event dedup is scoped and global history retains both projects", () => {
  const store = value(openWorkspaceStore(fixtureOptions()));
  try {
    const first = registration("one");
    const second = registration("two");
    value(store.registerProject(first));
    value(store.registerProject(second));
    const sourceEventId = "event.same-source-id";
    for (const project of [first, second]) {
      value(
        store.ingestEvent({
          projectId: project.projectId,
          contextId: project.context.contextId,
          kind: "host.actor.observed",
          source: "host",
          actorId: null,
          occurrenceAt: "2026-09-15T10:00:00.000Z",
          sourceEventId,
          sourceSequence: DecimalSchema.parse("7"),
          correlationId: null,
          causationId: null,
          planProvenance: null,
          payload: { observed: true },
        }),
      );
    }
    value(
      store.ingestEvent({
        projectId: first.projectId,
        contextId: first.context.contextId,
        kind: "host.actor.observed",
        source: "host",
        actorId: null,
        occurrenceAt: "2026-09-15T10:00:00.000Z",
        sourceEventId,
        sourceSequence: DecimalSchema.parse("7"),
        correlationId: null,
        causationId: null,
        planProvenance: null,
        payload: { observed: true },
      }),
    );
    const page = value(
      store.events(access("one", ["one", "two"]), {
        cursor: {
          scope: { kind: "all_authorized" },
          afterGlobalSequence: DecimalSchema.parse("0"),
        },
        limit: 32,
      }),
    );
    const observed = page.events.filter((event) => event.sourceEventId === sourceEventId);
    assert.equal(observed.length, 2);
    assert.deepEqual(
      observed.map((event) => event.projectId).sort(),
      [first.projectId, second.projectId].sort(),
    );
    assert.equal(
      BigInt(observed[0]?.globalSequence ?? "0") < BigInt(observed[1]?.globalSequence ?? "0"),
      true,
    );
  } finally {
    store.close();
  }
});

test("agent network and output reads cannot cross authorized project scope", () => {
  const store = value(openWorkspaceStore(fixtureOptions()));
  try {
    const first = registration("network-a");
    const second = registration("network-b");
    value(store.registerProject(first));
    value(store.registerProject(second));
    const sessionId = AgentSessionIdSchema.parse("session.network-a");
    const rootActor = AgentDescriptorSchema.parse({
      actorId: "actor.network-a-root",
      sessionId,
      projectId: first.projectId,
      contextId: first.context.contextId,
      role: "coordinator",
      parentActorId: null,
      displayName: "Coordinator",
      executionMode: "native",
      hostId: "host.local",
      nativeRef: { namespace: "codex.thread", value: "thread-root", incarnation: "1" },
      state: "active",
      revision: "1",
    });
    const childActor = AgentDescriptorSchema.parse({
      ...rootActor,
      actorId: "actor.network-a-child",
      role: "worker",
      parentActorId: rootActor.actorId,
      displayName: "Worker",
      nativeRef: { namespace: "codex.thread", value: "thread-child", incarnation: "1" },
    });
    const foreignActor = AgentDescriptorSchema.parse({
      ...rootActor,
      actorId: "actor.network-b",
      sessionId: "session.network-b",
      projectId: second.projectId,
      contextId: second.context.contextId,
      displayName: "Foreign worker",
    });
    value(store.upsertAgent(rootActor));
    value(store.upsertAgent(childActor));
    value(store.upsertAgent(foreignActor));
    assert.equal(
      store.upsertAgent(
        AgentDescriptorSchema.parse({
          ...childActor,
          projectId: second.projectId,
          contextId: second.context.contextId,
        }),
      ).ok,
      false,
    );
    value(
      store.recordAgentRelationship(
        AgentRelationshipSchema.parse({
          projectId: first.projectId,
          contextId: first.context.contextId,
          fromActorId: rootActor.actorId,
          toActorId: childActor.actorId,
          kind: "delegated",
          provenance: "host",
          sourceEventId: "codex.child.started",
          observedAt: "2026-09-15T10:00:00.000Z",
        }),
      ),
    );
    value(
      store.appendAgentOutput(
        AgentOutputItemSchema.parse({
          projectId: first.projectId,
          contextId: first.context.contextId,
          actorId: childActor.actorId,
          sessionId,
          runId: null,
          sequence: "1",
          kind: "text",
          bodyMarkdown: "Child output",
          artifactRefs: [],
          nativeRef: childActor.nativeRef,
          occurredAt: "2026-09-15T10:00:00.000Z",
        }),
      ),
    );
    const accessA = access("network-a", ["network-a"]);
    const network = value(
      store.read(accessA, {
        operation: "agent.network.v1",
        projectId: first.projectId,
        contextId: first.context.contextId,
      }),
    );
    assert.equal(network.operation, "agent.network.v1");
    if (network.operation === "agent.network.v1") {
      assert.equal(network.network.agents.length, 2);
      assert.equal(network.network.relationships.length, 1);
    }
    const output = value(
      store.read(accessA, {
        operation: "agent.output.page.v1",
        projectId: first.projectId,
        contextId: first.context.contextId,
        actorId: childActor.actorId,
        afterSequence: DecimalSchema.parse("0"),
        limit: 10,
      }),
    );
    assert.equal(output.operation, "agent.output.page.v1");
    if (output.operation === "agent.output.page.v1") {
      assert.equal(output.page.items[0]?.bodyMarkdown, "Child output");
    }
    assert.equal(
      store.read(accessA, {
        operation: "agent.output.page.v1",
        projectId: second.projectId,
        contextId: second.context.contextId,
        actorId: foreignActor.actorId,
        afterSequence: DecimalSchema.parse("0"),
        limit: 10,
      }).ok,
      false,
    );
  } finally {
    store.close();
  }
});

function fixtureOptions() {
  let next = 0;
  return {
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T10:00:00.000Z"),
    idFactory: (kind: string) => `${kind}.test-${String(++next)}`,
  };
}

function registration(name: string) {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: `registration.${name}`,
    projectId: `project.${name}`,
    displayName: `Project ${name}`,
    repositoryRootRefs: [`repository.${name}`],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: `context.${name}`,
      displayName: `Context ${name}`,
      workspaceRef: `workspace.${name}`,
      branchLabel: "main",
      revisionBinding: "revision.test",
      planning: { state: "unavailable", reason: "Synthetic store-only fixture" },
      coordinatorConversationId: `conversation.${name}`,
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex-default",
        label: "Codex",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: {
      cwd: `C:\\fixtures\\project-${name}`,
      launchProfileRef: `protected-profile.${name}`,
    },
  });
}

function access(name: string, projects: string[]): WorkspaceCommandContext {
  return {
    principalId: PrincipalIdSchema.parse(`principal.${name}`),
    actorId: ActorIdSchema.parse(`actor.${name}`),
    clientId: ClientIdSchema.parse(`client.${name}`),
    authorizedProjectIds: projects.map((project) => ProjectIdSchema.parse(`project.${project}`)),
  };
}

function value<T>(result: Result<T, WorkspaceError>): T {
  if (!result.ok) {
    throw new Error(
      `violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: fixture operation failed (${result.error.code}); fix surface: implementation`,
    );
  }
  return result.value;
}
