/** @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#context */
import { PlanBasisSchema } from "../quicklens-model/index.ts";
import { z } from "zod";
import {
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
  type ZapClient,
} from "../zap-client/index.ts";
import type { SpecificationWatch } from "../specification-watch/index.ts";
import { AuthoringContextSchema, EconomicsContextViewSchema } from "./schemas.ts";
import type { AuthoringContext, PlanAuthoringResult } from "./types.ts";
import { fail } from "./wire.ts";

const REQUIRED_QUERY_IDS: readonly string[] = [
  "zap.planning.active-context.v1",
  "zap.economics.active-context.v1",
  "zap.map.overview.v1",
];
const StrategyBindingSchema = z.looseObject({
  strategy_revision: U64WireSchema,
  strategy_semantic_digest: ZapDigestSchema,
});

export async function discover(
  reader: ZapClient,
  specifications: SpecificationWatch,
): Promise<PlanAuthoringResult<AuthoringContext>> {
  const [capabilities, active, specification, economics] = await Promise.all([
    reader.capabilities(),
    reader.activeContext(),
    specifications.capture(),
    reader.query(
      ZapIdSchema.parse("zap.economics.active-context.v1"),
      { maximum_candidates: 64, maximum_records: 256 },
      EconomicsContextViewSchema,
    ),
  ]);
  if (!capabilities.ok || !active.ok || !economics.ok || !specification.ok) {
    return fail("unavailable", "Authoring context could not be read from configured public inputs");
  }
  if (
    REQUIRED_QUERY_IDS.some(
      (queryId) => !capabilities.value.query_ids.some((value) => value === queryId),
    ) ||
    !capabilities.value.command_operations.includes("advance_change_admission")
  ) {
    return fail("unavailable", "Configured ZAP service lacks plan-authoring capabilities");
  }
  const context = active.value.items[0];
  const economicsContext = economics.value.items[0];
  if (
    context === undefined ||
    economicsContext === undefined ||
    context.active_outcome.state !== "present" ||
    context.current_strategy.state !== "present" ||
    context.adopted_milestone_plan.state !== "present"
  ) {
    return fail("unavailable", "Active outcome, current strategy and adopted plan are required");
  }
  const strategy = await reader.query(
    ZapIdSchema.parse("zap.map.overview.v1"),
    {
      strategy_id: context.current_strategy.strategic_revision_id,
      filter: "all",
      cursor: null,
      limit: 8,
      operation_budget: 64,
    },
    StrategyBindingSchema,
  );
  const strategyBinding = strategy.ok ? strategy.value.items[0] : undefined;
  if (strategyBinding === undefined) {
    return fail("unavailable", "Current strategy binding could not be read");
  }
  if (economicsContext.baseline_selection.state === "absent") {
    return fail("economics_context_unavailable", "No applicable economics baseline is available");
  }
  if (economicsContext.baseline_selection.state === "ambiguous") {
    return fail("ambiguous_baseline", "Economics baseline selection is ambiguous");
  }
  if (
    economics.value.store.store_id !== active.value.store.store_id ||
    economics.value.store.campaign_id !== active.value.store.campaign_id ||
    economics.value.store.base_id !== active.value.store.base_id ||
    economics.value.revision !== active.value.revision
  ) {
    return fail("stale_basis", "Planning and economics contexts came from different snapshots");
  }
  const intentBasis = PlanBasisSchema.parse({
    storeRef: `store:${active.value.store.store_id}`,
    baseRef: `base:${active.value.store.base_id}`,
    revision: active.value.revision,
    sourceBasisRef: `source-basis:${specification.value.digest}`,
  });
  return {
    ok: true,
    value: AuthoringContextSchema.parse({
      intentBasis,
      store: active.value.store,
      activeOutcomeId: context.active_outcome.outcome_id,
      activeOutcomeRevision: context.active_outcome.record_revision,
      currentStrategyId: context.current_strategy.strategic_revision_id,
      currentStrategyRecordRevision: strategyBinding.strategy_revision,
      currentStrategySemanticDigest: strategyBinding.strategy_semantic_digest,
      adoptedPlan: {
        outcome_id: context.adopted_milestone_plan.plan_key.outcome_id,
        generation: BigInt(context.adopted_milestone_plan.plan_key.generation),
      },
      adoptedPlanStateRevision: BigInt(context.adopted_milestone_plan.plan_state_revision),
      specificationFileCount: specification.value.fileCount,
      economics: economicsContext,
    }),
  };
}
