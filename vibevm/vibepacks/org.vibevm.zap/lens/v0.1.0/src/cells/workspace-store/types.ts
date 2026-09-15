/** Trusted workspace-store assembly contracts. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  MessageIdSchema,
  ActorIdSchema,
  WorkspaceIdSchema,
  DecimalSchema,
  JsonValueSchema,
  PublicConnectionSchema,
  type ClientRequestId,
} from "../protocol/index.ts";
import {
  ActionAvailabilitySchema,
  AgentDescriptorSchema,
  AgentOutputItemSchema,
  AgentRelationshipSchema,
  CoordinatorLaunchOptionSchema,
  CoordinatorSessionSchema,
  AgentSessionIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  type AgentDescriptor,
  type AgentOutputItem,
  type AgentRelationship,
  type CoordinatorSession,
  type ChatMessage,
  type ProjectExecutionState,
  type HistoryEvent,
  type ProjectId,
  type WorkContextId,
  type ProjectDetail,
  type WorkspaceEventIngest,
  type WorkspaceResult,
  type WorkspaceStorePort,
  NativeApprovalIdSchema,
  NativeApprovalRequestSchema,
  NativeHostRequestIdentitySchema,
  NativeQuestionMapSchema,
  QuestionGroupDraftSchema,
  QuestionGroupIdSchema,
  WorkspaceAccessContextSchema,
  type AgentQuestionBinding,
  type NativeApprovalRequest,
  type NativeInteractionRecord,
  type QuestionGroup,
} from "../workspace-model/index.ts";

const PlanningRegistrationSchema = z.discriminatedUnion("state", [
  z
    .object({
      state: z.literal("configured"),
      storeId: z.string().min(3).max(160),
      campaignId: z.string().min(3).max(160),
      baseId: z.string().min(3).max(160),
    })
    .strict(),
  z.object({ state: z.literal("unavailable"), reason: z.string().min(1).max(2_000) }).strict(),
]);

export const TrustedProjectRegistrationSchema = z
  .object({
    registrationId: ClientRequestIdSchema,
    projectId: ProjectIdSchema,
    displayName: z.string().min(1).max(256),
    repositoryRootRefs: z.array(z.string().min(3).max(256)).min(1).max(32),
    actions: z.record(z.string(), ActionAvailabilitySchema),
    context: z
      .object({
        contextId: WorkContextIdSchema,
        displayName: z.string().min(1).max(256),
        workspaceRef: z.string().min(3).max(256),
        branchLabel: z.string().max(512).nullable(),
        revisionBinding: z.string().max(512).nullable(),
        planning: PlanningRegistrationSchema,
        coordinatorConversationId: ConversationIdSchema,
        brokerScope: z
          .object({
            workspaceId: WorkspaceIdSchema,
            conversationId: ConversationIdSchema,
          })
          .strict()
          .optional(),
      })
      .strict(),
    coordinatorLaunchOptions: z.array(CoordinatorLaunchOptionSchema).min(1).max(32),
    protected: z
      .object({ cwd: z.string().min(1).max(32_000), launchProfileRef: z.string().min(3).max(512) })
      .strict(),
  })
  .strict();
export type TrustedProjectRegistration = z.infer<typeof TrustedProjectRegistrationSchema>;

export const TrustedProjectLaunchSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    cwd: z.string().min(1).max(32_000),
    launchProfileRef: z.string().min(3).max(512),
    agentScope: z
      .object({ workspaceId: WorkspaceIdSchema, conversationId: ConversationIdSchema })
      .strict()
      .nullable(),
  })
  .strict();
export type TrustedProjectLaunch = z.infer<typeof TrustedProjectLaunchSchema>;

export const CoordinatorLaunchClaimSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    claimId: ClientRequestIdSchema,
    principalId: z.string().min(3).max(160),
    actorKey: z.string().min(3).max(320),
    clientRequestId: ClientRequestIdSchema,
    profileId: z.string().min(3).max(160),
    interactionKind: z.literal("structured"),
    state: z.enum(["starting", "needs_reconcile", "running", "failed", "stopped"]),
    sessionId: z.string().min(3).max(160).nullable(),
    actorId: z.string().min(3).max(160).nullable(),
    nativeThreadId: z.string().min(1).max(512).nullable(),
    processEpoch: z.string().min(1).max(160).nullable(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type CoordinatorLaunchClaim = z.infer<typeof CoordinatorLaunchClaimSchema>;

export const CoordinatorLaunchClaimInputSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    claimId: ClientRequestIdSchema,
    principalId: z.string().min(3).max(160),
    actorKey: z.string().min(3).max(320),
    clientRequestId: ClientRequestIdSchema,
    profileId: z.string().min(3).max(160),
    interactionKind: z.literal("structured"),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type CoordinatorLaunchClaimInput = z.infer<typeof CoordinatorLaunchClaimInputSchema>;

export const CoordinatorLaunchReceiptSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    claimId: ClientRequestIdSchema,
    sessionId: z.string().min(3).max(160),
    actorId: z.string().min(3).max(160),
    nativeThreadId: z.string().min(1).max(512),
    processEpoch: z.string().min(1).max(160),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type CoordinatorLaunchReceipt = z.infer<typeof CoordinatorLaunchReceiptSchema>;

export const ProjectLifecycleSettlementSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sessionId: AgentSessionIdSchema,
    clientRequestId: ClientRequestIdSchema,
    action: z.enum(["pause", "stop", "continue"]),
    observation: z.enum(["requested", "settled", "unsupported", "refused", "uncertain"]),
    processEpoch: z.string().min(1).max(160).nullable(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ProjectLifecycleSettlement = z.infer<typeof ProjectLifecycleSettlementSchema>;

export const ChatDispatchClaimSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    messageId: MessageIdSchema,
    sessionId: AgentSessionIdSchema,
    processEpoch: z.string().min(1).max(160),
  })
  .strict();
export type ChatDispatchClaim = z.infer<typeof ChatDispatchClaimSchema>;

export const ChatDispatchSettlementSchema = ChatDispatchClaimSchema.extend({
  observation: z.enum(["host_accepted", "uncertain", "failed"]),
  nativeTurnId: z.string().min(1).max(512).nullable(),
  updatedAt: z.iso.datetime(),
}).strict();
export type ChatDispatchSettlement = z.infer<typeof ChatDispatchSettlementSchema>;

export const ObservedChatReplySchema = z
  .object({
    sourceEventId: z.string().min(1).max(512),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sessionId: AgentSessionIdSchema,
    processEpoch: z.string().min(1).max(160),
    nativeTurnId: z.string().min(1).max(512),
    actorId: ActorIdSchema,
    bodyMarkdown: z.string().min(1).max(64_000),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type ObservedChatReply = z.infer<typeof ObservedChatReplySchema>;

export const ObservedAgentOutputSchema = z
  .object({
    sourceEventId: z.string().min(1).max(512),
    output: AgentOutputItemSchema.omit({ sequence: true }),
  })
  .strict();
export type ObservedAgentOutput = z.infer<typeof ObservedAgentOutputSchema>;

export const AgentScopeResolutionSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();
export type AgentScopeResolution = z.infer<typeof AgentScopeResolutionSchema>;

export const NativeQuestionRecordInputSchema = z
  .object({
    access: WorkspaceAccessContextSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    clientRequestId: ClientRequestIdSchema,
    conversationId: ConversationIdSchema,
    draft: QuestionGroupDraftSchema,
    identity: NativeHostRequestIdentitySchema,
    questionMap: NativeQuestionMapSchema,
  })
  .strict();
export type NativeQuestionRecordInput = z.infer<typeof NativeQuestionRecordInputSchema>;

export const NativeApprovalRecordInputSchema = z
  .object({
    approval: NativeApprovalRequestSchema,
    identity: NativeHostRequestIdentitySchema,
  })
  .strict();
export type NativeApprovalRecordInput = z.infer<typeof NativeApprovalRecordInputSchema>;

export const NativeResponsePreparationSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    questionGroupId: QuestionGroupIdSchema.nullable(),
    nativeApprovalId: NativeApprovalIdSchema.nullable(),
    expectedRevision: DecimalSchema,
    currentProcessEpoch: z.string().min(1).max(160),
    response: JsonValueSchema,
    updatedAt: z.iso.datetime(),
  })
  .strict()
  .refine((value) => (value.questionGroupId === null) !== (value.nativeApprovalId === null), {
    message: "exactly one interaction identity is required",
  });
export type NativeResponsePreparation = z.infer<typeof NativeResponsePreparationSchema>;

export const NativeResponseSettlementSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    processEpoch: z.string().min(1).max(160),
    requestId: z.union([z.string().min(1).max(512), z.number().int()]),
    observation: z.enum(["host_accepted", "resolved", "refused", "uncertain", "stale"]),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type NativeResponseSettlement = z.infer<typeof NativeResponseSettlementSchema>;

export const AgentQuestionRecordInputSchema = z
  .object({
    access: WorkspaceAccessContextSchema,
    actor: PublicConnectionSchema,
    clientRequestId: ClientRequestIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    draft: QuestionGroupDraftSchema,
  })
  .strict();
export type AgentQuestionRecordInput = z.infer<typeof AgentQuestionRecordInputSchema>;

export const AgentAnswerSettlementSchema = z
  .object({
    questionGroupId: QuestionGroupIdSchema,
    expectedRevision: DecimalSchema,
    observation: z.enum(["persisted", "refused", "uncertain"]),
    messageId: MessageIdSchema.nullable(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type AgentAnswerSettlement = z.infer<typeof AgentAnswerSettlementSchema>;

export interface OpenWorkspaceStoreOptions {
  readonly databasePath: string;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export interface WorkspaceStore extends WorkspaceStorePort {
  registerProject(input: TrustedProjectRegistration): WorkspaceResult<ProjectDetail>;
  resolveProjectLaunch(
    projectId: z.infer<typeof ProjectIdSchema>,
    contextId: z.infer<typeof WorkContextIdSchema>,
  ): WorkspaceResult<TrustedProjectLaunch>;
  upsertCoordinatorSession(session: CoordinatorSession): WorkspaceResult<CoordinatorSession>;
  upsertAgent(agent: AgentDescriptor): WorkspaceResult<AgentDescriptor>;
  recordAgentRelationship(relationship: AgentRelationship): WorkspaceResult<AgentRelationship>;
  appendAgentOutput(output: AgentOutputItem): WorkspaceResult<AgentOutputItem>;
  appendObservedAgentOutput(input: ObservedAgentOutput): WorkspaceResult<AgentOutputItem | null>;
  ingestEvent(input: WorkspaceEventIngest): WorkspaceResult<null>;
  /** Trusted runtime seam: returns the committed history row, or null on replay. */
  ingestObservedEvent(input: WorkspaceEventIngest): WorkspaceResult<HistoryEvent | null>;
  claimCoordinatorLaunch(
    input: CoordinatorLaunchClaimInput,
  ): WorkspaceResult<{ claim: CoordinatorLaunchClaim; acquired: boolean }>;
  recordCoordinatorReceipt(
    input: CoordinatorLaunchReceipt,
  ): WorkspaceResult<CoordinatorLaunchClaim>;
  markCoordinatorClaimState(
    projectId: ProjectId,
    contextId: WorkContextId,
    claimId: ClientRequestId,
    state: CoordinatorLaunchClaim["state"],
    updatedAt: string,
  ): WorkspaceResult<CoordinatorLaunchClaim>;
  readCoordinatorClaim(
    projectId: ProjectId,
    contextId: WorkContextId,
  ): WorkspaceResult<CoordinatorLaunchClaim | null>;
  readProjectExecution(
    projectId: ProjectId,
    contextId: WorkContextId,
  ): WorkspaceResult<ProjectExecutionState>;
  requestProjectLifecycle(
    context: Parameters<WorkspaceStorePort["command"]>[0],
    request: Extract<
      Parameters<WorkspaceStorePort["command"]>[1],
      { operation: "project.pause.v1" | "project.stop.v1" | "project.continue.v1" }
    >,
  ): WorkspaceResult<{ execution: ProjectExecutionState; acquired: boolean }>;
  settleProjectLifecycle(input: ProjectLifecycleSettlement): WorkspaceResult<ProjectExecutionState>;
  queueChat(messageId: z.infer<typeof MessageIdSchema>): WorkspaceResult<ChatMessage>;
  nextQueuedChat(
    projectId: ProjectId,
    contextId: WorkContextId,
  ): WorkspaceResult<ChatMessage | null>;
  claimChatDispatch(
    input: ChatDispatchClaim,
  ): WorkspaceResult<{ message: ChatMessage; acquired: boolean }>;
  releaseChatDispatch(input: ChatDispatchClaim): WorkspaceResult<ChatMessage>;
  settleChatDispatch(input: ChatDispatchSettlement): WorkspaceResult<ChatMessage>;
  appendObservedChatReply(input: ObservedChatReply): WorkspaceResult<ChatMessage | null>;
  resolveAgentScope(
    workspaceId: z.infer<typeof WorkspaceIdSchema>,
    conversationId: z.infer<typeof ConversationIdSchema>,
  ): WorkspaceResult<AgentScopeResolution>;
  recordNativeQuestion(
    input: NativeQuestionRecordInput,
  ): WorkspaceResult<{ question: QuestionGroup; interaction: NativeInteractionRecord }>;
  recordNativeApproval(input: NativeApprovalRecordInput): WorkspaceResult<NativeApprovalRequest>;
  readNativeInteractionForQuestion(
    questionGroupId: z.infer<typeof QuestionGroupIdSchema>,
  ): WorkspaceResult<NativeInteractionRecord | null>;
  prepareNativeResponse(input: NativeResponsePreparation): WorkspaceResult<NativeInteractionRecord>;
  settleNativeResponse(input: NativeResponseSettlement): WorkspaceResult<NativeInteractionRecord>;
  pendingNativeResponses(
    projectId: ProjectId,
    contextId: WorkContextId,
    limit: number,
  ): WorkspaceResult<readonly NativeInteractionRecord[]>;
  recordAgentQuestion(
    input: AgentQuestionRecordInput,
  ): WorkspaceResult<{ question: QuestionGroup; binding: AgentQuestionBinding }>;
  readAgentQuestionBinding(
    questionGroupId: z.infer<typeof QuestionGroupIdSchema>,
  ): WorkspaceResult<AgentQuestionBinding | null>;
  settleAgentAnswer(input: AgentAnswerSettlement): WorkspaceResult<AgentQuestionBinding>;
  close(): WorkspaceResult<null>;
}

export const StoredCoordinatorSessionSchema = CoordinatorSessionSchema;
export const StoredAgentSchema = AgentDescriptorSchema;
export const StoredAgentRelationshipSchema = AgentRelationshipSchema;
export const StoredAgentOutputSchema = AgentOutputItemSchema;
