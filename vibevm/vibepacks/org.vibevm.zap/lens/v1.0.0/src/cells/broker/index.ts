/**
 * Durable, transport-independent broker seam.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-001#broker
 * @example
 * const opened = openBroker({ databasePath: ":memory:" });
 * if (opened.ok) opened.value.close();
 */
import type {
  AckInput,
  AckResult,
  ActorList,
  ActorDescriptor,
  AmendAnswerInput,
  AnswerQuestionInput,
  AskInput,
  BindingAuth,
  CancelQuestionInput,
  ConnectInput,
  Connection,
  DelegateInput,
  EmitInput,
  EnrollPrincipalInput,
  EventPage,
  EventsInput,
  ExpireActorInput,
  ForwardInboxInput,
  ForwardResult,
  InboxInput,
  InboxPage,
  MessageEnvelope,
  MessageId,
  PrincipalAuth,
  PrincipalEmitInput,
  PrincipalEnrollment,
  PublicConnection,
  Question,
  QuestionList,
  Result,
  ResumeInput,
  ScopedListInput,
} from "../protocol/index.ts";

export interface OpenBrokerOptions {
  readonly databasePath: string;
  readonly clock?: () => Date;
}

/** The complete stage-one core port consumed by HTTP, MCP and CLI adapters. */
export interface LensBroker {
  enrollPrincipal(input: EnrollPrincipalInput): Result<PrincipalEnrollment>;
  connect(input: ConnectInput): Result<Connection>;
  resume(input: ResumeInput): Result<Connection>;
  context(auth: BindingAuth): Result<PublicConnection>;
  delegate(auth: BindingAuth, input: DelegateInput): Result<Connection>;
  emit(auth: BindingAuth, input: EmitInput): Result<MessageEnvelope>;
  ask(auth: BindingAuth, input: AskInput): Result<Question>;
  inbox(auth: BindingAuth, input: InboxInput): Result<InboxPage>;
  planIntent(auth: BindingAuth, messageId: MessageId): Result<MessageEnvelope>;
  ack(auth: BindingAuth, input: AckInput): Result<AckResult>;
  events(auth: PrincipalAuth, input: EventsInput): Result<EventPage>;
  listActors(auth: PrincipalAuth, input: ScopedListInput): Result<ActorList>;
  listQuestions(auth: PrincipalAuth, input: ScopedListInput): Result<QuestionList>;
  emitPrincipal(auth: PrincipalAuth, input: PrincipalEmitInput): Result<MessageEnvelope>;
  answer(auth: PrincipalAuth, input: AnswerQuestionInput): Result<Question>;
  amendAnswer(auth: PrincipalAuth, input: AmendAnswerInput): Result<Question>;
  cancelQuestion(auth: BindingAuth, input: CancelQuestionInput): Result<Question>;
  expireActor(auth: BindingAuth, input: ExpireActorInput): Result<ActorDescriptor>;
  expireQuestions(limit?: number): Result<readonly Question[]>;
  forwardInbox(auth: BindingAuth, input: ForwardInboxInput): Result<ForwardResult>;
  executePlan(auth: BindingAuth, input: unknown): Result<never>;
  close(): Result<null>;
}

export { openBroker } from "./implementation.ts";
