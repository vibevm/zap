/**
 * Host-neutral coordinator, execution-host and strategy contracts.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-006#execution-backends
 * @scope spec://org.vibevm.zap/lens/PROP-009#project-lifecycle
 */
import { z } from "zod";
import { TaskSpecializationSchema } from "../execution-catalog/index.ts";
import {
  ActorIdSchema,
  ConversationIdSchema,
  DecimalSchema,
  JsonValueSchema,
  WorkspaceIdSchema,
  type JsonValue,
} from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ArtifactRefIdSchema,
  ExecutionHostIdSchema,
  NativeRefSchema,
  ManagedWorkspaceAssignmentSchema,
  ProjectPlanIdSchema,
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  TaskIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";

export const AgentRuntimeErrorSchema = z
  .object({
    code: z.enum([
      "invalid_input",
      "unsupported",
      "policy_denied",
      "not_found",
      "already_exists",
      "busy",
      "stale_epoch",
      "host_refused",
      "transport_lost",
      "protocol_error",
    ]),
    message: z.string().min(1).max(4_000),
    retry: z.enum(["never", "after_refresh", "after_reconcile"]),
  })
  .strict();
export type AgentRuntimeError = z.infer<typeof AgentRuntimeErrorSchema>;
export type AgentRuntimeResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: AgentRuntimeError };

export const CoordinatorCapabilitiesSchema = z
  .object({
    persistentThreads: z.boolean(),
    turnStart: z.boolean(),
    activeTurnSteer: z.boolean(),
    turnInterrupt: z.boolean(),
    historyRead: z.boolean(),
    nativeChildObservation: z.boolean(),
    nativeChildDirectInput: z.boolean(),
    structuredUserInput: z.boolean(),
    commandApproval: z.boolean(),
    managedTerminal: z.boolean(),
  })
  .strict();
export type CoordinatorCapabilities = z.infer<typeof CoordinatorCapabilitiesSchema>;

export const CoordinatorScopeSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    conversationId: ConversationIdSchema,
    coordinatorActorId: ActorIdSchema,
    hostId: ExecutionHostIdSchema,
  })
  .strict();
export type CoordinatorScope = z.infer<typeof CoordinatorScopeSchema>;

export const CoordinatorStartInputSchema = CoordinatorScopeSchema.extend({
  profileId: z.string().min(1).max(160),
  modelId: z.string().min(1).max(256).optional(),
  reasoningEffort: z
    .enum(["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"])
    .nullable()
    .optional(),
  cwd: z.string().min(1).max(32_768),
  bootstrapText: z.string().min(1).max(1_000_000),
  bootstrapBasis: z.string().min(1).max(512),
  agentScope: z
    .object({ workspaceId: WorkspaceIdSchema, conversationId: ConversationIdSchema })
    .strict()
    .nullable()
    .optional(),
  agentBinding: z
    .object({
      actorId: ActorIdSchema,
      adapterSessionId: z.string().min(24).max(160),
    })
    .strict()
    .nullable()
    .optional(),
}).strict();
export type CoordinatorStartInput = z.infer<typeof CoordinatorStartInputSchema>;

export const CoordinatorResumeInputSchema = CoordinatorScopeSchema.extend({
  profileId: z.string().min(1).max(160),
  modelId: z.string().min(1).max(256).optional(),
  reasoningEffort: z
    .enum(["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"])
    .nullable()
    .optional(),
  cwd: z.string().min(1).max(32_768),
  nativeThreadId: z.string().min(1).max(512),
  agentScope: CoordinatorStartInputSchema.shape.agentScope,
  agentBinding: CoordinatorStartInputSchema.shape.agentBinding,
}).strict();
export type CoordinatorResumeInput = z.infer<typeof CoordinatorResumeInputSchema>;

export const CoordinatorSessionDescriptorSchema = CoordinatorScopeSchema.extend({
  profileId: z.string().min(1).max(160),
  productId: z.string().min(1).max(160),
  role: z.literal("coordinator"),
  launchOrigin: z.literal("lens"),
  interactionKind: z.literal("structured"),
  state: z.enum([
    "bootstrapping",
    "ready",
    "running",
    "waiting_for_user",
    "pausing",
    "paused",
    "stopping",
    "stopped",
    "failed",
  ]),
  nativeThreadRef: NativeRefSchema,
  nativeSessionId: z.string().min(1).max(512),
  cwd: z.string().min(1).max(32_768),
  processEpoch: z.string().min(1).max(160),
  bootstrap: z.enum(["submitted", "not_observed"]),
  instructionSources: z.array(z.string().max(32_768)).max(1_000),
  capabilities: CoordinatorCapabilitiesSchema,
}).strict();
export type CoordinatorSessionDescriptor = z.infer<typeof CoordinatorSessionDescriptorSchema>;

export const CoordinatorTurnInputSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    text: z.string().min(1).max(1_000_000),
    clientMessageId: z.string().min(1).max(512),
  })
  .strict();
export type CoordinatorTurnInput = z.infer<typeof CoordinatorTurnInputSchema>;
export const CoordinatorSteerInputSchema = CoordinatorTurnInputSchema.extend({
  expectedNativeTurnId: z.string().min(1).max(512),
}).strict();
export type CoordinatorSteerInput = z.infer<typeof CoordinatorSteerInputSchema>;

export const CoordinatorTurnReceiptSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    nativeThreadId: z.string().min(1).max(512),
    nativeTurnId: z.string().min(1).max(512).nullable(),
    transportCorrelation: z
      .object({
        provenance: z.literal("transport_correlation"),
        clientMessageId: z.string().min(1).max(512),
        processEpoch: z.string().min(1).max(160),
      })
      .strict()
      .nullable()
      .optional(),
    observation: z.enum(["host_accepted", "running"]),
    processEpoch: z.string().min(1).max(160),
  })
  .strict();
export type CoordinatorTurnReceipt = z.infer<typeof CoordinatorTurnReceiptSchema>;

const HostRequestKindSchema = z.enum([
  "user_input",
  "command_approval",
  "file_approval",
  "permission_approval",
]);
export const PendingHostRequestSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    nativeThreadId: z.string().min(1).max(512),
    nativeTurnId: z.string().min(1).max(512).nullable(),
    nativeItemId: z.string().min(1).max(512),
    requestId: z.union([z.string(), z.number().int()]),
    processEpoch: z.string().min(1).max(160),
    kind: HostRequestKindSchema,
    body: JsonValueSchema,
  })
  .strict();
export type PendingHostRequest = z.infer<typeof PendingHostRequestSchema>;

export const HostRequestAnswerSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    requestId: z.union([z.string(), z.number().int()]),
    processEpoch: z.string().min(1).max(160),
    answer: JsonValueSchema,
  })
  .strict();
export type HostRequestAnswer = z.infer<typeof HostRequestAnswerSchema>;

export const CoordinatorEventSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    processEpoch: z.string().min(1).max(160),
    nativeThreadId: z.string().min(1).max(512).nullable(),
    nativeTurnId: z.string().min(1).max(512).nullable(),
    nativeItemId: z.string().min(1).max(512).nullable(),
    kind: z.enum([
      "process_started",
      "process_exited",
      "session_started",
      "session_resumed",
      "session_status",
      "turn_started",
      "turn_completed",
      "item_started",
      "item_completed",
      "message_delta",
      "native_child_observed",
      "native_message_observed",
      "host_request_pending",
      "host_request_resolved",
      "session_pause_requested",
      "session_paused",
      "session_stop_requested",
      "session_stopped",
      "session_continued",
      "lifecycle_uncertain",
      "protocol_error",
      "host_event_unmapped",
    ]),
    sourceEventId: z.string().min(1).max(512),
    transportCorrelation: z
      .object({
        provenance: z.literal("transport_correlation"),
        clientMessageId: z.string().min(1).max(512),
        processEpoch: z.string().min(1).max(160),
      })
      .strict()
      .nullable()
      .optional(),
    data: JsonValueSchema,
  })
  .strict();
export type CoordinatorEvent = z.infer<typeof CoordinatorEventSchema>;

export const CoordinatorHistorySchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    nativeThreadId: z.string().min(1).max(512),
    processEpoch: z.string().min(1).max(160),
    status: JsonValueSchema,
    turns: z.array(JsonValueSchema).max(100_000),
  })
  .strict();
export type CoordinatorHistory = z.infer<typeof CoordinatorHistorySchema>;

export const CoordinatorLifecycleCapabilitiesSchema = z
  .object({
    pause: z.enum(["interrupt_known_turns", "interrupt_owned_session", "unsupported"]),
    stop: z.enum(["owned_process", "unsupported"]),
    continue: z.enum(["saved_thread_resume", "unsupported"]),
    nativeChildren: z.enum(["known_active_turns", "unsupported"]),
  })
  .strict();
export type CoordinatorLifecycleCapabilities = z.infer<
  typeof CoordinatorLifecycleCapabilitiesSchema
>;

export const CoordinatorLifecycleInputSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    expectedProcessEpoch: z.string().min(1).max(160),
  })
  .strict();
export type CoordinatorLifecycleInput = z.infer<typeof CoordinatorLifecycleInputSchema>;

export const CoordinatorLifecycleReceiptSchema = z
  .object({
    coordinatorSessionId: AgentSessionIdSchema,
    action: z.enum(["pause", "stop", "continue"]),
    observation: z.enum(["requested", "settled", "unsupported", "refused", "uncertain"]),
    previousProcessEpoch: z.string().min(1).max(160),
    currentProcessEpoch: z.string().min(1).max(160).nullable(),
    nativeThreadId: z.string().min(1).max(512),
    targets: z
      .array(
        z
          .object({
            nativeThreadId: z.string().min(1).max(512),
            nativeTurnId: z.string().min(1).max(512),
            role: z.enum(["coordinator", "native_child"]),
            observation: z.enum(["requested", "settled", "unsupported", "refused", "uncertain"]),
          })
          .strict(),
      )
      .max(1_000),
    message: z.string().min(1).max(4_000),
  })
  .strict();
export type CoordinatorLifecycleReceipt = z.infer<typeof CoordinatorLifecycleReceiptSchema>;

export interface CoordinatorAdapter {
  readonly capabilities: CoordinatorCapabilities;
  readonly lifecycleCapabilities?: CoordinatorLifecycleCapabilities;
  start(input: CoordinatorStartInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>>;
  resume(input: CoordinatorResumeInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>>;
  readHistory(
    coordinatorSessionId: z.infer<typeof AgentSessionIdSchema>,
  ): Promise<AgentRuntimeResult<CoordinatorHistory>>;
  readNativeChildHistory(
    coordinatorSessionId: z.infer<typeof AgentSessionIdSchema>,
    nativeThreadId: string,
  ): Promise<AgentRuntimeResult<CoordinatorHistory>>;
  startTurn(input: CoordinatorTurnInput): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>>;
  steer(input: CoordinatorSteerInput): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>>;
  send(input: CoordinatorTurnInput): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>>;
  interrupt(
    coordinatorSessionId: z.infer<typeof AgentSessionIdSchema>,
    nativeTurnId: string,
  ): Promise<AgentRuntimeResult<void>>;
  respondToRequest(input: HostRequestAnswer): Promise<AgentRuntimeResult<void>>;
  restart(profileId: string): Promise<AgentRuntimeResult<readonly CoordinatorSessionDescriptor[]>>;
  pause?(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>>;
  stop?(input: CoordinatorLifecycleInput): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>>;
  continueSession?(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>>;
  subscribe(listener: (event: CoordinatorEvent) => void): () => void;
  close(): void;
}

export interface AgentHost {
  readonly hostId: z.infer<typeof ExecutionHostIdSchema>;
  readonly profileIds: readonly string[];
  openCoordinator(profileId: string): Promise<AgentRuntimeResult<CoordinatorAdapter>>;
}

export const WorkPacketSchema = z
  .object({
    taskId: TaskIdSchema,
    parentTaskId: TaskIdSchema.nullable(),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    planId: ProjectPlanIdSchema.nullable().default(null),
    workspaceAssignment: ManagedWorkspaceAssignmentSchema.nullable().default(null),
    goal: z.string().min(1).max(1_000_000),
    specialization: TaskSpecializationSchema.default("general"),
    contextRefs: z.array(ArtifactRefIdSchema).max(1_000),
    expectedResult: z.string().min(1).max(100_000),
    targetRefs: z.array(ProjectObjectReferenceSchema).max(256).default([]),
    sourceBasisRef: z.string().min(1).max(512),
    planRevision: DecimalSchema.nullable(),
    capabilities: z.array(z.string().min(1).max(160)).max(256),
    depth: z.number().int().min(0).max(100),
    budgets: z
      .object({
        maximumTurns: z.number().int().min(1).max(100_000),
        wallTimeMs: z.number().int().min(1_000).max(86_400_000),
      })
      .strict(),
    routing: z
      .object({
        preferredProduct: z.string().min(1).max(160).nullable(),
        executionMode: z.enum(["native", "managed"]),
      })
      .strict(),
  })
  .strict();
export type WorkPacket = z.infer<typeof WorkPacketSchema>;

export interface ExecutionStrategy {
  readonly id: string;
  propose(input: {
    readonly goal: string;
    readonly projectId: z.infer<typeof ProjectIdSchema>;
    readonly contextId: z.infer<typeof WorkContextIdSchema>;
    readonly contextRefs: readonly z.infer<typeof ArtifactRefIdSchema>[];
    readonly availableCapabilities: readonly string[];
  }): Promise<AgentRuntimeResult<readonly WorkPacket[]>>;
}

export function runtimeFailure(
  code: AgentRuntimeError["code"],
  message: string,
  retry: AgentRuntimeError["retry"] = "never",
): AgentRuntimeResult<never> {
  return { ok: false, error: { code, message, retry } };
}

export function jsonValue(value: unknown): AgentRuntimeResult<JsonValue> {
  const parsed = JsonValueSchema.safeParse(value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : runtimeFailure("protocol_error", "Host value is not lossless JSON");
}
