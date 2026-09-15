/**
 * Addressed messages, inbox observations, sparse acknowledgements and forwarding.
 * @scope spec://org.vibevm.zap/lens/PROP-001#delivery
 */
import { z } from "zod";

import {
  AckInputSchema,
  AckResultSchema,
  ActorIdSchema,
  BindingAuthSchema,
  DeliveryIdSchema,
  DeliveryObservationSchema,
  DeliverySchema,
  EmitInputSchema,
  EventPageSchema,
  EventsInputSchema,
  ForwardInboxInputSchema,
  ForwardResultSchema,
  InboxInputSchema,
  InboxPageSchema,
  JsonValueSchema,
  MessageEnvelopeSchema,
  MessageIdSchema,
  PrincipalAuthSchema,
  PrincipalEmitInputSchema,
  ReplyPolicySchema,
  type AckInput,
  type AckResult,
  type ActorId,
  type BindingAuth,
  type Delivery,
  type EmitInput,
  type EventPage,
  type EventsInput,
  type ForwardInboxInput,
  type ForwardResult,
  type InboxInput,
  type InboxPage,
  type MessageEnvelope,
  type MessageId,
  type PrincipalAuth,
  type PrincipalEmitInput,
  type Result,
} from "../protocol/index.ts";
import { fail, ok } from "./core.ts";
import { QuestionOperations } from "./questions.ts";

const MAX_PAYLOAD_BYTES = 64 * 1024;
const DeliveryOwnerSchema = z.object({ recipientActorId: ActorIdSchema });
const DeliveryForwardSchema = z.object({ deliveryId: DeliveryIdSchema, messageId: z.string() });
const EventHeadSchema = z.object({ head: z.bigint() });
const MessageRowSchema = z.object({
  messageId: z.string(),
  workspaceId: z.string(),
  conversationId: z.string(),
  fromActorId: z.string().nullable(),
  toActorId: z.string().nullable(),
  kind: z.string(),
  correlationId: z.string().nullable(),
  causationId: z.string().nullable(),
  sequence: z.bigint(),
  payloadJson: z.string(),
  createdAt: z.string(),
});
const DeliveryRowSchema = MessageRowSchema.extend({
  deliveryId: DeliveryIdSchema,
  logicalRecipientActorId: ActorIdSchema,
  recipientActorId: ActorIdSchema,
  forwardedFromDeliveryId: DeliveryIdSchema.nullable(),
  acknowledgedAt: z.string().nullable(),
  observationsJson: z.string(),
});

export class DeliveryOperations extends QuestionOperations {
  emitPrincipal(auth: PrincipalAuth, input: PrincipalEmitInput): Result<MessageEnvelope> {
    return this.safe(() => {
      const parsed = PrincipalEmitInputSchema.parse(input);
      const principal = this.principal(PrincipalAuthSchema.parse(auth));
      if (!principal.ok) return principal;
      const denied = this.requireCapability(principal.value, "message:emit");
      if (denied !== null) return denied;
      const scope = this.requireScope(principal.value, parsed.workspaceId, parsed.conversationId);
      if (scope !== null) return scope;
      const recipient = this.actor(parsed.toActorId);
      if (
        recipient === null ||
        recipient.workspaceId !== parsed.workspaceId ||
        recipient.conversationId !== parsed.conversationId
      ) {
        return fail(
          "forbidden",
          "delivery",
          "principal notice recipient is outside the authenticated scope",
          "select an actor returned by the same scoped actor listing",
        );
      }
      if (recipient.state !== "active") {
        return fail(
          "conflict",
          "delivery",
          "principal notice recipient is expired",
          "refresh eligible actor targets before retrying",
        );
      }
      if (Buffer.byteLength(JSON.stringify(parsed.payload)) > MAX_PAYLOAD_BYTES) {
        return fail(
          "backpressure",
          "protocol",
          "message payload exceeds the inline limit",
          "store the artifact separately and send a bounded reference",
        );
      }
      return this.idempotent(
        principal.value.principalId,
        "principal",
        parsed.clientRequestId,
        "emit_principal",
        parsed,
        MessageEnvelopeSchema,
        () => {
          const message = this.writeMessage({
            workspaceId: parsed.workspaceId,
            conversationId: parsed.conversationId,
            fromActorId: null,
            toActorId: parsed.toActorId,
            kind: "notice.created",
            correlationId: parsed.correlationId ?? null,
            causationId: parsed.causationId ?? null,
            payload: parsed.payload,
          });
          this.writeDelivery(message, parsed.toActorId, parsed.toActorId);
          return ok(message);
        },
      );
    });
  }

  emit(auth: BindingAuth, input: EmitInput): Result<MessageEnvelope> {
    return this.safe(() => {
      const parsed = EmitInputSchema.parse(input);
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "message:emit");
      if (denied !== null) return denied;
      if (actor.value.state !== "active") return this.expiredActorFailure();
      const recipient = this.actor(parsed.toActorId);
      if (recipient === null) {
        return fail(
          "not_found",
          "delivery",
          "recipient actor does not exist",
          "use an actor handle from this conversation",
        );
      }
      if (
        recipient.workspaceId !== actor.value.workspaceId ||
        recipient.conversationId !== actor.value.conversationId
      ) {
        return fail(
          "forbidden",
          "delivery",
          "recipient is outside the actor conversation",
          "address an actor in the authenticated workspace and conversation",
        );
      }
      if (Buffer.byteLength(JSON.stringify(parsed.payload)) > MAX_PAYLOAD_BYTES) {
        return fail(
          "backpressure",
          "protocol",
          "message payload exceeds the inline limit",
          "store the artifact separately and send a bounded reference",
        );
      }
      return this.idempotent(
        actor.value.principalId,
        actor.value.actorId,
        parsed.clientRequestId,
        "emit",
        parsed,
        MessageEnvelopeSchema,
        () => {
          const message = this.writeMessage({
            workspaceId: actor.value.workspaceId,
            conversationId: actor.value.conversationId,
            fromActorId: actor.value.actorId,
            toActorId: parsed.toActorId,
            kind: "notice.created",
            correlationId: parsed.correlationId ?? null,
            causationId: parsed.causationId ?? null,
            payload: parsed.payload,
          });
          this.writeDelivery(message, parsed.toActorId, parsed.toActorId);
          return ok(message);
        },
      );
    });
  }

  inbox(auth: BindingAuth, input: InboxInput): Result<InboxPage> {
    return this.safe(() => {
      const parsed = InboxInputSchema.parse(input);
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "inbox:read");
      if (denied !== null) return denied;
      return this.database.transaction(() => {
        const rows = this.deliveryRows(
          actor.value.actorId,
          BigInt(parsed.afterSequence),
          parsed.limit + 1,
        );
        const pageRows = rows.slice(0, parsed.limit);
        const offeredAt = this.now();
        for (const row of pageRows) {
          this.database.run(
            `INSERT OR IGNORE INTO delivery_observations
               (delivery_id, observation, binding_generation, attempt_id, created_at)
             VALUES (?, 'offered', ?, ?, ?)`,
            [row.deliveryId, actor.value.bindingGeneration, this.id("atm"), offeredAt],
          );
        }
        const deliveries = pageRows.map((value) =>
          value.observations.includes("offered")
            ? value
            : DeliverySchema.parse({
                ...value,
                observations: [...value.observations, "offered"],
              }),
        );
        const cursor = deliveries.at(-1)?.message.sequence ?? parsed.afterSequence;
        return ok(
          InboxPageSchema.parse({
            deliveries,
            observationCursor: cursor,
            hasMore: rows.length > parsed.limit,
          }),
        );
      });
    });
  }

  /** Exact authenticated proof that this binding was the selected planning recipient. */
  planIntent(auth: BindingAuth, messageId: MessageId): Result<MessageEnvelope> {
    return this.safe(() => {
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "plan:propose");
      if (denied !== null) return denied;
      if (actor.value.state !== "active") return this.expiredActorFailure();
      const message = this.messageById(MessageIdSchema.parse(messageId));
      if (
        message === null ||
        message.workspaceId !== actor.value.workspaceId ||
        message.conversationId !== actor.value.conversationId ||
        message.toActorId !== actor.value.actorId
      ) {
        return fail(
          "forbidden",
          "authority",
          "plan intent is not addressed to this authenticated actor",
          "use the exact plan-intent message delivered to this actor binding",
        );
      }
      const delivery = this.database.get(
        `SELECT recipient_actor_id AS recipientActorId FROM deliveries
          WHERE message_id = ? AND logical_recipient_actor_id = ?`,
        DeliveryOwnerSchema,
        [message.messageId, actor.value.actorId],
      );
      return delivery === null
        ? fail(
            "forbidden",
            "authority",
            "plan intent has no delivery for this authenticated actor",
            "use a broker-addressed plan intent rather than a self-asserted identifier",
          )
        : ok(message);
    });
  }

  ack(auth: BindingAuth, input: AckInput): Result<AckResult> {
    return this.safe(() => {
      const parsed = AckInputSchema.parse(input);
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "inbox:ack");
      if (denied !== null) return denied;
      return this.idempotent(
        actor.value.principalId,
        actor.value.actorId,
        parsed.clientRequestId,
        "ack",
        parsed,
        AckResultSchema,
        () => {
          for (const deliveryId of parsed.deliveryIds) {
            const owner = this.database.get(
              `SELECT recipient_actor_id AS recipientActorId
                 FROM deliveries WHERE delivery_id = ?`,
              DeliveryOwnerSchema,
              [deliveryId],
            );
            if (owner === null) {
              return fail(
                "not_found",
                "delivery",
                "delivery does not exist",
                "acknowledge only IDs returned by this actor inbox",
              );
            }
            if (owner.recipientActorId !== actor.value.actorId) {
              return fail(
                "forbidden",
                "delivery",
                "delivery belongs to another actor",
                "acknowledge only this binding's addressed inbox",
              );
            }
          }
          const now = this.now();
          for (const deliveryId of parsed.deliveryIds) {
            this.database.run(
              `UPDATE deliveries SET acknowledged_at = COALESCE(acknowledged_at, ?)
                WHERE delivery_id = ?`,
              [now, deliveryId],
            );
            this.database.run(
              `INSERT OR IGNORE INTO delivery_observations
                 (delivery_id, observation, binding_generation, attempt_id, created_at)
               VALUES (?, 'actor_acknowledged', ?, ?, ?)`,
              [deliveryId, actor.value.bindingGeneration, this.id("atm"), now],
            );
          }
          return ok(AckResultSchema.parse({ acknowledgedDeliveryIds: parsed.deliveryIds }));
        },
      );
    });
  }

  events(auth: PrincipalAuth, input: EventsInput): Result<EventPage> {
    return this.safe(() => {
      const parsed = EventsInputSchema.parse(input);
      const principal = this.principal(PrincipalAuthSchema.parse(auth));
      if (!principal.ok) return principal;
      const denied = this.requireCapability(principal.value, "events:read");
      if (denied !== null) return denied;
      const scopeError = this.requireScope(
        principal.value,
        parsed.workspaceId,
        parsed.conversationId,
      );
      if (scopeError !== null) return scopeError;
      const requestedCursor = BigInt(parsed.afterSequence);
      const head = this.database.get(
        `SELECT COALESCE(MAX(sequence), 0) AS head FROM messages
          WHERE workspace_id = ? AND conversation_id = ?`,
        EventHeadSchema,
        [parsed.workspaceId, parsed.conversationId],
      );
      if (head === null || requestedCursor > head.head) {
        return fail(
          "resync_required",
          "delivery",
          "event cursor is beyond the durable conversation head",
          "discard the cursor and request a fresh bounded snapshot",
        );
      }
      const rows = this.database.all(
        `SELECT message_id AS messageId, workspace_id AS workspaceId,
                conversation_id AS conversationId, from_actor_id AS fromActorId,
                to_actor_id AS toActorId, kind, correlation_id AS correlationId,
                causation_id AS causationId, sequence, payload_json AS payloadJson,
                created_at AS createdAt FROM messages
          WHERE workspace_id = ? AND conversation_id = ? AND sequence > ?
          ORDER BY sequence LIMIT ?`,
        MessageRowSchema,
        [parsed.workspaceId, parsed.conversationId, requestedCursor, parsed.limit + 1],
      );
      const first = rows.at(0);
      if (first !== undefined && first.sequence > requestedCursor + 1n) {
        return fail(
          "resync_required",
          "delivery",
          "event cursor crosses a detectable retained-sequence gap",
          "request a fresh bounded snapshot before consuming later events",
        );
      }
      const events = rows.slice(0, parsed.limit).map((row) => this.envelope(row));
      return ok(
        EventPageSchema.parse({
          events,
          observationCursor: events.at(-1)?.sequence ?? parsed.afterSequence,
          hasMore: rows.length > parsed.limit,
        }),
      );
    });
  }

  forwardInbox(auth: BindingAuth, input: ForwardInboxInput): Result<ForwardResult> {
    return this.safe(() => {
      const parsed = ForwardInboxInputSchema.parse(input);
      const caller = this.binding(BindingAuthSchema.parse(auth));
      if (!caller.ok) return caller;
      const denied = this.requireCapability(caller.value, "inbox:forward");
      if (denied !== null) return denied;
      const source = this.actor(parsed.fromActorId);
      if (
        source === null ||
        source.parentActorId !== parsed.toActorId ||
        parsed.toActorId !== caller.value.actorId
      ) {
        return fail(
          "forbidden",
          "delivery",
          "forwarding requires the child's declared parent",
          "use the parent binding and its immediate child actor ID",
        );
      }
      const replyPolicy = ReplyPolicySchema.parse(JSON.parse(source.replyPolicyJson));
      if (source.state !== "expired" || replyPolicy.kind !== "forward_parent") {
        return fail(
          "forbidden",
          "delivery",
          "actor state or reply policy does not permit forwarding",
          "expire a child whose declared reply policy forwards to its parent",
        );
      }
      return this.idempotent(
        caller.value.principalId,
        caller.value.actorId,
        parsed.clientRequestId,
        "forward_inbox",
        parsed,
        ForwardResultSchema,
        () => this.forwardBatch(source.actorId, caller.value.actorId, parsed.limit),
      );
    });
  }

  private forwardBatch(
    sourceActorId: ActorId,
    parentActorId: ActorId,
    limit: number,
  ): Result<ForwardResult> {
    const pending = this.database.all(
      `SELECT delivery_id AS deliveryId, message_id AS messageId FROM deliveries
        WHERE recipient_actor_id = ? AND acknowledged_at IS NULL
          AND NOT EXISTS (
            SELECT 1 FROM deliveries forwarded
             WHERE forwarded.message_id = deliveries.message_id
               AND forwarded.recipient_actor_id = ?
          )
        ORDER BY created_at LIMIT ?`,
      DeliveryForwardSchema,
      [sourceActorId, parentActorId, limit],
    );
    const forwarded = pending.map((delivery) => {
      const message = this.messageById(delivery.messageId);
      if (message === null) {
        throw new Error(
          "violates REQ spec://org.vibevm.zap/lens/PROP-001#delivery: forwarded message disappeared; fix surface: inspect message foreign keys",
        );
      }
      return this.writeDelivery(message, sourceActorId, parentActorId, delivery.deliveryId);
    });
    return ok(ForwardResultSchema.parse({ forwardedDeliveryIds: forwarded }));
  }

  private deliveryRows(actorId: ActorId, after: bigint, limit: number): Delivery[] {
    return this.database
      .all(
        `SELECT d.delivery_id AS deliveryId,
                d.logical_recipient_actor_id AS logicalRecipientActorId,
                d.recipient_actor_id AS recipientActorId,
                d.forwarded_from_delivery_id AS forwardedFromDeliveryId,
                d.acknowledged_at AS acknowledgedAt,
                COALESCE((SELECT json_group_array(DISTINCT observation)
                            FROM delivery_observations o
                           WHERE o.delivery_id = d.delivery_id), '[]') AS observationsJson,
                m.message_id AS messageId, m.workspace_id AS workspaceId,
                m.conversation_id AS conversationId, m.from_actor_id AS fromActorId,
                m.to_actor_id AS toActorId, m.kind, m.correlation_id AS correlationId,
                m.causation_id AS causationId, m.sequence, m.payload_json AS payloadJson,
                m.created_at AS createdAt
           FROM deliveries d JOIN messages m ON m.message_id = d.message_id
          WHERE d.recipient_actor_id = ? AND d.acknowledged_at IS NULL AND m.sequence > ?
          ORDER BY m.sequence LIMIT ?`,
        DeliveryRowSchema,
        [actorId, after, limit],
      )
      .map((row) =>
        DeliverySchema.parse({
          deliveryId: row.deliveryId,
          message: this.envelope(row),
          logicalRecipientActorId: row.logicalRecipientActorId,
          recipientActorId: row.recipientActorId,
          forwardedFromDeliveryId: row.forwardedFromDeliveryId,
          acknowledgedAt: row.acknowledgedAt,
          observations: z.array(DeliveryObservationSchema).parse(JSON.parse(row.observationsJson)),
        }),
      );
  }

  private envelope(row: z.infer<typeof MessageRowSchema>): MessageEnvelope {
    return MessageEnvelopeSchema.parse({
      protocol: "lens/1",
      messageId: row.messageId,
      workspaceId: row.workspaceId,
      conversationId: row.conversationId,
      fromActorId: row.fromActorId,
      toActorId: row.toActorId,
      kind: row.kind,
      correlationId: row.correlationId,
      causationId: row.causationId,
      sequence: row.sequence.toString(),
      payload: JsonValueSchema.parse(JSON.parse(row.payloadJson)),
      createdAt: row.createdAt,
    });
  }

  protected messageById(messageId: string): MessageEnvelope | null {
    const row = this.database.get(
      `SELECT message_id AS messageId, workspace_id AS workspaceId,
              conversation_id AS conversationId, from_actor_id AS fromActorId,
              to_actor_id AS toActorId, kind, correlation_id AS correlationId,
              causation_id AS causationId, sequence, payload_json AS payloadJson,
              created_at AS createdAt FROM messages WHERE message_id = ?`,
      MessageRowSchema,
      [messageId],
    );
    return row === null ? null : this.envelope(row);
  }
}
