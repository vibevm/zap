/** @scope spec://org.vibevm.zap/lens/PROP-005#history */
import { DecimalSchema } from "../protocol/index.ts";
import {
  type HistoryEvent,
  type WorkspaceAccessContext,
  type WorkspaceEventsRequest,
  type WorkspaceResult,
  type WorkspaceSubscribeRequest,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";

interface Subscriber {
  readonly access: WorkspaceAccessContext;
  readonly scope: WorkspaceEventsRequest["cursor"]["scope"];
  afterGlobalSequence: bigint;
  readonly queue: HistoryEvent[];
  readonly waiters: Array<() => void>;
}

function matchesScope(
  event: HistoryEvent,
  access: WorkspaceAccessContext,
  scope: Subscriber["scope"],
): boolean {
  if (!access.authorizedProjectIds.includes(event.projectId)) return false;
  if (scope.kind === "all_authorized") return true;
  if (scope.projectId !== event.projectId) return false;
  if (scope.kind === "project") return true;
  if (scope.contextId !== event.contextId) return false;
  return scope.kind === "context" || scope.actorId === event.actorId;
}

export class WorkspaceSubscriptionHub {
  readonly #store: WorkspaceStore;
  readonly #subscribers = new Set<Subscriber>();
  #closed = false;

  constructor(store: WorkspaceStore) {
    this.#store = store;
  }

  subscribe(
    access: WorkspaceAccessContext,
    request: WorkspaceSubscribeRequest,
  ): AsyncIterable<WorkspaceResult<HistoryEvent>> {
    return this.#run(access, request);
  }

  notify(event: HistoryEvent): void {
    for (const subscriber of this.#subscribers) {
      if (
        BigInt(event.globalSequence) <= subscriber.afterGlobalSequence ||
        !matchesScope(event, subscriber.access, subscriber.scope)
      )
        continue;
      subscriber.queue.push(event);
      for (const wake of subscriber.waiters) wake();
      subscriber.waiters.length = 0;
    }
  }

  close(): void {
    this.#closed = true;
    for (const subscriber of this.#subscribers) {
      for (const wake of subscriber.waiters) wake();
      subscriber.waiters.length = 0;
    }
    this.#subscribers.clear();
  }

  async *#run(
    access: WorkspaceAccessContext,
    request: WorkspaceSubscribeRequest,
  ): AsyncIterable<WorkspaceResult<HistoryEvent>> {
    const subscriber: Subscriber = {
      access,
      scope: request.cursor.scope,
      afterGlobalSequence: BigInt(request.cursor.afterGlobalSequence),
      queue: [],
      waiters: [],
    };
    this.#subscribers.add(subscriber);
    try {
      while (!this.#closed && request.signal?.aborted !== true) {
        const page = this.#store.events(access, {
          cursor: {
            scope: subscriber.scope,
            afterGlobalSequence: DecimalSchema.parse(String(subscriber.afterGlobalSequence)),
          },
          limit: 256,
        });
        if (!page.ok) {
          yield page;
          return;
        }
        if (page.value.events.length > 0) {
          for (const event of page.value.events) {
            subscriber.afterGlobalSequence = BigInt(event.globalSequence);
            yield { ok: true, value: event };
          }
          continue;
        }
        const queued = subscriber.queue.shift();
        if (queued !== undefined) {
          subscriber.afterGlobalSequence = BigInt(queued.globalSequence);
          yield { ok: true, value: queued };
          continue;
        }
        await new Promise<void>((resolve) => subscriber.waiters.push(resolve));
      }
    } finally {
      this.#subscribers.delete(subscriber);
    }
  }
}
