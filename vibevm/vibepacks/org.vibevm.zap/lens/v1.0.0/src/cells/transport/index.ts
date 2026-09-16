/**
 * Trusted transport helpers shared by HTTP and MCP adapters.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-001#transport
 * @example
 * const sessions = new AdapterSessions(() => "adapter.example");
 * const safe = sessions.retain(principalToken, connection);
 * sessions.resolve(safe.adapterSessionId);
 */
import type {
  AckInput,
  AckResult,
  ActorList,
  ActorDescriptor,
  ActorHandle,
  ActorId,
  ActorState,
  AnswerQuestionInput,
  AskInput,
  BindingAuth,
  BrokerError,
  Connection,
  ClientRequestId,
  ConnectInput,
  DelegateInput,
  EmitInput,
  ExpireActorInput,
  EventPage,
  EventsInput,
  InboxInput,
  WaitInboxInput,
  InboxPage,
  MessageEnvelope,
  MessageId,
  PrincipalAuth,
  PrincipalEmitInput,
  PublicConnection,
  Question,
  QuestionList,
  Result,
  ResumeInput,
  Credential,
  HostBinding,
  ForwardInboxInput,
  ForwardResult,
  LosslessDecimal,
  ReplyPolicy,
  ScopedListInput,
} from "../protocol/index.ts";
import { DecimalSchema, publicConnection } from "../protocol/index.ts";
import { z } from "zod";
import type { AgentQuestionInput, AgentQuestionPublisher } from "../workspace-interaction/index.ts";
import type { QuestionGroup } from "../workspace-model/index.ts";

const requirement = "spec://org.vibevm.zap/lens/PROP-001#identity";

export const AdapterSessionIdSchema = z
  .string()
  .min(24)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/)
  .brand<"AdapterSessionId">();
export type AdapterSessionId = z.infer<typeof AdapterSessionIdSchema>;

export interface PublicAdapterConnection {
  readonly adapterSessionId: AdapterSessionId;
  readonly connection: PublicConnection;
}

export type Awaitable<T> = T | Promise<T>;

export interface TransportBrokerPort {
  connect(input: ConnectInput): Awaitable<Result<Connection>>;
  resume(input: ResumeInput): Awaitable<Result<Connection>>;
  delegate(auth: BindingAuth, input: DelegateInput): Awaitable<Result<Connection>>;
  emit(auth: BindingAuth, input: EmitInput): Awaitable<Result<MessageEnvelope>>;
  ask(auth: BindingAuth, input: AskInput): Awaitable<Result<Question>>;
  inbox(auth: BindingAuth, input: InboxInput): Awaitable<Result<InboxPage>>;
  planIntent(auth: BindingAuth, messageId: MessageId): Awaitable<Result<MessageEnvelope>>;
  ack(auth: BindingAuth, input: AckInput): Awaitable<Result<AckResult>>;
  events(auth: PrincipalAuth, input: EventsInput): Awaitable<Result<EventPage>>;
  answer(auth: PrincipalAuth, input: AnswerQuestionInput): Awaitable<Result<Question>>;
  listActors(auth: PrincipalAuth, input: ScopedListInput): Awaitable<Result<ActorList>>;
  listQuestions(auth: PrincipalAuth, input: ScopedListInput): Awaitable<Result<QuestionList>>;
  emitPrincipal(auth: PrincipalAuth, input: PrincipalEmitInput): Awaitable<Result<MessageEnvelope>>;
  expireActor(auth: BindingAuth, input: ExpireActorInput): Awaitable<Result<ActorDescriptor>>;
  forwardInbox(auth: BindingAuth, input: ForwardInboxInput): Awaitable<Result<ForwardResult>>;
  context(auth: BindingAuth): Awaitable<Result<PublicConnection>>;
}

/** Safe agent-side port implemented locally in tests and through HTTP in production. */
export interface AgentTransportPort {
  connect(input: Omit<ConnectInput, "principalToken">): Awaitable<Result<PublicAdapterConnection>>;
  delegate(
    session: AdapterSessionId,
    input: DelegateInput,
  ): Awaitable<Result<PublicAdapterConnection>>;
  emit(session: AdapterSessionId, input: EmitInput): Awaitable<Result<MessageEnvelope>>;
  ask(session: AdapterSessionId, input: AskInput): Awaitable<Result<Question>>;
  inbox(session: AdapterSessionId, input: InboxInput): Awaitable<Result<InboxPage>>;
  waitInbox(session: AdapterSessionId, input: WaitInboxInput): Awaitable<Result<InboxPage>>;
  planIntent(session: AdapterSessionId, messageId: MessageId): Awaitable<Result<MessageEnvelope>>;
  ack(session: AdapterSessionId, input: AckInput): Awaitable<Result<AckResult>>;
  finish(
    session: AdapterSessionId,
    clientRequestId: ClientRequestId,
  ): Awaitable<Result<ActorDescriptor>>;
  forward(session: AdapterSessionId, input: ForwardInboxInput): Awaitable<Result<ForwardResult>>;
  context(session: AdapterSessionId): Awaitable<Result<PublicConnection>>;
  askUserQuestion?(
    session: AdapterSessionId,
    input: AgentQuestionInput,
  ): Awaitable<Result<QuestionGroup>>;
}

export interface PrincipalTransportPort {
  listActors(input: ScopedListInput): Awaitable<Result<ActorList>>;
  listQuestions(input: ScopedListInput): Awaitable<Result<QuestionList>>;
  answer(input: AnswerQuestionInput): Awaitable<Result<Question>>;
  emit(input: PrincipalEmitInput): Awaitable<Result<MessageEnvelope>>;
  events(input: EventsInput): Awaitable<Result<EventPage>>;
}

export interface LocalAgentTransportOptions {
  readonly broker: TransportBrokerPort;
  readonly principalToken: Credential;
  readonly adapterSessionIdFactory: () => string;
  readonly agentQuestions?: AgentQuestionPublisher;
}

/** Test/in-process composition; production MCP uses the background HTTP port. */
export function createLocalAgentTransport(options: LocalAgentTransportOptions): AgentTransportPort {
  const sessions = new AdapterSessions(options.adapterSessionIdFactory);
  return createRetainedAgentTransport({
    broker: options.broker,
    principalToken: options.principalToken,
    sessions,
    ...(options.agentQuestions === undefined ? {} : { agentQuestions: options.agentQuestions }),
  });
}

export interface RetainedAgentTransportOptions {
  readonly broker: TransportBrokerPort;
  readonly principalToken: Credential;
  readonly sessions: AdapterSessions;
  readonly agentQuestions?: AgentQuestionPublisher;
}

/** Reuses the HTTP gateway's authoritative durable adapter-session vault. */
export function createRetainedAgentTransport(
  options: RetainedAgentTransportOptions,
): AgentTransportPort {
  const sessions = options.sessions;
  const agentQuestions = options.agentQuestions;
  const auth = (session: AdapterSessionId): Result<BindingAuth> => sessions.resolve(session);
  return {
    connect: async (input) => {
      const result = await options.broker.connect({
        ...input,
        principalToken: options.principalToken,
      });
      return result.ok
        ? sessions.retain(
            options.principalToken,
            result.value,
            input.host,
            input.replyPolicy ?? { kind: "retain" },
          )
        : result;
    },
    delegate: async (session, input) => {
      const resolved = auth(session);
      if (!resolved.ok) return resolved;
      const result = await options.broker.delegate(resolved.value, input);
      return result.ok
        ? sessions.retain(
            options.principalToken,
            result.value,
            input.host,
            input.replyPolicy ?? { kind: "retain" },
          )
        : result;
    },
    emit: async (session, input) =>
      authorized(auth(session), (value) => options.broker.emit(value, input)),
    ask: async (session, input) =>
      authorized(auth(session), (value) => options.broker.ask(value, input)),
    inbox: async (session, input) =>
      authorized(auth(session), (value) => options.broker.inbox(value, input)),
    waitInbox: async (session, input) => {
      const resolved = auth(session);
      if (!resolved.ok) return resolved;
      const deadline = Date.now() + input.timeoutMilliseconds;
      let page = await options.broker.inbox(resolved.value, input);
      while (page.ok && page.value.deliveries.length === 0 && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, Math.min(50, deadline - Date.now())));
        page = await options.broker.inbox(resolved.value, input);
      }
      return page;
    },
    planIntent: async (session, messageId) =>
      authorized(auth(session), (value) => options.broker.planIntent(value, messageId)),
    ack: async (session, input) =>
      authorized(auth(session), (value) => options.broker.ack(value, input)),
    finish: async (session, clientRequestId) => {
      const actor = sessions.actor(session);
      return actor.ok
        ? authorized(auth(session), (value) =>
            options.broker.expireActor(value, {
              clientRequestId,
              actorId: actor.value.actorId,
            }),
          )
        : actor;
    },
    forward: async (session, input) =>
      authorized(auth(session), (value) => options.broker.forwardInbox(value, input)),
    context: async (session) => authorized(auth(session), (value) => options.broker.context(value)),
    ...(agentQuestions === undefined
      ? {}
      : {
          askUserQuestion: async (session: AdapterSessionId, input: AgentQuestionInput) => {
            const context = await authorized(auth(session), (value) =>
              options.broker.context(value),
            );
            return context.ok ? agentQuestions.publish(context.value, input) : context;
          },
        }),
  };
}

export function createLocalPrincipalTransport(
  broker: TransportBrokerPort,
  principalToken: Credential,
): PrincipalTransportPort {
  const auth = { principalToken };
  return {
    listActors: async (input) => broker.listActors(auth, input),
    listQuestions: async (input) => broker.listQuestions(auth, input),
    answer: async (input) => broker.answer(auth, input),
    emit: async (input) => broker.emitPrincipal(auth, input),
    events: async (input) => broker.events(auth, input),
  };
}

async function authorized<T>(
  auth: Result<BindingAuth>,
  operation: (value: BindingAuth) => Awaitable<Result<T>>,
): Promise<Result<T>> {
  return auth.ok ? await operation(auth.value) : auth;
}

export interface RetainedSession {
  readonly principalToken: Credential;
  readonly connection: Connection;
  readonly host: HostBinding;
  readonly replyPolicy: ReplyPolicy;
}

export interface AdapterSessionVault {
  actorState(actorId: ActorId): Result<ActorState>;
  get(id: AdapterSessionId): Result<RetainedSession>;
  list(): Result<readonly [AdapterSessionId, RetainedSession][]>;
  put(id: AdapterSessionId, session: RetainedSession): Result<null>;
  remove(id: AdapterSessionId): Result<null>;
  offerCursor(actorId: ActorId): Result<LosslessDecimal>;
  setOfferCursor(actorId: ActorId, cursor: LosslessDecimal): Result<null>;
}

export class MemoryAdapterSessionVault implements AdapterSessionVault {
  readonly #values = new Map<AdapterSessionId, RetainedSession>();
  readonly #offerCursors = new Map<ActorId, LosslessDecimal>();
  public actorState(actorId: ActorId): Result<ActorState> {
    const session = [...this.#values.values()].find(
      (value) => value.connection.actor.actorId === actorId,
    );
    return session === undefined
      ? failure("not_found", "adapter actor does not exist")
      : { ok: true, value: session.connection.actor.state };
  }
  public get(id: AdapterSessionId): Result<RetainedSession> {
    const value = this.#values.get(id);
    return value === undefined
      ? failure("not_found", "adapter session does not exist")
      : { ok: true, value };
  }
  public list(): Result<readonly [AdapterSessionId, RetainedSession][]> {
    return { ok: true, value: [...this.#values.entries()] };
  }
  public put(id: AdapterSessionId, session: RetainedSession): Result<null> {
    this.#values.set(id, session);
    return { ok: true, value: null };
  }
  public remove(id: AdapterSessionId): Result<null> {
    this.#values.delete(id);
    return { ok: true, value: null };
  }
  public offerCursor(actorId: ActorId): Result<LosslessDecimal> {
    return {
      ok: true,
      value: this.#offerCursors.get(actorId) ?? DecimalSchema.parse("0"),
    };
  }
  public setOfferCursor(actorId: ActorId, cursor: LosslessDecimal): Result<null> {
    this.#offerCursors.set(actorId, cursor);
    return { ok: true, value: null };
  }
}

/** Adapter-owned credentials never cross the public transport seam. */
export class AdapterSessions {
  readonly #idFactory: () => string;
  readonly #vault: AdapterSessionVault;

  public constructor(
    idFactory: () => string,
    vault: AdapterSessionVault = new MemoryAdapterSessionVault(),
  ) {
    this.#idFactory = idFactory;
    this.#vault = vault;
  }

  public retain(
    principalToken: Credential,
    connection: Connection,
    host: HostBinding,
    replyPolicy: ReplyPolicy,
  ): Result<PublicAdapterConnection> {
    const retained = this.#vault.list();
    if (!retained.ok) return retained;
    const existing = retained.value.find(
      ([, session]) =>
        session.principalToken === principalToken &&
        session.connection.actor.actorId === connection.actor.actorId,
    );
    if (existing !== undefined) {
      const [id] = existing;
      const replaced = this.#vault.put(id, { principalToken, connection, host, replyPolicy });
      return replaced.ok
        ? {
            ok: true,
            value: {
              adapterSessionId: id,
              connection: publicConnection(connection),
            },
          }
        : replaced;
    }
    const parsedId = AdapterSessionIdSchema.safeParse(this.#idFactory());
    if (!parsedId.success) {
      return failure(
        "invalid_input",
        "adapter session ID factory returned an invalid opaque identity",
      );
    }
    const stored = this.#vault.put(parsedId.data, {
      principalToken,
      connection,
      host,
      replyPolicy,
    });
    if (!stored.ok) return stored;
    return {
      ok: true,
      value: {
        adapterSessionId: parsedId.data,
        connection: publicConnection(connection),
      },
    };
  }

  public resolve(id: AdapterSessionId): Result<BindingAuth> {
    const loaded = this.#vault.get(id);
    if (!loaded.ok) return failure("unauthorized", "adapter session is unavailable or expired");
    const session = loaded.value;
    const state = this.#vault.actorState(session.connection.actor.actorId);
    if (!state.ok || state.value !== "active") {
      return failure("stale_binding", "adapter actor is not authoritatively active");
    }
    return {
      ok: true,
      value: {
        principalToken: session.principalToken,
        bindingToken: session.connection.credentials.bindingToken,
      },
    };
  }

  public actor(id: AdapterSessionId): Result<ActorDescriptor> {
    const loaded = this.#vault.get(id);
    return loaded.ok
      ? { ok: true, value: loaded.value.connection.actor }
      : failure("unauthorized", "adapter session is unavailable or expired");
  }

  public context(id: AdapterSessionId, principalToken: Credential): Result<PublicConnection> {
    const loaded = this.#vault.get(id);
    if (!loaded.ok || loaded.value.principalToken !== principalToken) {
      return failure("unauthorized", "adapter session does not belong to this principal");
    }
    const resolved = this.resolve(id);
    return resolved.ok ? { ok: true, value: publicConnection(loaded.value.connection) } : resolved;
  }

  public forget(id: AdapterSessionId): void {
    this.#vault.remove(id);
  }

  public retained(): Result<readonly [AdapterSessionId, RetainedSession][]> {
    return this.#vault.list();
  }

  public actorState(actorId: ActorId): Result<ActorState> {
    return this.#vault.actorState(actorId);
  }

  public offerCursor(actorId: ActorId): Result<LosslessDecimal> {
    return this.#vault.offerCursor(actorId);
  }

  public setOfferCursor(actorId: ActorId, cursor: LosslessDecimal): Result<null> {
    return this.#vault.setOfferCursor(actorId, cursor);
  }

  public bindingsFor(principalToken: Credential): Result<
    readonly {
      readonly actor: ActorDescriptor;
      readonly handle: Connection["handle"];
      readonly host: HostBinding;
    }[]
  > {
    const retained = this.#vault.list();
    if (!retained.ok) return retained;
    return {
      ok: true,
      value: retained.value
        .filter(([, session]) => session.principalToken === principalToken)
        .map(([, session]) => ({
          actor: session.connection.actor,
          handle: session.connection.handle,
          host: session.host,
        })),
    };
  }

  public sessionForActor(
    principalToken: Credential,
    actorId: ActorId,
  ): Result<{ readonly id: AdapterSessionId; readonly auth: BindingAuth }> {
    const retained = this.#vault.list();
    if (!retained.ok) return retained;
    const found = retained.value.find(
      ([, session]) =>
        session.principalToken === principalToken && session.connection.actor.actorId === actorId,
    );
    if (found === undefined) return failure("not_found", "routed actor session is unavailable");
    const resolved = this.resolve(found[0]);
    return resolved.ok ? { ok: true, value: { id: found[0], auth: resolved.value } } : resolved;
  }

  public handleFor(
    id: AdapterSessionId,
    principalToken: Credential,
  ): Result<ActorHandle & { readonly nativeAttested: boolean }> {
    const retained = this.#vault.get(id);
    return retained.ok && retained.value.principalToken === principalToken
      ? {
          ok: true,
          value: {
            ...retained.value.connection.handle,
            nativeAttested: retained.value.host.provenance === "attested",
          },
        }
      : failure("unauthorized", "adapter session does not belong to this principal");
  }

  public forwardAllowed(
    principalToken: Credential,
    fromActorId: ActorId,
    toActorId: ActorId,
  ): Result<null> {
    const retained = this.#vault.list();
    if (!retained.ok) return retained;
    const child = retained.value.find(
      ([, session]) =>
        session.principalToken === principalToken &&
        session.connection.actor.actorId === fromActorId,
    );
    return child !== undefined &&
      child[1].replyPolicy.kind === "forward_parent" &&
      child[1].connection.actor.parentActorId === toActorId
      ? { ok: true, value: null }
      : failure("forbidden", "forwarding does not match the child's declared parent policy");
  }

  public replace(
    id: AdapterSessionId,
    principalToken: Credential,
    connection: Connection,
    host: HostBinding,
    replyPolicy: ReplyPolicy,
  ): Result<null> {
    return this.#vault.put(id, { principalToken, connection, host, replyPolicy });
  }
}

export function failure(
  code: BrokerError["code"],
  why: string,
  fix = "reconnect through the trusted transport adapter",
): Result<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ ${requirement}: ${why}; fix surface: ${fix}`,
    },
  };
}
