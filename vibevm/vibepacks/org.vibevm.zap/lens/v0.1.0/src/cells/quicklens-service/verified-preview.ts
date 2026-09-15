/**
 * Backend-returned effect projection for authoritative human preview rows.
 * @scope spec://org.vibevm.zap/lens/PROP-002#plan-control
 */
import { z } from "zod";

import {
  PlanChangeSummarySchema,
  type PlanChangeSummary,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import {
  LosslessJsonSchema,
  parseCanonicalJson,
  type PreparedEffectComparisonView,
  type ProtectedCommand,
  type ZapId,
} from "../zap-client/index.ts";

const IdSchema = z
  .string()
  .min(1)
  .max(1_024)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);
const DigestSchema = z.string().regex(/^[0-9a-f]{64}$/);
const NormalizedU32Schema = z
  .string()
  .regex(/^(0|[1-9][0-9]*)$/)
  .transform(Number)
  .refine((value) => Number.isSafeInteger(value) && value <= 4_294_967_295);
const NormalizedByteSchema = z
  .string()
  .regex(/^(0|[1-9][0-9]{0,2})$/)
  .transform(Number)
  .refine((value) => value <= 255);
const SubjectSchema = z.object({ kind: z.string().min(1).max(160), id: IdSchema }).strict();
const EffectSchema = z
  .object({
    effect_id: IdSchema,
    index: NormalizedU32Schema,
    kind: IdSchema,
    payload: z
      .object({
        codec: z.literal("2"),
        canonical_json: z.array(NormalizedByteSchema).max(1_048_576),
      })
      .strict(),
    predecessors: z.array(IdSchema).max(200),
    product_event_id: IdSchema,
    basis: z.unknown(),
    declared_subjects: z.array(SubjectSchema).max(100),
    relevant_before: DigestSchema,
    declared_relevant_after: DigestSchema,
  })
  .strict();
const SelectedRequestSchema = z
  .object({
    alternative_id: IdSchema,
    committed_prefix: z.array(IdSchema).max(200),
    initial_basis: DigestSchema,
    effects: z.array(EffectSchema).min(1).max(200),
    no_op_basis: z.unknown().nullable(),
    request_digest: DigestSchema,
  })
  .strict();

const WorkTransitionSchema = z
  .object({
    schema: z.literal("zap-domain/work-transitioned/1"),
    work_id: IdSchema,
    from_state: z.string().min(1).max(160),
    to_state: z.string().min(1).max(160),
    successor_ids: z.array(IdSchema).max(100),
  })
  .strict();
const MilestoneAdoptionSchema = z
  .object({
    schema: z.literal("zap-domain/milestone-plan-adopted/1"),
    plan: z.looseObject({
      key: z
        .object({ outcome_id: IdSchema, generation: z.string().regex(/^(0|[1-9][0-9]*)$/) })
        .strict(),
    }),
    expected_plan_state_revision: z
      .string()
      .regex(/^(0|[1-9][0-9]*)$/)
      .nullable(),
  })
  .strict();

/** @implements spec://org.vibevm.zap/lens/PROP-002#plan-control */
export function deriveVerifiedPlanChanges(
  prepared: PreparedEffectComparisonView,
  selectedAlternativeId: ZapId,
): QuicklensResult<PlanChangeSummary[]> {
  const request = selectedRequest(prepared, selectedAlternativeId);
  if (request === null) {
    return failure("Selected backend-prepared alternative is absent or ambiguous.");
  }
  try {
    const rows = request.effects.map((effect) => effectRow(effect));
    return { ok: true, value: rows.map((row) => PlanChangeSummarySchema.parse(row)) };
  } catch {
    return failure("Selected backend-prepared effect payload cannot be rendered exactly.");
  }
}

export function verifiedPreparedProductMatches(
  prepared: PreparedEffectComparisonView,
  selectedAlternativeId: ZapId,
  product: ProtectedCommand,
): boolean {
  const request = selectedRequest(prepared, selectedAlternativeId);
  const effect = request?.effects[0];
  const payload = product.frame.payload.canonical_json;
  return (
    (request?.effects.length ?? 0) >= 1 &&
    effect?.kind === product.frame.header.kind &&
    effect.product_event_id === product.frame.header.event_id &&
    effect.payload.codec === String(product.frame.payload.codec) &&
    effect.payload.canonical_json.length === payload.length &&
    effect.payload.canonical_json.every((value, index) => value === payload[index])
  );
}

function selectedRequest(
  prepared: PreparedEffectComparisonView,
  selectedAlternativeId: ZapId,
): z.infer<typeof SelectedRequestSchema> | null {
  const matches = prepared.alternatives
    .map((alternative) => SelectedRequestSchema.safeParse(alternative.request))
    .filter(
      (candidate) => candidate.success && candidate.data.alternative_id === selectedAlternativeId,
    );
  return matches.length === 1 && matches[0]?.success ? matches[0].data : null;
}

type PreparedEffect = z.infer<typeof EffectSchema>;

function effectRow(effect: PreparedEffect): PlanChangeSummary {
  const bytes = Uint8Array.from(effect.payload.canonical_json);
  const payload = LosslessJsonSchema.parse(parseCanonicalJson(bytes));
  if (effect.kind === "domain.work-transitioned") {
    const transition = WorkTransitionSchema.safeParse(payload);
    if (transition.success) {
      return {
        subjectLabel: `Work ${transition.data.work_id}`,
        changeKind: effect.kind,
        beforeSummary: `State: ${human(transition.data.from_state)}.`,
        afterSummary:
          transition.data.successor_ids.length === 0
            ? `State: ${human(transition.data.to_state)}; no successors declared.`
            : `State: ${human(transition.data.to_state)}; successors: ${transition.data.successor_ids.join(", ")}.`,
      };
    }
  }
  if (effect.kind === "milestone.plan-adopted") {
    const adoption = MilestoneAdoptionSchema.safeParse(payload);
    if (adoption.success) {
      const key = adoption.data.plan.key;
      return {
        subjectLabel: `Outcome ${key.outcome_id}`,
        changeKind: effect.kind,
        beforeSummary:
          adoption.data.expected_plan_state_revision === null
            ? "No adopted milestone-plan state revision is expected."
            : `Expected adopted plan state revision: ${adoption.data.expected_plan_state_revision}.`,
        afterSummary: `Adopt milestone plan generation ${key.generation} for outcome ${key.outcome_id}.`,
      };
    }
  }
  const subjects = effect.declared_subjects.map((subject) => `${subject.kind}:${subject.id}`);
  return {
    subjectLabel: subjects.length === 0 ? `Effect ${effect.effect_id}` : subjects.join(", "),
    changeKind: bounded(effect.kind, 160),
    beforeSummary: bounded(
      `Relevant basis ${effect.relevant_before}; declared subjects: ${subjects.length === 0 ? "none" : subjects.join(", ")}.`,
      2_000,
    ),
    afterSummary: bounded(
      `Exact payload ${new TextDecoder().decode(bytes)}; declared relevant after ${effect.declared_relevant_after}.`,
      2_000,
    ),
  };
}

function human(value: string): string {
  return value.replaceAll("_", " ");
}

function bounded(value: string, maximum: number): string {
  return value.length <= maximum ? value : `${value.slice(0, maximum - 1)}…`;
}

function failure(message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_data",
      message,
      recovery:
        "Prepare the selected alternative again and use its exact backend-returned effects.",
    },
  };
}
