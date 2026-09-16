/** Named workspace command/query protocol. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  MessageIdSchema,
} from "../protocol/index.ts";
import { QuicklensSnapshotSchema } from "../quicklens-model/index.ts";
import {
  ActionAvailabilitySchema,
  AgentDescriptorSchema,
  AgentNetworkSchema,
  AgentOutputItemSchema,
  ChatMessageSchema,
  CoordinatorSessionSchema,
  HistoryEventSchema,
  ProjectDescriptorSchema,
  ProjectExecutionStateSchema,
  WorkContextDescriptorSchema,
} from "./entities.ts";
import {
  AgentSessionIdSchema,
  ProjectIdSchema,
  QuestionGroupIdSchema,
  WorkContextIdSchema,
} from "./ids.ts";
import {
  QuestionAnswerVersionSchema,
  QuestionGroupDraftSchema,
  QuestionGroupSchema,
  QuestionSubmissionSchema,
} from "./questions.ts";
import {
  NativeApprovalDecisionSchema,
  NativeApprovalIdSchema,
  NativeApprovalRequestSchema,
} from "./interactions.ts";
import {
  TerminalCommandResponseSchemas,
  TerminalCommandSchemas,
  TerminalReadRequestSchemas,
  TerminalReadResponseSchemas,
} from "./terminal.ts";
import {
  ModelPolicyChangeViewSchema,
  ModelPolicyGetInputSchema,
  ModelPolicyHistoryInputSchema,
  ModelPolicyPreviewInputSchema,
  ModelPolicyUpdateInputSchema,
  ModelPolicyVersionViewSchema,
  ModelSelectionGetInputSchema,
  ModelSelectionPreviewResultSchema,
  StoredModelPolicyViewSchema,
  StoredModelSelectionViewSchema,
} from "./model-policy.ts";
import {
  WorkspacePlanCommandRequestSchemas,
  WorkspacePlanCommandResponseSchemas,
} from "./planning.ts";
import {
  ManagedWorkCommandSchemas,
  ManagedWorkCommandResponseSchemas,
  ManagedWorkReadRequestSchemas,
  ManagedWorkReadResponseSchemas,
} from "./managed-work.ts";
import {
  AnnotationCommandRequestSchemas,
  AnnotationCommandResponseSchemas,
  AnnotationReadRequestSchemas,
  AnnotationReadResponseSchemas,
} from "./annotations.ts";
import {
  RepositoryWorkspaceCommandRequestSchemas,
  RepositoryWorkspaceCommandResponseSchemas,
  RepositoryWorkspaceReadRequestSchemas,
  RepositoryWorkspaceReadResponseSchemas,
} from "./repository-workspaces.ts";
import {
  ExecutionCatalogCommandRequestSchemas,
  ExecutionCatalogCommandResponseSchemas,
  ExecutionCatalogReadRequestSchemas,
  ExecutionCatalogReadResponseSchemas,
} from "./execution-catalog.ts";
import {
  type Awaitable,
  type WorkspaceAccessContext,
  type WorkspaceCommandContext,
  type WorkspaceResult,
} from "./access.ts";

export const HistoryScopeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("all_authorized") }).strict(),
  z.object({ kind: z.literal("project"), projectId: ProjectIdSchema }).strict(),
  z
    .object({
      kind: z.literal("context"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("actor"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      actorId: ActorIdSchema,
    })
    .strict(),
]);
export const HistoryCursorSchema = z
  .object({
    scope: HistoryScopeSchema,
    afterGlobalSequence: DecimalSchema,
  })
  .strict();
export type HistoryCursor = z.infer<typeof HistoryCursorSchema>;
export const HistoryPageSchema = z
  .object({
    events: z.array(HistoryEventSchema),
    resume: HistoryCursorSchema,
    next: HistoryCursorSchema.nullable(),
    coverage: z.discriminatedUnion("state", [
      z.object({ state: z.literal("complete") }).strict(),
      z
        .object({
          state: z.literal("gap"),
          firstAvailableSequence: DecimalSchema,
          reason: z.string().min(1).max(2_000),
        })
        .strict(),
    ]),
  })
  .strict();
export type HistoryPage = z.infer<typeof HistoryPageSchema>;

export const ChatPageSchema = z
  .object({
    messages: z.array(ChatMessageSchema),
    afterSequence: DecimalSchema,
    nextSequence: DecimalSchema.nullable(),
  })
  .strict();
export type ChatPage = z.infer<typeof ChatPageSchema>;
export const AgentOutputPageSchema = z
  .object({
    items: z.array(AgentOutputItemSchema),
    afterSequence: DecimalSchema,
    nextSequence: DecimalSchema.nullable(),
  })
  .strict();
export type AgentOutputPage = z.infer<typeof AgentOutputPageSchema>;
export const QuestionDetailSchema = z
  .object({
    question: QuestionGroupSchema,
    answerVersions: z.array(QuestionAnswerVersionSchema),
  })
  .strict();
export type QuestionDetail = z.infer<typeof QuestionDetailSchema>;
export const CoordinatorLaunchOptionSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    label: z.string().min(1).max(256),
    interactionKind: z.enum(["structured", "native_harness", "owned_terminal"]),
    availability: ActionAvailabilitySchema,
  })
  .strict();
export type CoordinatorLaunchOption = z.infer<typeof CoordinatorLaunchOptionSchema>;
export const ProjectDetailSchema = z
  .object({
    project: ProjectDescriptorSchema,
    contexts: z.array(WorkContextDescriptorSchema),
    coordinator: CoordinatorSessionSchema.nullable(),
    coordinatorLaunchOptions: z.array(CoordinatorLaunchOptionSchema),
  })
  .strict();
export type ProjectDetail = z.infer<typeof ProjectDetailSchema>;
export const ProjectSnapshotStateSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("ready"), snapshot: QuicklensSnapshotSchema }).strict(),
  z
    .object({
      state: z.literal("unavailable"),
      code: z.enum(["zap_not_configured", "zap_unavailable", "stale_context"]),
      reason: z.string().min(1).max(2_000),
    })
    .strict(),
]);
export type ProjectSnapshotState = z.infer<typeof ProjectSnapshotStateSchema>;
export const WorkspaceReadRequestSchema = z.discriminatedUnion("operation", [
  ...ExecutionCatalogReadRequestSchemas,
  ...AnnotationReadRequestSchemas,
  z.object({ operation: z.literal("project.list.v1") }).strict(),
  z
    .object({
      operation: z.literal("project.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema.optional(),
    })
    .strict(),
  z
    .object({
      operation: z.literal("project.snapshot.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("project.execution.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("context.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("chat.page.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      conversationId: ConversationIdSchema,
      afterSequence: DecimalSchema,
      limit: z.number().int().min(1).max(256),
    })
    .strict(),
  z
    .object({
      operation: z.literal("question.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      questionGroupId: QuestionGroupIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("question.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      state: QuestionGroupSchema.shape.state.nullable(),
      limit: z.number().int().min(1).max(256),
    })
    .strict(),
  z
    .object({
      operation: z.literal("native-approval.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      nativeApprovalId: NativeApprovalIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("native-approval.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      state: NativeApprovalRequestSchema.shape.state.nullable(),
      limit: z.number().int().min(1).max(256),
    })
    .strict(),
  z
    .object({
      operation: z.literal("session.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      sessionId: AgentSessionIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("session.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("agent.list.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("agent.network.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("agent.output.page.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      actorId: ActorIdSchema,
      afterSequence: DecimalSchema,
      limit: z.number().int().min(1).max(256),
    })
    .strict(),
  ...TerminalReadRequestSchemas,
  ...ManagedWorkReadRequestSchemas,
  ...RepositoryWorkspaceReadRequestSchemas,
  ModelPolicyGetInputSchema.extend({ operation: z.literal("model-policy.get.v1") }).strict(),
  ModelPolicyPreviewInputSchema.extend({
    operation: z.literal("model-policy.preview.v1"),
  }).strict(),
  ModelPolicyHistoryInputSchema.extend({
    operation: z.literal("model-policy.history.v1"),
  }).strict(),
  ModelSelectionGetInputSchema.extend({
    operation: z.literal("model-selection.get.v1"),
  }).strict(),
]);
export type WorkspaceReadRequest = z.infer<typeof WorkspaceReadRequestSchema>;
export const WorkspaceReadResponseSchema = z.discriminatedUnion("operation", [
  ...ExecutionCatalogReadResponseSchemas,
  ...AnnotationReadResponseSchemas,
  z
    .object({ operation: z.literal("project.list.v1"), projects: z.array(ProjectDescriptorSchema) })
    .strict(),
  z.object({ operation: z.literal("project.get.v1"), detail: ProjectDetailSchema }).strict(),
  z
    .object({
      operation: z.literal("project.snapshot.v1"),
      snapshot: ProjectSnapshotStateSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("project.execution.get.v1"),
      execution: ProjectExecutionStateSchema,
    })
    .strict(),
  z
    .object({ operation: z.literal("context.get.v1"), context: WorkContextDescriptorSchema })
    .strict(),
  z.object({ operation: z.literal("chat.page.v1"), page: ChatPageSchema }).strict(),
  z.object({ operation: z.literal("question.get.v1"), detail: QuestionDetailSchema }).strict(),
  z
    .object({ operation: z.literal("question.list.v1"), questions: z.array(QuestionGroupSchema) })
    .strict(),
  z
    .object({
      operation: z.literal("native-approval.get.v1"),
      approval: NativeApprovalRequestSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("native-approval.list.v1"),
      approvals: z.array(NativeApprovalRequestSchema),
    })
    .strict(),
  z.object({ operation: z.literal("session.get.v1"), session: CoordinatorSessionSchema }).strict(),
  z
    .object({
      operation: z.literal("session.list.v1"),
      sessions: z.array(CoordinatorSessionSchema),
    })
    .strict(),
  z
    .object({ operation: z.literal("agent.list.v1"), agents: z.array(AgentDescriptorSchema) })
    .strict(),
  z.object({ operation: z.literal("agent.network.v1"), network: AgentNetworkSchema }).strict(),
  z.object({ operation: z.literal("agent.output.page.v1"), page: AgentOutputPageSchema }).strict(),
  ...TerminalReadResponseSchemas,
  ...ManagedWorkReadResponseSchemas,
  ...RepositoryWorkspaceReadResponseSchemas,
  z
    .object({ operation: z.literal("model-policy.get.v1"), policy: StoredModelPolicyViewSchema })
    .strict(),
  z
    .object({
      operation: z.literal("model-policy.preview.v1"),
      result: ModelSelectionPreviewResultSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("model-policy.history.v1"),
      versions: z.array(ModelPolicyVersionViewSchema),
      changes: z.array(ModelPolicyChangeViewSchema),
    })
    .strict(),
  z
    .object({
      operation: z.literal("model-selection.get.v1"),
      selection: StoredModelSelectionViewSchema,
    })
    .strict(),
]);
export type WorkspaceReadResponse = z.infer<typeof WorkspaceReadResponseSchema>;

const ScopedRequestSchema = z.object({
  clientRequestId: ClientRequestIdSchema,
  projectId: ProjectIdSchema,
  contextId: WorkContextIdSchema,
});
const ProjectLifecycleCommandSchema = ScopedRequestSchema.extend({
  sessionId: AgentSessionIdSchema.nullable(),
  expectedRevision: DecimalSchema,
  reasonMarkdown: z.string().min(1).max(8_000),
});
export const WorkspaceCommandRequestSchema = z.discriminatedUnion("operation", [
  ...ExecutionCatalogCommandRequestSchemas,
  ...AnnotationCommandRequestSchemas,
  ...WorkspacePlanCommandRequestSchemas,
  ScopedRequestSchema.extend({
    operation: z.literal("chat.post.v1"),
    conversationId: ConversationIdSchema,
    bodyMarkdown: z.string().min(1).max(64_000),
    artifactRefs: z.array(z.string().min(3).max(160)).max(256),
    correlationId: z.string().min(3).max(160).nullable(),
    causationMessageId: MessageIdSchema.nullable(),
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("question.create.v1"),
    conversationId: ConversationIdSchema,
    draft: QuestionGroupDraftSchema,
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("question.answer.v1"),
    questionGroupId: QuestionGroupIdSchema,
    expectedRevision: DecimalSchema,
    submission: QuestionSubmissionSchema,
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("question.amend.v1"),
    questionGroupId: QuestionGroupIdSchema,
    expectedRevision: DecimalSchema,
    submission: QuestionSubmissionSchema,
    amendmentReasonMarkdown: z.string().min(1).max(8_000),
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("question.cancel.v1"),
    questionGroupId: QuestionGroupIdSchema,
    expectedRevision: DecimalSchema,
    reasonMarkdown: z.string().min(1).max(8_000),
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("native-approval.respond.v1"),
    nativeApprovalId: NativeApprovalIdSchema,
    expectedRevision: DecimalSchema,
    response: NativeApprovalDecisionSchema,
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("session.start.v1"),
    interactionKind: z.enum(["structured", "native_harness", "owned_terminal"]),
    profileId: z.string().min(3).max(160),
  }).strict(),
  ScopedRequestSchema.extend({
    operation: z.literal("session.interrupt.v1"),
    sessionId: AgentSessionIdSchema,
    expectedRevision: DecimalSchema,
    reasonMarkdown: z.string().min(1).max(8_000),
  }).strict(),
  ProjectLifecycleCommandSchema.extend({ operation: z.literal("project.pause.v1") }).strict(),
  ProjectLifecycleCommandSchema.extend({ operation: z.literal("project.stop.v1") }).strict(),
  ProjectLifecycleCommandSchema.extend({ operation: z.literal("project.continue.v1") }).strict(),
  ModelPolicyUpdateInputSchema.extend({ operation: z.literal("model-policy.update.v1") }).strict(),
  ...TerminalCommandSchemas,
  ...ManagedWorkCommandSchemas,
  ...RepositoryWorkspaceCommandRequestSchemas,
]);
export type WorkspaceCommandRequest = z.infer<typeof WorkspaceCommandRequestSchema>;
const PendingActionSchema = z
  .object({
    requestId: z.string().min(3).max(160),
    state: z.literal("pending"),
    availability: ActionAvailabilitySchema,
  })
  .strict();
export const WorkspaceCommandResponseSchema = z.discriminatedUnion("operation", [
  ...ExecutionCatalogCommandResponseSchemas,
  ...AnnotationCommandResponseSchemas,
  ...WorkspacePlanCommandResponseSchemas,
  z.object({ operation: z.literal("chat.post.v1"), message: ChatMessageSchema }).strict(),
  z.object({ operation: z.literal("question.create.v1"), question: QuestionGroupSchema }).strict(),
  z
    .object({
      operation: z.literal("question.answer.v1"),
      question: QuestionGroupSchema,
      answerVersion: QuestionAnswerVersionSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("question.amend.v1"),
      question: QuestionGroupSchema,
      answerVersion: QuestionAnswerVersionSchema,
    })
    .strict(),
  z.object({ operation: z.literal("question.cancel.v1"), question: QuestionGroupSchema }).strict(),
  z
    .object({
      operation: z.literal("native-approval.respond.v1"),
      approval: NativeApprovalRequestSchema,
    })
    .strict(),
  z.object({ operation: z.literal("session.start.v1"), action: PendingActionSchema }).strict(),
  z.object({ operation: z.literal("session.interrupt.v1"), action: PendingActionSchema }).strict(),
  z
    .object({ operation: z.literal("project.pause.v1"), execution: ProjectExecutionStateSchema })
    .strict(),
  z
    .object({ operation: z.literal("project.stop.v1"), execution: ProjectExecutionStateSchema })
    .strict(),
  z
    .object({ operation: z.literal("project.continue.v1"), execution: ProjectExecutionStateSchema })
    .strict(),
  z
    .object({
      operation: z.literal("model-policy.update.v1"),
      policy: StoredModelPolicyViewSchema,
      change: ModelPolicyChangeViewSchema,
    })
    .strict(),
  ...TerminalCommandResponseSchemas,
  ...ManagedWorkCommandResponseSchemas,
  ...RepositoryWorkspaceCommandResponseSchemas,
]);
export type WorkspaceCommandResponse = z.infer<typeof WorkspaceCommandResponseSchema>;

export const WorkspaceEventsRequestSchema = z
  .object({ cursor: HistoryCursorSchema, limit: z.number().int().min(1).max(512) })
  .strict();
export type WorkspaceEventsRequest = z.infer<typeof WorkspaceEventsRequestSchema>;
export const WorkspaceSubscribeRequestSchema = z
  .object({
    cursor: HistoryCursorSchema,
    signal: z.custom<AbortSignal>((value) => value instanceof AbortSignal).optional(),
  })
  .strict();
export type WorkspaceSubscribeRequest = z.infer<typeof WorkspaceSubscribeRequestSchema>;

export interface WorkspaceClientPort {
  read(request: WorkspaceReadRequest): Awaitable<WorkspaceResult<WorkspaceReadResponse>>;
  command(request: WorkspaceCommandRequest): Awaitable<WorkspaceResult<WorkspaceCommandResponse>>;
  events(request: WorkspaceEventsRequest): Awaitable<WorkspaceResult<HistoryPage>>;
  subscribe(
    request: WorkspaceSubscribeRequest,
  ): AsyncIterable<WorkspaceResult<z.infer<typeof HistoryEventSchema>>>;
}

export interface WorkspaceStorePort {
  read(
    context: WorkspaceAccessContext,
    request: WorkspaceReadRequest,
  ): WorkspaceResult<WorkspaceReadResponse>;
  command(
    context: WorkspaceCommandContext,
    request: WorkspaceCommandRequest,
  ): WorkspaceResult<WorkspaceCommandResponse>;
  events(
    context: WorkspaceAccessContext,
    request: WorkspaceEventsRequest,
  ): WorkspaceResult<HistoryPage>;
}

export type TerminalControl = Extract<
  WorkspaceCommandRequest,
  {
    operation:
      | "terminal.acquire.v1"
      | "terminal.release.v1"
      | "terminal.input.v1"
      | "terminal.resize.v1"
      | "terminal.interrupt.v1"
      | "terminal.stop.v1";
  }
>;
