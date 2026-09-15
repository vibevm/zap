/** Durable coordinator chat dispatch and replies. @scope spec://org.vibevm.zap/lens/PROP-005#chat */
import { z } from "zod";
import { DecimalSchema, MessageIdSchema } from "../protocol/index.ts";
import {
  ChatMessageSchema,
  type ChatMessage,
  type ProjectId,
  type WorkContextId,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";
import {
  ChatDispatchClaimSchema,
  ChatDispatchSettlementSchema,
  ObservedChatReplySchema,
  type ChatDispatchClaim,
  type ChatDispatchSettlement,
  type ObservedChatReply,
} from "./types.ts";

const ChatRowSchema = z.object({ public_json: z.string() });
const DispatchRowSchema = z.object({
  session_id: z.string(),
  process_epoch: z.string(),
  native_turn_id: z.string().nullable(),
  state: z.string(),
});
const CountSchema = z.object({ value: z.bigint() });
const DispatchMessageRowSchema = z.object({ public_json: z.string() });

export function queueChat(
  state: WorkspaceState,
  messageId: z.infer<typeof MessageIdSchema>,
): WorkspaceResult<ChatMessage> {
  try {
    const message = chatMessage(state, messageId);
    if (message === null) return failure("not_found", "chat message does not exist");
    if (message.deliveryState !== "persisted") return { ok: true, value: message };
    const queued = delivery(message, "queued", state.now());
    writeMessage(state, queued);
    return { ok: true, value: queued };
  } catch {
    return failure("storage_failure", "chat queue update failed");
  }
}

export function nextQueuedChat(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
): WorkspaceResult<ChatMessage | null> {
  try {
    const rows = state.database.all(
      `SELECT c.public_json FROM workspace_chat c
       LEFT JOIN workspace_chat_dispatch d ON d.message_id = c.message_id
       WHERE c.project_id = ? AND c.context_id = ? AND d.message_id IS NULL
         AND json_extract(c.public_json, '$.role') = 'user'
         AND json_extract(c.public_json, '$.deliveryState') = 'queued'
       ORDER BY c.sequence LIMIT 1`,
      ChatRowSchema,
      [projectId, contextId],
    );
    const message = rows.map((row) => state.parse(row.public_json, ChatMessageSchema))[0];
    return { ok: true, value: message ?? null };
  } catch {
    return failure("storage_failure", "queued chat read failed");
  }
}

export function claimChatDispatch(
  state: WorkspaceState,
  raw: ChatDispatchClaim,
): WorkspaceResult<{ message: ChatMessage; acquired: boolean }> {
  const input = ChatDispatchClaimSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "chat dispatch claim is malformed");
  try {
    return state.database.transaction(() => {
      const message = chatMessage(state, input.data.messageId);
      if (
        message === null ||
        message.projectId !== input.data.projectId ||
        message.contextId !== input.data.contextId ||
        message.role !== "user"
      ) {
        return failure("not_found", "chat message does not exist in the requested context");
      }
      const existing = dispatch(state, input.data.messageId);
      if (existing !== null) {
        return existing.session_id === input.data.sessionId &&
          existing.process_epoch === input.data.processEpoch
          ? { ok: true, value: { message, acquired: false } }
          : failure("conflict", "chat dispatch is already owned by another session epoch");
      }
      if (message.deliveryState !== "queued" && message.deliveryState !== "persisted") {
        return failure("conflict", "chat message is not eligible for dispatch");
      }
      const claimed = delivery(message, "uncertain", state.now());
      writeMessage(state, claimed);
      state.database.run(
        `INSERT INTO workspace_chat_dispatch(message_id, session_id, process_epoch, native_turn_id, state)
         VALUES(?, ?, ?, NULL, 'dispatching')`,
        [claimed.messageId, input.data.sessionId, input.data.processEpoch],
      );
      return { ok: true, value: { message: claimed, acquired: true } };
    });
  } catch {
    return failure("storage_failure", "chat dispatch claim failed");
  }
}

export function settleChatDispatch(
  state: WorkspaceState,
  raw: ChatDispatchSettlement,
): WorkspaceResult<ChatMessage> {
  const input = ChatDispatchSettlementSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "chat dispatch settlement is malformed");
  try {
    return state.database.transaction(() => {
      const message = chatMessage(state, input.data.messageId);
      const existing = dispatch(state, input.data.messageId);
      if (message === null || existing === null) {
        return failure("not_found", "chat dispatch does not exist");
      }
      if (
        existing.session_id !== input.data.sessionId ||
        existing.process_epoch !== input.data.processEpoch
      ) {
        return failure("conflict", "chat dispatch settlement has a stale session epoch");
      }
      if (input.data.observation === "host_accepted" && input.data.nativeTurnId === null) {
        return failure("invalid_input", "host-accepted chat requires a native turn id");
      }
      const nextState =
        input.data.observation === "host_accepted"
          ? "host_accepted"
          : input.data.observation === "uncertain"
            ? "uncertain"
            : "failed";
      const updated = delivery(message, nextState, input.data.updatedAt);
      writeMessage(state, updated);
      state.database.run(
        `UPDATE workspace_chat_dispatch SET state = ?, native_turn_id = ? WHERE message_id = ?`,
        [input.data.observation, input.data.nativeTurnId, input.data.messageId],
      );
      return { ok: true, value: updated };
    });
  } catch {
    return failure("storage_failure", "chat dispatch settlement failed");
  }
}

export function releaseChatDispatch(
  state: WorkspaceState,
  raw: ChatDispatchClaim,
): WorkspaceResult<ChatMessage> {
  const input = ChatDispatchClaimSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "chat dispatch release is malformed");
  try {
    const message = chatMessage(state, input.data.messageId);
    const existing = dispatch(state, input.data.messageId);
    if (
      message === null ||
      existing === null ||
      existing.session_id !== input.data.sessionId ||
      existing.process_epoch !== input.data.processEpoch ||
      existing.native_turn_id !== null ||
      existing.state !== "dispatching"
    ) {
      return failure("conflict", "chat dispatch release does not match an unaccepted claim");
    }
    state.database.run("DELETE FROM workspace_chat_dispatch WHERE message_id = ?", [
      input.data.messageId,
    ]);
    const queued = delivery(message, "queued", state.now());
    writeMessage(state, queued);
    return { ok: true, value: queued };
  } catch {
    return failure("storage_failure", "chat dispatch release failed");
  }
}

export function appendObservedChatReply(
  state: WorkspaceState,
  raw: ObservedChatReply,
): WorkspaceResult<ChatMessage | null> {
  const input = ObservedChatReplySchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "observed chat reply is malformed");
  try {
    return state.database.transaction(() => appendReply(state, input.data));
  } catch {
    return failure("storage_failure", "observed chat reply write failed");
  }
}

function appendReply(
  state: WorkspaceState,
  input: ObservedChatReply,
): WorkspaceResult<ChatMessage | null> {
  const existingSource = state.database.get(
    "SELECT COUNT(*) AS value FROM workspace_chat_reply_sources WHERE source_event_id = ?",
    CountSchema,
    [input.sourceEventId],
  );
  if (existingSource?.value === 1n) return { ok: true, value: null };
  const dispatched = state.database.get(
    `SELECT c.public_json FROM workspace_chat_dispatch d
     JOIN workspace_chat c ON c.message_id = d.message_id
     WHERE d.session_id = ? AND d.process_epoch = ? AND d.native_turn_id = ?`,
    DispatchMessageRowSchema,
    [input.sessionId, input.processEpoch, input.nativeTurnId],
  );
  const request =
    dispatched === null ? null : state.parse(dispatched.public_json, ChatMessageSchema);
  if (
    request === null ||
    request.projectId !== input.projectId ||
    request.contextId !== input.contextId
  ) {
    return failure("conflict", "observed reply does not match a durable chat dispatch");
  }
  writeMessage(state, delivery(request, "answered", input.occurredAt));
  const sequence = state.nextConversation(input.projectId, input.contextId, request.conversationId);
  const reply = ChatMessageSchema.parse({
    messageId: MessageIdSchema.parse(state.id("message")),
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: request.conversationId,
    senderActorId: input.actorId,
    role: "assistant",
    bodyMarkdown: input.bodyMarkdown,
    artifactRefs: [],
    correlationId: input.nativeTurnId,
    causationMessageId: request.messageId,
    deliveryState: "answered",
    revision: DecimalSchema.parse(String(sequence)),
    createdAt: input.occurredAt,
    updatedAt: input.occurredAt,
  });
  state.database.run(
    `INSERT INTO workspace_chat(message_id, project_id, context_id, conversation_id, sequence, public_json)
     VALUES(?, ?, ?, ?, ?, ?)`,
    [
      reply.messageId,
      reply.projectId,
      reply.contextId,
      reply.conversationId,
      sequence,
      state.json(reply),
    ],
  );
  state.database.run(
    "INSERT INTO workspace_chat_reply_sources(source_event_id, message_id) VALUES(?, ?)",
    [input.sourceEventId, reply.messageId],
  );
  state.appendHistory({
    projectId: reply.projectId,
    contextId: reply.contextId,
    kind: "chat.message.answered",
    source: "host",
    actorId: input.actorId,
    occurrenceAt: input.occurredAt,
    sourceEventId: input.sourceEventId,
    sourceSequence: null,
    correlationId: input.nativeTurnId,
    causationId: null,
    planProvenance: null,
    payload: { requestMessageId: request.messageId, replyMessageId: reply.messageId },
  });
  return { ok: true, value: reply };
}

function delivery(
  message: ChatMessage,
  deliveryState: ChatMessage["deliveryState"],
  updatedAt: string,
): ChatMessage {
  return ChatMessageSchema.parse({ ...message, deliveryState, updatedAt });
}
function chatMessage(
  state: WorkspaceState,
  messageId: z.infer<typeof MessageIdSchema>,
): ChatMessage | null {
  const row = state.database.get(
    "SELECT public_json FROM workspace_chat WHERE message_id = ?",
    ChatRowSchema,
    [messageId],
  );
  return row === null ? null : state.parse(row.public_json, ChatMessageSchema);
}
function dispatch(state: WorkspaceState, messageId: z.infer<typeof MessageIdSchema>) {
  return state.database.get(
    `SELECT session_id, process_epoch, native_turn_id, state
     FROM workspace_chat_dispatch WHERE message_id = ?`,
    DispatchRowSchema,
    [messageId],
  );
}
function writeMessage(state: WorkspaceState, message: ChatMessage): void {
  state.database.run("UPDATE workspace_chat SET public_json = ? WHERE message_id = ?", [
    state.json(message),
    message.messageId,
  ]);
}
