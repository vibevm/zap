/**
 * The lens/1 wire contract shared by the broker, adapters and clients.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-001#protocol
 * @example
 * const parsed = MessageEnvelopeSchema.safeParse(candidate);
 * if (parsed.success) consume(parsed.data.messageId);
 */
import { z } from "zod";

const opaqueId = z
  .string()
  .min(3)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/);

/** @implements spec://org.vibevm.zap/lens/PROP-001#protocol */
export const WorkspaceIdSchema = opaqueId.brand<"WorkspaceId">();
export type WorkspaceId = z.infer<typeof WorkspaceIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#protocol */
export const ConversationIdSchema = opaqueId.brand<"ConversationId">();
export type ConversationId = z.infer<typeof ConversationIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#identity */
export const PrincipalIdSchema = opaqueId.brand<"PrincipalId">();
export type PrincipalId = z.infer<typeof PrincipalIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#identity */
export const ActorIdSchema = opaqueId.brand<"ActorId">();
export type ActorId = z.infer<typeof ActorIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#identity */
export const BindingIdSchema = opaqueId.brand<"BindingId">();
export type BindingId = z.infer<typeof BindingIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#delivery */
export const MessageIdSchema = opaqueId.brand<"MessageId">();
export type MessageId = z.infer<typeof MessageIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#delivery */
export const DeliveryIdSchema = opaqueId.brand<"DeliveryId">();
export type DeliveryId = z.infer<typeof DeliveryIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#questions */
export const QuestionIdSchema = opaqueId.brand<"QuestionId">();
export type QuestionId = z.infer<typeof QuestionIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#protocol */
export const ClientRequestIdSchema = opaqueId.brand<"ClientRequestId">();
export type ClientRequestId = z.infer<typeof ClientRequestIdSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-001#protocol */
export const DecimalSchema = z
  .string()
  .regex(/^(0|[1-9][0-9]*)$/)
  .brand<"LosslessDecimal">();
export type LosslessDecimal = z.infer<typeof DecimalSchema>;

export const CredentialSchema = z.string().min(24).max(512).brand<"Credential">();
export type Credential = z.infer<typeof CredentialSchema>;

export const CapabilitySchema = z.enum([
  "message:emit",
  "question:ask",
  "question:answer",
  "question:amend",
  "question:cancel",
  "inbox:read",
  "inbox:ack",
  "inbox:forward",
  "events:read",
  "actor:delegate",
  "actor:expire",
  "plan:propose",
  "plan:approve",
  "plan:execute",
]);
export type Capability = z.infer<typeof CapabilitySchema>;

export const PrincipalKindSchema = z.enum([
  "agent",
  "viewer",
  "human_responder",
  "human_plan_approver",
  "trusted_execution_adapter",
]);
export type PrincipalKind = z.infer<typeof PrincipalKindSchema>;

export const HostKindSchema = z.enum([
  "codex",
  "claude_code",
  "opencode",
  "qwen_code",
  "lens",
  "test",
]);
export type HostKind = z.infer<typeof HostKindSchema>;

export const HostBindingSchema = z
  .object({
    kind: HostKindSchema,
    sessionId: z.string().min(1).max(512).optional(),
    subagentId: z.string().min(1).max(512).optional(),
    provenance: z.enum(["attested", "explicit_handle", "unverified"]),
  })
  .strict();
export type HostBinding = z.infer<typeof HostBindingSchema>;

export const ReplyPolicySchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("retain") }).strict(),
  z.object({ kind: z.literal("forward_parent") }).strict(),
]);
export type ReplyPolicy = z.infer<typeof ReplyPolicySchema>;

export const ActorStateSchema = z.enum(["active", "expired"]);
export type ActorState = z.infer<typeof ActorStateSchema>;

export const ActorDescriptorSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    parentActorId: ActorIdSchema.nullable(),
    state: ActorStateSchema,
    capabilities: z.array(CapabilitySchema),
    hostKind: HostKindSchema,
    hostProvenance: HostBindingSchema.shape.provenance,
  })
  .strict();
export type ActorDescriptor = z.infer<typeof ActorDescriptorSchema>;

export const ActorHandleSchema = z
  .object({
    actorId: ActorIdSchema,
    bindingId: BindingIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    generation: DecimalSchema,
  })
  .strict();
export type ActorHandle = z.infer<typeof ActorHandleSchema>;

export const BindingCredentialsSchema = z
  .object({
    bindingToken: CredentialSchema,
    resumeCredential: CredentialSchema,
  })
  .strict();
export type BindingCredentials = z.infer<typeof BindingCredentialsSchema>;

export const ConnectionSchema = z
  .object({
    actor: ActorDescriptorSchema,
    handle: ActorHandleSchema,
    credentials: BindingCredentialsSchema,
  })
  .strict();
export type Connection = z.infer<typeof ConnectionSchema>;

export const PublicConnectionSchema = ConnectionSchema.omit({ credentials: true });
export type PublicConnection = z.infer<typeof PublicConnectionSchema>;

/** Remove credential material before a connection crosses a trusted adapter boundary. */
export function publicConnection(connection: Connection): PublicConnection {
  return PublicConnectionSchema.parse({
    actor: connection.actor,
    handle: connection.handle,
  });
}

export const BindingAuthSchema = z
  .object({
    principalToken: CredentialSchema,
    bindingToken: CredentialSchema,
  })
  .strict();
export type BindingAuth = z.infer<typeof BindingAuthSchema>;

export const PrincipalAuthSchema = z.object({ principalToken: CredentialSchema }).strict();
export type PrincipalAuth = z.infer<typeof PrincipalAuthSchema>;

export const BrokerErrorCodeSchema = z.enum([
  "invalid_input",
  "unauthorized",
  "forbidden",
  "not_found",
  "conflict",
  "stale_revision",
  "already_answered",
  "stale_binding",
  "idempotency_conflict",
  "backpressure",
  "resync_required",
  "unsupported_operation",
  "storage_failure",
  "closed",
]);
export type BrokerErrorCode = z.infer<typeof BrokerErrorCodeSchema>;

export const BrokerErrorSchema = z
  .object({
    code: BrokerErrorCodeSchema,
    message: z.string().startsWith("violates REQ spec://"),
    details: z.record(z.string(), z.unknown()).optional(),
  })
  .strict();
export type BrokerError = z.infer<typeof BrokerErrorSchema>;

export type Result<T, E = BrokerError> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export const MessageKindSchema = z.enum([
  "notice.created",
  "question.created",
  "question.answered",
  "question.amended",
  "question.cancelled",
  "question.expired",
  "delivery.forwarded",
]);
export type MessageKind = z.infer<typeof MessageKindSchema>;

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { readonly [key: string]: JsonValue };

export const JsonValueSchema: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.null(),
    z.boolean(),
    z.number(),
    z.string(),
    z.array(JsonValueSchema),
    z.record(z.string(), JsonValueSchema),
  ]),
);

export const MessageEnvelopeSchema = z
  .object({
    protocol: z.literal("lens/1"),
    messageId: MessageIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    fromActorId: ActorIdSchema.nullable(),
    toActorId: ActorIdSchema.nullable(),
    kind: MessageKindSchema,
    correlationId: opaqueId.nullable(),
    causationId: MessageIdSchema.nullable(),
    sequence: DecimalSchema,
    payload: JsonValueSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type MessageEnvelope = z.infer<typeof MessageEnvelopeSchema>;

export const DeliveryObservationSchema = z.enum([
  "persisted",
  "offered",
  "host_accepted",
  "actor_acknowledged",
  "held",
  "rejected",
  "expired",
  "cancelled",
  "uncertain",
]);
export type DeliveryObservation = z.infer<typeof DeliveryObservationSchema>;

export const DeliverySchema = z
  .object({
    deliveryId: DeliveryIdSchema,
    message: MessageEnvelopeSchema,
    logicalRecipientActorId: ActorIdSchema,
    recipientActorId: ActorIdSchema,
    forwardedFromDeliveryId: DeliveryIdSchema.nullable(),
    acknowledgedAt: z.iso.datetime().nullable(),
    observations: z.array(DeliveryObservationSchema),
  })
  .strict();
export type Delivery = z.infer<typeof DeliverySchema>;

export const QuestionStateSchema = z.enum(["open", "answered", "cancelled", "expired"]);
export type QuestionState = z.infer<typeof QuestionStateSchema>;

export const AnswerModeSchema = z.enum(["free_text", "single_choice"]);
export const QuestionChoiceSchema = z
  .object({ id: opaqueId, label: z.string().min(1).max(512) })
  .strict();

export const QuestionSchema = z
  .object({
    questionId: QuestionIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    originActorId: ActorIdSchema,
    prompt: z.string().min(1).max(16_384),
    answerMode: AnswerModeSchema,
    choices: z.array(QuestionChoiceSchema).max(100),
    independentWorkAvailable: z.boolean(),
    replyPolicy: ReplyPolicySchema,
    state: QuestionStateSchema,
    revision: DecimalSchema,
    deadlineAt: z.iso.datetime().nullable(),
    answer: JsonValueSchema.nullable(),
  })
  .strict();
export type Question = z.infer<typeof QuestionSchema>;

export const EnrollPrincipalInputSchema = z
  .object({
    kind: PrincipalKindSchema,
    workspaceIds: z.array(WorkspaceIdSchema).min(1),
    conversationIds: z.array(ConversationIdSchema).min(1),
    capabilities: z.array(CapabilitySchema),
  })
  .strict();
export type EnrollPrincipalInput = z.infer<typeof EnrollPrincipalInputSchema>;

export const PrincipalEnrollmentSchema = z
  .object({ principalId: PrincipalIdSchema, principalToken: CredentialSchema })
  .strict();
export type PrincipalEnrollment = z.infer<typeof PrincipalEnrollmentSchema>;

export const ConnectInputSchema = z
  .object({
    principalToken: CredentialSchema,
    clientRequestId: ClientRequestIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    capabilities: z.array(CapabilitySchema),
    host: HostBindingSchema,
    replyPolicy: ReplyPolicySchema.default({ kind: "retain" }),
  })
  .strict();
export type ConnectInput = z.input<typeof ConnectInputSchema>;

export const ResumeInputSchema = z
  .object({
    principalToken: CredentialSchema,
    clientRequestId: ClientRequestIdSchema,
    actorId: ActorIdSchema,
    resumeCredential: CredentialSchema,
    host: HostBindingSchema,
  })
  .strict();
export type ResumeInput = z.infer<typeof ResumeInputSchema>;

export const DelegateInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    capabilities: z.array(CapabilitySchema),
    host: HostBindingSchema,
    replyPolicy: ReplyPolicySchema.default({ kind: "retain" }),
  })
  .strict();
export type DelegateInput = z.input<typeof DelegateInputSchema>;

export const EmitInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    toActorId: ActorIdSchema,
    payload: JsonValueSchema,
    correlationId: opaqueId.nullable().optional(),
    causationId: MessageIdSchema.nullable().optional(),
  })
  .strict();
export type EmitInput = z.infer<typeof EmitInputSchema>;

export const AskInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    prompt: z.string().min(1).max(16_384),
    answerMode: AnswerModeSchema,
    choices: z.array(QuestionChoiceSchema).max(100).default([]),
    independentWorkAvailable: z.boolean().default(true),
    deadlineAt: z.iso.datetime().nullable().optional(),
  })
  .strict()
  .superRefine((value, context) => {
    if (value.answerMode === "single_choice" && value.choices.length < 2) {
      context.addIssue({
        code: "custom",
        message:
          "violates REQ spec://org.vibevm.zap/lens/PROP-001#questions: single-choice questions need at least two choices; fix surface: provide two or more stable choices",
      });
    }
    const ids = value.choices.map((choice) => choice.id);
    if (new Set(ids).size !== ids.length) {
      context.addIssue({
        code: "custom",
        message:
          "violates REQ spec://org.vibevm.zap/lens/PROP-001#questions: choice IDs must be unique; fix surface: assign one stable ID to each distinct choice",
      });
    }
  });
export type AskInput = z.input<typeof AskInputSchema>;

export const AnswerQuestionInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    questionId: QuestionIdSchema,
    expectedRevision: DecimalSchema,
    answer: JsonValueSchema,
  })
  .strict();
export type AnswerQuestionInput = z.infer<typeof AnswerQuestionInputSchema>;

export const AmendAnswerInputSchema = AnswerQuestionInputSchema;
export type AmendAnswerInput = AnswerQuestionInput;

export const CancelQuestionInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    questionId: QuestionIdSchema,
    expectedRevision: DecimalSchema,
  })
  .strict();
export type CancelQuestionInput = z.infer<typeof CancelQuestionInputSchema>;

export const InboxInputSchema = z
  .object({
    afterSequence: DecimalSchema.default(DecimalSchema.parse("0")),
    limit: z.number().int().min(1).max(100).default(50),
  })
  .strict();
export type InboxInput = z.input<typeof InboxInputSchema>;

export const AckInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    deliveryIds: z.array(DeliveryIdSchema).min(1).max(100),
  })
  .strict();
export type AckInput = z.infer<typeof AckInputSchema>;

export const AckResultSchema = z
  .object({ acknowledgedDeliveryIds: z.array(DeliveryIdSchema) })
  .strict();
export type AckResult = z.infer<typeof AckResultSchema>;

export const ExpireActorInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    actorId: ActorIdSchema,
  })
  .strict();
export type ExpireActorInput = z.infer<typeof ExpireActorInputSchema>;

export const ForwardInboxInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    fromActorId: ActorIdSchema,
    toActorId: ActorIdSchema,
    limit: z.number().int().min(1).max(100).default(50),
  })
  .strict();
export type ForwardInboxInput = z.input<typeof ForwardInboxInputSchema>;

export const ForwardResultSchema = z
  .object({ forwardedDeliveryIds: z.array(DeliveryIdSchema) })
  .strict();
export type ForwardResult = z.infer<typeof ForwardResultSchema>;

export const EventsInputSchema = z
  .object({
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    afterSequence: DecimalSchema.default(DecimalSchema.parse("0")),
    limit: z.number().int().min(1).max(100).default(50),
  })
  .strict();
export type EventsInput = z.input<typeof EventsInputSchema>;

export const InboxPageSchema = z
  .object({
    deliveries: z.array(DeliverySchema),
    observationCursor: DecimalSchema,
    hasMore: z.boolean(),
  })
  .strict();
export type InboxPage = z.infer<typeof InboxPageSchema>;

export const EventPageSchema = z
  .object({
    events: z.array(MessageEnvelopeSchema),
    observationCursor: DecimalSchema,
    hasMore: z.boolean(),
  })
  .strict();
export type EventPage = z.infer<typeof EventPageSchema>;

/** @implements spec://org.vibevm.zap/lens/PROP-002#interaction */
export const ScopedListInputSchema = z
  .object({
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    limit: z.number().int().min(1).max(100).default(50),
  })
  .strict();
export type ScopedListInput = z.input<typeof ScopedListInputSchema>;

export const ActorViewSchema = z
  .object({
    actor: ActorDescriptorSchema,
    label: z.string().min(1).max(256),
    eligiblePlanTarget: z.boolean(),
  })
  .strict();
export type ActorView = z.infer<typeof ActorViewSchema>;

export const ActorListSchema = z
  .object({ actors: z.array(ActorViewSchema), hasMore: z.boolean() })
  .strict();
export type ActorList = z.infer<typeof ActorListSchema>;

export const QuestionViewRecordSchema = z
  .object({
    question: QuestionSchema,
    addressedActorLabel: z.string().min(1).max(256),
    amendmentCount: DecimalSchema,
  })
  .strict();
export type QuestionViewRecord = z.infer<typeof QuestionViewRecordSchema>;

export const QuestionListSchema = z
  .object({ questions: z.array(QuestionViewRecordSchema), hasMore: z.boolean() })
  .strict();
export type QuestionList = z.infer<typeof QuestionListSchema>;

export const PrincipalEmitInputSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    toActorId: ActorIdSchema,
    payload: JsonValueSchema,
    correlationId: opaqueId.nullable().optional(),
    causationId: MessageIdSchema.nullable().optional(),
  })
  .strict();
export type PrincipalEmitInput = z.infer<typeof PrincipalEmitInputSchema>;
