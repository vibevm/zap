/** Typed milestone precursor preparation. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#effects */
import { ZapIdSchema, canonicalQueryInput } from "../zap-client/index.ts";
import {
  MilestonePrecursorAuthoringInputSchema,
  MilestoneReadBindingSchema,
  PreparedMilestonePrecursorsSchema,
} from "./schemas.ts";
import type {
  AuthoringContext,
  MilestonePrecursorAuthoringInput,
  PlanAuthoringOptions,
  PlanAuthoringResult,
  PreparedMilestonePrecursors,
} from "./types.ts";
import { derivedId, fail } from "./wire.ts";

export async function prepareMilestonePrecursors(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawInput: MilestonePrecursorAuthoringInput,
): Promise<PlanAuthoringResult<PreparedMilestonePrecursors>> {
  const parsed = MilestonePrecursorAuthoringInputSchema.safeParse(rawInput);
  if (!parsed.success || !sameBasis(parsed.data.intentBasis, context.intentBasis)) {
    return fail("stale_basis", "Milestone precursor input does not bind the current intent basis");
  }
  const prepared: PreparedMilestonePrecursors["changes"][number][] = [];
  for (const [index, change] of parsed.data.changes.entries()) {
    const effectId = derivedId(parsed.data.operationId, `effect-${String(index)}`);
    const predecessors =
      index === 0 ? [] : [derivedId(parsed.data.operationId, `effect-${String(index - 1)}`)];
    const productEventId = derivedId(parsed.data.operationId, `product-event-${String(index)}`);
    const productCommandId = derivedId(parsed.data.operationId, `product-command-${String(index)}`);
    if (change.kind === "create") {
      const milestoneId = derivedId(parsed.data.operationId, `milestone-${String(index)}`);
      const revisionId = derivedId(parsed.data.operationId, `milestone-revision-${String(index)}`);
      prepared.push({
        kind: change.kind,
        milestoneId,
        revisionId,
        productCommandId,
        effect: {
          effect_id: effectId,
          index,
          kind: ZapIdSchema.parse("milestone.created"),
          payload: mutableQuery({
            schema: "zap-domain/milestone-created/1",
            milestone_id: milestoneId,
            revision_id: revisionId,
            affected_work_ids: change.affected_work_ids,
            definition: bindDefinition(context, change.definition),
          }),
          predecessors,
          product_event_id: productEventId,
        },
      });
      continue;
    }
    const current = await options.reader.query(
      ZapIdSchema.parse("zap.milestone.read"),
      { milestone_id: change.milestone_id, evaluate_current_achievement: false },
      MilestoneReadBindingSchema,
    );
    if (!current.ok) {
      return fail("stale_basis", "Milestone revision head could not be read");
    }
    const binding = current.value.items[0];
    if (
      binding === undefined ||
      String(current.value.revision) !== context.intentBasis.revision ||
      binding.head.milestone_id !== change.milestone_id
    ) {
      return fail("stale_basis", "Milestone revision head could not be bound to the intent basis");
    }
    const revisionId = derivedId(parsed.data.operationId, `milestone-revision-${String(index)}`);
    prepared.push({
      kind: change.kind,
      milestoneId: change.milestone_id,
      revisionId,
      productCommandId,
      effect: {
        effect_id: effectId,
        index,
        kind: ZapIdSchema.parse("milestone.revised"),
        payload: mutableQuery({
          schema: "zap-domain/milestone-revised/1",
          milestone_id: change.milestone_id,
          revision_id: revisionId,
          expected_head_revision: BigInt(binding.head.revision),
          expected_current_revision_id: binding.head.current_revision_id,
          expected_current_fingerprint: binding.current_revision.semantic_fingerprint,
          affected_work_ids: change.affected_work_ids,
          definition: bindDefinition(context, change.definition),
          conservation: {
            retained_obligation_ids: binding.current_revision.definition.required_obligation_ids,
            retained_consumers: binding.current_revision.definition.consumers,
            retained_contributions: binding.current_revision.definition.contributions,
            retained_dependencies: binding.current_revision.definition.dependencies,
            reason: change.conservation_reason,
          },
        }),
        predecessors,
        product_event_id: productEventId,
      },
    });
  }
  return {
    ok: true,
    value: PreparedMilestonePrecursorsSchema.parse({
      intentBasis: parsed.data.intentBasis,
      operationId: parsed.data.operationId,
      changes: prepared,
    }),
  };
}

function mutableQuery(value: Parameters<typeof canonicalQueryInput>[0]) {
  const encoded = canonicalQueryInput(value);
  return { codec: encoded.codec, canonical_json: [...encoded.canonical_json] };
}

function bindDefinition(
  context: AuthoringContext,
  definition: MilestonePrecursorAuthoringInput["changes"][number]["definition"],
) {
  return {
    strategic_revision_id: context.currentStrategyId,
    strategic_record_revision: BigInt(context.currentStrategyRecordRevision),
    strategic_semantic_digest: context.currentStrategySemanticDigest,
    outcome_id: context.activeOutcomeId,
    outcome_revision: BigInt(context.activeOutcomeRevision),
    ...definition,
  };
}

function sameBasis(
  left: MilestonePrecursorAuthoringInput["intentBasis"],
  right: AuthoringContext["intentBasis"],
): boolean {
  return (
    left.storeRef === right.storeRef &&
    left.baseRef === right.baseRef &&
    left.revision === right.revision &&
    left.sourceBasisRef === right.sourceBasisRef
  );
}
