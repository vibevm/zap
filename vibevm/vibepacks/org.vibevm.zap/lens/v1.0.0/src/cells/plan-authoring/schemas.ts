/** Runtime contracts for trusted authoring inputs and durable outputs. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#metadata */
import { z } from "zod";
import { PlanBasisSchema } from "../quicklens-model/index.ts";
import {
  AdvanceChangeAdmissionInputSchema,
  PreparedCompositeSuccessorSchema,
  StoredPreparedComparisonSchema,
  U32WireSchema,
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
} from "../zap-client/index.ts";
import { encodeCanonicalJson } from "../zap-client/index.ts";
import type { CanonicalJsonInput } from "../zap-client/index.ts";

const U64InputSchema = z
  .union([U64WireSchema, z.bigint(), z.string().regex(/^(0|[1-9][0-9]*)$/)])
  .transform((value) => BigInt(value))
  .pipe(z.bigint().min(0n).max(18_446_744_073_709_551_615n));
const StoredU64Schema = z.union([U64WireSchema, z.string().regex(/^(0|[1-9][0-9]*)$/)]);
const U32InputSchema = z
  .union([U32WireSchema, z.number().int(), z.string().regex(/^(0|[1-9][0-9]*)$/)])
  .transform((value) => Number(value))
  .pipe(z.number().int().min(0).max(4_294_967_295));
const CanonicalSchema = z.custom<CanonicalJsonInput>((value) => {
  try {
    encodeCanonicalJson(value);
    return true;
  } catch {
    return false;
  }
});
const IdListSchema = z.array(ZapIdSchema).refine((values) =>
  values.every((value, index) => {
    const prior = values[index - 1];
    return prior === undefined || prior < value;
  }),
);
const SubjectSchema = z.object({ kind: z.string().min(1), id: ZapIdSchema }).strict();
const SubjectListSchema = z.array(SubjectSchema).refine((values) =>
  values.every((value, index) => {
    const prior = values[index - 1];
    return prior === undefined || compareSubject(prior, value) < 0;
  }),
);
const SUBJECT_KINDS = [
  "campaign",
  "intent",
  "outcome",
  "obligation",
  "work",
  "contract",
  "source",
  "evidence",
  "decision",
  "review",
  "deferral",
  "lowering",
  "dream",
  "job",
  "verification",
  "hold",
  "pause",
  "effect",
  "resource",
];

function compareSubject(left: { kind: string; id: string }, right: { kind: string; id: string }) {
  const leftRank = SUBJECT_KINDS.indexOf(left.kind);
  const rightRank = SUBJECT_KINDS.indexOf(right.kind);
  return leftRank === rightRank ? left.id.localeCompare(right.id) : leftRank - rightRank;
}

const PlanKeySchema = z.object({ outcome_id: ZapIdSchema, generation: U64InputSchema }).strict();
const SourceCaptureSchema = z.object({ source_id: ZapIdSchema, digest: ZapDigestSchema }).strict();
const EvidenceListSchema = IdListSchema;
const UtilitySchema = z
  .object({
    overall: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    owner_benefit: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    risk_reduction: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    urgency: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    strategic_optionality: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    reversibility: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    confidence: z.enum(["unknown", "low", "moderate", "high"]),
    basis: z.string().min(1).max(4_096),
    evidence_refs: EvidenceListSchema,
  })
  .strict();
const HoursIntervalSchema = z
  .object({ low: U64InputSchema, high: U64InputSchema.nullable() })
  .strict();
const CostSchema = z
  .object({
    expected_elapsed: U64InputSchema.nullable(),
    elapsed_interval: HoursIntervalSchema,
    expected_passive_wait: U64InputSchema.nullable(),
    passive_wait_interval: HoursIntervalSchema,
    total_agent_hours: U64InputSchema.nullable(),
    agent_hours_interval: HoursIntervalSchema,
    precision: z.enum(["measured", "bounded_estimate", "order_of_magnitude"]),
    consequence: z.enum(["negligible", "low", "moderate", "high", "critical"]),
    categories: z.array(
      z
        .object({
          category: z.enum([
            "implementation",
            "verification",
            "migration",
            "documentation",
            "proof_revalidation",
            "dependencies_consumers",
            "operations_maintenance",
            "passive_wait_external",
            "fog_uncertainty",
          ]),
          applicability: z.enum(["included", "not_applicable", "unknown"]),
          agent_hours: HoursIntervalSchema,
          elapsed: HoursIntervalSchema,
          consequence: z.enum(["negligible", "low", "moderate", "high", "critical"]),
          basis: z.string().min(1).max(4_096),
          evidence_refs: EvidenceListSchema,
        })
        .strict(),
    ),
    unknowns: z.array(CanonicalSchema),
    excluded_costs: z.array(CanonicalSchema),
    attribution_summary: z.string().min(1).max(4_096),
  })
  .strict();
const NecessitySchema = z
  .object({
    class: z.enum(["mandatory_problem", "obligatory_safeguard", "optional_improvement"]),
    obligation_ids: IdListSchema,
    constraint_refs: z.array(z.string().startsWith("spec://").includes("#")),
    problem: z.string().min(1).max(4_096),
    basis: z.string().min(1).max(4_096),
    evidence_refs: EvidenceListSchema,
  })
  .strict();
const TeamModelSchema = z
  .object({
    model_id: z.string().min(1).max(256),
    profile_digest: ZapDigestSchema,
    executor_classes: z.array(
      z
        .object({
          class_id: z.string().min(1).max(256),
          capability_ids: z.array(z.string().min(1).max(256)),
          nominal_capacity: U32InputSchema.pipe(z.number().min(1)),
        })
        .strict(),
    ),
    nominal_parallelism: U32InputSchema.pipe(z.number().min(1)),
    resource_capacities: z.array(
      z
        .object({
          resource_id: ZapIdSchema,
          nominal_capacity: U32InputSchema.pipe(z.number().min(1)),
        })
        .strict(),
    ),
    scheduling_assumptions: z.array(z.string().min(1).max(4_096)),
    evidence_refs: EvidenceListSchema,
  })
  .strict();
const EstimationSchema = z
  .object({
    elapsed: U64InputSchema,
    agent_hours: U64InputSchema,
    stopped_because: z.enum(["sufficient", "budget_reached", "evidence_unavailable"]),
    assumptions: z.array(z.string().min(1).max(4_096)),
    evidence_refs: EvidenceListSchema,
  })
  .strict();
const PlanContentSchema = z
  .object({
    milestone_revision_ids: IdListSchema.min(1),
    admission_work_ids: IdListSchema,
    focus_milestone_revision_id: ZapIdSchema.nullable(),
    frontier_milestone_revision_ids: IdListSchema,
    horizons: z.array(CanonicalSchema),
    obligation_coverage: z.array(
      z
        .object({ obligation_id: ZapIdSchema, milestone_revision_ids: IdListSchema.min(1) })
        .strict(),
    ),
    rationales: z.array(
      z
        .object({
          milestone_revision_id: ZapIdSchema,
          kind: z.enum([
            "consumer_outcome",
            "decision_boundary",
            "independently_provable_capability",
          ]),
          sources: z.array(SourceCaptureSchema).min(1),
          explanation: z.string().min(1).max(4_096),
        })
        .strict(),
    ),
  })
  .strict();

const CanonicalPayloadSchema = z
  .object({ codec: z.literal(2), canonical_json: z.array(z.number().int().min(0).max(255)) })
  .strict();

const MilestoneContributionSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("work"), work_id: ZapIdSchema }).strict(),
  z.object({ kind: z.literal("evidence"), evidence_id: ZapIdSchema }).strict(),
  z
    .object({
      kind: z.literal("milestone"),
      milestone_id: ZapIdSchema,
      revision_id: ZapIdSchema,
      semantic_fingerprint: ZapDigestSchema,
    })
    .strict(),
]);
const MilestoneDependencySchema = z
  .object({
    milestone_id: ZapIdSchema,
    revision_id: ZapIdSchema,
    semantic_fingerprint: ZapDigestSchema,
    kind: z.enum(["preparation_prerequisite", "achievement_prerequisite"]),
  })
  .strict();
const MilestoneDefinitionDraftSchema = z
  .object({
    name: z.string().min(1).max(4_096),
    purpose: z.string().min(1).max(4_096),
    result_criterion: z.string().min(1).max(4_096),
    consumers: SubjectListSchema,
    required_obligation_ids: IdListSchema,
    contributions: z.array(MilestoneContributionSchema),
    dependencies: z.array(MilestoneDependencySchema),
    lifecycle: z.enum(["active", "retired"]),
    retirement_reason: z.string().min(1).max(4_096).nullable(),
  })
  .strict();

export const MilestonePrecursorAuthoringInputSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    changes: z
      .array(
        z.discriminatedUnion("kind", [
          z
            .object({
              kind: z.literal("create"),
              affected_work_ids: IdListSchema,
              definition: MilestoneDefinitionDraftSchema,
            })
            .strict(),
          z
            .object({
              kind: z.literal("revise"),
              milestone_id: ZapIdSchema,
              affected_work_ids: IdListSchema,
              definition: MilestoneDefinitionDraftSchema,
              conservation_reason: z.string().min(1).max(4_096),
            })
            .strict(),
        ]),
      )
      .min(1)
      .max(16),
  })
  .strict();

const PreparedMilestoneChangeSchema = z
  .object({
    kind: z.enum(["create", "revise"]),
    milestoneId: ZapIdSchema,
    revisionId: ZapIdSchema,
    productCommandId: ZapIdSchema,
    effect: z
      .object({
        effect_id: ZapIdSchema,
        index: z.number().int().min(0),
        kind: ZapIdSchema,
        payload: CanonicalPayloadSchema,
        predecessors: IdListSchema,
        product_event_id: ZapIdSchema,
      })
      .strict(),
  })
  .strict();

export const PreparedMilestonePrecursorsSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    changes: z.array(PreparedMilestoneChangeSchema).min(1).max(16),
  })
  .strict();

export const MilestoneReadBindingSchema = z
  .object({
    head: z
      .object({
        milestone_id: ZapIdSchema,
        current_revision_id: ZapIdSchema,
        latest_achievement_id: ZapIdSchema.nullable(),
        revision: StoredU64Schema,
      })
      .strict(),
    current_revision: z
      .object({
        revision_id: ZapIdSchema,
        milestone_id: ZapIdSchema,
        previous_revision_id: ZapIdSchema.nullable(),
        definition: MilestoneDefinitionDraftSchema.extend({
          strategic_revision_id: ZapIdSchema,
          strategic_record_revision: StoredU64Schema,
          strategic_semantic_digest: ZapDigestSchema,
          outcome_id: ZapIdSchema,
          outcome_revision: StoredU64Schema,
        }),
        semantic_fingerprint: ZapDigestSchema,
        proof_fingerprint: ZapDigestSchema,
        revision: StoredU64Schema,
      })
      .strict(),
    latest_achievement: z.unknown().nullable(),
    current_validity: z.string().nullable(),
    proof_evaluation_cost: z.string(),
  })
  .strict();

export const SuccessorPlanAuthoringInputSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    plan: z
      .object({
        key: PlanKeySchema,
        previous: PlanKeySchema,
        strategic_revision_id: ZapIdSchema,
        strategic_record_revision: U64InputSchema,
        strategic_semantic_digest: ZapDigestSchema,
        outcome_revision: U64InputSchema,
        expected_plan_state_revision: U64InputSchema,
        content: PlanContentSchema,
      })
      .strict(),
    economics: z
      .object({
        change_id: ZapIdSchema,
        summary: z.string().min(1).max(4_096),
        necessity: NecessitySchema,
        team_model: TeamModelSchema,
        proposal: z
          .object({
            summary: z.string().min(1).max(4_096),
            solves_mandatory_problem: z.boolean(),
            preserved_obligations: IdListSchema,
            sacrificed_obligations: IdListSchema,
            utility: UtilitySchema,
            cost: CostSchema,
            feasibility: z.enum(["feasible", "infeasible", "unknown"]),
            basis: z.string().min(1).max(4_096),
            evidence_refs: IdListSchema,
          })
          .strict(),
        no_op: z
          .object({
            summary: z.string().min(1).max(4_096),
            utility: UtilitySchema,
            cost: CostSchema,
            feasibility: z.enum(["feasible", "infeasible", "unknown"]),
            basis: z.string().min(1).max(4_096),
            evidence_refs: IdListSchema,
          })
          .strict(),
        comparison_reasons: z.array(z.string().min(1).max(4_096)).min(1),
        estimation: EstimationSchema,
      })
      .strict(),
  })
  .strict();

export const EconomicsContextViewSchema = z
  .object({
    policy: z
      .object({
        policy_id: ZapIdSchema,
        record_revision: StoredU64Schema,
        digest: ZapDigestSchema,
        persisted: z.boolean(),
      })
      .strict(),
    baseline_candidates: z.array(
      z
        .object({
          baseline_id: ZapIdSchema,
          record_revision: StoredU64Schema,
          active_outcome_id: ZapIdSchema,
          change_policy_revision: StoredU64Schema,
        })
        .strict(),
    ),
    baseline_selection: z.discriminatedUnion("state", [
      z.object({ state: z.literal("absent") }).strict(),
      z.object({ state: z.literal("unique"), baseline_id: ZapIdSchema }).strict(),
      z.object({ state: z.literal("ambiguous"), truncated: z.boolean() }).strict(),
    ]),
  })
  .strict();

export const AuthoringContextSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    store: AdvanceChangeAdmissionInputSchema.shape.store,
    activeOutcomeId: ZapIdSchema,
    activeOutcomeRevision: StoredU64Schema,
    currentStrategyId: ZapIdSchema,
    currentStrategyRecordRevision: StoredU64Schema,
    currentStrategySemanticDigest: ZapDigestSchema,
    adoptedPlan: PlanKeySchema,
    adoptedPlanStateRevision: U64InputSchema,
    specificationFileCount: z.number().int().min(0),
    economics: EconomicsContextViewSchema,
  })
  .strict();

export const MetadataReceiptSchema = z
  .object({ commandId: ZapIdSchema, commandDigest: ZapDigestSchema, revision: StoredU64Schema })
  .strict();
export const PreparedCommandSchema = z
  .object({
    stage: z.enum(["plan_proposal", "assessment_proposal"]),
    command: AdvanceChangeAdmissionInputSchema.shape.product,
    commandDigest: ZapDigestSchema,
  })
  .strict();
const EffectSchema = z
  .object({
    effectId: ZapIdSchema,
    index: z.number().int().min(0),
    kind: ZapIdSchema,
    payload: z.object({ codec: z.literal(2), canonical_json: z.array(z.number().int()) }).strict(),
    predecessors: IdListSchema,
    productEventId: ZapIdSchema,
    productCommandId: ZapIdSchema,
    subjects: SubjectListSchema,
  })
  .strict();

export const PreparedPlanProposalSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    semanticInput: CanonicalPayloadSchema,
    planPayload: CanonicalPayloadSchema,
    command: PreparedCommandSchema,
  })
  .strict();

export const PreparedCompositePlanProposalSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    semanticInput: CanonicalPayloadSchema,
    planPayload: CanonicalPayloadSchema,
    precursors: PreparedMilestonePrecursorsSchema,
    composite: PreparedCompositeSuccessorSchema,
  })
  .strict();

export const PreparedAssessmentProposalSchema = z
  .object({
    planProposal: z.union([PreparedPlanProposalSchema, PreparedCompositePlanProposalSchema]),
    planReceipt: MetadataReceiptSchema,
    assessmentId: ZapIdSchema,
    alternativeId: ZapIdSchema,
    planPayload: CanonicalPayloadSchema,
    effects: z.array(EffectSchema).min(1),
    command: PreparedCommandSchema,
  })
  .strict();

export const PreparedSuccessorSchema = z
  .object({
    intentBasis: PlanBasisSchema,
    preparedBasis: PlanBasisSchema,
    operationId: ZapIdSchema,
    assessmentId: ZapIdSchema,
    alternativeId: ZapIdSchema,
    sourceAssessmentDigest: ZapDigestSchema,
    assessmentProposalRevision: StoredU64Schema,
    planPayload: CanonicalPayloadSchema,
    effects: z.array(EffectSchema).min(1),
    planReceipt: MetadataReceiptSchema,
    assessmentReceipt: MetadataReceiptSchema,
  })
  .strict();

export const PreparedAdmissionStepSchema = z
  .object({
    executionBasis: PlanBasisSchema,
    effectIndex: z.number().int().min(0),
    comparisonView: StoredPreparedComparisonSchema,
    advanceRequest: AdvanceChangeAdmissionInputSchema,
    productDigest: ZapDigestSchema,
  })
  .strict();

export type SuccessorPlanAuthoringInput = z.infer<typeof SuccessorPlanAuthoringInputSchema>;
export type MilestonePrecursorAuthoringInput = z.infer<
  typeof MilestonePrecursorAuthoringInputSchema
>;
export type PreparedMilestonePrecursors = z.infer<typeof PreparedMilestonePrecursorsSchema>;
export type AuthoringContext = z.infer<typeof AuthoringContextSchema>;
export type PreparedSuccessor = z.infer<typeof PreparedSuccessorSchema>;
export type PreparedAdmissionStep = z.infer<typeof PreparedAdmissionStepSchema>;
export type PreparedCommand = z.infer<typeof PreparedCommandSchema>;
export type PreparedPlanProposal = z.infer<typeof PreparedPlanProposalSchema>;
export type PreparedCompositePlanProposal = z.infer<typeof PreparedCompositePlanProposalSchema>;
export type PreparedAssessmentProposal = z.infer<typeof PreparedAssessmentProposalSchema>;
