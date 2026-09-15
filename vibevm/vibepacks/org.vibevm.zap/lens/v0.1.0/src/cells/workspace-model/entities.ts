/** Shared project, session, chat, execution and history DTOs. @scope spec://org.vibevm.zap/lens/PROP-005#root */
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  JsonValueSchema,
  MessageIdSchema,
} from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ArtifactRefIdSchema,
  AttemptIdSchema,
  ClientIdSchema,
  ExecutionHostIdSchema,
  HistoryEventIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  TaskIdSchema,
  TerminalIdSchema,
  TerminalLeaseIdSchema,
  WorkContextIdSchema,
} from "./ids.ts";

export const ActionAvailabilitySchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("available") }).strict(),
  z
    .object({
      state: z.literal("unavailable"),
      code: z.enum(["unsupported", "policy_denied", "not_configured", "busy", "stale"]),
      reason: z.string().min(1).max(2_000),
    })
    .strict(),
]);
export type ActionAvailability = z.infer<typeof ActionAvailabilitySchema>;

export const ProjectDescriptorSchema = z
  .object({
    projectId: ProjectIdSchema,
    displayName: z.string().min(1).max(256),
    repositoryRootRefs: z.array(z.string().min(3).max(256)).min(1).max(32),
    defaultContextId: WorkContextIdSchema,
    actions: z.record(z.string(), ActionAvailabilitySchema),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ProjectDescriptor = z.infer<typeof ProjectDescriptorSchema>;

const PlanningBindingSchema = z.discriminatedUnion("state", [
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
export const WorkContextDescriptorSchema = z
  .object({
    contextId: WorkContextIdSchema,
    projectId: ProjectIdSchema,
    displayName: z.string().min(1).max(256),
    workspaceRef: z.string().min(3).max(256),
    branchLabel: z.string().max(512).nullable(),
    revisionBinding: z.string().max(512).nullable(),
    planning: PlanningBindingSchema,
    coordinatorConversationId: ConversationIdSchema,
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type WorkContextDescriptor = z.infer<typeof WorkContextDescriptorSchema>;

export const NativeRefSchema = z
  .object({
    namespace: z.string().min(1).max(160),
    value: z.string().min(1).max(512),
    incarnation: DecimalSchema,
  })
  .strict();
export type NativeRef = z.infer<typeof NativeRefSchema>;

export const TerminalCapabilitySchema = z.discriminatedUnion("state", [
  z
    .object({
      state: z.literal("unavailable"),
      reason: z.enum(["native_session", "host_unsupported", "policy_disabled", "not_owned"]),
    })
    .strict(),
  z
    .object({
      state: z.literal("available"),
      terminalId: TerminalIdSchema,
      ownerClientId: ClientIdSchema.nullable(),
      controlEpoch: DecimalSchema,
      leaseId: TerminalLeaseIdSchema.nullable(),
      leaseExpiresAt: z.iso.datetime().nullable(),
      inputEnabled: z.boolean(),
    })
    .strict(),
]);
export type TerminalCapability = z.infer<typeof TerminalCapabilitySchema>;

export const CoordinatorSessionStateSchema = z.enum([
  "starting",
  "bootstrapping",
  "ready",
  "running",
  "waiting_for_user",
  "pausing",
  "paused",
  "stopping",
  "stopped",
  "failed",
]);
export const CoordinatorSessionSchema = z
  .object({
    sessionId: AgentSessionIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    conversationId: ConversationIdSchema,
    coordinatorActorId: ActorIdSchema,
    role: z.literal("coordinator"),
    launchOrigin: z.enum(["lens", "user"]),
    interactionKind: z.enum(["structured", "native_harness", "owned_terminal"]),
    hostId: ExecutionHostIdSchema,
    nativeRef: NativeRefSchema.nullable(),
    terminalId: TerminalIdSchema.nullable().optional(),
    terminal: TerminalCapabilitySchema,
    state: CoordinatorSessionStateSchema,
    actions: z.record(z.string(), ActionAvailabilitySchema),
    bootstrapBasis: z.string().min(1).max(512).nullable(),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type CoordinatorSession = z.infer<typeof CoordinatorSessionSchema>;

export const ProjectExecutionStateSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    state: z.enum([
      "uninitialized",
      "running",
      "pausing",
      "paused",
      "stopping",
      "stopped",
      "continuing",
      "uncertain",
    ]),
    sessionId: AgentSessionIdSchema.nullable(),
    processEpoch: z.string().min(1).max(160).nullable(),
    pendingAction: z
      .object({
        action: z.enum(["pause", "stop", "continue"]),
        clientRequestId: ClientRequestIdSchema,
        previousState: z.enum([
          "uninitialized",
          "running",
          "pausing",
          "paused",
          "stopping",
          "stopped",
          "continuing",
          "uncertain",
        ]),
        observation: z.enum(["requested", "settled", "unsupported", "refused", "uncertain"]),
        reasonMarkdown: z.string().min(1).max(8_000),
      })
      .strict()
      .nullable(),
    lastAction: z
      .object({
        action: z.enum(["pause", "stop", "continue"]),
        clientRequestId: ClientRequestIdSchema,
        observation: z.enum(["requested", "settled", "unsupported", "refused", "uncertain"]),
      })
      .strict()
      .nullable(),
    revision: DecimalSchema,
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ProjectExecutionState = z.infer<typeof ProjectExecutionStateSchema>;

export const AgentDescriptorSchema = z
  .object({
    actorId: ActorIdSchema,
    sessionId: AgentSessionIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    role: z.enum(["coordinator", "worker"]),
    parentActorId: ActorIdSchema.nullable(),
    displayName: z.string().min(1).max(256),
    executionMode: z.enum(["native", "managed"]),
    hostId: ExecutionHostIdSchema,
    nativeRef: NativeRefSchema.nullable(),
    terminalId: TerminalIdSchema.nullable().optional(),
    state: z.enum(["starting", "active", "waiting_for_user", "stopped", "failed", "unknown"]),
    revision: DecimalSchema,
  })
  .strict();
export type AgentDescriptor = z.infer<typeof AgentDescriptorSchema>;

export const AgentRelationshipSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    fromActorId: ActorIdSchema,
    toActorId: ActorIdSchema,
    kind: z.enum(["parent", "delegated", "message"]),
    provenance: z.enum(["host", "broker", "server"]),
    sourceEventId: z.string().min(1).max(512).nullable(),
    observedAt: z.iso.datetime(),
  })
  .strict();
export type AgentRelationship = z.infer<typeof AgentRelationshipSchema>;

export const AgentNetworkSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    agents: z.array(AgentDescriptorSchema),
    relationships: z.array(AgentRelationshipSchema),
    coverage: z.discriminatedUnion("state", [
      z.object({ state: z.literal("complete") }).strict(),
      z.object({ state: z.literal("partial"), reason: z.string().min(1).max(2_000) }).strict(),
    ]),
  })
  .strict();
export type AgentNetwork = z.infer<typeof AgentNetworkSchema>;

export const AgentOutputItemSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    actorId: ActorIdSchema,
    sessionId: AgentSessionIdSchema,
    runId: RunIdSchema.nullable(),
    sequence: DecimalSchema,
    kind: z.enum(["text", "tool", "execution", "status", "error"]),
    bodyMarkdown: z.string().max(64_000),
    artifactRefs: z.array(ArtifactRefIdSchema).max(256),
    nativeRef: NativeRefSchema.nullable(),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type AgentOutputItem = z.infer<typeof AgentOutputItemSchema>;

const BasisRefSchema = z
  .object({ sourceBasisRef: z.string().min(1).max(512), planRevision: DecimalSchema.nullable() })
  .strict();
export const RunAttemptSchema = z
  .object({
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    taskId: TaskIdSchema,
    parentTaskId: TaskIdSchema.nullable(),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sessionId: AgentSessionIdSchema,
    actorId: ActorIdSchema,
    hostId: ExecutionHostIdSchema,
    executionMode: z.enum(["native", "managed"]),
    state: z.enum([
      "queued",
      "starting",
      "running",
      "waiting_for_user",
      "completed",
      "failed",
      "cancelled",
      "uncertain",
    ]),
    parentRunId: RunIdSchema.nullable(),
    depth: z.number().int().min(0).max(100),
    basis: BasisRefSchema,
    resultArtifactRefs: z.array(ArtifactRefIdSchema).max(256),
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type RunAttempt = z.infer<typeof RunAttemptSchema>;

export const ChatDeliveryStateSchema = z.enum([
  "persisted",
  "queued",
  "host_accepted",
  "running",
  "answered",
  "failed",
  "uncertain",
]);
export const ChatMessageSchema = z
  .object({
    messageId: MessageIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    conversationId: ConversationIdSchema,
    senderActorId: ActorIdSchema.nullable(),
    role: z.enum(["user", "assistant", "execution", "system"]),
    bodyMarkdown: z.string().min(1).max(64_000),
    artifactRefs: z.array(ArtifactRefIdSchema).max(256),
    correlationId: z.string().min(3).max(160).nullable(),
    causationMessageId: MessageIdSchema.nullable(),
    deliveryState: ChatDeliveryStateSchema,
    revision: DecimalSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ChatMessage = z.infer<typeof ChatMessageSchema>;

export const HistoryEventSchema = z
  .object({
    historyEventId: HistoryEventIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema.nullable(),
    globalSequence: DecimalSchema,
    projectSequence: DecimalSchema,
    sourceSequence: DecimalSchema.nullable(),
    kind: z.string().min(3).max(160),
    source: z.enum(["lens", "host", "zap"]),
    actorId: ActorIdSchema.nullable(),
    occurrenceAt: z.iso.datetime(),
    ingestedAt: z.iso.datetime(),
    sourceEventId: z.string().min(1).max(512).nullable(),
    correlationId: z.string().min(3).max(160).nullable(),
    causationId: z.string().min(3).max(160).nullable(),
    planProvenance: z
      .object({
        previousPlanId: z.string().min(1).max(256).nullable(),
        proposedPlanId: z.string().min(1).max(256).nullable(),
        sourceBasisRef: z.string().min(1).max(512),
        appliedRevision: DecimalSchema.nullable(),
      })
      .strict()
      .nullable(),
    payload: JsonValueSchema,
  })
  .strict();
export type HistoryEvent = z.infer<typeof HistoryEventSchema>;

export const TerminalEventSchema = z
  .object({
    terminalId: TerminalIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sequence: DecimalSchema,
    controlEpoch: DecimalSchema,
    kind: z.enum(["output", "input_echo", "resized", "control_changed", "exited", "gap"]),
    data: JsonValueSchema,
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalEvent = z.infer<typeof TerminalEventSchema>;
