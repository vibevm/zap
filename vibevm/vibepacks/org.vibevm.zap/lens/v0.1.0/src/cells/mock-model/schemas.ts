/** Pure Zap mock-model contracts. @scope spec://org.vibevm.zap/lens/PROP-013#root */
import { z } from "zod";

export const ZAP_MOCK_MODEL_ID = "zap-mock/deterministic-v1" as const;

const IdSchema = z.string().min(1).max(160);
const TextSchema = z.string().max(32_000);
const InputKindSchema = z.enum([
  "start",
  "message",
  "tick",
  "open_gate",
  "answer",
  "ack",
  "report",
  "pause",
  "continue",
  "restart",
]);
const StepKindSchema = z.enum([
  "ready",
  "echo",
  "busy_gate",
  "question",
  "late_answer",
  "report",
  "pause_continue",
  "restart",
  "delivery_fault",
]);
const EffectKindSchema = z.enum([
  "session_ready",
  "turn_started",
  "assistant_message",
  "turn_settled",
  "busy",
  "question",
  "answer_received",
  "answer_acknowledged",
  "work_reported",
  "paused",
  "continued",
  "restarted",
  "delivery",
]);

export const ZapMockScenarioStepSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("ready") }).strict(),
  z.object({ kind: z.literal("echo"), prefix: z.string().max(1_000).default("") }).strict(),
  z
    .object({
      kind: z.literal("busy_gate"),
      gateId: IdSchema,
      completionText: TextSchema.default("gate opened"),
    })
    .strict(),
  z
    .object({
      kind: z.literal("question"),
      questionId: IdSchema,
      prompt: z.string().min(1).max(8_000),
      options: z.array(z.string().min(1).max(1_000)).min(1).max(32),
    })
    .strict(),
  z
    .object({
      kind: z.literal("late_answer"),
      questionId: IdSchema,
      acknowledgementId: IdSchema,
    })
    .strict(),
  z.object({ kind: z.literal("report"), reportId: IdSchema }).strict(),
  z.object({ kind: z.literal("pause_continue") }).strict(),
  z.object({ kind: z.literal("restart") }).strict(),
  z
    .object({
      kind: z.literal("delivery_fault"),
      deliveryId: IdSchema,
      fault: z.enum(["duplicate", "uncertain"]),
    })
    .strict(),
]);
export type ZapMockScenarioStep = z.infer<typeof ZapMockScenarioStepSchema>;

export const ZapMockScenarioSchema = z
  .object({
    scenarioId: IdSchema,
    steps: z.array(ZapMockScenarioStepSchema).min(1).max(256),
  })
  .strict()
  .superRefine((scenario, context) => {
    if (scenario.steps[0]?.kind !== "ready")
      context.addIssue({ code: "custom", path: ["steps", 0], message: "first step must be ready" });
    const gates = new Set<string>();
    const questions = new Set<string>();
    const acknowledgements = new Set<string>();
    const reports = new Set<string>();
    const deliveries = new Set<string>();
    for (const [index, step] of scenario.steps.entries()) {
      if (step.kind === "busy_gate") unique(step.gateId, gates, index, "gateId", context);
      if (step.kind === "question")
        unique(step.questionId, questions, index, "questionId", context);
      if (step.kind === "late_answer") {
        if (!questions.has(step.questionId))
          context.addIssue({
            code: "custom",
            path: ["steps", index, "questionId"],
            message: "late answer must reference an earlier question",
          });
        unique(step.acknowledgementId, acknowledgements, index, "acknowledgementId", context);
      }
      if (step.kind === "report") unique(step.reportId, reports, index, "reportId", context);
      if (step.kind === "delivery_fault")
        unique(step.deliveryId, deliveries, index, "deliveryId", context);
    }
  });
export type ZapMockScenario = z.infer<typeof ZapMockScenarioSchema>;

export const ZapMockInputSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("start"), inputId: IdSchema }).strict(),
  z
    .object({
      kind: z.literal("message"),
      inputId: IdSchema,
      messageId: IdSchema,
      text: TextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("tick"),
      inputId: IdSchema,
      count: z.number().int().min(1).max(1_000),
    })
    .strict(),
  z.object({ kind: z.literal("open_gate"), inputId: IdSchema, gateId: IdSchema }).strict(),
  z
    .object({
      kind: z.literal("answer"),
      inputId: IdSchema,
      questionId: IdSchema,
      answerVersion: IdSchema,
      text: TextSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("ack"),
      inputId: IdSchema,
      acknowledgementId: IdSchema,
      answerVersion: IdSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("report"),
      inputId: IdSchema,
      reportId: IdSchema,
      summary: TextSchema,
    })
    .strict(),
  z.object({ kind: z.literal("pause"), inputId: IdSchema }).strict(),
  z.object({ kind: z.literal("continue"), inputId: IdSchema }).strict(),
  z.object({ kind: z.literal("restart"), inputId: IdSchema }).strict(),
]);
export type ZapMockInput = z.infer<typeof ZapMockInputSchema>;

const EffectBaseSchema = z.object({ effectId: IdSchema, logicalTick: z.number().int().min(0) });
export const ZapMockEffectSchema = z.discriminatedUnion("kind", [
  EffectBaseSchema.extend({
    kind: z.literal("session_ready"),
    modelId: z.literal(ZAP_MOCK_MODEL_ID),
    provenance: z.literal("synthetic"),
  }).strict(),
  EffectBaseSchema.extend({ kind: z.literal("turn_started"), messageId: IdSchema }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("assistant_message"),
    messageId: IdSchema,
    text: TextSchema,
  }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("turn_settled"),
    reason: z.enum(["completed", "question", "gate"]),
  }).strict(),
  EffectBaseSchema.extend({ kind: z.literal("busy"), gateId: IdSchema }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("question"),
    questionId: IdSchema,
    prompt: z.string().min(1).max(8_000),
    options: z.array(z.string().min(1).max(1_000)).min(1).max(32),
  }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("answer_received"),
    questionId: IdSchema,
    answerVersion: IdSchema,
  }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("answer_acknowledged"),
    questionId: IdSchema,
    answerVersion: IdSchema,
    acknowledgementId: IdSchema,
  }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("work_reported"),
    reportId: IdSchema,
    summary: TextSchema,
  }).strict(),
  EffectBaseSchema.extend({ kind: z.literal("paused") }).strict(),
  EffectBaseSchema.extend({ kind: z.literal("continued") }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("restarted"),
    generation: z.number().int().min(1),
  }).strict(),
  EffectBaseSchema.extend({
    kind: z.literal("delivery"),
    deliveryId: IdSchema,
    observation: z.enum(["accepted", "duplicate", "uncertain", "input_duplicate_ignored"]),
  }).strict(),
]);
export type ZapMockEffect = z.infer<typeof ZapMockEffectSchema>;

export const ZapMockStateSchema = z
  .object({
    modelId: z.literal(ZAP_MOCK_MODEL_ID),
    seed: z.string().min(1).max(512),
    scenarioId: IdSchema,
    scenarioFingerprint: z.string().length(8),
    cursor: z.number().int().min(0),
    logicalTick: z.number().int().min(0),
    status: z.enum(["created", "idle", "busy", "paused", "complete"]),
    generation: z.number().int().min(1),
    effectSequence: z.number().int().min(0),
    traceSequence: z.number().int().min(0),
    rngState: z.number().int().min(1).max(4_294_967_295),
    activeGateId: IdSchema.nullable(),
    activeMessageId: IdSchema.nullable(),
    phase: z
      .enum(["none", "busy_started", "awaiting_answer", "awaiting_ack", "paused"])
      .default("none"),
    suspendedStatus: z.enum(["idle", "busy"]).nullable(),
    suspendedPhase: z.enum(["none", "busy_started", "awaiting_answer", "awaiting_ack"]).nullable(),
    pendingQuestionId: IdSchema.nullable(),
    pendingAnswerVersion: IdSchema.nullable(),
    seenInputIds: z.array(IdSchema).max(10_000),
  })
  .strict();
export type ZapMockState = z.infer<typeof ZapMockStateSchema>;

export const ZapMockTraceEntrySchema = z
  .object({
    sequence: z.number().int().min(1),
    logicalTick: z.number().int().min(0),
    inputId: IdSchema,
    inputFingerprint: z.string().length(8),
    inputKind: InputKindSchema,
    stepKind: StepKindSchema,
    cursorBefore: z.number().int().min(0),
    cursorAfter: z.number().int().min(0),
    status: ZapMockStateSchema.shape.status,
    effectKinds: z.array(EffectKindSchema).max(32),
  })
  .strict();
export type ZapMockTraceEntry = z.infer<typeof ZapMockTraceEntrySchema>;

export const ZapMockSnapshotSchema = z
  .object({
    schemaVersion: z.literal("zap-mock-model.snapshot.v1"),
    scenario: ZapMockScenarioSchema,
    state: ZapMockStateSchema,
    trace: z.array(ZapMockTraceEntrySchema).max(10_000),
  })
  .strict();
export type ZapMockSnapshot = z.infer<typeof ZapMockSnapshotSchema>;

function unique(
  value: string,
  seen: Set<string>,
  index: number,
  key: string,
  context: z.RefinementCtx,
): void {
  if (seen.has(value))
    context.addIssue({
      code: "custom",
      path: ["steps", index, key],
      message: `${key} must be unique`,
    });
  seen.add(value);
}
