/** @scope spec://org.vibevm.zap/lens/PROP-002#interaction */
import {
  DecimalSchema,
  type ConversationId,
  type EventsInput,
  type LosslessDecimal,
  type WorkspaceId,
} from "../protocol/index.ts";
import type { Awaitable } from "../transport/index.ts";
import type { InvalidationReason } from "../quicklens-model/index.ts";

export interface InvalidationMonitorOptions {
  readonly broker: InvalidationBrokerPort;
  readonly zap: InvalidationZapPort;
  readonly workspaceId: WorkspaceId;
  readonly conversationId: ConversationId;
  readonly intervalMilliseconds: number;
}

interface InvalidationBrokerPort {
  events(input: EventsInput): Awaitable<
    | {
        readonly ok: true;
        readonly value: {
          readonly observationCursor: LosslessDecimal;
          readonly events: readonly { readonly kind: string }[];
        };
      }
    | { readonly ok: false }
  >;
}

interface InvalidationZapPort {
  activeContext(): Promise<
    { readonly ok: true; readonly value: { readonly revision: string } } | { readonly ok: false }
  >;
}

export class InvalidationMonitor {
  readonly #options: InvalidationMonitorOptions;
  #cursor = DecimalSchema.parse("0");
  #zapRevision: string | undefined;
  #initialized = false;
  #timer: NodeJS.Timeout | undefined;
  #listener: ((reason: InvalidationReason) => void) | undefined;
  #running = false;
  #generation = 0;

  constructor(options: InvalidationMonitorOptions) {
    this.#options = options;
  }

  start(listener: (reason: InvalidationReason) => void): void {
    this.#listener = listener;
    if (this.#running) return;
    this.#running = true;
    void this.tick(++this.#generation);
  }

  stop(): void {
    this.#running = false;
    this.#generation += 1;
    this.#listener = undefined;
    if (this.#timer !== undefined) clearTimeout(this.#timer);
    this.#timer = undefined;
  }

  private async tick(generation: number): Promise<void> {
    const [events, active] = await Promise.all([
      this.#options.broker.events({
        workspaceId: this.#options.workspaceId,
        conversationId: this.#options.conversationId,
        afterSequence: this.#cursor,
        limit: 100,
      }),
      this.#options.zap.activeContext(),
    ]);
    if (!this.#running || generation !== this.#generation) return;
    if (events.ok) {
      this.#cursor = events.value.observationCursor;
      if (events.value.events.length > 0) {
        const question = events.value.events.some((event) => event.kind.startsWith("question."));
        this.#listener?.(question ? "questions" : "events");
      }
    } else if (this.#initialized) {
      this.#listener?.("reconnect");
    }
    if (active.ok) {
      const revision = active.value.revision;
      if (this.#initialized && this.#zapRevision !== undefined && revision !== this.#zapRevision) {
        this.#listener?.("plan");
      }
      this.#zapRevision = revision;
    } else if (this.#initialized) {
      this.#listener?.("reconnect");
    }
    this.#initialized = true;
    if (generation === this.#generation) {
      this.#timer = setTimeout(
        () => void this.tick(generation),
        this.#options.intervalMilliseconds,
      );
    }
  }
}
