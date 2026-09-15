/** Fresh per-effect admission preparation. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#effects */
import { PlanBasisSchema } from "../quicklens-model/index.ts";
import {
  ZapDigestSchema,
  ZapIdSchema,
  parseCanonicalJson,
  protectedCommandDigest,
  type PrepareComparisonRequest,
  type ZapId,
} from "../zap-client/index.ts";
import { PreparedAdmissionStepSchema, PreparedSuccessorSchema } from "./schemas.ts";
import type {
  AuthoringContext,
  PlanAuthoringOptions,
  PlanAuthoringResult,
  PrepareEffectInput,
  PreparedAdmissionStep,
  PreparedSuccessor,
} from "./types.ts";
import { readAssessmentDigest } from "./metadata.ts";
import { canonicalValue, command, fail, preparedRequest } from "./wire.ts";

export async function prepareEffect(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawPrepared: PreparedSuccessor,
  input: PrepareEffectInput,
): Promise<PlanAuthoringResult<PreparedAdmissionStep>> {
  const parsed = PreparedSuccessorSchema.safeParse(rawPrepared);
  if (!parsed.success || !validPrefix(parsed.data, input)) {
    return fail("invalid_input", "Effect index or committed prefix is invalid");
  }
  const prepared = parsed.data;
  if (
    context.intentBasis.storeRef !== prepared.intentBasis.storeRef ||
    context.intentBasis.baseRef !== prepared.intentBasis.baseRef ||
    context.intentBasis.sourceBasisRef !== prepared.intentBasis.sourceBasisRef
  ) {
    return fail(
      "stale_basis",
      "Store, base or specification basis changed before effect preparation",
    );
  }
  const remaining = prepared.effects.slice(input.effectIndex);
  const selected = remaining[0];
  if (selected === undefined) return fail("invalid_input", "No remaining effect exists");
  const comparison: PrepareComparisonRequest = {
    at: { kind: "current" },
    actor: null,
    draft: {
      assessment_id: prepared.assessmentId,
      alternatives: [
        {
          alternative_id: prepared.alternativeId,
          committed_prefix: [...input.completedPrefix],
          effects: remaining.map((effect) => ({
            effect_id: effect.effectId,
            index: effect.index,
            kind: effect.kind,
            payload: effect.payload,
            predecessors: [...effect.predecessors],
            product_event_id: effect.productEventId,
          })),
          no_op_basis: null,
        },
      ],
      policy: "required",
      capacity: "not_applicable",
      closure: "known_graph",
    },
  };
  const compared = await options.reader.prepareComparison(comparison);
  if (!compared.ok)
    return fail("refused", `Remaining-effect preparation failed: ${compared.error.kind}`);
  if (String(compared.value.observed_revision) !== context.intentBasis.revision) {
    return fail("stale_basis", "Effect preparation revision differs from refreshed context");
  }
  const request = preparedRequest(compared.value);
  if (!request.ok) return request;
  const preparedEffect = request.value.effects[0];
  if (preparedEffect === undefined || preparedEffect.effect_id !== selected.effectId) {
    return fail("refused", "Prepared next effect differs from immutable metadata");
  }
  const localBasis = preparedEffect.relevant_before;
  const assessment = await readAssessmentDigest(options, context, prepared.assessmentId);
  if (!assessment.ok) return assessment;
  let decoded: unknown;
  try {
    decoded = parseCanonicalJson(Uint8Array.from(selected.payload.canonical_json));
  } catch {
    return fail("invalid_input", "Immutable product payload is not canonical JSON");
  }
  const payload = canonicalValue(decoded);
  if (!payload.ok) return payload;
  const product = command(
    context.store,
    { commandId: selected.productCommandId, eventId: selected.productEventId },
    BigInt(context.intentBasis.revision) + (assessment.value.adjudicated ? 1n : 2n),
    selected.kind,
    payload.value,
    { kind: "exact", digest: localBasis },
    "Apply one admitted successor-plan effect",
    null,
  );
  const productDigest = ZapDigestSchema.parse(protectedCommandDigest(product));
  const executionBasis = PlanBasisSchema.parse(context.intentBasis);
  return {
    ok: true,
    value: PreparedAdmissionStepSchema.parse({
      executionBasis,
      effectIndex: input.effectIndex,
      comparisonView: compared.value,
      advanceRequest: {
        operation_id: prepared.operationId,
        store: context.store,
        expected_revision: BigInt(prepared.assessmentProposalRevision),
        action: ZapIdSchema.parse("plan.lower"),
        assessment_id: prepared.assessmentId,
        alternative_id: prepared.alternativeId,
        source_assessment_digest: prepared.sourceAssessmentDigest,
        assessment_digest: assessment.value.digest,
        relevant_basis: compared.value.relevant_basis,
        comparison,
        product,
        decision_id: input.decisionId,
        exception_id: null,
      },
      productDigest,
    }),
  };
}

function validPrefix(prepared: PreparedSuccessor, input: PrepareEffectInput): boolean {
  return (
    Number.isInteger(input.effectIndex) &&
    input.effectIndex >= 0 &&
    input.effectIndex < prepared.effects.length &&
    input.completedPrefix.length === input.effectIndex &&
    input.completedPrefix.every(
      (effectId: ZapId, index) => effectId === prepared.effects[index]?.effectId,
    )
  );
}
