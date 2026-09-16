/** Bounded coherent map composition. @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map */
import { QuicklensRefSchema } from "../quicklens-model/index.ts";
import type {
  GraphNavigation,
  GraphPlanMember,
  QuicklensError,
  QuicklensResult,
  SemanticObject,
  SemanticRelationship,
} from "../quicklens-model/index.ts";
import {
  ZapIdSchema,
  type ActiveContextView,
  type ZapCapabilities,
  type ZapClient,
  type ZapClientError,
  type ZapClientResult,
  type ZapQueryPage,
} from "../zap-client/index.ts";
import { mapCard, mapMilestonePlanProjection, mapRelationships } from "./mapping.ts";
import {
  MapObjectResultSchema,
  MapObjectRefSchema,
  MapOverviewResultSchema,
  MilestonePlanViewSchema,
  MilestoneRevisionViewSchema,
  type MapObjectRef,
  type MapOverviewResult,
  type MilestonePlanView,
  type SemanticCardWire,
} from "./wire.ts";

export interface ReadMapOptions {
  readonly zap: ZapClient;
  readonly capabilities: ZapCapabilities;
  readonly activePage: ZapQueryPage<ActiveContextView>;
  readonly active: ActiveContextView;
  readonly signal: AbortSignal;
  readonly pageLimit: number;
  readonly maximumPages: number;
}

export async function readMap(options: ReadMapOptions): Promise<
  QuicklensResult<{
    objects: SemanticObject[];
    relationships: SemanticRelationship[];
    milestone: MilestonePlanView | null;
    navigation: GraphNavigation;
    partial: boolean;
  }>
> {
  const required = [
    "zap.map.overview.v1",
    "zap.map.object.v1",
    "zap.milestone.plan",
    "zap.milestone.revision",
  ];
  if (!required.every((id) => options.capabilities.query_ids.includes(ZapIdSchema.parse(id)))) {
    return fail("unavailable", "Configured ZAP service lacks required Quicklens queries");
  }
  const milestone = await readMilestone(options);
  if (!milestone.ok) return milestone;
  const projection =
    milestone.value === null
      ? {
          strategyId: null,
          queryObjects: [],
          memberRevisionIds: [],
          objects: [],
          navigation: {
            adoptedPlanRef: null,
            members: [],
            focusRef: null,
            currentWorkRefs: [],
          },
        }
      : mapMilestonePlanProjection(milestone.value);
  const strategyId =
    options.active.current_strategy.state === "present"
      ? options.active.current_strategy.strategic_revision_id
      : projection.strategyId;
  const members = await readMembers(projection.memberRevisionIds, options);
  if (!members.ok) return members;
  if (strategyId === null) {
    return {
      ok: true,
      value: {
        objects: [...projection.objects],
        relationships: [],
        milestone: milestone.value,
        navigation: navigation(
          options.active,
          { ...projection.navigation, members: members.value.members },
          true,
          members.value.reassessmentReason,
        ),
        partial: true,
      },
    };
  }
  const overviewCards: SemanticCardWire[] = [];
  let cursor: MapOverviewResult["next"] = null;
  for (
    let pageIndex = 0;
    options.active.current_strategy.state === "present" && pageIndex < options.maximumPages;
    pageIndex += 1
  ) {
    const overview: ZapClientResult<ZapQueryPage<MapOverviewResult>> = await options.zap.query(
      ZapIdSchema.parse("zap.map.overview.v1"),
      {
        strategy_id: strategyId,
        filter: "all",
        cursor: cursor === null ? null : overviewCursorInput(cursor),
        limit: options.pageLimit,
        operation_budget: options.pageLimit,
      },
      MapOverviewResultSchema,
      options.signal,
    );
    if (!overview.ok) return zapFail(overview.error);
    if (!sameRevision(overview.value, options.activePage)) {
      return fail("stale_basis", "map query crossed the active-context revision");
    }
    const result: MapOverviewResult | undefined = overview.value.items[0];
    if (result === undefined || result.through_revision !== options.activePage.revision) {
      return fail("stale_basis", "map result does not match active-context revision");
    }
    overviewCards.push(...result.cards);
    cursor = result.next;
    if (cursor === null) break;
  }
  const enriched = await enrichOverview(strategyId, overviewCards, options);
  if (!enriched.ok) return enriched;
  const hydrated = await hydrateReferenced(
    strategyId,
    enriched.value,
    [
      ...projection.queryObjects,
      ...members.value.objects,
      ...(options.active.active_outcome.state === "present"
        ? [
            MapObjectRefSchema.parse({
              kind: "viewer",
              id: { kind: "outcome", id: options.active.active_outcome.outcome_id },
            }),
          ]
        : []),
    ],
    options,
  );
  if (!hydrated.ok) return hydrated;
  const partial =
    cursor !== null ||
    hydrated.value.partial ||
    hydrated.value.cards.some((card) => !card.relationships_complete);
  const navigationPartial = partial || members.value.partial;
  return {
    ok: true,
    value: {
      objects: uniqueObjects([...hydrated.value.cards.map(mapCard), ...projection.objects]),
      relationships: uniqueRelationships(hydrated.value.cards.flatMap(mapRelationships)),
      milestone: milestone.value,
      navigation: navigation(
        options.active,
        { ...projection.navigation, members: members.value.members },
        navigationPartial,
        members.value.reassessmentReason,
      ),
      partial: navigationPartial,
    },
  };
}

async function readMembers(
  revisionIds: readonly string[],
  options: ReadMapOptions,
): Promise<
  QuicklensResult<{
    members: GraphPlanMember[];
    objects: MapObjectRef[];
    partial: boolean;
    reassessmentReason: string | null;
  }>
> {
  const bounded = revisionIds.slice(0, options.pageLimit);
  const members: GraphPlanMember[] = [];
  const objects: MapObjectRef[] = [];
  let partial = bounded.length !== revisionIds.length;
  let unresolved = bounded.length !== revisionIds.length;
  for (const revisionId of bounded) {
    const result = await options.zap.query(
      ZapIdSchema.parse("zap.milestone.revision"),
      { revision_id: revisionId },
      MilestoneRevisionViewSchema,
      options.signal,
    );
    if (!result.ok) {
      if (
        result.error.kind === "http_refusal" &&
        result.error.refusal.code === "missing_reference"
      ) {
        partial = true;
        unresolved = true;
        continue;
      }
      return zapFail(result.error);
    }
    const item = result.value.items[0];
    if (
      !sameRevision(result.value, options.activePage) ||
      item === undefined ||
      item.revision.revision_id !== revisionId
    ) {
      return fail("stale_basis", "milestone revision query crossed the active snapshot");
    }
    members.push({
      ref: QuicklensRefSchema.parse(`milestone:${item.revision.milestone_id}`),
      revisionRef: QuicklensRefSchema.parse(`milestone_revision:${item.revision.revision_id}`),
      isCurrent: item.is_current,
    });
    objects.push(MapObjectRefSchema.parse({ kind: "milestone", id: item.revision.milestone_id }));
    if (!item.is_current) partial = true;
  }
  return {
    ok: true,
    value: {
      members: [...new Map(members.map((member) => [member.revisionRef, member])).values()],
      objects,
      partial,
      reassessmentReason: members.some((member) => !member.isCurrent)
        ? "An adopted milestone revision is no longer the current milestone head."
        : unresolved
          ? "Some adopted milestone revisions could not be resolved within the bounded snapshot."
          : null,
    },
  };
}

function navigation(
  active: ActiveContextView,
  plan: ReturnType<typeof mapMilestonePlanProjection>["navigation"],
  partial: boolean,
  reassessmentReason: string | null,
): GraphNavigation {
  return {
    activeOutcomeRef:
      active.active_outcome.state === "present"
        ? QuicklensRefSchema.parse(`outcome:${active.active_outcome.outcome_id}`)
        : null,
    currentStrategyRef:
      active.current_strategy.state === "present"
        ? QuicklensRefSchema.parse(`strategy:${active.current_strategy.strategic_revision_id}`)
        : null,
    ...plan,
    dependencyDirection: "prerequisite_to_dependent",
    completeness: partial ? "partial" : "complete",
    reassessmentReason,
  };
}

async function enrichOverview(
  strategyId: string,
  cards: readonly SemanticCardWire[],
  options: ReadMapOptions,
): Promise<QuicklensResult<SemanticCardWire[]>> {
  const results: SemanticCardWire[] = [];
  for (const card of cards) {
    if (card.relationships_complete) {
      results.push(card);
      continue;
    }
    const detail = await readCard(strategyId, card.object, options);
    if (!detail.ok) return detail;
    if (detail.value !== null) results.push(detail.value);
  }
  return { ok: true, value: results };
}

async function hydrateReferenced(
  strategyId: string,
  initial: SemanticCardWire[],
  additional: readonly MapObjectRef[],
  options: ReadMapOptions,
): Promise<QuicklensResult<{ cards: SemanticCardWire[]; partial: boolean }>> {
  const present = new Set(initial.map((card) => refKey(card.object)));
  const candidates = new Map<string, MapObjectRef>();
  for (const reference of additional) {
    const key = refKey(reference);
    if (!present.has(key)) candidates.set(key, reference);
  }
  for (const relationship of initial.flatMap((card) => card.relationships)) {
    for (const reference of [relationship.from, relationship.to]) {
      const key = refKey(reference);
      if (!present.has(key)) candidates.set(key, reference);
    }
  }
  const bounded = [...candidates.values()].slice(0, options.pageLimit);
  let partial = candidates.size > bounded.length;
  const cards = [...initial];
  for (const reference of bounded) {
    const detail = await readCard(strategyId, reference, options);
    if (!detail.ok) {
      if (detail.error.code === "unavailable") {
        partial = true;
        continue;
      }
      return detail;
    }
    if (detail.value === null) partial = true;
    else cards.push(detail.value);
  }
  return { ok: true, value: { cards, partial } };
}

async function readCard(
  strategyId: string,
  object: MapObjectRef,
  options: ReadMapOptions,
): Promise<QuicklensResult<SemanticCardWire | null>> {
  const detail = await options.zap.query(
    ZapIdSchema.parse("zap.map.object.v1"),
    {
      strategy_id: strategyId,
      object,
      cursor: null,
      relationship_limit: options.pageLimit,
      operation_budget: options.pageLimit,
    },
    MapObjectResultSchema,
    options.signal,
  );
  if (!detail.ok) return zapFail(detail.error);
  const item = detail.value.items[0];
  if (
    !sameRevision(detail.value, options.activePage) ||
    item?.through_revision !== options.activePage.revision
  ) {
    return fail("stale_basis", "object query crossed the active-context revision");
  }
  return { ok: true, value: item.card };
}

async function readMilestone(
  options: ReadMapOptions,
): Promise<QuicklensResult<MilestonePlanView | null>> {
  if (options.active.active_outcome.state === "uninitialized") return { ok: true, value: null };
  const result = await options.zap.query(
    ZapIdSchema.parse("zap.milestone.plan"),
    {
      outcome_id: options.active.active_outcome.outcome_id,
      plan_key: null,
      maximum_milestones: options.pageLimit,
    },
    MilestonePlanViewSchema,
    options.signal,
  );
  if (!result.ok) return zapFail(result.error);
  return sameRevision(result.value, options.activePage)
    ? { ok: true, value: result.value.items[0] ?? null }
    : fail("stale_basis", "milestone query crossed the active-context revision");
}

function overviewCursorInput(cursor: MapOverviewResult["next"]) {
  if (cursor === null) return null;
  return {
    ...cursor,
    snapshot_revision: BigInt(cursor.snapshot_revision),
    strategy_revision: BigInt(cursor.strategy_revision),
  };
}

function sameRevision<T>(left: ZapQueryPage<T>, right: ZapQueryPage<unknown>): boolean {
  return (
    left.store.store_id === right.store.store_id &&
    left.store.base_id === right.store.base_id &&
    left.revision === right.revision
  );
}

function refKey(value: MapObjectRef): string {
  return JSON.stringify(value);
}

function uniqueRelationships(values: SemanticRelationship[]): SemanticRelationship[] {
  return [...new Map(values.map((value) => [value.ref, value])).values()];
}

function uniqueObjects(values: SemanticObject[]): SemanticObject[] {
  return [...new Map(values.map((value) => [value.ref, value])).values()];
}

function zapFail(value: ZapClientError): QuicklensResult<never> {
  if (value.kind === "http_refusal" && value.refusal.code === "missing_reference") {
    return fail("unavailable", value.refusal.message);
  }
  return fail(
    value.kind === "stale_context" ? "stale_basis" : "invalid_data",
    value.kind === "configuration" ? `ZAP configuration: ${value.message}` : `ZAP ${value.kind}`,
  );
}

function fail(code: QuicklensError["code"], message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: { code, message, recovery: "Refresh the exact ZAP snapshot and retry." },
  };
}
