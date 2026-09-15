/** Explicitly synthetic shared-workspace port for UI acceptance. @scope spec://org.vibevm.zap/lens/PROP-005#project-views */
import { createQuicklensDemoDataSource } from "../quicklens-demo/index.ts";
import {
  AgentNetworkSchema,
  AgentOutputItemSchema,
  ChatMessageSchema,
  CoordinatorSessionSchema,
  ProjectExecutionStateSchema,
  HistoryEventSchema,
  ProjectDescriptorSchema,
  QuestionAnswerVersionSchema,
  QuestionGroupSchema,
  WorkContextDescriptorSchema,
  type AgentNetwork,
  type AgentOutputItem,
  type ChatMessage,
  type HistoryEvent,
  type ProjectId,
  type QuestionAnswerVersion,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import { eventInScope, fail, ok, pending } from "./helpers.ts";

const AT = "2026-09-15T10:00:00.000Z";
const available = { state: "available" } as const;
const projectAlpha = ProjectDescriptorSchema.parse({
  projectId: "project.lens",
  displayName: "Lens",
  repositoryRootRefs: ["repository.lens"],
  defaultContextId: "context.lens.main",
  actions: { startCoordinator: available },
  revision: "7",
  createdAt: AT,
  updatedAt: AT,
});
const projectBeta = ProjectDescriptorSchema.parse({
  projectId: "project.zap",
  displayName: "ZAP",
  repositoryRootRefs: ["repository.zap"],
  defaultContextId: "context.zap.main",
  actions: { startCoordinator: available },
  revision: "4",
  createdAt: AT,
  updatedAt: AT,
});
const contextAlpha = WorkContextDescriptorSchema.parse({
  contextId: "context.lens.main",
  projectId: projectAlpha.projectId,
  displayName: "Main worktree",
  workspaceRef: "workspace.lens.main",
  branchLabel: "main",
  revisionBinding: "accepted workspace fixture",
  planning: {
    state: "configured",
    storeId: "store.lens",
    campaignId: "campaign.lens",
    baseId: "base.lens",
  },
  coordinatorConversationId: "conversation.lens",
  revision: "3",
  createdAt: AT,
  updatedAt: AT,
});
const contextBeta = WorkContextDescriptorSchema.parse({
  contextId: "context.zap.main",
  projectId: projectBeta.projectId,
  displayName: "Planning engine",
  workspaceRef: "workspace.zap.main",
  branchLabel: "main",
  revisionBinding: "synthetic board fixture",
  planning: {
    state: "configured",
    storeId: "store.zap",
    campaignId: "campaign.zap",
    baseId: "base.zap",
  },
  coordinatorConversationId: "conversation.zap",
  revision: "2",
  createdAt: AT,
  updatedAt: AT,
});
const coordinator = CoordinatorSessionSchema.parse({
  sessionId: "session.lens.coordinator",
  projectId: projectAlpha.projectId,
  contextId: contextAlpha.contextId,
  conversationId: contextAlpha.coordinatorConversationId,
  coordinatorActorId: "act.lens.coordinator",
  role: "coordinator",
  launchOrigin: "lens",
  interactionKind: "structured",
  hostId: "host.local.codex",
  nativeRef: { namespace: "codex.thread", value: "synthetic-thread", incarnation: "1" },
  terminal: { state: "unavailable", reason: "native_session" },
  state: "running",
  actions: { interrupt: available },
  bootstrapBasis: "Full project boot recorded at revision 7",
  revision: "5",
  createdAt: AT,
  updatedAt: AT,
});
const executionAlpha = ProjectExecutionStateSchema.parse({
  projectId: projectAlpha.projectId,
  contextId: contextAlpha.contextId,
  state: "running",
  sessionId: coordinator.sessionId,
  processEpoch: "synthetic-epoch",
  pendingAction: null,
  lastAction: null,
  revision: "5",
  updatedAt: AT,
});
const executionBeta = ProjectExecutionStateSchema.parse({
  projectId: projectBeta.projectId,
  contextId: contextBeta.contextId,
  state: "uninitialized",
  sessionId: null,
  processEpoch: null,
  pendingAction: null,
  lastAction: null,
  revision: "1",
  updatedAt: AT,
});

const networkAlpha = AgentNetworkSchema.parse({
  projectId: projectAlpha.projectId,
  contextId: contextAlpha.contextId,
  agents: [
    agent(
      "act.lens.coordinator",
      "session.lens.coordinator",
      "Lens coordinator",
      null,
      "coordinator",
      "active",
    ),
    agent(
      "act.lens.ui",
      "session.lens.ui",
      "Workspace UI",
      "act.lens.coordinator",
      "worker",
      "active",
    ),
    agent(
      "act.lens.api",
      "session.lens.api",
      "Workspace API",
      "act.lens.coordinator",
      "worker",
      "active",
    ),
    agent(
      "act.lens.runtime",
      "session.lens.runtime",
      "Codex runtime",
      "act.lens.coordinator",
      "worker",
      "waiting_for_user",
    ),
  ],
  relationships: [
    relation("act.lens.coordinator", "act.lens.ui", "event.delegate.ui"),
    relation("act.lens.coordinator", "act.lens.api", "event.delegate.api"),
    relation("act.lens.coordinator", "act.lens.runtime", "event.delegate.runtime"),
  ],
  coverage: { state: "complete" },
});
const networkBeta = AgentNetworkSchema.parse({
  projectId: projectBeta.projectId,
  contextId: contextBeta.contextId,
  agents: [],
  relationships: [],
  coverage: { state: "complete" },
});

const question = QuestionGroupSchema.parse({
  questionGroupId: "question-group.workspace",
  projectId: projectAlpha.projectId,
  contextId: contextAlpha.contextId,
  conversationId: contextAlpha.coordinatorConversationId,
  originActorId: "act.lens.runtime",
  messageId: "message.question.workspace",
  title: "Choose the first workspace behavior",
  introductionMarkdown:
    "This synthetic group demonstrates the portable /ZapAskUserQuestion contract.",
  items: [
    {
      questionItemId: "question-item.scope",
      header: "Start view",
      promptMarkdown: "Which activity scope should open first?",
      contextMarkdown: "The choice changes only this client view.",
      artifactRefs: [],
      required: true,
      answerMode: "single_choice",
      options: [
        option("option.scope.all", "All projects", "Show authorized global activity first."),
        option("option.scope.project", "Current project", "Focus on the selected project."),
      ],
      customAnswer: null,
      recommendation: {
        optionIds: ["option.scope.all"],
        explanationMarkdown: "All projects makes concurrent coordinator activity visible.",
      },
    },
    {
      questionItemId: "question-item.panels",
      header: "Panels",
      promptMarkdown: "Which secondary panels should remain close to the agent network?",
      contextMarkdown: null,
      artifactRefs: [],
      required: true,
      answerMode: "multiple_choice",
      options: [
        option("option.panels.chat", "Chat", "Durable coordinator conversation."),
        option("option.panels.questions", "Questions", "Grouped actionable requests."),
        option("option.panels.history", "History", "Paged event and plan provenance."),
      ],
      customAnswer: { allowed: true, label: "Another panel", multiline: false, maximumLength: 200 },
      recommendation: null,
    },
    {
      questionItemId: "question-item.note",
      header: "Context",
      promptMarkdown: "Add any context the coordinator should retain.",
      contextMarkdown: null,
      artifactRefs: [],
      required: false,
      answerMode: "multiline_text",
      options: [],
      customAnswer: null,
      recommendation: null,
    },
  ],
  independentWorkAvailable: true,
  state: "open",
  revision: "1",
  deadlineAt: null,
  createdAt: AT,
  updatedAt: AT,
});

const outputs = [
  output(
    "act.lens.coordinator",
    "session.lens.coordinator",
    "1",
    "text",
    "Started the Lens project coordinator after the complete project boot.",
  ),
  output(
    "act.lens.coordinator",
    "session.lens.coordinator",
    "2",
    "tool",
    "Delegated bounded workspace, API, and runtime tasks through native Codex agents.",
  ),
  output(
    "act.lens.ui",
    "session.lens.ui",
    "1",
    "status",
    "Building the shared project, agent, activity, chat, and question workspace.",
  ),
  output(
    "act.lens.api",
    "session.lens.api",
    "1",
    "text",
    "Published the platform-neutral WorkspaceClientPort and durable event contracts.",
  ),
  output(
    "act.lens.runtime",
    "session.lens.runtime",
    "1",
    "status",
    "Waiting for a scoped user decision through /ZapAskUserQuestion.",
  ),
];
const messages: ChatMessage[] = [
  ChatMessageSchema.parse({
    messageId: "message.chat.user",
    projectId: projectAlpha.projectId,
    contextId: contextAlpha.contextId,
    conversationId: contextAlpha.coordinatorConversationId,
    senderActorId: null,
    role: "user",
    bodyMarkdown: "Make Codex project activity visible without replacing the coding workflow.",
    artifactRefs: [],
    correlationId: null,
    causationMessageId: null,
    deliveryState: "answered",
    revision: "1",
    createdAt: AT,
    updatedAt: AT,
  }),
  ChatMessageSchema.parse({
    messageId: "message.chat.assistant",
    projectId: projectAlpha.projectId,
    contextId: contextAlpha.contextId,
    conversationId: contextAlpha.coordinatorConversationId,
    senderActorId: "act.lens.coordinator",
    role: "assistant",
    bodyMarkdown:
      "The first screen now prioritizes projects, scoped activity, native agents, and sourced output.",
    artifactRefs: [],
    correlationId: "message.chat.user",
    causationMessageId: "message.chat.user",
    deliveryState: "answered",
    revision: "1",
    createdAt: AT,
    updatedAt: AT,
  }),
];
const history: HistoryEvent[] = [
  event("history.1", projectAlpha.projectId, "1", "project.registered", "lens", null),
  event("history.2", projectBeta.projectId, "2", "project.registered", "lens", null),
  event(
    "history.3",
    projectAlpha.projectId,
    "3",
    "coordinator.running",
    "host",
    "act.lens.coordinator",
  ),
  event("history.4", projectAlpha.projectId, "4", "agent.delegated", "host", "act.lens.ui"),
  event("history.5", projectAlpha.projectId, "5", "question.created", "lens", "act.lens.runtime"),
  HistoryEventSchema.parse({
    ...event("history.6", projectBeta.projectId, "6", "plan.rebuild.proposed", "zap", null),
    planProvenance: {
      previousPlanId: "plan.zap.3",
      proposedPlanId: "plan.zap.4",
      sourceBasisRef: "source-basis.zap.4",
      appliedRevision: null,
    },
  }),
];

/** @implements spec://org.vibevm.zap/lens/PROP-005#incremental-delivery */
export function createWorkspaceDemoPort(): WorkspaceClientPort {
  let nextChat = 3;
  let answerVersions: QuestionAnswerVersion[] = [];
  return {
    read: async (request) => {
      switch (request.operation) {
        case "project.list.v1":
          return ok({ operation: request.operation, projects: [projectAlpha, projectBeta] });
        case "project.get.v1": {
          const lens = request.projectId === projectAlpha.projectId;
          return ok({
            operation: request.operation,
            detail: {
              project: lens ? projectAlpha : projectBeta,
              contexts: [lens ? contextAlpha : contextBeta],
              coordinator: lens ? coordinator : null,
              coordinatorLaunchOptions: [
                {
                  profileId: "profile.codex.native",
                  label: "Codex coordinator · native agents",
                  interactionKind: "structured",
                  availability: available,
                },
              ],
            },
          });
        }
        case "project.snapshot.v1": {
          const demo = await createQuicklensDemoDataSource().read({
            signal: new AbortController().signal,
          });
          return demo.ok
            ? ok({
                operation: request.operation,
                snapshot: { state: "ready", snapshot: demo.value },
              })
            : fail("Demo plan snapshot is unavailable");
        }
        case "project.execution.get.v1":
          return ok({
            operation: request.operation,
            execution:
              request.projectId === projectAlpha.projectId ? executionAlpha : executionBeta,
          });
        case "model-policy.get.v1":
        case "model-policy.preview.v1":
        case "model-policy.history.v1":
        case "model-selection.get.v1":
          return fail("Model policy is unavailable in the synthetic demo.");
        case "context.get.v1":
          return ok({
            operation: request.operation,
            context: request.projectId === projectAlpha.projectId ? contextAlpha : contextBeta,
          });
        case "chat.page.v1":
          return ok({
            operation: request.operation,
            page: { messages, afterSequence: request.afterSequence, nextSequence: null },
          });
        case "question.get.v1":
          return ok({ operation: request.operation, detail: { question, answerVersions } });
        case "question.list.v1":
          return ok({
            operation: request.operation,
            questions: request.projectId === projectAlpha.projectId ? [question] : [],
          });
        case "session.get.v1":
          return ok({ operation: request.operation, session: coordinator });
        case "session.list.v1":
          return ok({
            operation: request.operation,
            sessions: request.projectId === projectAlpha.projectId ? [coordinator] : [],
          });
        case "agent.list.v1":
          return ok({ operation: request.operation, agents: network(request.projectId).agents });
        case "agent.network.v1":
          return ok({ operation: request.operation, network: network(request.projectId) });
        case "agent.output.page.v1":
          return ok({
            operation: request.operation,
            page: {
              items: outputs.filter((item) => item.actorId === request.actorId),
              afterSequence: request.afterSequence,
              nextSequence: null,
            },
          });
      }
      return fail("This synthetic workspace does not implement the requested read");
    },
    command: (request) => {
      if (request.operation === "chat.post.v1") {
        const message = ChatMessageSchema.parse({
          messageId: `message.chat.${String(nextChat++)}`,
          projectId: request.projectId,
          contextId: request.contextId,
          conversationId: request.conversationId,
          senderActorId: null,
          role: "user",
          bodyMarkdown: request.bodyMarkdown,
          artifactRefs: [],
          correlationId: request.correlationId,
          causationMessageId: request.causationMessageId,
          deliveryState: "queued",
          revision: "1",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        });
        messages.push(message);
        return ok({ operation: request.operation, message });
      }
      if (request.operation === "question.answer.v1" || request.operation === "question.amend.v1") {
        const version = QuestionAnswerVersionSchema.parse({
          answerVersionId: `answer-version.${String(answerVersions.length + 1)}`,
          questionGroupId: request.questionGroupId,
          revision: String(answerVersions.length + 2),
          previousVersionId: answerVersions.at(-1)?.answerVersionId ?? null,
          submission: request.submission,
          responderActorId: null,
          responderPrincipalId: "principal.demo.human",
          amendmentReasonMarkdown:
            request.operation === "question.amend.v1" ? request.amendmentReasonMarkdown : null,
          createdAt: new Date().toISOString(),
          metadata: {},
        });
        answerVersions = [...answerVersions, version];
        return ok({ operation: request.operation, question, answerVersion: version });
      }
      if (request.operation === "session.start.v1") {
        return ok({ operation: request.operation, action: pending(request) });
      }
      return fail("This synthetic workspace does not perform that command");
    },
    events: (request) => {
      const scoped = history.filter((item) => eventInScope(item, request.cursor.scope));
      return ok({
        events: scoped,
        resume: { ...request.cursor, afterGlobalSequence: DecimalSchema.parse("6") },
        next: null,
        coverage: { state: "complete" },
      });
    },
    subscribe: async function* (request) {
      if (request.signal?.aborted) return;
      await new Promise<void>((resolve) => {
        request.signal?.addEventListener(
          "abort",
          () => {
            resolve();
          },
          { once: true },
        );
      });
      for (const event of [] satisfies HistoryEvent[]) yield ok(event);
    },
  };
}

function agent(
  actorId: string,
  sessionId: string,
  displayName: string,
  parentActorId: string | null,
  role: "coordinator" | "worker",
  state: "active" | "waiting_for_user",
) {
  return {
    actorId,
    sessionId,
    projectId: projectAlpha.projectId,
    contextId: contextAlpha.contextId,
    role,
    parentActorId,
    displayName,
    executionMode: "native" as const,
    hostId: "host.local.codex",
    nativeRef: { namespace: "codex.actor", value: actorId, incarnation: "1" },
    state,
    revision: "1",
  };
}

function relation(fromActorId: string, toActorId: string, sourceEventId: string) {
  return {
    projectId: projectAlpha.projectId,
    contextId: contextAlpha.contextId,
    fromActorId,
    toActorId,
    kind: "delegated" as const,
    provenance: "host" as const,
    sourceEventId,
    observedAt: AT,
  };
}

function option(optionId: string, label: string, description: string) {
  return { optionId, label, description, previewMarkdown: null, artifactRefs: [] };
}

function output(
  actorId: string,
  sessionId: string,
  sequence: string,
  kind: AgentOutputItem["kind"],
  bodyMarkdown: string,
) {
  return AgentOutputItemSchema.parse({
    projectId: projectAlpha.projectId,
    contextId: contextAlpha.contextId,
    actorId,
    sessionId,
    runId: null,
    sequence,
    kind,
    bodyMarkdown,
    artifactRefs: [],
    nativeRef: { namespace: "codex.item", value: `${actorId}.${sequence}`, incarnation: "1" },
    occurredAt: AT,
  });
}

function event(
  historyEventId: string,
  projectId: ProjectId,
  sequence: string,
  kind: string,
  source: "lens" | "host" | "zap",
  actorId: string | null,
) {
  return HistoryEventSchema.parse({
    historyEventId,
    projectId,
    contextId:
      projectId === projectAlpha.projectId ? contextAlpha.contextId : contextBeta.contextId,
    globalSequence: sequence,
    projectSequence: sequence,
    sourceSequence: sequence,
    kind,
    source,
    actorId,
    occurrenceAt: AT,
    ingestedAt: AT,
    sourceEventId: `source.${historyEventId}`,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  });
}

function network(projectId: ProjectId): AgentNetwork {
  return projectId === projectAlpha.projectId ? networkAlpha : networkBeta;
}

export const WORKSPACE_DEMO_LABEL = "Synthetic Zap Quick Lens workspace";
