/**
 * @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map
 * @scope spec://org.vibevm.zap/lens/PROP-004#planning-navigation
 */
import { z } from "zod";
import {
  LosslessJsonSchema,
  U32WireSchema,
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
} from "../zap-client/index.ts";

const ViewerKinds = [
  "work",
  "obligation",
  "contract",
  "outcome",
  "source",
  "fact",
  "region",
  "evidence",
  "hold",
  "decision",
  "candidate_review",
] as const;
const ViewerRefSchema = z
  .object({
    kind: z.literal("viewer"),
    id: z.object({ kind: z.enum(ViewerKinds), id: ZapIdSchema }).strict(),
  })
  .strict();
export const MapObjectRefSchema = z.discriminatedUnion("kind", [
  ViewerRefSchema,
  z.object({ kind: z.literal("strategy"), id: ZapIdSchema }).strict(),
  z.object({ kind: z.literal("resource"), id: ZapIdSchema }).strict(),
  z.object({ kind: z.literal("milestone"), id: ZapIdSchema }).strict(),
  z.object({ kind: z.literal("information_opportunity"), id: ZapIdSchema }).strict(),
  z
    .object({ kind: z.literal("strategic_fork"), strategy_id: ZapIdSchema, fork_id: ZapIdSchema })
    .strict(),
]);
export type MapObjectRef = z.infer<typeof MapObjectRefSchema>;

const Text4096Schema = z.string().max(4_096);
const SemanticTypeSchema = z.enum([
  "strategy",
  "work",
  "obligation",
  "contract",
  "outcome",
  "source",
  "fact",
  "region",
  "evidence",
  "hold",
  "decision",
  "candidate_review",
  "execution_resource",
  "milestone",
  "information_opportunity",
  "strategic_fork",
]);
const TextSourceSchema = z.enum([
  "strategic_node",
  "work_record",
  "task_contract",
  "outcome_record",
  "obligation_record",
  "knowledge_record",
  "reference_identity",
  "milestone_record",
  "information_opportunity_record",
]);
const MapTextSchema = z.discriminatedUnion("state", [
  z
    .object({
      state: z.literal("available"),
      value: z.string().max(16_384),
      source: TextSourceSchema,
    })
    .strict(),
  z.object({ state: z.literal("missing"), reason: Text4096Schema }).strict(),
]);
const SourceStateSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("materialized"), record_revision: U64WireSchema }).strict(),
  z.object({ state: z.literal("strategic_node_only"), strategy_revision: U64WireSchema }).strict(),
  z.object({ state: z.literal("reference_only"), reason: Text4096Schema }).strict(),
]);
const WorkStateSchema = z.enum([
  "planned",
  "ready",
  "active",
  "candidate",
  "accepted",
  "blocked",
  "deferred",
  "dropped",
  "superseded",
]);
const AcceptanceSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("strategy_state"),
      state: z.enum(["candidate", "current", "superseded"]),
    })
    .strict(),
  z
    .object({
      kind: z.literal("work_record_state"),
      state: WorkStateSchema,
      declared_criteria: z.array(Text4096Schema).max(100),
    })
    .strict(),
  z.object({ kind: z.literal("strategic_node_only") }).strict(),
  z
    .object({
      kind: z.literal("obligation_state"),
      status: z.enum(["active", "replaced", "excluded", "unattainable"]),
      disposition: z.enum(["retained", "replaced", "excluded", "unattainable"]),
    })
    .strict(),
  z
    .object({
      kind: z.literal("outcome_state"),
      status: z.enum(["proposed", "active", "superseded"]),
    })
    .strict(),
  z
    .object({
      kind: z.literal("milestone_state"),
      lifecycle: z.enum(["active", "retired"]),
      latest_achievement_id: ZapIdSchema.nullable(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("information_opportunity_state"),
      freshness: z.enum(["current", "stale", "unsupported_decision_basis"]),
      selected_work_id: ZapIdSchema.nullable(),
    })
    .strict(),
  z.object({ kind: z.literal("not_applicable"), reason: Text4096Schema }).strict(),
]);
const BlockerSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("not_evaluated"), reason: Text4096Schema }).strict(),
  z
    .object({ state: z.literal("established"), blockers: z.array(MapObjectRefSchema).max(100) })
    .strict(),
  z.object({ state: z.literal("not_applicable"), reason: Text4096Schema }).strict(),
]);
const AssessmentFreshnessSchema = z.enum(["current", "stale", "unavailable"]);
const GradeSchema = z.enum(["unassessed", "low", "medium", "high"]);
const HoursIntervalSchema = z
  .object({ low: U64WireSchema, high: U64WireSchema.nullable() })
  .strict();
const EstimateSchema = z
  .object({
    range: HoursIntervalSchema,
    precision: z.enum(["measured", "bounded_estimate", "order_of_magnitude"]),
    source: Text4096Schema,
    assumptions: z.array(Text4096Schema).max(32),
  })
  .strict();
const AssessmentContentSchema = z
  .object({
    display_label: Text4096Schema.nullable(),
    explanation: Text4096Schema.nullable(),
    remaining_agent_hours: EstimateSchema.nullable(),
    remaining_elapsed: EstimateSchema.nullable(),
    remaining_passive_wait: EstimateSchema.nullable(),
    complexity: z.object({ grade: GradeSchema, rationale: Text4096Schema.nullable() }).strict(),
    difficulty: z
      .object({
        grade: GradeSchema,
        rationale: Text4096Schema.nullable(),
        executor_assumptions: z.array(Text4096Schema).max(32),
        knowledge_assumptions: z.array(Text4096Schema).max(32),
      })
      .strict(),
    uncertainty: z
      .object({
        confidence: GradeSchema,
        rationale: Text4096Schema.nullable(),
        unknowns: z.array(Text4096Schema).max(64),
      })
      .strict(),
    evidence_refs: z.array(ZapIdSchema).max(128),
  })
  .strict();
const AssessmentSchema = z
  .object({
    freshness: AssessmentFreshnessSchema,
    record: z
      .object({
        work_id: ZapIdSchema,
        source_fingerprint: ZapDigestSchema,
        content: AssessmentContentSchema,
        revision: U64WireSchema,
      })
      .strict(),
  })
  .strict();
const AssessmentStateSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("not_applicable") }).strict(),
  z.object({ state: z.literal("unavailable"), reason: Text4096Schema }).strict(),
  z.object({ state: z.literal("available"), freshness: AssessmentFreshnessSchema }).strict(),
]);
const RelationshipSchema = z
  .object({
    id: ZapDigestSchema,
    from: MapObjectRefSchema,
    to: MapObjectRefSchema,
    kind: z.enum([
      "contains",
      "work_prerequisite",
      "contract_for",
      "covers_obligation",
      "owns_obligation",
      "outcome_scope",
      "successor",
      "acceptance_duty",
      "supports",
      "verifies",
      "derived_from",
      "affects",
      "consumes",
      "resource_use",
      "read_subject",
      "write_subject",
      "applies_to",
      "source_reference",
      "evidence_reference",
      "alternative",
      "contributes_to",
      "milestone_preparation_prerequisite",
      "milestone_achievement_prerequisite",
      "decision_support",
      "selected_information_work",
    ]),
    ownership_role: z
      .enum(["implementation", "verification", "integration", "acceptance"])
      .nullable(),
    source: LosslessJsonSchema,
  })
  .strict();

export const SemanticCardWireSchema = z
  .object({
    object: MapObjectRefSchema,
    semantic_type: SemanticTypeSchema,
    canonical_name: z.string().min(1).max(4_096),
    description: MapTextSchema,
    purpose: MapTextSchema,
    expected_result: MapTextSchema,
    source_state: SourceStateSchema,
    acceptance: AcceptanceSchema,
    reasons: z.array(Text4096Schema).max(100),
    blockers: BlockerSchema,
    sources: z.array(ZapIdSchema),
    evidence: z.array(ZapIdSchema),
    work_kind: z
      .enum(["portfolio", "campaign", "phase", "workstream", "group", "atom", "gate", "horizon"])
      .nullable(),
    work_type: z.enum(["evidence", "decision", "change", "verification", "integration"]).nullable(),
    landmark: z
      .enum(["portfolio", "campaign", "phase", "workstream", "group", "gate", "horizon"])
      .nullable(),
    relationships: z.array(RelationshipSchema),
    relationship_gaps: z.array(LosslessJsonSchema),
    relationships_complete: z.boolean(),
    assessment_source_fingerprint: ZapDigestSchema.nullable(),
    assessment_state: AssessmentStateSchema,
    assessment: AssessmentSchema.nullable(),
    observation_revision: U64WireSchema,
    underlying: LosslessJsonSchema.nullable(),
  })
  .strict();
export type SemanticCardWire = z.infer<typeof SemanticCardWireSchema>;

const OverviewCursorSchema = z
  .object({
    store_id: ZapIdSchema,
    base_id: ZapIdSchema,
    query_epoch: U32WireSchema,
    snapshot_revision: U64WireSchema,
    strategy_id: ZapIdSchema,
    strategy_revision: U64WireSchema,
    strategy_semantic_digest: ZapDigestSchema,
    normalized_filter: ZapDigestSchema,
    last_examined_work_id: ZapIdSchema,
  })
  .strict();
export const MapOverviewResultSchema = z
  .object({
    strategy_id: ZapIdSchema,
    strategy_revision: U64WireSchema,
    strategy_semantic_digest: ZapDigestSchema,
    strategy_state: z.enum(["candidate", "current", "superseded"]),
    source_plan_nodes: U64WireSchema,
    source_plan_encoded_bytes: U64WireSchema,
    examined: U32WireSchema,
    emitted: U32WireSchema,
    examined_index_rows: U64WireSchema,
    emitted_relationships: U64WireSchema,
    cards: z.array(SemanticCardWireSchema),
    missing_work_ids: z.array(ZapIdSchema),
    next: OverviewCursorSchema.nullable(),
    through_revision: U64WireSchema,
  })
  .strict();
export type MapOverviewResult = z.infer<typeof MapOverviewResultSchema>;

export const MapObjectResultSchema = z
  .object({
    card: SemanticCardWireSchema,
    examined_relationships: U32WireSchema,
    emitted_relationships: U32WireSchema,
    examined_index_rows: U64WireSchema,
    next: LosslessJsonSchema.nullable(),
    through_revision: U64WireSchema,
  })
  .strict();

export const MilestonePlanViewSchema = z
  .object({
    outcome_id: ZapIdSchema,
    adopted_state: LosslessJsonSchema.nullable(),
    plan: LosslessJsonSchema.nullable(),
    status: z.enum([
      "no_adopted_plan",
      "proposal",
      "adopted",
      "adopted_needs_reassessment",
      "all_satisfied",
    ]),
    focus: LosslessJsonSchema.nullable(),
    focus_achievement_validity: z.string().nullable(),
    unsatisfied_milestone_revision_ids: z.array(ZapIdSchema),
    distant_horizons: z.array(LosslessJsonSchema),
    gaps: z.array(LosslessJsonSchema),
    query_cost: z
      .object({
        whole_plan_record_decoded: z.boolean(),
        milestone_records_decoded: U32WireSchema,
        proof_validity_evaluations: U32WireSchema,
        store_wide_proof_cost: z.string(),
      })
      .strict(),
  })
  .strict();
export type MilestonePlanView = z.infer<typeof MilestonePlanViewSchema>;

export const MilestoneRevisionQueryInputSchema = z.object({ revision_id: ZapIdSchema }).strict();
export const MilestoneRevisionViewSchema = z
  .object({
    revision: z
      .object({
        revision_id: ZapIdSchema,
        milestone_id: ZapIdSchema,
        previous_revision_id: ZapIdSchema.nullable(),
        definition: LosslessJsonSchema,
        semantic_fingerprint: ZapDigestSchema,
        proof_fingerprint: ZapDigestSchema,
        revision: U64WireSchema,
      })
      .strict(),
    is_current: z.boolean(),
  })
  .strict();
export type MilestoneRevisionView = z.infer<typeof MilestoneRevisionViewSchema>;
