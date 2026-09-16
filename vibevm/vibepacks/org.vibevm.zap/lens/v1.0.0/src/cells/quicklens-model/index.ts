/**
 * Platform-safe semantic model and injected data-source port for Quicklens.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shared-client
 * @example
 * const result = await source.read({ signal: controller.signal });
 * if (result.ok) renderSnapshot(result.value);
 */
import { z } from "zod";

export const QuicklensRefSchema = z.string().min(1).max(1_152).brand<"QuicklensRef">();
export type QuicklensRef = z.infer<typeof QuicklensRefSchema>;

export const ExactDecimalSchema = z
  .string()
  .regex(/^(0|[1-9][0-9]*)$/)
  .brand<"ExactDecimal">();
export type ExactDecimal = z.infer<typeof ExactDecimalSchema>;

export const SourceModeSchema = z.enum(["live", "demo"]);
export type SourceMode = z.infer<typeof SourceModeSchema>;

export const ViewPhaseSchema = z.enum(["ready", "partial", "stale"]);
export type ViewPhase = z.infer<typeof ViewPhaseSchema>;

export const ObjectCategorySchema = z.enum(["task", "milestone", "resource", "other"]);
export type ObjectCategory = z.infer<typeof ObjectCategorySchema>;

export const StatusToneSchema = z.enum(["neutral", "active", "success", "warning", "danger"]);
export type StatusTone = z.infer<typeof StatusToneSchema>;

export const SemanticStatusSchema = z
  .object({
    code: z.string().min(1).max(160),
    label: z.string().min(1).max(160),
    tone: StatusToneSchema,
  })
  .strict();
export type SemanticStatus = z.infer<typeof SemanticStatusSchema>;

export const ProvenanceSchema = z
  .object({
    sourceType: z.string().min(1).max(160),
    label: z.string().min(1).max(512),
    detail: z.string().max(2_000).nullable(),
  })
  .strict();
export type Provenance = z.infer<typeof ProvenanceSchema>;

export const MetricSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("unknown"), reason: z.string().min(1).max(512) }).strict(),
  z
    .object({
      state: z.literal("known"),
      value: z.string().min(1).max(160),
      unit: z.string().min(1).max(80).nullable(),
      explanation: z.string().max(4_096).nullable(),
    })
    .strict(),
]);
export type Metric = z.infer<typeof MetricSchema>;

export const PlanningMetricsSchema = z
  .object({
    complexity: MetricSchema,
    difficulty: MetricSchema,
    effort: MetricSchema,
    waiting: MetricSchema,
    uncertainty: MetricSchema,
  })
  .strict();
export type PlanningMetrics = z.infer<typeof PlanningMetricsSchema>;

export const SemanticObjectSchema = z
  .object({
    ref: QuicklensRefSchema,
    category: ObjectCategorySchema,
    semanticType: z.string().min(1).max(160),
    title: z.string().min(1).max(4_096),
    description: z.string().max(16_384).nullable().default(null),
    purpose: z.string().max(16_384).nullable(),
    expectedResult: z.string().max(16_384).nullable().default(null),
    acceptance: z.string().max(16_384).nullable(),
    reasons: z.array(z.string().min(1).max(4_096)).max(100).default([]),
    blockers: z
      .array(
        z.object({ ref: QuicklensRefSchema, fallbackLabel: z.string().min(1).max(256) }).strict(),
      )
      .max(100)
      .default([]),
    blockerSummary: z.string().min(1).max(4_096).nullable().default(null),
    status: SemanticStatusSchema,
    metrics: PlanningMetricsSchema,
    provenance: z.array(ProvenanceSchema).max(100),
    position: z.object({ x: z.number(), y: z.number() }).strict().nullable(),
  })
  .strict();
export type SemanticObject = z.infer<typeof SemanticObjectSchema>;

export const SemanticRelationshipSchema = z
  .object({
    ref: QuicklensRefSchema,
    source: QuicklensRefSchema,
    target: QuicklensRefSchema,
    semanticType: z.string().min(1).max(160),
    label: z.string().min(1).max(256),
    status: SemanticStatusSchema.nullable().default(null),
    provenance: z.array(ProvenanceSchema).max(20),
  })
  .strict();
export type SemanticRelationship = z.infer<typeof SemanticRelationshipSchema>;

export const GraphPlanMemberSchema = z
  .object({
    ref: QuicklensRefSchema,
    revisionRef: QuicklensRefSchema,
    isCurrent: z.boolean(),
  })
  .strict();
export type GraphPlanMember = z.infer<typeof GraphPlanMemberSchema>;

export const GraphNavigationSchema = z
  .object({
    activeOutcomeRef: QuicklensRefSchema.nullable(),
    currentStrategyRef: QuicklensRefSchema.nullable(),
    adoptedPlanRef: QuicklensRefSchema.nullable(),
    members: z.array(GraphPlanMemberSchema).max(1_000),
    focusRef: QuicklensRefSchema.nullable(),
    currentWorkRefs: z.array(QuicklensRefSchema).max(1_000),
    dependencyDirection: z.literal("prerequisite_to_dependent"),
    completeness: z.enum(["complete", "partial"]),
    reassessmentReason: z.string().min(1).max(2_000).nullable(),
  })
  .strict();
export type GraphNavigation = z.infer<typeof GraphNavigationSchema>;

export const QuestionViewSchema = z
  .object({
    ref: QuicklensRefSchema,
    addressedActorLabel: z.string().min(1).max(256),
    prompt: z.string().min(1).max(16_384),
    state: z.enum(["pending", "answered", "cancelled", "expired"]),
    revision: ExactDecimalSchema,
    answerMode: z.enum(["free_text", "single_choice"]),
    choices: z.array(z.object({ id: z.string(), label: z.string() }).strict()).max(100),
    answer: z.string().max(16_384).nullable(),
    amendmentCount: ExactDecimalSchema,
  })
  .strict();
export type QuestionView = z.infer<typeof QuestionViewSchema>;

export const AgentTargetViewSchema = z
  .object({
    ref: QuicklensRefSchema,
    label: z.string().min(1).max(256),
    host: z.string().min(1).max(160),
    parentRef: QuicklensRefSchema.nullable(),
    state: z.enum(["active", "busy", "offline"]),
    eligible: z.boolean(),
    reason: z.string().min(1).max(1_000).nullable(),
  })
  .strict();
export type AgentTargetView = z.infer<typeof AgentTargetViewSchema>;

export const PlanBasisSchema = z
  .object({
    storeRef: QuicklensRefSchema,
    baseRef: QuicklensRefSchema,
    revision: ExactDecimalSchema,
    sourceBasisRef: QuicklensRefSchema,
  })
  .strict();
export type PlanBasis = z.infer<typeof PlanBasisSchema>;

export const ActionAvailabilitySchema = z
  .object({
    enabled: z.boolean(),
    reason: z.string().min(1).max(1_000).nullable(),
  })
  .strict();
export type ActionAvailability = z.infer<typeof ActionAvailabilitySchema>;

export const PlanStatusSchema = z
  .object({
    outcomeLabel: z.string().min(1).max(512),
    strategyLabel: z.string().min(1).max(512),
    planLabel: z.string().min(1).max(512),
    basis: PlanBasisSchema,
    state: z.enum(["current", "reassessment", "held", "uncertain"]),
    detail: z.string().max(4_000).nullable(),
    decision: z
      .object({ operationRef: QuicklensRefSchema, holdRef: QuicklensRefSchema })
      .strict()
      .nullable()
      .default(null),
    actions: z
      .object({
        propose: ActionAvailabilitySchema,
        preview: ActionAvailabilitySchema,
        apply: ActionAvailabilitySchema,
        reconcile: ActionAvailabilitySchema,
        decide: ActionAvailabilitySchema.default({
          enabled: false,
          reason: "No held owner decision is available.",
        }),
      })
      .strict(),
  })
  .strict()
  .superRefine((value, context) => {
    const validDecision =
      value.state === "held"
        ? !value.actions.decide.enabled || value.decision !== null
        : value.decision === null && !value.actions.decide.enabled;
    if (!validDecision) {
      context.addIssue({
        code: "custom",
        message:
          "violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: owner decision identity or availability escaped held state; fix surface: expose the decision binding only for an exact held operation",
      });
    }
  });
export type PlanStatus = z.infer<typeof PlanStatusSchema>;

export const QuicklensSnapshotSchema = z
  .object({
    sourceMode: SourceModeSchema,
    sourceLabel: z.string().min(1).max(512),
    phase: ViewPhaseSchema,
    phaseDetail: z.string().max(2_000).nullable(),
    capturedAt: z.iso.datetime(),
    revision: ExactDecimalSchema,
    objects: z.array(SemanticObjectSchema),
    relationships: z.array(SemanticRelationshipSchema),
    navigation: GraphNavigationSchema.optional(),
    questions: z.array(QuestionViewSchema),
    questionAnswer: ActionAvailabilitySchema.default({ enabled: true, reason: null }),
    agentTargets: z.array(AgentTargetViewSchema),
    plan: PlanStatusSchema.nullable(),
  })
  .strict();
export type QuicklensSnapshot = z.infer<typeof QuicklensSnapshotSchema>;

export const QuicklensErrorSchema = z
  .object({
    code: z.enum([
      "unavailable",
      "invalid_data",
      "stale_basis",
      "forbidden",
      "unsupported",
      "uncertain",
      "cancelled",
    ]),
    message: z.string().min(1).max(2_000),
    recovery: z.string().min(1).max(2_000),
  })
  .strict();
export type QuicklensError = z.infer<typeof QuicklensErrorSchema>;

export type QuicklensResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: QuicklensError };

export interface ReadSnapshotInput {
  readonly signal: AbortSignal;
}

export interface AnswerQuestionInput {
  readonly questionRef: QuicklensRef;
  readonly expectedRevision: ExactDecimal;
  readonly answer: string;
}

export interface PlanIntentInput {
  readonly text: string;
  readonly basis: PlanBasis;
  readonly targetActorRef: QuicklensRef;
}

export interface PlanPreviewInput {
  readonly intentRef: QuicklensRef;
  readonly basis: PlanBasis;
}

export interface PlanApplyInput {
  readonly operationRef: QuicklensRef;
  readonly previewRef: QuicklensRef;
  readonly basis: PlanBasis;
}

export interface PlanReconcileInput {
  readonly operationRef: QuicklensRef;
  readonly basis: PlanBasis;
}

export const PlanDecisionInputSchema = z
  .object({
    operationRef: QuicklensRefSchema,
    holdRef: QuicklensRefSchema,
    basis: PlanBasisSchema,
    choice: z.enum(["approve", "reject", "revise", "defer"]),
    reason: z.string().min(1).max(4_096),
  })
  .strict();
export type PlanDecisionInput = z.infer<typeof PlanDecisionInputSchema>;

export const PlanChangeSummarySchema = z
  .object({
    subjectLabel: z.string().min(1).max(512),
    changeKind: z.string().min(1).max(160),
    beforeSummary: z.string().min(1).max(2_000),
    afterSummary: z.string().min(1).max(2_000),
  })
  .strict();
export type PlanChangeSummary = z.infer<typeof PlanChangeSummarySchema>;

export const PlanPreviewViewSchema = z
  .object({
    basis: PlanBasisSchema,
    changes: z.array(PlanChangeSummarySchema).min(1).max(200),
  })
  .strict();
export type PlanPreviewView = z.infer<typeof PlanPreviewViewSchema>;

export const PlanOperationResultSchema = z
  .object({
    operationRef: QuicklensRefSchema,
    previewRef: QuicklensRefSchema.nullable(),
    preview: PlanPreviewViewSchema.nullable(),
    state: z.enum([
      "queued",
      "requested",
      "prepared",
      "admitted",
      "completed",
      "held",
      "rejected",
      "uncertain",
    ]),
    message: z.string().min(1).max(2_000),
    nextBasis: PlanBasisSchema.nullable(),
  })
  .strict()
  .superRefine((value, context) => {
    const prepared = value.state === "prepared";
    if (prepared !== (value.previewRef !== null && value.preview !== null)) {
      context.addIssue({
        code: "custom",
        message:
          "violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: prepared preview identity and summary must appear together; fix surface: return previewRef plus exact-basis changes only for the prepared state",
      });
    }
  });
export type PlanOperationResult = z.infer<typeof PlanOperationResultSchema>;

export type InvalidationReason = "events" | "questions" | "plan" | "reconnect";
export type Unsubscribe = () => void;

/** Injected browser contract. Capability decisions come from this source, never the renderer. */
export interface QuicklensDataSource {
  read(input: ReadSnapshotInput): Promise<QuicklensResult<QuicklensSnapshot>>;
  subscribe?(listener: (reason: InvalidationReason) => void): Unsubscribe;
  answerQuestion(input: AnswerQuestionInput): Promise<QuicklensResult<QuestionView>>;
  proposePlanIntent(input: PlanIntentInput): Promise<QuicklensResult<PlanOperationResult>>;
  previewPlan(input: PlanPreviewInput): Promise<QuicklensResult<PlanOperationResult>>;
  applyPlan(input: PlanApplyInput): Promise<QuicklensResult<PlanOperationResult>>;
  reconcilePlan(input: PlanReconcileInput): Promise<QuicklensResult<PlanOperationResult>>;
  decidePlan(input: PlanDecisionInput): Promise<QuicklensResult<PlanOperationResult>>;
}

export function unavailableDataSource(reason: string): QuicklensDataSource {
  const unavailable = <T>(): Promise<QuicklensResult<T>> =>
    Promise.resolve({
      ok: false,
      error: {
        code: "unavailable",
        message: reason,
        recovery: "Connect Quicklens to the configured lens and ZAP adapters, then refresh.",
      },
    });
  return {
    read: unavailable,
    answerQuestion: unavailable,
    proposePlanIntent: unavailable,
    previewPlan: unavailable,
    applyPlan: unavailable,
    reconcilePlan: unavailable,
    decidePlan: unavailable,
  };
}
