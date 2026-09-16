/** Composite successor candidate preparation. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#metadata */
import { ZapIdSchema, canonicalQueryInput } from "../zap-client/index.ts";
import {
  PreparedCompositePlanProposalSchema,
  PreparedMilestonePrecursorsSchema,
  SuccessorPlanAuthoringInputSchema,
} from "./schemas.ts";
import type {
  AuthoringContext,
  PlanAuthoringOptions,
  PlanAuthoringResult,
  PreparedCompositePlanProposal,
  PreparedMilestonePrecursors,
  MetadataReceipt,
  PreparedAssessmentProposal,
  SuccessorPlanAuthoringInput,
} from "./types.ts";
import { derivedId, fail } from "./wire.ts";
import { buildAssessment, decodeCanonical, decodeSuccessorInput } from "./metadata.ts";

export async function prepareCompositeSuccessor(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawInput: SuccessorPlanAuthoringInput,
  rawPrecursors: PreparedMilestonePrecursors,
): Promise<PlanAuthoringResult<PreparedCompositePlanProposal>> {
  const input = SuccessorPlanAuthoringInputSchema.safeParse(rawInput);
  const precursors = PreparedMilestonePrecursorsSchema.safeParse(rawPrecursors);
  if (!input.success || !precursors.success) {
    return fail("invalid_input", "Composite successor input is invalid");
  }
  if (
    input.data.operationId !== precursors.data.operationId ||
    !sameBasis(input.data.intentBasis, precursors.data.intentBasis) ||
    !sameBasis(input.data.intentBasis, context.intentBasis)
  ) {
    return fail("stale_basis", "Composite successor stages do not share one intent basis");
  }
  if (
    precursors.data.changes.some(
      (change) => !input.data.plan.content.milestone_revision_ids.includes(change.revisionId),
    )
  ) {
    return fail("invalid_input", "Successor plan omits a prepared milestone revision");
  }
  const result = await options.reader.prepareCompositeSuccessor({
    operation_id: input.data.operationId,
    store: context.store,
    expected_revision: BigInt(context.intentBasis.revision),
    precursors: {
      alternative_id: derivedId(input.data.operationId, "alternative"),
      committed_prefix: [],
      effects: precursors.data.changes.map((change) => change.effect),
      no_op_basis: null,
    },
    plan_intent: canonicalQueryInput({
      key: input.data.plan.key,
      previous: input.data.plan.previous,
      strategic_revision_id: input.data.plan.strategic_revision_id,
      strategic_record_revision: input.data.plan.strategic_record_revision,
      strategic_semantic_digest: input.data.plan.strategic_semantic_digest,
      outcome_revision: input.data.plan.outcome_revision,
      expected_plan_state_revision: input.data.plan.expected_plan_state_revision,
      content: input.data.plan.content,
    }),
  });
  if (!result.ok) {
    const detail =
      result.error.kind === "http_refusal"
        ? `${result.error.refusal.code}: ${result.error.refusal.message}`
        : result.error.kind;
    return fail(
      result.error.kind === "http_refusal" ? "refused" : "unavailable",
      `Composite successor preparation failed: ${detail}`,
    );
  }
  return {
    ok: true,
    value: PreparedCompositePlanProposalSchema.parse({
      intentBasis: context.intentBasis,
      operationId: input.data.operationId,
      semanticInput: canonicalQueryInput(input.data),
      planPayload: result.value.plan,
      precursors: precursors.data,
      composite: result.value,
    }),
  };
}

export async function prepareCompositeAssessment(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawComposite: PreparedCompositePlanProposal,
  planReceipt: MetadataReceipt,
): Promise<PlanAuthoringResult<PreparedAssessmentProposal>> {
  const parsed = PreparedCompositePlanProposalSchema.safeParse(rawComposite);
  if (!parsed.success || !receiptMatches(parsed.data, planReceipt)) {
    return fail("invalid_input", "Composite candidate receipt does not match its preparation");
  }
  if (context.intentBasis.revision !== planReceipt.revision) {
    return fail("stale_basis", "Composite candidate receipt is not the current metadata revision");
  }
  const input = decodeSuccessorInput(parsed.data.semanticInput);
  const plan = decodeCanonical(parsed.data.planPayload);
  if (!input.ok) return input;
  if (!plan.ok) return plan;
  const operationId = parsed.data.operationId;
  const adoptionIndex = parsed.data.precursors.changes.length;
  const adoptionIds = {
    effectId: derivedId(operationId, `effect-${String(adoptionIndex)}`),
    productCommandId: derivedId(operationId, `product-command-${String(adoptionIndex)}`),
    productEventId: derivedId(operationId, `product-event-${String(adoptionIndex)}`),
  };
  const priorEffect = parsed.data.precursors.changes[adoptionIndex - 1]?.effect.effect_id;
  const effects = [
    ...parsed.data.precursors.changes.map((change) => change.effect),
    {
      effect_id: adoptionIds.effectId,
      index: adoptionIndex,
      kind: ZapIdSchema.parse("milestone.plan-adopted"),
      payload: mutableQuery({
        schema: "zap-domain/milestone-plan-adopted/1",
        plan: plan.value,
        expected_plan_state_revision: input.value.plan.expected_plan_state_revision,
      }),
      predecessors: priorEffect === undefined ? [] : [priorEffect],
      product_event_id: adoptionIds.productEventId,
    },
  ];
  const comparison = await options.reader.prepareComparison({
    at: { kind: "current" },
    actor: null,
    draft: {
      assessment_id: derivedId(operationId, "assessment"),
      alternatives: [
        {
          alternative_id: derivedId(operationId, "alternative"),
          committed_prefix: [],
          effects,
          no_op_basis: null,
        },
      ],
      policy: "required",
      capacity: "not_applicable",
      closure: "known_graph",
    },
  });
  if (!comparison.ok) {
    return fail("refused", `Composite comparison preparation failed: ${comparison.error.kind}`);
  }
  return buildAssessment(
    context,
    parsed.data,
    input.value,
    plan.value,
    planReceipt,
    comparison.value,
    {
      assessmentId: derivedId(operationId, "assessment"),
      alternativeId: derivedId(operationId, "alternative"),
      effects: [
        ...parsed.data.precursors.changes.map((change) => ({
          effectId: change.effect.effect_id,
          productCommandId: change.productCommandId,
          productEventId: change.effect.product_event_id,
        })),
        adoptionIds,
      ],
    },
  );
}

function sameBasis(
  left: SuccessorPlanAuthoringInput["intentBasis"],
  right: AuthoringContext["intentBasis"],
): boolean {
  return (
    left.storeRef === right.storeRef &&
    left.baseRef === right.baseRef &&
    left.revision === right.revision &&
    left.sourceBasisRef === right.sourceBasisRef
  );
}

function receiptMatches(
  prepared: PreparedCompositePlanProposal,
  receipt: MetadataReceipt,
): boolean {
  return (
    prepared.composite.reconciliation.command_id === receipt.commandId &&
    prepared.composite.reconciliation.command_digest === receipt.commandDigest
  );
}

function mutableQuery(value: Parameters<typeof canonicalQueryInput>[0]) {
  const encoded = canonicalQueryInput(value);
  return { codec: encoded.codec, canonical_json: [...encoded.canonical_json] };
}
