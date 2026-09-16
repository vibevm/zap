/** Coordinator chat persistence, dispatch and reply correlation. @scope spec://org.vibevm.zap/lens/PROP-005#chat */
import { z } from "zod";
import {
  type AgentDescriptor,
  type ChatMessage,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { CoordinatorEvent } from "../agent-runtime/index.ts";
import type { Launch, LaunchActions } from "./launch.ts";
import { workspaceFailure } from "./errors.ts";

type ChatRequest = Extract<WorkspaceCommandRequest, { operation: "chat.post.v1" }>;
const ReplySchema = z.looseObject({
  type: z.literal("agentMessage"),
  phase: z.enum(["commentary", "final_answer"]).optional(),
  text: z.string().min(1).max(64_000).optional(),
  bodyMarkdown: z.string().min(1).max(64_000).optional(),
});

export async function postCoordinatorChat(
  actions: LaunchActions,
  access: WorkspaceAccessContext,
  request: ChatRequest,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const posted = actions.store.command(access, request);
  if (!posted.ok || posted.value.operation !== "chat.post.v1") return posted;
  const queued = actions.store.queueChat(posted.value.message.messageId);
  if (!queued.ok) return queued;
  const execution = actions.store.readProjectExecution(request.projectId, request.contextId);
  if (!execution.ok) return execution;
  const launch =
    execution.value.sessionId === null
      ? undefined
      : actions.launches.get(execution.value.sessionId);
  if (!dispatchable(execution.value.state, execution.value.processEpoch, launch)) {
    return chatResponse(queued.value);
  }
  return dispatchMessage(actions, launch, queued.value);
}

export async function dispatchNextChat(
  actions: LaunchActions,
  launch: Launch,
): Promise<WorkspaceResult<ChatMessage | null>> {
  const execution = actions.store.readProjectExecution(launch.projectId, launch.contextId);
  if (!execution.ok) return execution;
  if (!dispatchable(execution.value.state, execution.value.processEpoch, launch)) {
    return { ok: true, value: null };
  }
  const queued = actions.store.nextQueuedChat(launch.projectId, launch.contextId);
  if (!queued.ok || queued.value === null) return queued;
  const dispatched = await dispatchMessage(actions, launch, queued.value);
  if (!dispatched.ok) return dispatched;
  return dispatched.value.operation === "chat.post.v1"
    ? { ok: true, value: dispatched.value.message }
    : workspaceFailure("storage_failure", "chat dispatch response changed operation");
}

async function dispatchMessage(
  actions: LaunchActions,
  launch: Launch,
  message: ChatMessage,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const claimInput = {
    projectId: launch.projectId,
    contextId: launch.contextId,
    messageId: message.messageId,
    sessionId: launch.scope.coordinatorSessionId,
    processEpoch: launch.processEpoch,
  };
  const claim = actions.store.claimChatDispatch(claimInput);
  if (!claim.ok) return claim;
  if (!claim.value.acquired) return chatResponse(claim.value.message);
  const sent = await launch.adapter.send({
    coordinatorSessionId: launch.scope.coordinatorSessionId,
    text: message.bodyMarkdown,
    clientMessageId: message.messageId,
  });
  if (!sent.ok && sent.error.code === "busy") {
    const released = actions.store.releaseChatDispatch(claimInput);
    return released.ok ? chatResponse(released.value) : released;
  }
  const settlement = actions.store.settleChatDispatch({
    ...claimInput,
    observation: sent.ok
      ? "host_accepted"
      : transportLost(sent.error.code)
        ? "uncertain"
        : "failed",
    nativeTurnId: sent.ok ? sent.value.nativeTurnId : null,
    transportCorrelation: sent.ok ? (sent.value.transportCorrelation ?? null) : null,
    updatedAt: actions.clock().toISOString(),
  });
  if (settlement.ok) actions.drainChatReplies(launch);
  return settlement.ok ? chatResponse(settlement.value) : settlement;
}

export function observeChatReply(
  actions: LaunchActions,
  launch: Launch,
  event: CoordinatorEvent,
  actor: AgentDescriptor | undefined,
): WorkspaceResult<ChatMessage | null> {
  if (
    event.kind !== "item_completed" ||
    event.nativeItemId === null ||
    (event.nativeTurnId === null && (event.transportCorrelation ?? null) === null) ||
    actor?.role !== "coordinator"
  ) {
    return { ok: true, value: null };
  }
  const reply = ReplySchema.safeParse(event.data);
  if (reply.success && reply.data.phase === "commentary") return { ok: true, value: null };
  const body = reply.success ? (reply.data.bodyMarkdown ?? reply.data.text) : undefined;
  if (body === undefined) return { ok: true, value: null };
  const correlation = event.transportCorrelation ?? null;
  return actions.store.appendObservedChatReply({
    sourceEventId: `chat-reply:${event.nativeThreadId ?? "root"}:${event.nativeTurnId ?? correlation?.clientMessageId ?? "transport"}:${event.nativeItemId}`,
    projectId: launch.projectId,
    contextId: launch.contextId,
    sessionId: launch.scope.coordinatorSessionId,
    processEpoch: event.processEpoch,
    nativeTurnId: event.nativeTurnId,
    transportCorrelation: correlation,
    actorId: actor.actorId,
    bodyMarkdown: body,
    occurredAt: actions.clock().toISOString(),
  });
}

function dispatchable(
  executionState: string,
  processEpoch: string | null,
  launch: Launch | undefined,
): launch is Launch {
  return (
    executionState === "running" &&
    launch !== undefined &&
    processEpoch === launch.processEpoch &&
    launch.descriptor.state === "ready"
  );
}

function transportLost(code: string): boolean {
  return code === "transport_lost" || code === "protocol_error";
}

function chatResponse(message: ChatMessage): WorkspaceResult<WorkspaceCommandResponse> {
  return { ok: true, value: { operation: "chat.post.v1", message } };
}
