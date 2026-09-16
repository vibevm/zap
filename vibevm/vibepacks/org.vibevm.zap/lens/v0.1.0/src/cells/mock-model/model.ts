/** Deterministic Zap mock-model reducer. @scope spec://org.vibevm.zap/lens/PROP-013#root */
import { z } from "zod";
import {
  ZAP_MOCK_MODEL_ID,
  ZapMockEffectSchema,
  ZapMockInputSchema,
  ZapMockScenarioSchema,
  ZapMockSnapshotSchema,
  ZapMockStateSchema,
  ZapMockTraceEntrySchema,
  type ZapMockEffect,
  type ZapMockInput,
  type ZapMockScenario,
  type ZapMockSnapshot,
  type ZapMockState,
  type ZapMockTraceEntry,
} from "./schemas.ts";

export type ZapMockModelErrorCode =
  | "invalid_configuration"
  | "invalid_input"
  | "snapshot_mismatch"
  | "unexpected_input";
export type ZapMockModelResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: { readonly code: ZapMockModelErrorCode; readonly message: string };
    };

export interface ZapMockTransition {
  readonly state: ZapMockState;
  readonly effects: readonly ZapMockEffect[];
  readonly trace: ZapMockTraceEntry;
}

export interface ZapMockModel {
  dispatch(input: unknown): ZapMockModelResult<ZapMockTransition>;
  state(): ZapMockState;
  trace(): readonly ZapMockTraceEntry[];
  snapshot(): ZapMockSnapshot;
}

const CreateInputSchema = z
  .object({
    seed: z.string().min(1).max(512),
    scenario: ZapMockScenarioSchema,
    snapshot: ZapMockSnapshotSchema.optional(),
  })
  .strict();

export function createZapMockModel(input: unknown): ZapMockModelResult<ZapMockModel> {
  const parsed = CreateInputSchema.safeParse(input);
  if (!parsed.success)
    return failure("invalid_configuration", "mock model configuration is invalid");
  const scenario = parsed.data.scenario;
  const fingerprint = scenarioFingerprint(scenario);
  const restored = parsed.data.snapshot;
  if (
    restored !== undefined &&
    (restored.state.seed !== parsed.data.seed ||
      restored.state.scenarioFingerprint !== fingerprint ||
      scenarioFingerprint(restored.scenario) !== fingerprint ||
      JSON.stringify(restored.scenario) !== JSON.stringify(scenario))
  )
    return failure("snapshot_mismatch", "mock snapshot does not match seed and scenario");
  let current =
    restored?.state ??
    ZapMockStateSchema.parse({
      modelId: ZAP_MOCK_MODEL_ID,
      seed: parsed.data.seed,
      scenarioId: scenario.scenarioId,
      scenarioFingerprint: fingerprint,
      cursor: 0,
      logicalTick: 0,
      status: "created",
      generation: 1,
      effectSequence: 0,
      traceSequence: 0,
      rngState: seedState(parsed.data.seed),
      activeGateId: null,
      activeMessageId: null,
      phase: "none",
      suspendedStatus: null,
      suspendedPhase: null,
      pendingQuestionId: null,
      pendingAnswerVersion: null,
      seenInputIds: [],
    });
  const trace = restored === undefined ? [] : [...restored.trace];
  return {
    ok: true,
    value: {
      dispatch(raw) {
        const checked = ZapMockInputSchema.safeParse(raw);
        if (!checked.success) return failure("invalid_input", "mock model input is invalid");
        if (
          trace.length >= 10_000 ||
          (!current.seenInputIds.includes(checked.data.inputId) &&
            current.seenInputIds.length >= 10_000)
        )
          return failure("unexpected_input", "mock model bounded history is full");
        const reduced = reduce(scenario, current, checked.data);
        if (!reduced.ok) return reduced;
        current = reduced.value.state;
        trace.push(reduced.value.trace);
        return reduced;
      },
      state: () => ZapMockStateSchema.parse(current),
      trace: () => trace.map((entry) => ZapMockTraceEntrySchema.parse(entry)),
      snapshot: () =>
        ZapMockSnapshotSchema.parse({
          schemaVersion: "zap-mock-model.snapshot.v1",
          scenario,
          state: current,
          trace,
        }),
    },
  };
}

function reduce(
  scenario: ZapMockScenario,
  previous: ZapMockState,
  input: ZapMockInput,
): ZapMockModelResult<ZapMockTransition> {
  const step = scenario.steps[previous.cursor];
  if (step === undefined) return failure("unexpected_input", "mock scenario is already complete");
  const next = ZapMockStateSchema.parse({
    ...previous,
    logicalTick: previous.logicalTick + (input.kind === "tick" ? input.count : 1),
    traceSequence: previous.traceSequence + 1,
    seenInputIds: previous.seenInputIds.includes(input.inputId)
      ? previous.seenInputIds
      : [...previous.seenInputIds, input.inputId],
  });
  const effects: ZapMockEffect[] = [];
  if (previous.seenInputIds.includes(input.inputId)) {
    effects.push(
      effect(next, {
        kind: "delivery",
        deliveryId: input.inputId,
        observation: "input_duplicate_ignored",
      }),
    );
    return success(previous, next, input, step.kind, effects);
  }
  if (step.kind !== "pause_continue") {
    const lifecycle = globalPause(input, next, effects);
    if (lifecycle?.handled === true) return success(previous, next, input, step.kind, effects);
    if (lifecycle !== null) return failure("unexpected_input", lifecycle.message);
  }
  const mismatch = applyStep(step, input, next, effects, scenario.steps.length);
  return mismatch === null
    ? success(previous, next, input, step.kind, effects)
    : failure("unexpected_input", mismatch);
}

function applyStep(
  step: ZapMockScenario["steps"][number],
  input: ZapMockInput,
  state: ZapMockState,
  effects: ZapMockEffect[],
  stepCount: number,
): string | null {
  if (step.kind === "ready") {
    if (input.kind !== "start") return "ready step requires start input";
    effects.push(
      effect(state, { kind: "session_ready", modelId: ZAP_MOCK_MODEL_ID, provenance: "synthetic" }),
    );
    advance(state, stepCount, "idle");
    return null;
  }
  if (step.kind === "echo") {
    if (input.kind !== "message") return "echo step requires message input";
    effects.push(effect(state, { kind: "turn_started", messageId: input.messageId }));
    effects.push(
      effect(state, {
        kind: "assistant_message",
        messageId: input.messageId,
        text: `${step.prefix}${input.text}`,
      }),
    );
    effects.push(effect(state, { kind: "turn_settled", reason: "completed" }));
    advance(state, stepCount, "idle");
    return null;
  }
  if (step.kind === "busy_gate") return busyGate(step, input, state, effects, stepCount);
  if (step.kind === "question") {
    if (input.kind !== "message") return "question step requires message input";
    effects.push(effect(state, { kind: "turn_started", messageId: input.messageId }));
    effects.push(
      effect(state, {
        kind: "question",
        questionId: step.questionId,
        prompt: step.prompt,
        options: step.options,
      }),
    );
    effects.push(effect(state, { kind: "turn_settled", reason: "question" }));
    state.pendingQuestionId = step.questionId;
    state.phase = "awaiting_answer";
    advanceCursor(state, stepCount);
    return null;
  }
  if (step.kind === "late_answer") return lateAnswer(step, input, state, effects, stepCount);
  if (step.kind === "report") {
    if (input.kind !== "report" || input.reportId !== step.reportId)
      return "report step requires its declared report input";
    effects.push(
      effect(state, { kind: "work_reported", reportId: input.reportId, summary: input.summary }),
    );
    advance(state, stepCount, "idle");
    return null;
  }
  if (step.kind === "pause_continue") return pauseContinue(input, state, effects, stepCount);
  if (step.kind === "restart") {
    if (input.kind !== "restart") return "restart step requires restart input";
    state.generation += 1;
    effects.push(effect(state, { kind: "restarted", generation: state.generation }));
    advance(state, stepCount, "idle");
    return null;
  }
  if (input.kind !== "message") return "delivery fault step requires message input";
  effects.push(effect(state, { kind: "turn_started", messageId: input.messageId }));
  if (step.fault === "duplicate") {
    effects.push(
      effect(state, { kind: "delivery", deliveryId: step.deliveryId, observation: "accepted" }),
    );
    effects.push(
      effect(state, { kind: "delivery", deliveryId: step.deliveryId, observation: "duplicate" }),
    );
  } else {
    effects.push(
      effect(state, { kind: "delivery", deliveryId: step.deliveryId, observation: "uncertain" }),
    );
  }
  advance(state, stepCount, "idle");
  return null;
}

function busyGate(
  step: Extract<ZapMockScenario["steps"][number], { kind: "busy_gate" }>,
  input: ZapMockInput,
  state: ZapMockState,
  effects: ZapMockEffect[],
  stepCount: number,
): string | null {
  if (state.phase !== "busy_started") {
    if (input.kind !== "message") return "busy gate requires an initial message";
    state.status = "busy";
    state.phase = "busy_started";
    state.activeGateId = step.gateId;
    state.activeMessageId = input.messageId;
    effects.push(effect(state, { kind: "turn_started", messageId: input.messageId }));
    effects.push(effect(state, { kind: "busy", gateId: step.gateId }));
    return null;
  }
  if (input.kind === "tick") return null;
  if (input.kind !== "open_gate" || input.gateId !== step.gateId)
    return "busy gate requires its declared open_gate input";
  const messageId = state.activeMessageId;
  if (messageId === null) return "busy gate lost its active message";
  effects.push(effect(state, { kind: "assistant_message", messageId, text: step.completionText }));
  effects.push(effect(state, { kind: "turn_settled", reason: "gate" }));
  state.activeGateId = null;
  state.activeMessageId = null;
  state.phase = "none";
  advance(state, stepCount, "idle");
  return null;
}

function lateAnswer(
  step: Extract<ZapMockScenario["steps"][number], { kind: "late_answer" }>,
  input: ZapMockInput,
  state: ZapMockState,
  effects: ZapMockEffect[],
  stepCount: number,
): string | null {
  if (state.phase === "awaiting_answer") {
    if (input.kind !== "answer" || input.questionId !== step.questionId)
      return "late answer step requires its question answer";
    state.pendingAnswerVersion = input.answerVersion;
    state.phase = "awaiting_ack";
    effects.push(
      effect(state, {
        kind: "answer_received",
        questionId: input.questionId,
        answerVersion: input.answerVersion,
      }),
    );
    return null;
  }
  if (
    state.phase !== "awaiting_ack" ||
    input.kind !== "ack" ||
    input.acknowledgementId !== step.acknowledgementId ||
    input.answerVersion !== state.pendingAnswerVersion
  )
    return "late answer step requires exact acknowledgement";
  effects.push(
    effect(state, {
      kind: "answer_acknowledged",
      questionId: step.questionId,
      answerVersion: input.answerVersion,
      acknowledgementId: input.acknowledgementId,
    }),
  );
  state.pendingQuestionId = null;
  state.pendingAnswerVersion = null;
  state.phase = "none";
  advance(state, stepCount, "idle");
  return null;
}

function pauseContinue(
  input: ZapMockInput,
  state: ZapMockState,
  effects: ZapMockEffect[],
  stepCount: number,
): string | null {
  if (state.phase !== "paused") {
    if (input.kind !== "pause") return "pause/continue step requires pause input";
    state.status = "paused";
    state.suspendedStatus = "idle";
    state.suspendedPhase = "none";
    state.phase = "paused";
    effects.push(effect(state, { kind: "paused" }));
    return null;
  }
  if (input.kind !== "continue") return "paused step requires continue input";
  state.phase = "none";
  state.suspendedStatus = null;
  state.suspendedPhase = null;
  effects.push(effect(state, { kind: "continued" }));
  advance(state, stepCount, "idle");
  return null;
}

function globalPause(
  input: ZapMockInput,
  state: ZapMockState,
  effects: ZapMockEffect[],
): { readonly handled: true } | { readonly handled: false; readonly message: string } | null {
  if (input.kind === "pause") {
    if (state.status !== "idle" && state.status !== "busy")
      return { handled: false, message: "pause requires an active idle or busy session" };
    state.suspendedStatus = state.status;
    state.suspendedPhase = state.phase === "paused" ? null : state.phase;
    state.status = "paused";
    state.phase = "paused";
    effects.push(effect(state, { kind: "paused" }));
    return { handled: true };
  }
  if (input.kind === "continue") {
    if (
      state.status !== "paused" ||
      state.suspendedStatus === null ||
      state.suspendedPhase === null
    )
      return { handled: false, message: "continue requires a paused session" };
    state.status = state.suspendedStatus;
    state.phase = state.suspendedPhase;
    state.suspendedStatus = null;
    state.suspendedPhase = null;
    effects.push(effect(state, { kind: "continued" }));
    return { handled: true };
  }
  if (state.status === "paused")
    return { handled: false, message: "paused session requires continue input" };
  return null;
}

function effect(state: ZapMockState, payload: Record<string, unknown>): ZapMockEffect {
  state.effectSequence += 1;
  state.rngState = nextRandom(state.rngState);
  return ZapMockEffectSchema.parse({
    effectId: `mock.${String(state.effectSequence)}.${state.rngState.toString(16).padStart(8, "0")}`,
    logicalTick: state.logicalTick,
    ...payload,
  });
}

function success(
  previous: ZapMockState,
  next: ZapMockState,
  input: ZapMockInput,
  stepKind: ZapMockScenario["steps"][number]["kind"],
  effects: readonly ZapMockEffect[],
): ZapMockModelResult<ZapMockTransition> {
  const trace = ZapMockTraceEntrySchema.parse({
    sequence: next.traceSequence,
    logicalTick: next.logicalTick,
    inputId: input.inputId,
    inputFingerprint: fingerprint(input),
    inputKind: input.kind,
    stepKind,
    cursorBefore: previous.cursor,
    cursorAfter: next.cursor,
    status: next.status,
    effectKinds: effects.map((item) => item.kind),
  });
  return { ok: true, value: { state: ZapMockStateSchema.parse(next), effects, trace } };
}

function advance(state: ZapMockState, stepCount: number, status: "idle"): void {
  state.status = status;
  state.phase = "none";
  advanceCursor(state, stepCount);
}

function advanceCursor(state: ZapMockState, stepCount: number): void {
  state.cursor += 1;
  if (state.cursor >= stepCount) state.status = "complete";
}

function scenarioFingerprint(scenario: ZapMockScenario): string {
  return fingerprint(scenario);
}

function fingerprint(value: unknown): string {
  let hash = 2_166_136_261;
  for (const character of JSON.stringify(value)) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16_777_619) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function seedState(seed: string): number {
  const parsed = Number.parseInt(fingerprint({ seed }), 16);
  return parsed === 0 ? 1 : parsed;
}

function nextRandom(value: number): number {
  let next = value >>> 0;
  next ^= next << 13;
  next ^= next >>> 17;
  next ^= next << 5;
  next >>>= 0;
  return next === 0 ? 1 : next;
}

function failure(code: ZapMockModelErrorCode, message: string): ZapMockModelResult<never> {
  return { ok: false, error: { code, message } };
}
