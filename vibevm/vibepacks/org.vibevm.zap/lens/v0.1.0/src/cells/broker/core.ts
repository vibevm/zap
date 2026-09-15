/** @scope spec://org.vibevm.zap/lens/PROP-001#broker */
import { createHash, randomBytes, randomUUID } from "node:crypto";

import { z, type ZodType } from "zod";

import {
  ActorDescriptorSchema,
  ActorIdSchema,
  BindingIdSchema,
  BrokerErrorSchema,
  CapabilitySchema,
  ConversationIdSchema,
  CredentialSchema,
  DeliveryIdSchema,
  HostKindSchema,
  JsonValueSchema,
  MessageEnvelopeSchema,
  MessageIdSchema,
  PrincipalIdSchema,
  PrincipalAuthSchema,
  PrincipalKindSchema,
  QuestionSchema,
  ReplyPolicySchema,
  WorkspaceIdSchema,
  type ActorDescriptor,
  type ActorId,
  type ActorState,
  type BindingAuth,
  type BindingId,
  type BrokerErrorCode,
  type Capability,
  type ClientRequestId,
  type ConversationId,
  type Connection,
  type DeliveryId,
  type HostBinding,
  type HostKind,
  type JsonValue,
  type MessageEnvelope,
  type MessageId,
  type MessageKind,
  type PrincipalAuth,
  type PrincipalId,
  type PrincipalKind,
  type Question,
  type ReplyPolicy,
  type Result,
  type WorkspaceId,
} from "../protocol/index.ts";
import type { BrokerDatabase } from "./database.ts";

const REQ = "spec://org.vibevm.zap/lens/PROP-001";

export function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

export function fail<T>(
  code: BrokerErrorCode,
  anchor: string,
  why: string,
  fix: string,
  details?: Readonly<Record<string, unknown>>,
): Result<T> {
  const error = {
    code,
    message: `violates REQ ${REQ}#${anchor}: ${why}; fix surface: ${fix}`,
    ...(details === undefined ? {} : { details }),
  };
  return { ok: false, error: BrokerErrorSchema.parse(error) };
}

const PrincipalRowSchema = z
  .object({
    principalId: PrincipalIdSchema,
    kind: PrincipalKindSchema,
    workspaceIdsJson: z.string(),
    conversationIdsJson: z.string(),
    capabilitiesJson: z.string(),
  })
  .strict();

const ActorRowSchema = z
  .object({
    actorId: ActorIdSchema,
    principalId: PrincipalIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    parentActorId: ActorIdSchema.nullable(),
    currentGeneration: z.bigint(),
    state: z.enum(["active", "expired"]),
    capabilitiesJson: z.string(),
    hostKind: HostKindSchema,
    hostSessionId: z.string().nullable(),
    hostSubagentId: z.string().nullable(),
    hostProvenance: z.enum(["attested", "explicit_handle", "unverified"]),
    replyPolicyJson: z.string(),
    resumeTokenHash: z.string(),
  })
  .strict();

const BindingRowSchema = ActorRowSchema.extend({
  bindingId: BindingIdSchema,
  bindingGeneration: z.bigint(),
  bindingActive: z.bigint(),
});

const QuestionRowSchema = z
  .object({
    questionId: z.string(),
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    originActorId: ActorIdSchema,
    prompt: z.string(),
    answerMode: z.enum(["free_text", "single_choice"]),
    choicesJson: z.string(),
    independentWorkAvailable: z.bigint(),
    replyPolicyJson: z.string(),
    state: z.enum(["open", "answered", "cancelled", "expired"]),
    revision: z.bigint(),
    deadlineAt: z.string().nullable(),
    answerJson: z.string().nullable(),
  })
  .strict();

const IdempotencyRowSchema = z.object({ requestDigest: z.string(), resultJson: z.string() });
const SequenceRowSchema = z.object({ value: z.bigint() });

export interface PrincipalContext {
  readonly principalId: PrincipalId;
  readonly kind: PrincipalKind;
  readonly workspaceIds: readonly WorkspaceId[];
  readonly conversationIds: readonly ConversationId[];
  readonly capabilities: readonly Capability[];
}

export interface BindingContext {
  readonly actorId: ActorId;
  readonly principalId: PrincipalId;
  readonly workspaceId: WorkspaceId;
  readonly conversationId: ConversationId;
  readonly parentActorId: ActorId | null;
  readonly currentGeneration: bigint;
  readonly state: ActorState;
  readonly capabilities: readonly Capability[];
  readonly hostKind: HostKind;
  readonly hostSessionId: string | null;
  readonly hostSubagentId: string | null;
  readonly hostProvenance: "attested" | "explicit_handle" | "unverified";
  readonly replyPolicy: ReplyPolicy;
  readonly resumeTokenHash: string;
  readonly bindingId: BindingId;
  readonly bindingGeneration: bigint;
  readonly bindingActive: bigint;
}

interface CapabilityContext {
  readonly capabilities: readonly Capability[];
}

export interface MessageWrite {
  readonly workspaceId: z.infer<typeof WorkspaceIdSchema>;
  readonly conversationId: z.infer<typeof ConversationIdSchema>;
  readonly fromActorId: ActorId | null;
  readonly toActorId: ActorId | null;
  readonly kind: MessageKind;
  readonly correlationId: string | null;
  readonly causationId: MessageId | null;
  readonly payload: JsonValue;
}

function parseJson<T>(schema: ZodType<T>, text: string): T {
  return schema.parse(JSON.parse(text));
}

function canonicalJson(value: JsonValue): string {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "number" ||
    typeof value === "string"
  ) {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const entries = Object.keys(value)
    .sort()
    .map((key) => {
      const child = value[key];
      if (child === undefined) {
        throw new Error(
          `violates REQ ${REQ}#protocol: canonical JSON contained undefined; fix surface: validate the request before hashing`,
        );
      }
      return `${JSON.stringify(key)}:${canonicalJson(child)}`;
    });
  return `{${entries.join(",")}}`;
}

function digest(value: unknown): string {
  const json = JsonValueSchema.parse(JSON.parse(JSON.stringify(value)));
  return createHash("sha256").update(canonicalJson(json)).digest("hex");
}

export function credentialHash(value: string): string {
  return digest(value);
}

export abstract class BrokerCore {
  protected readonly database: BrokerDatabase;
  protected readonly clock: () => Date;
  private closed = false;

  constructor(database: BrokerDatabase, clock: () => Date) {
    this.database = database;
    this.clock = clock;
  }

  protected now(): string {
    return this.clock().toISOString();
  }

  protected id(prefix: string): string {
    return `${prefix}_${randomUUID().replaceAll("-", "")}`;
  }

  protected credential(): z.infer<typeof CredentialSchema> {
    return CredentialSchema.parse(randomBytes(32).toString("base64url"));
  }

  protected safe<T>(operation: () => Result<T>): Result<T> {
    if (this.closed) {
      return fail("closed", "broker", "broker is closed", "open a new broker instance");
    }
    try {
      return operation();
    } catch (cause: unknown) {
      if (cause instanceof z.ZodError) {
        return fail(
          "invalid_input",
          "protocol",
          "input failed runtime schema validation",
          "correct the request against the exported runtime schema",
        );
      }
      return fail(
        "storage_failure",
        "broker",
        "broker storage operation failed",
        "inspect private broker diagnostics and retry",
      );
    }
  }

  protected closeDatabase(): Result<null> {
    if (this.closed) return ok(null);
    try {
      this.database.close();
      this.closed = true;
      return ok(null);
    } catch {
      return fail(
        "storage_failure",
        "broker",
        "broker store did not close cleanly",
        "retry after releasing local store users",
      );
    }
  }

  protected principal(auth: PrincipalAuth): Result<PrincipalContext> {
    const parsed = PrincipalAuthSchema.parse(auth);
    const row = this.database.get(
      `SELECT principal_id AS principalId, kind, workspace_ids_json AS workspaceIdsJson,
              conversation_ids_json AS conversationIdsJson, capabilities_json AS capabilitiesJson
         FROM principals WHERE token_hash = ?`,
      PrincipalRowSchema,
      [credentialHash(parsed.principalToken)],
    );
    if (row === null) {
      return fail(
        "unauthorized",
        "authority",
        "principal credential is unknown",
        "enroll or reconnect with the protected principal credential",
      );
    }
    return ok({
      ...row,
      workspaceIds: parseJson(z.array(WorkspaceIdSchema), row.workspaceIdsJson),
      conversationIds: parseJson(z.array(ConversationIdSchema), row.conversationIdsJson),
      capabilities: parseJson(z.array(CapabilitySchema), row.capabilitiesJson),
    });
  }

  protected binding(auth: BindingAuth): Result<BindingContext> {
    const principal = this.principal({ principalToken: auth.principalToken });
    if (!principal.ok) return principal;
    const row = this.database.get(
      `SELECT a.actor_id AS actorId, a.principal_id AS principalId,
              a.workspace_id AS workspaceId, a.conversation_id AS conversationId,
              a.parent_actor_id AS parentActorId, a.current_generation AS currentGeneration,
              a.state, a.capabilities_json AS capabilitiesJson, a.host_kind AS hostKind,
              a.host_session_id AS hostSessionId, a.host_subagent_id AS hostSubagentId,
              a.host_provenance AS hostProvenance, a.reply_policy_json AS replyPolicyJson,
              a.resume_token_hash AS resumeTokenHash, b.binding_id AS bindingId,
              b.generation AS bindingGeneration, b.active AS bindingActive
         FROM bindings b JOIN actors a ON a.actor_id = b.actor_id
        WHERE b.token_hash = ? AND b.principal_id = ?`,
      BindingRowSchema,
      [credentialHash(auth.bindingToken), principal.value.principalId],
    );
    if (row === null) {
      return fail(
        "unauthorized",
        "identity",
        "binding credential is unknown",
        "connect or resume the durable actor",
      );
    }
    if (row.bindingActive !== 1n || row.bindingGeneration !== row.currentGeneration) {
      return fail(
        "stale_binding",
        "identity",
        "binding generation has been fenced",
        "resume the actor and use only the newest binding credential",
      );
    }
    return ok({
      ...row,
      capabilities: parseJson(z.array(CapabilitySchema), row.capabilitiesJson),
      replyPolicy: parseJson(ReplyPolicySchema, row.replyPolicyJson),
    });
  }

  protected requireCapability(
    context: CapabilityContext,
    capability: Capability,
  ): Result<never> | null {
    return context.capabilities.includes(capability)
      ? null
      : fail(
          "forbidden",
          "authority",
          `capability ${capability} is outside this binding`,
          "use a principal or delegated binding with the required scoped capability",
        );
  }

  protected requireScope(
    principal: PrincipalContext,
    workspaceId: string,
    conversationId: string,
  ): Result<never> | null {
    return principal.workspaceIds.includes(WorkspaceIdSchema.parse(workspaceId)) &&
      principal.conversationIds.includes(ConversationIdSchema.parse(conversationId))
      ? null
      : fail(
          "forbidden",
          "authority",
          "workspace or conversation is outside the principal scope",
          "use credentials explicitly enrolled for this workspace and conversation",
        );
  }

  protected idempotent<T>(
    principalId: PrincipalId,
    actorKey: string,
    clientRequestId: ClientRequestId,
    operation: string,
    input: unknown,
    outputSchema: ZodType<T>,
    write: () => Result<T>,
  ): Result<T> {
    return this.database.transaction(() => {
      const requestDigest = digest({ operation, input });
      const previous = this.database.get(
        `SELECT request_digest AS requestDigest, result_json AS resultJson
           FROM idempotency WHERE principal_id = ? AND actor_key = ? AND client_request_id = ?`,
        IdempotencyRowSchema,
        [principalId, actorKey, clientRequestId],
      );
      if (previous !== null) {
        if (previous.requestDigest !== requestDigest) {
          return fail(
            "idempotency_conflict",
            "protocol",
            "client request ID was reused for different content",
            "allocate a new client request ID for the changed operation",
          );
        }
        const stored = z
          .discriminatedUnion("ok", [
            z.object({ ok: z.literal(true), value: z.unknown() }),
            z.object({ ok: z.literal(false), error: BrokerErrorSchema }),
          ])
          .parse(JSON.parse(previous.resultJson));
        return stored.ok
          ? ok(outputSchema.parse(stored.value))
          : { ok: false, error: stored.error };
      }
      const result = write();
      this.database.run(
        `INSERT INTO idempotency
           (principal_id, actor_key, client_request_id, request_digest, result_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?)`,
        [principalId, actorKey, clientRequestId, requestDigest, JSON.stringify(result), this.now()],
      );
      return result;
    });
  }

  protected nextSequence(workspaceId: string, conversationId: string): bigint {
    this.database.run(
      `INSERT INTO conversation_sequences(workspace_id, conversation_id, value) VALUES (?, ?, 0)
       ON CONFLICT(workspace_id, conversation_id) DO NOTHING`,
      [workspaceId, conversationId],
    );
    const row = this.database.get(
      `UPDATE conversation_sequences SET value = value + 1
        WHERE workspace_id = ? AND conversation_id = ? RETURNING value`,
      SequenceRowSchema,
      [workspaceId, conversationId],
    );
    if (row === null) {
      throw new Error(
        `violates REQ ${REQ}#delivery: sequence allocation returned no row; fix surface: inspect the conversation sequence transaction`,
      );
    }
    return row.value;
  }

  protected writeMessage(input: MessageWrite): MessageEnvelope {
    const messageId = MessageIdSchema.parse(this.id("msg"));
    const sequence = this.nextSequence(input.workspaceId, input.conversationId);
    const createdAt = this.now();
    this.database.run(
      `INSERT INTO messages
         (message_id, workspace_id, conversation_id, from_actor_id, to_actor_id, kind,
          correlation_id, causation_id, sequence, payload_json, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        messageId,
        input.workspaceId,
        input.conversationId,
        input.fromActorId,
        input.toActorId,
        input.kind,
        input.correlationId,
        input.causationId,
        sequence,
        JSON.stringify(input.payload),
        createdAt,
      ],
    );
    return MessageEnvelopeSchema.parse({
      protocol: "lens/1",
      messageId,
      workspaceId: input.workspaceId,
      conversationId: input.conversationId,
      fromActorId: input.fromActorId,
      toActorId: input.toActorId,
      kind: input.kind,
      correlationId: input.correlationId,
      causationId: input.causationId,
      sequence: sequence.toString(),
      payload: input.payload,
      createdAt,
    });
  }

  protected writeDelivery(
    message: MessageEnvelope,
    logicalRecipient: ActorId,
    recipient: ActorId,
    forwardedFrom: DeliveryId | null = null,
  ): DeliveryId {
    const deliveryId = DeliveryIdSchema.parse(this.id("dlv"));
    const createdAt = this.now();
    this.database.run(
      `INSERT INTO deliveries
         (delivery_id, message_id, logical_recipient_actor_id, recipient_actor_id,
          forwarded_from_delivery_id, created_at) VALUES (?, ?, ?, ?, ?, ?)`,
      [deliveryId, message.messageId, logicalRecipient, recipient, forwardedFrom, createdAt],
    );
    this.database.run(
      `INSERT INTO delivery_observations(delivery_id, observation, created_at)
       VALUES (?, 'persisted', ?)`,
      [deliveryId, createdAt],
    );
    return deliveryId;
  }

  protected actor(actorId: ActorId): z.infer<typeof ActorRowSchema> | null {
    return this.database.get(
      `SELECT actor_id AS actorId, principal_id AS principalId, workspace_id AS workspaceId,
              conversation_id AS conversationId, parent_actor_id AS parentActorId,
              current_generation AS currentGeneration, state,
              capabilities_json AS capabilitiesJson, host_kind AS hostKind,
              host_session_id AS hostSessionId, host_subagent_id AS hostSubagentId,
              host_provenance AS hostProvenance, reply_policy_json AS replyPolicyJson,
              resume_token_hash AS resumeTokenHash FROM actors WHERE actor_id = ?`,
      ActorRowSchema,
      [actorId],
    );
  }

  protected descriptor(row: z.infer<typeof ActorRowSchema>): ActorDescriptor {
    return ActorDescriptorSchema.parse({
      principalId: row.principalId,
      actorId: row.actorId,
      workspaceId: row.workspaceId,
      conversationId: row.conversationId,
      parentActorId: row.parentActorId,
      state: row.state,
      capabilities: parseJson(z.array(CapabilitySchema), row.capabilitiesJson),
      hostKind: row.hostKind,
      hostProvenance: row.hostProvenance,
    });
  }

  protected question(questionId: string): Question | null {
    const row = this.database.get(
      `SELECT question_id AS questionId, workspace_id AS workspaceId,
              conversation_id AS conversationId, origin_actor_id AS originActorId,
              prompt, answer_mode AS answerMode, choices_json AS choicesJson,
              independent_work_available AS independentWorkAvailable,
              reply_policy_json AS replyPolicyJson, state, revision,
              deadline_at AS deadlineAt, answer_json AS answerJson
         FROM questions WHERE question_id = ?`,
      QuestionRowSchema,
      [questionId],
    );
    if (row === null) return null;
    return QuestionSchema.parse({
      questionId: row.questionId,
      workspaceId: row.workspaceId,
      conversationId: row.conversationId,
      originActorId: row.originActorId,
      prompt: row.prompt,
      answerMode: row.answerMode,
      choices: parseJson(z.array(z.object({ id: z.string(), label: z.string() })), row.choicesJson),
      independentWorkAvailable: row.independentWorkAvailable === 1n,
      replyPolicy: parseJson(ReplyPolicySchema, row.replyPolicyJson),
      state: row.state,
      revision: row.revision.toString(),
      deadlineAt: row.deadlineAt,
      answer: row.answerJson === null ? null : parseJson(z.unknown(), row.answerJson),
    });
  }

  protected connection(
    row: z.infer<typeof ActorRowSchema>,
    bindingId: string,
    bindingToken: string,
    resumeCredential: string,
  ): Connection {
    return {
      actor: this.descriptor(row),
      handle: {
        actorId: row.actorId,
        bindingId: BindingIdSchema.parse(bindingId),
        workspaceId: row.workspaceId,
        conversationId: row.conversationId,
        generation: z
          .string()
          .regex(/^(0|[1-9][0-9]*)$/)
          .brand<"LosslessDecimal">()
          .parse(row.currentGeneration.toString()),
      },
      credentials: {
        bindingToken: CredentialSchema.parse(bindingToken),
        resumeCredential: CredentialSchema.parse(resumeCredential),
      },
    };
  }

  protected hostValues(host: HostBinding): readonly [string, string | null, string | null, string] {
    return [host.kind, host.sessionId ?? null, host.subagentId ?? null, host.provenance];
  }
}
