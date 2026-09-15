/** Codex public event and native-child normalization. @scope spec://org.vibevm.zap/lens/PROP-005#history */
import type { z } from "zod";
import {
  jsonValue,
  type CoordinatorEvent,
  type PendingHostRequest,
} from "../agent-runtime/index.ts";
import {
  CodexCollabItemSchema,
  CodexNotificationSchema,
  CodexServerRequestSchema,
  CodexThreadStatusSchema,
  publicItem,
  type CodexPublicItem,
  type CodexServerRequest,
  type CodexThread,
  type CodexWireMessage,
} from "./protocol.ts";
import { requestKey, requestKind, stateOf } from "./helpers.ts";
import type { SessionState, WorkerState } from "./state.ts";

export class CodexEventRouter {
  readonly #sessions: ReadonlyMap<string, SessionState>;
  readonly #workers: Map<string, WorkerState>;
  readonly #listeners = new Set<(event: CoordinatorEvent) => void>();

  constructor(sessions: ReadonlyMap<string, SessionState>, workers: Map<string, WorkerState>) {
    this.#sessions = sessions;
    this.#workers = workers;
  }

  subscribe(listener: (event: CoordinatorEvent) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  message(worker: WorkerState, message: CodexWireMessage): void {
    if (this.#workers.get(worker.ownerCoordinatorSessionId) !== worker) return;
    if ("id" in message && "method" in message) {
      const request = CodexServerRequestSchema.safeParse(message);
      if (!request.success) {
        this.protocolEvent(worker, "Unsupported or malformed Codex server request");
        return;
      }
      this.#pendingRequest(worker, request.data);
      return;
    }
    if (!("method" in message)) {
      this.protocolEvent(worker, "Unmatched Codex response");
      return;
    }
    const notification = CodexNotificationSchema.safeParse(message);
    if (notification.success) {
      this.#notification(worker, notification.data);
    } else {
      this.unmappedEvent(worker, message.method);
    }
  }

  observeChildren(
    session: SessionState,
    items: readonly CodexPublicItem[],
    worker?: WorkerState,
  ): void {
    for (const item of items) {
      const collab = CodexCollabItemSchema.safeParse(item);
      if (!collab.success) continue;
      for (const receiverId of collab.data.receiverThreadIds) {
        const isSpawn = collab.data.tool === "spawnAgent";
        if (isSpawn && !session.childThreads.has(receiverId)) {
          session.childThreads.set(receiverId, null);
        }
        if (worker === undefined) continue;
        this.emit(
          session,
          worker,
          isSpawn ? "native_child_observed" : "native_message_observed",
          collab.data.senderThreadId,
          null,
          collab.data.id,
          {
            fromNativeThreadId: collab.data.senderThreadId,
            toNativeThreadId: receiverId,
            relationship: isSpawn ? "parent" : "message",
            tool: collab.data.tool,
            canAcceptDirectInput: null,
          },
        );
      }
    }
  }

  emit(
    session: SessionState,
    worker: WorkerState,
    kind: CoordinatorEvent["kind"],
    nativeThreadId: string | null,
    nativeTurnId: string | null,
    nativeItemId: string | null,
    data: unknown,
  ): void {
    if (this.#workers.get(worker.ownerCoordinatorSessionId) !== worker) return;
    const safe = jsonValue(data);
    if (!safe.ok) return;
    const event: CoordinatorEvent = {
      coordinatorSessionId: session.descriptor.coordinatorSessionId,
      processEpoch: worker.epoch,
      nativeThreadId,
      nativeTurnId,
      nativeItemId,
      kind,
      sourceEventId: `${worker.epoch}:${String(++worker.sequence)}`,
      data: safe.value,
    };
    for (const listener of this.#listeners) listener(event);
  }

  protocolEvent(worker: WorkerState, message: string): void {
    const session = this.#sessions.get(worker.ownerCoordinatorSessionId);
    if (session !== undefined) {
      this.emit(session, worker, "protocol_error", null, null, null, { message });
    }
  }

  unmappedEvent(worker: WorkerState, method: string): void {
    const session = this.#sessions.get(worker.ownerCoordinatorSessionId);
    if (session !== undefined) {
      this.emit(session, worker, "host_event_unmapped", null, null, null, {
        method,
        coverage: "informational",
      });
    }
  }

  exited(worker: WorkerState, code: number | null): void {
    if (this.#workers.get(worker.ownerCoordinatorSessionId) !== worker) return;
    const session = this.#sessions.get(worker.ownerCoordinatorSessionId);
    if (session === undefined) {
      this.#workers.delete(worker.ownerCoordinatorSessionId);
      return;
    }
    session.activeTurnId = null;
    session.childActiveTurns.clear();
    session.pending.clear();
    const stopping = session.lifecycle === "stop_requested";
    session.lifecycle = stopping ? "stopped" : "uncertain";
    session.stopObservation = stopping ? "settled" : "uncertain";
    session.descriptor = { ...session.descriptor, state: stopping ? "stopped" : "failed" };
    this.emit(session, worker, stopping ? "session_stopped" : "process_exited", null, null, null, {
      code,
    });
    this.#workers.delete(worker.ownerCoordinatorSessionId);
  }

  #pendingRequest(worker: WorkerState, request: CodexServerRequest): void {
    const session = this.#byThread(worker, request.params.threadId);
    if (session === undefined) return;
    const body = jsonValue(request.params);
    if (!body.ok) {
      this.protocolEvent(worker, "Codex host request body is not lossless JSON");
      return;
    }
    const pending: PendingHostRequest = {
      coordinatorSessionId: session.descriptor.coordinatorSessionId,
      nativeThreadId: request.params.threadId,
      nativeTurnId: request.params.turnId,
      nativeItemId: request.params.itemId,
      requestId: request.id,
      processEpoch: worker.epoch,
      kind: requestKind(request.method),
      body: body.value,
    };
    session.retiredRequests.delete(requestKey(request.id));
    session.pending.set(requestKey(request.id), pending);
    if (request.params.threadId === session.descriptor.nativeThreadRef.value) {
      session.descriptor = { ...session.descriptor, state: "waiting_for_user" };
    }
    this.emit(
      session,
      worker,
      "host_request_pending",
      request.params.threadId,
      request.params.turnId,
      request.params.itemId,
      pending,
    );
  }

  #notification(worker: WorkerState, notification: z.infer<typeof CodexNotificationSchema>): void {
    const threadId =
      notification.method === "thread/started"
        ? notification.params.thread.id
        : notification.params.threadId;
    const session = this.#byThread(worker, threadId);
    if (session === undefined) {
      if (notification.method === "thread/started" && notification.params.thread.parentThreadId) {
        const parent = this.#byThread(worker, notification.params.thread.parentThreadId);
        if (parent !== undefined) this.#recordChild(parent, worker, notification.params.thread);
      }
      return;
    }
    const isCoordinatorThread = threadId === session.descriptor.nativeThreadRef.value;
    if (notification.method === "thread/started") {
      if (!isCoordinatorThread && notification.params.thread.parentThreadId !== null) {
        this.#recordChild(session, worker, notification.params.thread);
      }
      return;
    }
    if (notification.method === "turn/started") {
      if (isCoordinatorThread) {
        session.activeTurnId = notification.params.turn.id;
        if (session.lifecycle === "active") {
          session.descriptor = { ...session.descriptor, state: "running" };
        }
      } else {
        session.childActiveTurns.set(threadId, notification.params.turn.id);
      }
      if (session.lifecycle === "pause_requested" || session.lifecycle === "paused") {
        this.#trackPauseTurn(session, worker, threadId, notification.params.turn.id);
      }
      this.emit(session, worker, "turn_started", threadId, notification.params.turn.id, null, {});
      return;
    }
    if (notification.method === "turn/completed") {
      if (isCoordinatorThread) {
        if (session.activeTurnId === notification.params.turn.id) session.activeTurnId = null;
        if (session.lifecycle === "active") {
          session.descriptor = {
            ...session.descriptor,
            state: notification.params.turn.status === "failed" ? "failed" : "ready",
          };
        }
      } else if (session.childActiveTurns.get(threadId) === notification.params.turn.id) {
        session.childActiveTurns.delete(threadId);
      }
      session.pauseTargets.delete(pauseTargetKey(threadId, notification.params.turn.id));
      this.emit(session, worker, "turn_completed", threadId, notification.params.turn.id, null, {
        status: notification.params.turn.status,
        error: notification.params.turn.error ?? null,
      });
      if (session.lifecycle === "pause_requested" && session.pauseTargets.size === 0) {
        session.lifecycle = "paused";
        session.pauseObservation = "settled";
        session.descriptor = { ...session.descriptor, state: "paused" };
        this.emit(session, worker, "session_paused", threadId, notification.params.turn.id, null, {
          observation: "settled",
        });
      }
      return;
    }
    if (notification.method === "item/started" || notification.method === "item/completed") {
      this.observeChildren(session, [notification.params.item], worker);
      this.emit(
        session,
        worker,
        notification.method === "item/started" ? "item_started" : "item_completed",
        threadId,
        notification.params.turnId,
        notification.params.item.id,
        publicItem(notification.params.item),
      );
      return;
    }
    if (notification.method === "item/agentMessage/delta") {
      this.emit(
        session,
        worker,
        "message_delta",
        threadId,
        notification.params.turnId,
        notification.params.itemId,
        { delta: notification.params.delta },
      );
      return;
    }
    if (notification.method === "serverRequest/resolved") {
      const key = requestKey(notification.params.requestId);
      session.pending.delete(key);
      session.answeredRequests.delete(key);
      session.retiredRequests.delete(key);
      this.emit(session, worker, "host_request_resolved", threadId, null, null, {
        requestId: notification.params.requestId,
      });
      return;
    }
    if (notification.method === "thread/closed") {
      if (isCoordinatorThread) {
        if (session.activeTurnId !== null) {
          session.pauseTargets.delete(pauseTargetKey(threadId, session.activeTurnId));
        }
        session.activeTurnId = null;
        session.pending.clear();
        session.answeredRequests.clear();
        session.descriptor = { ...session.descriptor, state: "stopped" };
      } else {
        const childTurn = session.childActiveTurns.get(threadId);
        if (childTurn !== undefined) {
          session.pauseTargets.delete(pauseTargetKey(threadId, childTurn));
          session.childActiveTurns.delete(threadId);
        }
        if (session.lifecycle === "pause_requested" && session.pauseTargets.size === 0) {
          session.lifecycle = "paused";
          session.pauseObservation = "settled";
          session.descriptor = { ...session.descriptor, state: "paused" };
          this.emit(session, worker, "session_paused", threadId, childTurn ?? null, null, {
            observation: "settled",
          });
        }
      }
      this.emit(session, worker, "session_status", threadId, null, null, {
        status: { type: "closed" },
      });
      return;
    }
    const status = CodexThreadStatusSchema.parse(notification.params.status);
    if (isCoordinatorThread && session.lifecycle === "active") {
      session.descriptor = { ...session.descriptor, state: stateOf(status) };
    }
    this.emit(session, worker, "session_status", threadId, null, null, {
      status,
    });
  }

  #recordChild(session: SessionState, worker: WorkerState, thread: CodexThread): void {
    session.childThreads.set(thread.id, thread);
    this.emit(session, worker, "native_child_observed", thread.id, null, null, {
      fromNativeThreadId: thread.parentThreadId,
      toNativeThreadId: thread.id,
      relationship: "parent",
      canAcceptDirectInput: thread.canAcceptDirectInput ?? null,
    });
  }

  #trackPauseTurn(
    session: SessionState,
    worker: WorkerState,
    threadId: string,
    turnId: string,
  ): void {
    const key = pauseTargetKey(threadId, turnId);
    session.pauseTargets.add(key);
    session.lifecycle = "pause_requested";
    session.pauseObservation = "requested";
    session.descriptor = { ...session.descriptor, state: "pausing" };
    this.emit(session, worker, "session_pause_requested", threadId, turnId, null, {
      lateObservedTurn: true,
    });
    void worker.process.request("turn/interrupt", { threadId, turnId }).then((result) => {
      if (result.ok || !session.pauseTargets.has(key)) return;
      session.pauseObservation = result.error.kind === "rpc" ? "refused" : "uncertain";
      this.emit(session, worker, "lifecycle_uncertain", threadId, turnId, null, {
        action: "pause",
        observation: session.pauseObservation,
      });
    });
  }

  #byThread(worker: WorkerState, threadId: string): SessionState | undefined {
    const session = this.#sessions.get(worker.ownerCoordinatorSessionId);
    if (
      session !== undefined &&
      session.descriptor.processEpoch === worker.epoch &&
      (session.descriptor.nativeThreadRef.value === threadId || session.childThreads.has(threadId))
    ) {
      return session;
    }
    return undefined;
  }
}

function pauseTargetKey(threadId: string, turnId: string): string {
  return `${threadId}\u0000${turnId}`;
}
