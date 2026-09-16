/** @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map */
import { z } from "zod";
import {
  ExactDecimalSchema,
  QuicklensRefSchema,
  type Metric,
  type GraphNavigation,
  type PlanningMetrics,
  type QuestionView,
  type SemanticObject,
  type SemanticRelationship,
} from "../quicklens-model/index.ts";
import type { QuestionViewRecord } from "../protocol/index.ts";
import {
  MapObjectRefSchema,
  type MapObjectRef,
  type MilestonePlanView,
  type SemanticCardWire,
} from "./wire.ts";

const ProjectionIdSchema = z
  .string()
  .min(1)
  .max(1_024)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);
const PlanProjectionSchema = z.looseObject({
  strategic_revision_id: ProjectionIdSchema,
  key: z.looseObject({ outcome_id: ProjectionIdSchema, generation: ProjectionIdSchema }).optional(),
  content: z.looseObject({
    admission_work_ids: z.array(ProjectionIdSchema).max(100),
    milestone_revision_ids: z.array(ProjectionIdSchema).max(200),
    frontier_milestone_revision_ids: z.array(ProjectionIdSchema).max(200),
    focus_milestone_revision_id: ProjectionIdSchema.nullable(),
  }),
});
const FocusProjectionSchema = z.looseObject({
  milestone_id: ProjectionIdSchema,
  revision_id: ProjectionIdSchema,
  definition: z.looseObject({
    name: z.string().min(1).max(4_096),
    purpose: z.string().max(4_096),
    result_criterion: z.string().max(4_096),
    lifecycle: z.enum(["active", "retired"]),
  }),
});

export function mapCard(card: SemanticCardWire): SemanticObject {
  const acceptance = acceptanceView(card.acceptance);
  return {
    ref: mapObjectRef(card.object),
    category: category(card.semantic_type),
    semanticType: card.semantic_type,
    title: card.canonical_name,
    description: textValue(card.description),
    purpose: textValue(card.purpose),
    expectedResult: textValue(card.expected_result),
    acceptance: acceptance.summary,
    reasons: card.reasons,
    blockers: blockers(card.blockers),
    blockerSummary: blockerSummary(card.blockers),
    status: {
      code: `${card.semantic_type}:${acceptance.code}`,
      label: acceptance.label,
      tone: acceptance.tone,
    },
    metrics: metrics(card),
    provenance: [
      {
        sourceType: "zap.semantic_card",
        label: `Observed at revision ${card.observation_revision.toString()}`,
        detail: sourceStateDetail(card.source_state),
      },
      ...assessmentProvenance(card),
      ...card.sources.map((source) => ({
        sourceType: "zap.source",
        label: source,
        detail: null,
      })),
      ...card.evidence.map((evidence) => ({
        sourceType: "zap.evidence",
        label: evidence,
        detail: null,
      })),
    ].slice(0, 100),
    position: null,
  };
}

export function mapRelationships(card: SemanticCardWire): SemanticRelationship[] {
  return card.relationships.map((relationship) => ({
    ref: QuicklensRefSchema.parse(`relationship:${relationship.id}`),
    source: mapObjectRef(relationship.from),
    target: mapObjectRef(relationship.to),
    semanticType: relationship.kind,
    label: label(relationship.kind),
    status: null,
    provenance: [
      {
        sourceType: "zap.relationship",
        label: relationship.kind,
        detail: relationshipSourceDetail(relationship.source),
      },
    ],
  }));
}

export function mapQuestion(row: QuestionViewRecord): QuestionView {
  return {
    ref: QuicklensRefSchema.parse(`question:${row.question.questionId}`),
    addressedActorLabel: row.addressedActorLabel,
    prompt: row.question.prompt,
    state: row.question.state === "open" ? "pending" : row.question.state,
    revision: ExactDecimalSchema.parse(row.question.revision),
    answerMode: row.question.answerMode,
    choices: row.question.choices,
    answer:
      row.question.answer === null
        ? null
        : typeof row.question.answer === "string"
          ? row.question.answer
          : safeJson(row.question.answer, 16_384),
    amendmentCount: ExactDecimalSchema.parse(row.amendmentCount),
  };
}

export function mapObjectRef(value: MapObjectRef) {
  switch (value.kind) {
    case "viewer":
      return QuicklensRefSchema.parse(`${value.id.kind}:${value.id.id}`);
    case "strategic_fork":
      return QuicklensRefSchema.parse(`strategic_fork:${value.strategy_id}:${value.fork_id}`);
    default:
      return QuicklensRefSchema.parse(`${value.kind}:${value.id}`);
  }
}

export interface MilestonePlanProjection {
  readonly strategyId: string | null;
  readonly memberRevisionIds: readonly string[];
  readonly queryObjects: readonly MapObjectRef[];
  readonly objects: readonly SemanticObject[];
  readonly navigation: Pick<
    GraphNavigation,
    "adoptedPlanRef" | "members" | "focusRef" | "currentWorkRefs"
  >;
}

/** @implements spec://org.vibevm.zap/lens/PROP-002#semantic-map */
export function mapMilestonePlanProjection(plan: MilestonePlanView): MilestonePlanProjection {
  const record = PlanProjectionSchema.safeParse(plan.plan);
  const focus = FocusProjectionSchema.safeParse(plan.focus);
  const planned = new Set<string>(record.success ? record.data.content.milestone_revision_ids : []);
  const frontier = new Set<string>(
    record.success ? record.data.content.frontier_milestone_revision_ids : [],
  );
  const unsatisfied = new Set<string>(plan.unsatisfied_milestone_revision_ids);
  for (const revision of frontier) planned.add(revision);
  for (const revision of unsatisfied) planned.add(revision);
  if (focus.success) planned.add(focus.data.revision_id);

  const objects = [...planned].map((revisionId) =>
    focus.success && focus.data.revision_id === revisionId
      ? focusMilestone(focus.data, unsatisfied.has(revisionId), frontier.has(revisionId))
      : milestonePlaceholder(revisionId, unsatisfied.has(revisionId), frontier.has(revisionId)),
  );
  const queryObjects: MapObjectRef[] = [];
  queryObjects.push(
    MapObjectRefSchema.parse({ kind: "viewer", id: { kind: "outcome", id: plan.outcome_id } }),
  );
  if (focus.success) {
    queryObjects.push(MapObjectRefSchema.parse({ kind: "milestone", id: focus.data.milestone_id }));
  }
  if (record.success) {
    for (const workId of record.data.content.admission_work_ids) {
      queryObjects.push(
        MapObjectRefSchema.parse({ kind: "viewer", id: { kind: "work", id: workId } }),
      );
    }
  }
  return {
    strategyId: record.success ? record.data.strategic_revision_id : null,
    memberRevisionIds: [...planned],
    queryObjects: [...new Map(queryObjects.map((value) => [mapObjectRef(value), value])).values()],
    objects,
    navigation: {
      adoptedPlanRef:
        record.success && record.data.key !== undefined
          ? QuicklensRefSchema.parse(
              `milestone_plan:${record.data.key.outcome_id}:${record.data.key.generation}`,
            )
          : null,
      members: [],
      focusRef: focus.success
        ? QuicklensRefSchema.parse(`milestone:${focus.data.milestone_id}`)
        : null,
      currentWorkRefs: [],
    },
  };
}

type FocusProjection = z.infer<typeof FocusProjectionSchema>;

function focusMilestone(
  focus: FocusProjection,
  unsatisfied: boolean,
  frontier: boolean,
): SemanticObject {
  return {
    ref: QuicklensRefSchema.parse(`milestone:${focus.milestone_id}`),
    category: "milestone",
    semanticType: "milestone",
    title: focus.definition.name,
    description: "Current focus milestone from the adopted milestone plan.",
    purpose: focus.definition.purpose,
    expectedResult: focus.definition.result_criterion,
    acceptance: `Result criterion: ${focus.definition.result_criterion}`,
    reasons: milestoneReasons(true, unsatisfied, frontier),
    blockers: [],
    blockerSummary: "Readiness and blocker evaluation require the semantic-card query.",
    status: milestonePlanStatus(focus.definition.lifecycle, unsatisfied, frontier),
    metrics: unknownMetrics("No descriptive assessment was included in the milestone-plan query."),
    provenance: [
      {
        sourceType: "zap.milestone_plan",
        label: "Current milestone-plan focus",
        detail: `Lifecycle: ${label(focus.definition.lifecycle)}.`,
      },
    ],
    position: null,
  };
}

function milestonePlaceholder(
  revisionId: string,
  unsatisfied: boolean,
  frontier: boolean,
): SemanticObject {
  return {
    ref: QuicklensRefSchema.parse(`milestone_revision:${revisionId}`),
    category: "milestone",
    semanticType: "milestone_revision",
    title: unsatisfied
      ? "Unsatisfied milestone"
      : frontier
        ? "Frontier milestone"
        : "Planned milestone",
    description:
      "Milestone revision referenced by the adopted plan; its definition is not loaded yet.",
    purpose: null,
    expectedResult: null,
    acceptance: unsatisfied ? "The current plan reports this milestone as unsatisfied." : null,
    reasons: milestoneReasons(false, unsatisfied, frontier),
    blockers: [],
    blockerSummary: "Load the typed milestone card to evaluate blockers.",
    status: milestonePlanStatus("active", unsatisfied, frontier),
    metrics: unknownMetrics("No descriptive assessment was included in the milestone-plan query."),
    provenance: [
      {
        sourceType: "zap.milestone_plan",
        label: "Adopted milestone-plan reference",
        detail:
          "The stable revision identity is retained for hydration without displaying it as a title.",
      },
    ],
    position: null,
  };
}

function milestoneReasons(focus: boolean, unsatisfied: boolean, frontier: boolean): string[] {
  return [
    ...(focus ? ["This milestone is the current plan focus."] : []),
    ...(frontier ? ["This milestone is on the current plan frontier."] : []),
    ...(unsatisfied ? ["Current achievement evidence does not yet satisfy this milestone."] : []),
  ];
}

function milestonePlanStatus(
  lifecycle: "active" | "retired",
  unsatisfied: boolean,
  frontier: boolean,
): SemanticObject["status"] {
  if (lifecycle === "retired") return { code: "retired", label: "Retired", tone: "neutral" };
  if (unsatisfied) return { code: "unsatisfied", label: "Unsatisfied", tone: "warning" };
  if (frontier) return { code: "frontier", label: "Frontier", tone: "active" };
  return { code: "planned", label: "Planned", tone: "neutral" };
}

function category(value: string): "task" | "milestone" | "resource" | "other" {
  if (value === "work") return "task";
  if (value === "milestone") return "milestone";
  if (value === "execution_resource") return "resource";
  return "other";
}

function textValue(
  value: { state: "available"; value: string } | { state: "missing" },
): string | null {
  return value.state === "available" ? value.value : null;
}

type Acceptance = SemanticCardWire["acceptance"];
type StatusTone = SemanticObject["status"]["tone"];

function acceptanceView(value: Acceptance): {
  readonly code: string;
  readonly label: string;
  readonly tone: StatusTone;
  readonly summary: string;
} {
  switch (value.kind) {
    case "strategy_state":
      return status(value.state, `Strategy is ${label(value.state).toLocaleLowerCase()}.`);
    case "work_record_state": {
      const criteria = value.declared_criteria.length
        ? ` Criteria: ${value.declared_criteria.join(" · ")}`
        : " No declared criteria were supplied.";
      return status(
        value.state,
        bounded(`Work is ${label(value.state).toLocaleLowerCase()}.${criteria}`, 16_384),
      );
    }
    case "strategic_node_only":
      return status(
        "strategic_node_only",
        "Only the strategic node is available; no materialized work acceptance exists yet.",
      );
    case "obligation_state":
      return status(
        value.status,
        `Obligation is ${label(value.status).toLocaleLowerCase()} with ${label(value.disposition).toLocaleLowerCase()} disposition.`,
      );
    case "outcome_state":
      return status(value.status, `Outcome is ${label(value.status).toLocaleLowerCase()}.`);
    case "milestone_state":
      return status(
        value.lifecycle,
        value.latest_achievement_id === null
          ? `Milestone is ${value.lifecycle}; no achievement is recorded.`
          : `Milestone is ${value.lifecycle}; a latest achievement is recorded.`,
      );
    case "information_opportunity_state":
      return status(
        value.freshness,
        value.selected_work_id === null
          ? `Information opportunity is ${label(value.freshness).toLocaleLowerCase()} and has no selected work.`
          : `Information opportunity is ${label(value.freshness).toLocaleLowerCase()} with selected work.`,
      );
    case "not_applicable":
      return {
        code: "not_applicable",
        label: "Not applicable",
        tone: "neutral",
        summary: value.reason,
      };
  }
}

function status(code: string, summary: string) {
  return { code, label: label(code), tone: tone(code), summary };
}

function metrics(card: SemanticCardWire): PlanningMetrics {
  const unavailable = assessmentUnavailable(card);
  const assessment = card.assessment;
  if (unavailable !== null || assessment === null)
    return unknownMetrics(unavailable ?? "Assessment record is absent.");
  const content = assessment.record.content;
  const freshness = assessment.freshness === "stale" ? "Stale assessment. " : "";
  return {
    complexity: gradeMetric(content.complexity.grade, content.complexity.rationale, freshness),
    difficulty: gradeMetric(
      content.difficulty.grade,
      joined([
        content.difficulty.rationale,
        assumptions("Executor", content.difficulty.executor_assumptions),
        assumptions("Knowledge", content.difficulty.knowledge_assumptions),
      ]),
      freshness,
    ),
    effort: estimateMetric(
      content.remaining_agent_hours,
      freshness,
      content.remaining_elapsed === null
        ? null
        : `Elapsed range: ${hoursRange(content.remaining_elapsed.range)} hours.`,
    ),
    waiting: estimateMetric(content.remaining_passive_wait, freshness, null),
    uncertainty: confidenceMetric(
      content.uncertainty.confidence,
      joined([
        content.uncertainty.rationale,
        assumptions("Unknowns", content.uncertainty.unknowns),
      ]),
      freshness,
    ),
  };
}

function unknownMetrics(reason: string): PlanningMetrics {
  const boundedReason = bounded(reason, 512);
  return {
    complexity: { state: "unknown", reason: boundedReason },
    difficulty: { state: "unknown", reason: boundedReason },
    effort: { state: "unknown", reason: boundedReason },
    waiting: { state: "unknown", reason: boundedReason },
    uncertainty: { state: "unknown", reason: boundedReason },
  };
}

function assessmentUnavailable(card: SemanticCardWire): string | null {
  if (card.assessment_state.state === "unavailable") return card.assessment_state.reason;
  if (card.assessment_state.state === "not_applicable")
    return "A descriptive work assessment does not apply to this object.";
  return card.assessment === null
    ? "Assessment availability was reported without its record."
    : null;
}

type Assessment = NonNullable<SemanticCardWire["assessment"]>;
type Estimate = NonNullable<Assessment["record"]["content"]["remaining_agent_hours"]>;

function gradeMetric(grade: string, rationale: string | null, freshness: string): Metric {
  return grade === "unassessed"
    ? { state: "unknown", reason: "This dimension is unassessed." }
    : {
        state: "known",
        value: label(grade),
        unit: null,
        explanation: bounded(`${freshness}${rationale ?? ""}`, 4_096) || null,
      };
}

function confidenceMetric(confidence: string, rationale: string | null, freshness: string): Metric {
  return confidence === "unassessed"
    ? { state: "unknown", reason: "Uncertainty confidence is unassessed." }
    : {
        state: "known",
        value: label(confidence),
        unit: null,
        explanation: bounded(`${freshness}${rationale ?? ""}`, 4_096) || null,
      };
}

function estimateMetric(
  estimate: Estimate | null,
  freshness: string,
  extra: string | null,
): Metric {
  if (estimate === null) return { state: "unknown", reason: "No current estimate was supplied." };
  return {
    state: "known",
    value: hoursRange(estimate.range),
    unit: "hours",
    explanation: bounded(
      joined([
        `${freshness}${label(estimate.precision)} estimate from ${estimate.source}.`,
        assumptions("Assumptions", estimate.assumptions),
        extra,
      ]) ?? "",
      4_096,
    ),
  };
}

function hoursRange(range: Estimate["range"]): string {
  const low = microHours(range.low);
  return range.high === null
    ? `at least ${low}`
    : range.high === range.low
      ? low
      : `${low}–${microHours(range.high)}`;
}

function microHours(value: string): string {
  const exact = BigInt(value);
  const whole = exact / 1_000_000n;
  const remainder = exact % 1_000_000n;
  if (remainder === 0n) return whole.toString();
  return `${whole.toString()}.${remainder.toString().padStart(6, "0").replace(/0+$/u, "")}`;
}

function blockers(value: SemanticCardWire["blockers"]): SemanticObject["blockers"] {
  return value.state === "established"
    ? value.blockers.map((blocker) => ({
        ref: mapObjectRef(blocker),
        fallbackLabel: `${mapObjectKind(blocker)} blocker`,
      }))
    : [];
}

function blockerSummary(value: SemanticCardWire["blockers"]): string {
  if (value.state === "not_evaluated") return `Not evaluated: ${value.reason}`;
  if (value.state === "not_applicable") return `Not applicable: ${value.reason}`;
  return value.blockers.length === 0
    ? "No blockers are established."
    : `${String(value.blockers.length)} blocker${value.blockers.length === 1 ? "" : "s"} established.`;
}

function mapObjectKind(value: MapObjectRef): string {
  return label(value.kind === "viewer" ? value.id.kind : value.kind);
}

function sourceStateDetail(value: SemanticCardWire["source_state"]): string {
  if (value.state === "materialized")
    return `Materialized record at revision ${value.record_revision.toString()}.`;
  if (value.state === "strategic_node_only")
    return `Strategic node only at revision ${value.strategy_revision.toString()}.`;
  return bounded(`Reference only: ${value.reason}`, 2_000);
}

function assessmentProvenance(card: SemanticCardWire) {
  if (card.assessment === null) return [];
  return [
    {
      sourceType: "zap.work_assessment",
      label: `${label(card.assessment.freshness)} assessment`,
      detail:
        card.assessment.record.content.explanation === null
          ? null
          : bounded(card.assessment.record.content.explanation, 2_000),
    },
  ];
}

function relationshipSourceDetail(value: unknown): string {
  const source = z.looseObject({ kind: z.unknown(), revision: z.unknown() }).safeParse(value);
  if (!source.success) return "Typed ZAP relationship source.";
  const kind = typeof source.data.kind === "string" ? label(source.data.kind) : "Typed";
  const revision = source.data.revision;
  return typeof revision === "bigint" || typeof revision === "string"
    ? `${kind} source at revision ${revision.toString()}.`
    : `${kind} relationship source.`;
}

function safeJson(value: unknown, maximum: number): string {
  const encoded = JSON.stringify(value, (_key, candidate: unknown) =>
    typeof candidate === "bigint" ? candidate.toString() : candidate,
  );
  return bounded(encoded, maximum);
}

function assumptions(labelText: string, values: readonly string[]): string | null {
  return values.length === 0 ? null : `${labelText}: ${values.join(" · ")}.`;
}

function joined(values: readonly (string | null)[]): string | null {
  const present = values.filter((value): value is string => value !== null && value.length > 0);
  return present.length === 0 ? null : present.join(" ");
}

function bounded(value: string, maximum: number): string {
  return value.length <= maximum ? value : `${value.slice(0, Math.max(0, maximum - 1))}…`;
}

function label(value: string): string {
  return value.replaceAll("_", " ").replace(/\b\w/gu, (character) => character.toUpperCase());
}

function tone(value: string): "neutral" | "active" | "success" | "warning" | "danger" {
  if (["accepted", "completed", "current", "satisfied"].includes(value)) return "success";
  if (["active", "ready"].includes(value)) return "active";
  if (["blocked", "failed", "expired", "cancelled", "unattainable"].includes(value))
    return "danger";
  if (["held", "stale", "unknown", "deferred", "unsupported_decision_basis"].includes(value))
    return "warning";
  return "neutral";
}
