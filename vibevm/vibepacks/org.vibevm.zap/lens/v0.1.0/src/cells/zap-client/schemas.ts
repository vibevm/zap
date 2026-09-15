/** Runtime schemas for the exact Rust machine wire. @scope spec://org.vibevm.zap/lens/PROP-002#read-model */
import { z } from "zod";
import { byteArray, encodeCanonicalJson, normalizeLossless, wireU32, wireU64 } from "./codec.ts";
import type { CanonicalJsonInput, U64 } from "./types.ts";
import { ZapDigestSchema, ZapIdSchema } from "./schemas-base.ts";
const U64_MAX = 18_446_744_073_709_551_615n;
export const U64WireSchema = transformUnknown(wireU64);
export const U32WireSchema = transformUnknown(wireU32);
export const ByteArrayWireSchema = transformUnknown(byteArray);
export const LosslessJsonSchema = transformUnknown(normalizeLossless);
const PositiveU32WireSchema = U32WireSchema.refine((value) => value > 0);
const U16WireSchema = U32WireSchema.refine((value) => value <= 65_535);
const IdentifierListSchema = z.array(ZapIdSchema);
export const StoreIdentitySchema = z
  .object({
    store_id: ZapIdSchema,
    campaign_id: ZapIdSchema,
    base_id: ZapIdSchema,
    store_epoch: z.string().regex(/^zap\/[1-9][0-9]*$/),
    codec_epoch: PositiveU32WireSchema,
    reducer_epoch: PositiveU32WireSchema,
  })
  .strict();
const IndexAlgorithmSchema = z
  .object({ family: ZapIdSchema, fingerprint: ZapDigestSchema })
  .strict();
const IndexCatalogSchema = z
  .object({
    version: U32WireSchema,
    query_epoch: PositiveU32WireSchema,
    covered_revision: U64WireSchema,
    families: IdentifierListSchema,
    algorithms: z.array(IndexAlgorithmSchema),
  })
  .strict();
export const CapabilitiesSchema = z
  .object({
    schema: z.string().min(1),
    read_operations: z.array(z.string()),
    command_operations: z.array(z.string()),
    query_ids: IdentifierListSchema,
    unavailable_operations: z.array(z.string()),
    max_page_items: U32WireSchema,
    local_bind_default: z.boolean(),
  })
  .strict();
export const SnapshotSchema = z
  .object({
    store: StoreIdentitySchema,
    revision: U64WireSchema,
    head_event_digest: ZapDigestSchema,
    event_count: U64WireSchema,
    record_count: U64WireSchema,
    index_count: U64WireSchema,
    projection_digest: ZapDigestSchema,
    physical_schema_version: U16WireSchema,
    physical_schema: z.enum(["v1", "v2"]),
    physical_projection_algorithm: z.enum(["v1_tables", "v2_tables"]),
    physical_projection_digest: ZapDigestSchema,
    logical_row_digest: ZapDigestSchema,
    derived_index_catalog: IndexCatalogSchema.nullable(),
  })
  .strict();
export const EventCursorSchema = z
  .object({
    store: StoreIdentitySchema,
    revision: U64WireSchema,
    next_sequence: U64WireSchema,
  })
  .strict();
export const EventSummarySchema = z
  .object({
    sequence: U64WireSchema,
    digest: ZapDigestSchema,
    header: LosslessJsonSchema.nullable(),
    reason: LosslessJsonSchema.nullable(),
    authority: LosslessJsonSchema.nullable(),
    action_admission: LosslessJsonSchema.nullable(),
    artifacts: z.array(ZapDigestSchema),
  })
  .strict();
export const EventPageSchema = z
  .object({
    store: StoreIdentitySchema,
    revision: U64WireSchema,
    events: z.array(EventSummarySchema),
    resume: EventCursorSchema,
    next: EventCursorSchema.nullable(),
  })
  .strict();
const ActiveContextSnapshotSchema = z
  .object({
    store_id: ZapIdSchema,
    campaign_id: ZapIdSchema,
    base_id: ZapIdSchema,
    revision: U64WireSchema,
  })
  .strict();
const ActiveOutcomeSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("uninitialized") }).strict(),
  z
    .object({
      state: z.literal("present"),
      outcome_id: ZapIdSchema,
      record_revision: U64WireSchema,
    })
    .strict(),
]);
const CurrentStrategySchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("absent") }).strict(),
  z
    .object({
      state: z.literal("present"),
      strategic_revision_id: ZapIdSchema,
      record_revision: U64WireSchema,
    })
    .strict(),
]);
const PlanKeySchema = z.object({ outcome_id: ZapIdSchema, generation: U64WireSchema }).strict();
const PlanGapSchema = z.enum([
  "plan_record_missing",
  "plan_state_mismatch",
  "outcome_binding_stale",
  "strategy_record_missing",
  "strategy_binding_stale",
  "current_strategy_mismatch",
]);
export const AdoptedMilestonePlanSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("absent") }).strict(),
  z
    .object({
      state: z.literal("present"),
      plan_key: PlanKeySchema,
      plan_state_revision: U64WireSchema,
    })
    .strict(),
  z
    .object({
      state: z.literal("needs_reassessment"),
      plan_key: PlanKeySchema,
      plan_state_revision: U64WireSchema,
      gaps: z.array(PlanGapSchema),
    })
    .strict(),
]);
export const ActiveContextViewSchema = z
  .object({
    snapshot: ActiveContextSnapshotSchema,
    active_outcome: ActiveOutcomeSchema,
    current_strategy: CurrentStrategySchema,
    adopted_milestone_plan: AdoptedMilestonePlanSchema,
  })
  .strict();
export const QueryPageWireSchema = z
  .object({
    store: StoreIdentitySchema,
    revision: U64WireSchema,
    query_epoch: PositiveU32WireSchema,
    items: z.array(ByteArrayWireSchema),
    completeness: z.enum(["complete", "more", "unknown_boundary"]),
  })
  .strict();
const SubjectKinds = [
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
] as const;
const SubjectRefSchema = z.object({ kind: z.enum(SubjectKinds), id: ZapIdSchema }).strict();
export const PreparedBundleSchema = z
  .object({
    store: StoreIdentitySchema,
    observed_revision: U64WireSchema,
    request: LosslessJsonSchema,
    preflight: LosslessJsonSchema,
    affected_scopes: z.array(LosslessJsonSchema.nullable()),
  })
  .strict();
const WorkExecutionObservationSchema = z
  .object({
    job_id: ZapIdSchema,
    attempt_id: ZapIdSchema,
    work_id: ZapIdSchema,
    contract_id: ZapIdSchema,
    contract_digest: ZapDigestSchema,
    validation_generation: U64WireSchema,
    subjects: z.array(SubjectRefSchema),
    execution: z.enum([
      "prepared",
      "dispatch_pending",
      "starting",
      "running",
      "stop_requested",
      "stopping",
      "succeeded",
      "failed",
      "stopped",
      "interrupted",
      "unknown_effect",
    ]),
    effect: z.enum(["not_started", "intent_committed", "started", "completed", "unknown"]),
    safe_state: z.enum(["unknown", "needs_reconcile", "not_started", "safe", "completed"]),
    revision: U64WireSchema,
  })
  .strict();
const AffectedJobSchema = z
  .object({
    request_digest: ZapDigestSchema,
    observed_revision: U64WireSchema,
    jobs: z.array(WorkExecutionObservationSchema),
    completeness: z.enum(["complete", "unknown"]),
  })
  .strict();
const AffectedScopeSchema = z
  .object({
    request_digest: ZapDigestSchema,
    observed_revision: U64WireSchema,
    affected_work_ids: z.array(ZapIdSchema),
    dependent_work_ids: z.array(ZapIdSchema),
    subjects: z.array(SubjectRefSchema),
    unknown_boundary: z.array(SubjectRefSchema),
    completeness: z.enum(["complete", "incomplete"]),
    relevant_basis: ZapDigestSchema,
    jobs: AffectedJobSchema,
    digest: ZapDigestSchema,
  })
  .strict();
const PreparedComparisonBaseSchema = z
  .object({
    store: StoreIdentitySchema,
    observed_revision: U64WireSchema,
    alternatives: z.array(PreparedBundleSchema),
    basis_request: LosslessJsonSchema,
    relevant_basis: ZapDigestSchema,
    affected_scopes: z.array(AffectedScopeSchema),
  })
  .strict();
export const PreparedComparisonSchema = PreparedComparisonBaseSchema.refine(
  (value) => value.affected_scopes.length === value.alternatives.length,
);
const StoredU64Schema = z.custom<U64>(
  (value) =>
    typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value) && BigInt(value) <= U64_MAX,
);
const StoredStoreIdentitySchema = StoreIdentitySchema.extend({
  codec_epoch: z.number().int().min(1).max(4_294_967_295),
  reducer_epoch: z.number().int().min(1).max(4_294_967_295),
}).strict();
const StoredPreparedBundleSchema = PreparedBundleSchema.extend({
  store: StoredStoreIdentitySchema,
  observed_revision: StoredU64Schema,
}).strict();
const StoredWorkExecutionObservationSchema = WorkExecutionObservationSchema.extend({
  validation_generation: StoredU64Schema,
  revision: StoredU64Schema,
}).strict();
const StoredAffectedJobSchema = AffectedJobSchema.extend({
  observed_revision: StoredU64Schema,
  jobs: z.array(StoredWorkExecutionObservationSchema),
}).strict();
const StoredAffectedScopeSchema = AffectedScopeSchema.extend({
  observed_revision: StoredU64Schema,
  jobs: StoredAffectedJobSchema,
}).strict();
export const StoredPreparedComparisonSchema = PreparedComparisonBaseSchema.extend({
  store: StoredStoreIdentitySchema,
  observed_revision: StoredU64Schema,
  alternatives: z.array(StoredPreparedBundleSchema),
  affected_scopes: z.array(StoredAffectedScopeSchema),
})
  .strict()
  .refine((value) => value.affected_scopes.length === value.alternatives.length);
export const ProjectedRecordSchema = z
  .object({
    store: StoreIdentitySchema,
    observed_revision: U64WireSchema,
    family: ZapIdSchema,
    key: ByteArrayWireSchema,
    canonical_value: ByteArrayWireSchema.nullable(),
    preparation: PreparedBundleSchema,
  })
  .strict();
const CommitReceiptSchema = z
  .object({
    store: StoreIdentitySchema,
    command_id: ZapIdSchema,
    event_id: ZapIdSchema,
    transaction_id: ZapIdSchema,
    revision: U64WireSchema,
    event_digest: ZapDigestSchema,
    output: ByteArrayWireSchema,
    disposition: z.enum(["committed", "exact_retry", "reconciled_committed"]),
  })
  .strict();
export const SubmissionStatusSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("committed"), receipt: CommitReceiptSchema }).strict(),
  z.object({ status: z.literal("not_committed"), command_id: ZapIdSchema }).strict(),
  z
    .object({
      status: z.literal("unknown"),
      command_id: ZapIdSchema,
      command_digest: ZapDigestSchema,
    })
    .strict(),
]);
const CanonicalInputSchema = z.custom<CanonicalJsonInput>((value) => {
  try {
    encodeCanonicalJson(value);
    return true;
  } catch {
    return false;
  }
});
const BigU64Schema = z.bigint().min(0n).max(U64_MAX);
const ByteInputSchema = z.array(z.number().int().min(0).max(255));
const QueryInputSchema = z
  .object({ codec: z.literal(2), canonical_json: ByteInputSchema })
  .strict();
const QueryInputWireResponseSchema = z
  .object({
    codec: U32WireSchema.refine((value) => value === 2).transform(() => 2 as const),
    canonical_json: ByteArrayWireSchema.transform((value) => [...value]),
  })
  .strict();
const PreparationReadSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("current") }).strict(),
  z.object({ kind: z.literal("revision"), revision: BigU64Schema }).strict(),
]);
const EffectDraftSchema = z
  .object({
    effect_id: ZapIdSchema,
    index: z.number().int().min(0).max(4_294_967_295),
    kind: ZapIdSchema,
    payload: QueryInputSchema,
    predecessors: sortedIds(),
    product_event_id: ZapIdSchema,
  })
  .strict();
const EffectDraftWireSchema = EffectDraftSchema.extend({
  index: U32WireSchema,
  payload: QueryInputWireResponseSchema,
}).strict();
const BundleDraftSchema = z
  .object({
    alternative_id: ZapIdSchema,
    committed_prefix: sortedIds(),
    effects: z.array(EffectDraftSchema),
    no_op_basis: CanonicalInputSchema.nullable(),
  })
  .strict();
const BundleDraftWireSchema = BundleDraftSchema.extend({
  effects: z.array(EffectDraftWireSchema),
}).strict();
export const PrepareBundleInputSchema = z
  .object({
    at: PreparationReadSchema,
    actor: CanonicalInputSchema.nullable(),
    draft: BundleDraftSchema,
  })
  .strict();
export const PrepareComparisonInputSchema = z
  .object({
    at: PreparationReadSchema,
    actor: CanonicalInputSchema.nullable(),
    draft: z
      .object({
        assessment_id: ZapIdSchema,
        alternatives: z.array(BundleDraftSchema),
        policy: CanonicalInputSchema,
        capacity: CanonicalInputSchema,
        closure: CanonicalInputSchema,
      })
      .strict(),
  })
  .strict();
export const PrepareProjectedInputSchema = PrepareBundleInputSchema.extend({
  record: z.object({ family: ZapIdSchema, key: ByteInputSchema.min(1).max(4096) }).strict(),
}).strict();
const BasisSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("not_applicable") }).strict(),
  z.object({ kind: z.literal("exact"), digest: ZapDigestSchema }).strict(),
]);
const HeaderSchema = z
  .object({
    protocol: z.number().int().min(1).max(4_294_967_295),
    store_id: ZapIdSchema,
    campaign_id: ZapIdSchema,
    base_id: ZapIdSchema,
    command_id: ZapIdSchema,
    event_id: ZapIdSchema,
    expected_revision: BigU64Schema,
    kind: ZapIdSchema,
    causes: sortedIds(),
    basis: BasisSchema,
  })
  .strict();
const ReasonSchema = z
  .object({
    summary: utf8String(1, 4096),
    evidence: sortedIds(),
    decision: ZapIdSchema.nullable(),
    change: ZapIdSchema.nullable(),
  })
  .strict();
const ProtectedCommandSchema = z
  .object({
    frame: z
      .object({ header: HeaderSchema, reason: ReasonSchema, payload: QueryInputSchema })
      .strict(),
  })
  .strict();
export const ProtectedSubmissionSchema = z
  .object({
    command: ProtectedCommandSchema,
    reconciliation: z.object({ command_id: ZapIdSchema, command_digest: ZapDigestSchema }).strict(),
  })
  .strict()
  .refine((value) => value.command.frame.header.command_id === value.reconciliation.command_id);
export const ReconcileInputSchema = z
  .object({ command_id: ZapIdSchema, command_digest: ZapDigestSchema })
  .strict();
const BigU64InputSchema = z
  .union([z.bigint(), z.string().regex(/^(0|[1-9][0-9]*)$/)])
  .transform((value) => BigInt(value))
  .refine((value) => value <= U64_MAX);
const CompositeStoreInputSchema = z
  .object({
    store_id: ZapIdSchema,
    campaign_id: ZapIdSchema,
    base_id: ZapIdSchema,
    store_epoch: z.string().regex(/^zap\/[1-9][0-9]*$/),
    codec_epoch: z.number().int().min(1).max(4_294_967_295),
    reducer_epoch: z.number().int().min(1).max(4_294_967_295),
  })
  .strict();
export const PrepareCompositeSuccessorInputSchema = z
  .object({
    operation_id: ZapIdSchema,
    store: CompositeStoreInputSchema,
    expected_revision: BigU64InputSchema,
    precursors: BundleDraftSchema,
    plan_intent: QueryInputSchema,
  })
  .strict();
const PreparedCompositeRequestWireSchema = PrepareCompositeSuccessorInputSchema.extend({
  store: StoreIdentitySchema,
  expected_revision: U64WireSchema,
  precursors: BundleDraftWireSchema,
  plan_intent: QueryInputWireResponseSchema,
}).strict();
const StoredPreparedCompositeRequestSchema = PrepareCompositeSuccessorInputSchema.extend({
  expected_revision: StoredU64Schema,
}).strict();
export const PreparedCompositeSuccessorWireSchema = z
  .object({
    request: PreparedCompositeRequestWireSchema,
    request_digest: ZapDigestSchema,
    plan: QueryInputWireResponseSchema,
    precursor_preparation: PreparedBundleSchema,
    reconciliation: ReconcileInputSchema,
  })
  .strict();
export const PreparedCompositeSuccessorSchema = PreparedCompositeSuccessorWireSchema.extend({
  request: StoredPreparedCompositeRequestSchema,
  plan: QueryInputSchema,
  precursor_preparation: StoredPreparedBundleSchema,
  replay: QueryInputSchema,
}).strict();
export const RecordedCompositeSuccessorWireSchema = z
  .object({
    prepared: PreparedCompositeSuccessorWireSchema,
    submission: SubmissionStatusSchema,
  })
  .strict();
export const AdvanceChangeAdmissionInputSchema = z
  .object({
    operation_id: ZapIdSchema,
    store: z
      .object({
        store_id: ZapIdSchema,
        campaign_id: ZapIdSchema,
        base_id: ZapIdSchema,
        store_epoch: z.string().regex(/^zap\/[1-9][0-9]*$/),
        codec_epoch: z.number().int().min(1).max(4_294_967_295),
        reducer_epoch: z.number().int().min(1).max(4_294_967_295),
      })
      .strict(),
    expected_revision: BigU64Schema,
    action: ZapIdSchema,
    assessment_id: ZapIdSchema,
    alternative_id: ZapIdSchema,
    source_assessment_digest: ZapDigestSchema,
    assessment_digest: ZapDigestSchema,
    relevant_basis: ZapDigestSchema,
    comparison: PrepareComparisonInputSchema,
    product: ProtectedCommandSchema,
    decision_id: ZapIdSchema.nullable(),
    exception_id: ZapIdSchema.nullable(),
  })
  .strict();
export { RefusalSchema, ResyncSchema, ChangeAdmissionViewSchema } from "./admission.ts";
export { ZapDigestSchema, ZapIdSchema } from "./schemas-base.ts";
export function machineResponse<T>(kind: string, value: z.ZodType<T>) {
  return z.object({ kind: z.literal(kind), value }).strict();
}
function transformUnknown<T>(transform: (value: unknown) => T) {
  return z.unknown().transform((value, context): T => {
    try {
      return transform(value);
    } catch {
      context.addIssue({ code: "custom", message: "invalid exact ZAP wire value" });
      return z.NEVER;
    }
  });
}
function utf8String(minimum: number, maximum: number) {
  return z.string().refine((value) => {
    const size = new TextEncoder().encode(value).length;
    return size >= minimum && size <= maximum;
  });
}
function sortedIds() {
  return IdentifierListSchema.refine((values) =>
    values.every((value, index) => {
      const previous = values[index - 1];
      return previous === undefined || previous < value;
    }),
  );
}
