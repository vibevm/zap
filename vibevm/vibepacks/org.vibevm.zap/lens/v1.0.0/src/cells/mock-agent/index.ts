/** Synthetic public AgentHost/CoordinatorAdapter over ZapMockModel. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import {
  CoordinatorCapabilitiesSchema,
  CoordinatorEventSchema,
  CoordinatorHistorySchema,
  CoordinatorLifecycleCapabilitiesSchema,
  CoordinatorLifecycleReceiptSchema,
  CoordinatorResumeInputSchema,
  CoordinatorStartInputSchema,
  CoordinatorTurnInputSchema,
  type AgentHost,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorEvent,
  type CoordinatorHistory,
  type CoordinatorLifecycleInput,
  type CoordinatorLifecycleReceipt,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorTurnInput,
  type CoordinatorTurnReceipt,
} from "../agent-runtime/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import {
  ZAP_MOCK_MODEL_ID,
  type ZapMockInput,
  type ZapMockModel,
  type ZapMockSnapshot,
  type ZapMockState,
} from "../mock-model/index.ts";
import { mockDescriptor as descriptor, mockFailure as failure } from "./adapter-result.ts";
export { ZapMockScenarioFileSchema } from "./managed-process.ts";

export interface ZapMockAgentOptions {
  readonly modelFactory: (snapshot?: ZapMockSnapshot) => ZapMockModel;
  readonly questionBridge?: ZapMockQuestionBridge;
  readonly profileId?: string;
  readonly hostId?: string;
}
export interface ZapMockTestDriver {
  dispatch(sessionId: string, input: ZapMockInput): Promise<AgentRuntimeResult<null>>;
  observations(): readonly ZapMockTurnObservation[];
  state(sessionId: string): ZapMockState | null;
}
export interface ZapMockTurnObservation {
  readonly sessionId: string;
  readonly clientMessageId: string;
  readonly text: string;
  readonly accepted: boolean;
}

export interface ZapMockQuestionBridge {
  publishQuestion(input: {
    readonly adapterSessionId: string;
    readonly questionId: string;
    readonly prompt: string;
    readonly options: readonly string[];
  }): Promise<MockBridgeResult<{ readonly questionGroupId: string }>>;
  readAnswer(adapterSessionId: string): Promise<MockBridgeResult<readonly MockAnswerDelivery[]>>;
  acknowledge(input: {
    readonly adapterSessionId: string;
    readonly deliveryId: string;
    readonly answerVersion: string;
  }): Promise<MockBridgeResult<null>>;
}
export interface MockAnswerDelivery {
  readonly deliveryId: string;
  readonly questionId: string;
  readonly answerVersion: string;
  readonly text: string;
}
interface ZapMockAdapterCheckpoint {
  readonly model: ZapMockSnapshot;
  readonly pendingQuestionId: string | null;
  readonly pendingQuestionGroupId: string | null;
  readonly pendingAcknowledgementId: string | null;
}
export type MockBridgeResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: { readonly message: string } };

const capabilities = CoordinatorCapabilitiesSchema.parse({
  persistentThreads: true,
  turnStart: true,
  activeTurnSteer: false,
  turnInterrupt: true,
  historyRead: true,
  nativeChildObservation: false,
  nativeChildDirectInput: false,
  structuredUserInput: true,
  commandApproval: false,
  managedTerminal: false,
});
const lifecycleCapabilities = CoordinatorLifecycleCapabilitiesSchema.parse({
  pause: "interrupt_owned_session",
  stop: "owned_process",
  continue: "saved_thread_resume",
  nativeChildren: "unsupported",
});

export function createZapMockAgent(options: ZapMockAgentOptions): {
  readonly host: AgentHost;
  readonly driver: ZapMockTestDriver;
} {
  const profileId = options.profileId ?? "profile.zap-mock";
  const hostId = ExecutionHostIdSchema.parse(options.hostId ?? "host.zap-mock");
  const snapshots = new Map<string, ZapMockAdapterCheckpoint>();
  const adapters = new Map<string, MockAdapter>();
  const host: AgentHost = {
    hostId,
    profileIds: [profileId],
    openCoordinator: async (requested) => {
      await Promise.resolve();
      if (requested !== profileId) return failure("not_found", "ZapMock profile is not registered");
      const adapter = new MockAdapter(
        options.modelFactory,
        hostId,
        options.questionBridge,
        snapshots,
      );
      adapters.set(`adapter.${String(adapters.size + 1)}`, adapter);
      return { ok: true, value: adapter };
    },
  };
  const driver: ZapMockTestDriver = {
    async dispatch(sessionId, input) {
      const adapter = [...adapters.values()].find(
        (candidate) => candidate.sessionId() === sessionId,
      );
      return adapter === undefined
        ? failure("not_found", "ZapMock session is not registered")
        : adapter.testDispatch(input);
    },
    observations: () => [...adapters.values()].flatMap((adapter) => adapter.observations()),
    state(sessionId) {
      const adapter = [...adapters.values()].find(
        (candidate) => candidate.sessionId() === sessionId,
      );
      return adapter?.modelState() ?? null;
    },
  };
  return {
    host,
    driver,
  };
}

class MockAdapter implements CoordinatorAdapter {
  readonly capabilities = capabilities;
  readonly lifecycleCapabilities = lifecycleCapabilities;
  #model: ZapMockModel;
  #descriptor: CoordinatorSessionDescriptor | null = null;
  #listener: ((event: CoordinatorEvent) => void) | null = null;
  #turnId: string | null = null;
  #clientMessageId: string | null = null;
  #sequence = 0;
  #closed = false;
  readonly #factory: (snapshot?: ZapMockSnapshot) => ZapMockModel;
  readonly #hostId: ReturnType<typeof ExecutionHostIdSchema.parse>;
  readonly #questionBridge: ZapMockQuestionBridge | undefined;
  #adapterSessionId: string | null = null;
  #pendingQuestionId: string | null = null;
  #pendingQuestionGroupId: string | null = null;
  #pendingAcknowledgementId: string | null = null;
  readonly #observations: ZapMockTurnObservation[] = [];
  readonly #snapshots: Map<string, ZapMockAdapterCheckpoint>;

  constructor(
    factory: (snapshot?: ZapMockSnapshot) => ZapMockModel,
    hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
    questionBridge: ZapMockQuestionBridge | undefined,
    snapshots: Map<string, ZapMockAdapterCheckpoint>,
  ) {
    this.#factory = factory;
    this.#hostId = hostId;
    this.#model = factory();
    this.#questionBridge = questionBridge;
    this.#snapshots = snapshots;
  }
  async start(
    raw: CoordinatorStartInput,
  ): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    const input = CoordinatorStartInputSchema.safeParse(raw);
    if (!input.success) return failure("invalid_input", "ZapMock start input is malformed");
    if (this.#closed || this.#descriptor !== null)
      return failure("already_exists", "ZapMock session exists");
    this.#model = this.#factory();
    this.#adapterSessionId = input.data.agentBinding?.adapterSessionId ?? null;
    this.#pendingQuestionId = null;
    this.#pendingQuestionGroupId = null;
    this.#pendingAcknowledgementId = null;
    this.#descriptor = descriptor(
      input.data,
      this.#hostId,
      "1",
      "submitted",
      "ready",
      capabilities,
    );
    const dispatched = await this.#dispatch(
      { kind: "start", inputId: `input.start.${input.data.coordinatorSessionId}` },
      "session_started",
    );
    if (!dispatched) {
      this.#descriptor = null;
      return failure("protocol_error", "ZapMock model refused start input");
    }
    return { ok: true, value: this.#descriptor };
  }
  async resume(
    raw: CoordinatorResumeInput,
  ): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    await Promise.resolve();
    const input = CoordinatorResumeInputSchema.safeParse(raw);
    if (!input.success) return failure("invalid_input", "ZapMock resume input is malformed");
    if (this.#closed) return failure("transport_lost", "ZapMock agent is closed");
    const saved = this.#snapshots.get(input.data.coordinatorSessionId);
    if (saved === undefined)
      return failure("not_found", "ZapMock session has no persisted checkpoint to resume");
    this.#model = this.#factory(saved.model);
    this.#descriptor = descriptor(
      input.data,
      this.#hostId,
      "2",
      "not_observed",
      "ready",
      capabilities,
      input.data.nativeThreadId,
    );
    this.#adapterSessionId = input.data.agentBinding?.adapterSessionId ?? null;
    this.#pendingQuestionId = saved.pendingQuestionId;
    this.#pendingQuestionGroupId = saved.pendingQuestionGroupId;
    this.#pendingAcknowledgementId = saved.pendingAcknowledgementId;
    this.#emit("session_resumed", null, null, { model: ZAP_MOCK_MODEL_ID });
    return { ok: true, value: this.#descriptor };
  }
  async send(raw: CoordinatorTurnInput): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return this.#turn(CoordinatorTurnInputSchema.safeParse(raw));
  }
  async startTurn(raw: CoordinatorTurnInput): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return this.#turn(CoordinatorTurnInputSchema.safeParse(raw));
  }
  async steer(raw: {
    readonly coordinatorSessionId: string;
    readonly text: string;
    readonly clientMessageId: string;
    readonly expectedNativeTurnId: string;
  }): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return this.#turn(CoordinatorTurnInputSchema.safeParse(raw));
  }
  async interrupt(sessionId: string, turnId: string): Promise<AgentRuntimeResult<void>> {
    if (this.#descriptor?.coordinatorSessionId !== sessionId || this.#turnId !== turnId)
      return failure("stale_epoch", "ZapMock turn is stale");
    if (
      !(await this.#dispatch(
        { kind: "pause", inputId: `input.pause.${String(++this.#sequence)}` },
        "session_paused",
      ))
    )
      return failure("protocol_error", "ZapMock model refused interrupt input");
    return { ok: true, value: undefined };
  }
  async respondToRequest(): Promise<AgentRuntimeResult<void>> {
    await Promise.resolve();
    return failure("unsupported", "ZapMock uses the authenticated question bridge for user input");
  }
  async readHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    await Promise.resolve();
    if (this.#descriptor?.coordinatorSessionId !== sessionId)
      return failure("not_found", "ZapMock session not found");
    const snapshot = this.#model.snapshot();
    return {
      ok: true,
      value: CoordinatorHistorySchema.parse({
        coordinatorSessionId: sessionId,
        nativeThreadId: this.#thread(),
        processEpoch: this.#epoch(),
        status: snapshot.state.status,
        turns: snapshot.trace,
      }),
    };
  }
  async readNativeChildHistory(sessionId: string): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return this.readHistory(sessionId);
  }
  async restart(): Promise<AgentRuntimeResult<readonly CoordinatorSessionDescriptor[]>> {
    await Promise.resolve();
    return { ok: true, value: this.#descriptor === null ? [] : [this.#descriptor] };
  }
  async pause(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    return this.#lifecycle(input, "pause", "pause");
  }
  async stop(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    return this.#lifecycle(input, "stop", "stop");
  }
  async continueSession(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    return this.#lifecycle(input, "continue", "continue");
  }
  subscribe(listener: (event: CoordinatorEvent) => void): () => void {
    this.#listener = listener;
    return () => {
      if (this.#listener === listener) this.#listener = null;
    };
  }
  close(): void {
    this.#closed = true;
    this.#listener = null;
  }
  sessionId(): string | null {
    return this.#descriptor?.coordinatorSessionId ?? null;
  }
  async testDispatch(input: ZapMockInput): Promise<AgentRuntimeResult<null>> {
    if (this.#descriptor === null) return failure("not_found", "ZapMock session is not started");
    return (await this.#dispatch(input, "session_status"))
      ? { ok: true, value: null }
      : failure("protocol_error", "ZapMock test input was refused");
  }
  observations(): readonly ZapMockTurnObservation[] {
    return [...this.#observations];
  }
  modelState(): ZapMockState {
    return this.#model.state();
  }

  async #turn(
    parsed: ReturnType<typeof CoordinatorTurnInputSchema.safeParse>,
  ): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    if (!parsed.success)
      return Promise.resolve(failure("invalid_input", "ZapMock turn is malformed"));
    if (this.#descriptor?.coordinatorSessionId !== parsed.data.coordinatorSessionId)
      return Promise.resolve(failure("stale_epoch", "ZapMock turn session scope is stale"));
    if (this.#descriptor.state !== "ready")
      return Promise.resolve(failure("busy", "ZapMock session is not ready"));
    this.#turnId = `turn.${String(++this.#sequence)}`;
    this.#clientMessageId = parsed.data.clientMessageId;
    this.#descriptor = { ...this.#descriptor, state: "running" };
    let consumedAnswer: MockAnswerDelivery | null = null;
    let modelInput: unknown = {
      kind: "message",
      inputId: `input.${this.#turnId}`,
      messageId: parsed.data.clientMessageId,
      text: parsed.data.text,
    };
    if (this.#pendingQuestionId !== null) {
      if (this.#questionBridge === undefined || this.#adapterSessionId === null)
        return Promise.resolve(failure("unsupported", "ZapMock question bridge is not configured"));
      const inbox = await this.#questionBridge.readAnswer(this.#adapterSessionId);
      if (!inbox.ok) return Promise.resolve(failure("transport_lost", inbox.error.message));
      const answer = inbox.value.find((item) => item.questionId === this.#pendingQuestionGroupId);
      if (answer === undefined)
        return Promise.resolve(failure("busy", "ZapMock question answer is not available"));
      consumedAnswer = answer;
      modelInput = {
        kind: "answer",
        inputId: `input.${this.#turnId}`,
        questionId: this.#pendingQuestionId,
        answerVersion: answer.answerVersion,
        text: answer.text,
      };
    }
    const dispatched = await this.#dispatch(modelInput, "turn_started");
    this.#observations.push({
      sessionId: parsed.data.coordinatorSessionId,
      clientMessageId: parsed.data.clientMessageId,
      text: parsed.data.text,
      accepted: dispatched,
    });
    if (!dispatched)
      return Promise.resolve(failure("protocol_error", "ZapMock model refused turn input"));
    if (consumedAnswer !== null && this.#pendingAcknowledgementId !== null) {
      const acknowledged = await this.#dispatch(
        {
          kind: "ack",
          inputId: `input.ack.${consumedAnswer.answerVersion}`,
          acknowledgementId: this.#pendingAcknowledgementId,
          answerVersion: consumedAnswer.answerVersion,
        },
        "turn_started",
      );
      if (!acknowledged)
        return Promise.resolve(failure("protocol_error", "ZapMock acknowledgement was refused"));
      if (this.#questionBridge !== undefined && this.#adapterSessionId !== null) {
        const brokerAck = await this.#questionBridge.acknowledge({
          adapterSessionId: this.#adapterSessionId,
          deliveryId: consumedAnswer.deliveryId,
          answerVersion: consumedAnswer.answerVersion,
        });
        if (!brokerAck.ok)
          return Promise.resolve(failure("transport_lost", brokerAck.error.message));
      }
    }
    const epoch = this.#epoch();
    return Promise.resolve({
      ok: true,
      value: {
        coordinatorSessionId: parsed.data.coordinatorSessionId,
        nativeThreadId: this.#thread(),
        nativeTurnId: null,
        transportCorrelation: {
          provenance: "transport_correlation",
          clientMessageId: parsed.data.clientMessageId,
          processEpoch: epoch,
        },
        observation: "host_accepted",
        processEpoch: epoch,
      },
    });
  }
  async #dispatch(input: unknown, firstEvent: CoordinatorEvent["kind"]): Promise<boolean> {
    const result = this.#model.dispatch(input);
    if (!result.ok) {
      this.#emit("protocol_error", null, null, { message: result.error.message });
      return false;
    }
    this.#emit(
      firstEvent,
      null,
      null,
      firstEvent === "session_status"
        ? { status: result.value.state.status === "busy" ? "running" : "ready" }
        : { model: ZAP_MOCK_MODEL_ID },
    );
    if (this.#descriptor !== null)
      this.#descriptor = {
        ...this.#descriptor,
        state:
          result.value.state.status === "paused"
            ? "paused"
            : result.value.state.status === "busy"
              ? "running"
              : result.value.state.status === "complete"
                ? "ready"
                : "ready",
      };
    for (const effect of result.value.effects) {
      if (effect.kind === "assistant_message")
        this.#emit("item_completed", null, `item.${effect.messageId}`, {
          type: "agentMessage",
          status: "completed",
          text: effect.text,
        });
      if (effect.kind === "question") {
        if (this.#questionBridge === undefined || this.#adapterSessionId === null) return false;
        const published = await this.#questionBridge.publishQuestion({
          adapterSessionId: this.#adapterSessionId,
          questionId: effect.questionId,
          prompt: effect.prompt,
          options: effect.options,
        });
        if (!published.ok) return false;
        this.#pendingQuestionId = effect.questionId;
        this.#pendingQuestionGroupId = published.value.questionGroupId;
        const followUp = this.#model
          .snapshot()
          .scenario.steps.find(
            (step) => step.kind === "late_answer" && step.questionId === effect.questionId,
          );
        this.#pendingAcknowledgementId =
          followUp?.kind === "late_answer" ? followUp.acknowledgementId : null;
        this.#emit("host_request_pending", null, `item.${effect.questionId}`, {
          kind: "user_input",
          requestId: effect.questionId,
          body: { prompt: effect.prompt, options: effect.options },
        });
      }
      if (effect.kind === "turn_settled")
        this.#emit("turn_completed", this.#turnId, null, { reason: effect.reason });
      if (effect.kind === "paused") this.#emit("session_paused", this.#turnId, null, {});
      if (effect.kind === "continued") this.#emit("session_continued", this.#turnId, null, {});
      if (effect.kind === "answer_acknowledged") {
        this.#pendingQuestionId = null;
        this.#pendingQuestionGroupId = null;
        this.#pendingAcknowledgementId = null;
      }
    }
    this.#checkpoint();
    return true;
  }
  async #lifecycle(
    input: CoordinatorLifecycleInput,
    action: CoordinatorLifecycleReceipt["action"],
    modelKind: "pause" | "stop" | "continue",
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    if (
      this.#descriptor?.coordinatorSessionId !== input.coordinatorSessionId ||
      this.#epoch() !== input.expectedProcessEpoch
    )
      return Promise.resolve(failure("stale_epoch", "ZapMock lifecycle epoch is stale"));
    const completed = this.#model.state().status === "complete";
    const dispatched =
      modelKind === "stop" || completed
        ? true
        : await this.#dispatch(
            { kind: modelKind, inputId: `input.${action}.${String(++this.#sequence)}` },
            action === "continue" ? "session_continued" : "session_paused",
          );
    if (!dispatched)
      return Promise.resolve(failure("protocol_error", "ZapMock lifecycle model refused input"));
    if (action === "stop") this.#emit("session_stopped", this.#turnId, null, {});
    else if (completed)
      this.#emit(
        action === "continue" ? "session_continued" : "session_paused",
        this.#turnId,
        null,
        {},
      );
    const nextEpoch = action === "continue" ? String(Number(input.expectedProcessEpoch) + 1) : null;
    this.#descriptor = {
      ...this.#descriptor,
      state: action === "continue" ? "ready" : action === "pause" ? "paused" : "stopped",
      ...(nextEpoch === null ? {} : { processEpoch: nextEpoch }),
    };
    this.#checkpoint();
    return Promise.resolve({
      ok: true,
      value: CoordinatorLifecycleReceiptSchema.parse({
        coordinatorSessionId: input.coordinatorSessionId,
        action,
        observation: "settled",
        previousProcessEpoch: input.expectedProcessEpoch,
        currentProcessEpoch: nextEpoch,
        nativeThreadId: this.#thread(),
        targets: [],
        message: `ZapMock ${action} settled`,
      }),
    });
  }
  #emit(
    kind: CoordinatorEvent["kind"],
    turn: string | null,
    item: string | null,
    data: unknown,
  ): void {
    if (this.#descriptor === null) return;
    this.#listener?.(
      CoordinatorEventSchema.parse({
        coordinatorSessionId: this.#descriptor.coordinatorSessionId,
        processEpoch: this.#epoch(),
        nativeThreadId: this.#thread(),
        nativeTurnId: turn,
        nativeItemId: item,
        kind,
        sourceEventId: `mock.event.${this.#descriptor.coordinatorSessionId}.${String(++this.#sequence)}`,
        data,
        ...(kind === "item_completed" && turn === null && this.#clientMessageId !== null
          ? {
              transportCorrelation: {
                provenance: "transport_correlation",
                clientMessageId: this.#clientMessageId,
                processEpoch: this.#epoch(),
              },
            }
          : {}),
      }),
    );
  }
  #thread(): string {
    return this.#descriptor?.nativeThreadRef.value ?? "mock.thread.pending";
  }
  #epoch(): string {
    return this.#descriptor?.processEpoch ?? "1";
  }
  #checkpoint(): void {
    if (this.#descriptor === null) return;
    this.#snapshots.set(this.#descriptor.coordinatorSessionId, {
      model: this.#model.snapshot(),
      pendingQuestionId: this.#pendingQuestionId,
      pendingQuestionGroupId: this.#pendingQuestionGroupId,
      pendingAcknowledgementId: this.#pendingAcknowledgementId,
    });
  }
}
