/** Public successor-plan and assessment proposal sequence. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#metadata */
import { PlanBasisSchema } from "../quicklens-model/index.ts";
import { z } from "zod";
import {
  ZapDigestSchema,
  ZapIdSchema,
  canonicalQueryInput,
  encodeCanonicalJson,
  parseCanonicalJson,
  protectedCommandDigest,
  type CanonicalJsonInput,
  type PreparedEffectComparisonView,
  type QueryInputWire,
  type ZapId,
} from "../zap-client/index.ts";
import {
  PreparedAssessmentProposalSchema,
  PreparedPlanProposalSchema,
  PreparedSuccessorSchema,
  SuccessorPlanAuthoringInputSchema,
} from "./schemas.ts";
import type {
  AuthoringContext,
  MetadataReceipt,
  PlanAuthoringOptions,
  PlanAuthoringResult,
  PreparedAssessmentProposal,
  PreparedCommand,
  PreparedCompositePlanProposal,
  PreparedPlanProposal,
  PreparedSuccessor,
  SuccessorPlanAuthoringInput,
} from "./types.ts";
import {
  canonicalBytesDigest,
  canonicalDigest,
  canonicalValue,
  command,
  comparisonBasis,
  derivedId,
  fail,
  noOpBasis,
  preparedRequest,
} from "./wire.ts";

export async function prepareSuccessor(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  input: SuccessorPlanAuthoringInput,
): Promise<PlanAuthoringResult<PreparedPlanProposal>> {
  if (!sameBasis(input.intentBasis, context.intentBasis)) {
    return fail("stale_basis", "Successor input does not bind the discovered intent basis");
  }
  if (
    input.plan.key.outcome_id !== context.activeOutcomeId ||
    input.plan.previous.outcome_id !== context.adoptedPlan.outcome_id ||
    input.plan.previous.generation !== context.adoptedPlan.generation ||
    input.plan.strategic_revision_id !== context.currentStrategyId ||
    input.plan.expected_plan_state_revision !== context.adoptedPlanStateRevision
  ) {
    return fail("stale_basis", "Successor lineage differs from the active planning context");
  }
  const planBasis = await preparePlanBasis(options, context, input);
  if (!planBasis.ok) return planBasis;
  const planRevision = BigInt(context.intentBasis.revision) + 1n;
  const planWithoutFingerprint = {
    key: input.plan.key,
    previous: input.plan.previous,
    strategic_revision_id: input.plan.strategic_revision_id,
    strategic_record_revision: input.plan.strategic_record_revision,
    strategic_semantic_digest: input.plan.strategic_semantic_digest,
    outcome_revision: input.plan.outcome_revision,
    relevant_basis: planBasis.value,
    content: input.plan.content,
    revision: planRevision,
  };
  const plan = {
    ...planWithoutFingerprint,
    semantic_fingerprint: canonicalDigest([
      planWithoutFingerprint.key,
      planWithoutFingerprint.previous,
      planWithoutFingerprint.strategic_revision_id,
      planWithoutFingerprint.strategic_record_revision,
      planWithoutFingerprint.strategic_semantic_digest,
      planWithoutFingerprint.outcome_revision,
      planWithoutFingerprint.relevant_basis,
      planWithoutFingerprint.content,
    ]),
  };
  const planCommandId = derivedId(input.operationId, "plan-command");
  const planCommand = command(
    context.store,
    { commandId: planCommandId, eventId: derivedId(input.operationId, "plan-event") },
    BigInt(context.intentBasis.revision),
    ZapIdSchema.parse("milestone.plan-proposed"),
    { plan, schema: "zap-domain/milestone-plan-proposed/1" },
    { kind: "exact", digest: planBasis.value },
    "Propose one successor milestone plan",
    input.economics.change_id,
  );
  return {
    ok: true,
    value: PreparedPlanProposalSchema.parse({
      intentBasis: context.intentBasis,
      operationId: input.operationId,
      semanticInput: canonicalQueryInput(input),
      planPayload: canonicalQueryInput(plan),
      command: {
        stage: "plan_proposal",
        command: planCommand,
        commandDigest: ZapDigestSchema.parse(protectedCommandDigest(planCommand)),
      },
    }),
  };
}

export async function prepareAssessment(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawPlan: PreparedPlanProposal,
  planReceipt: MetadataReceipt,
): Promise<PlanAuthoringResult<PreparedAssessmentProposal>> {
  const parsed = PreparedPlanProposalSchema.safeParse(rawPlan);
  if (!parsed.success || !receiptMatches(parsed.data.command, planReceipt)) {
    return fail("invalid_input", "Plan proposal receipt does not match its durable command");
  }
  if (context.intentBasis.revision !== planReceipt.revision) {
    return fail("stale_basis", "Plan receipt is not the current metadata revision");
  }
  const input = decodeSuccessorInput(parsed.data.semanticInput);
  const plan = decodeCanonical(parsed.data.planPayload);
  if (!input.ok) return input;
  if (!plan.ok) return plan;
  const operationId = parsed.data.operationId;
  const adoptionIds = {
    effectId: derivedId(operationId, "effect-0"),
    productCommandId: derivedId(operationId, "product-command-0"),
    productEventId: derivedId(operationId, "product-event-0"),
  };
  const ids = {
    assessmentId: derivedId(operationId, "assessment"),
    alternativeId: derivedId(operationId, "alternative"),
    effects: [adoptionIds],
  };
  const adoptionPayload = {
    expected_plan_state_revision: input.value.plan.expected_plan_state_revision,
    plan: plan.value,
    schema: "zap-domain/milestone-plan-adopted/1",
  };
  const comparison = await options.reader.prepareComparison({
    at: { kind: "current" },
    actor: null,
    draft: {
      assessment_id: ids.assessmentId,
      alternatives: [
        {
          alternative_id: ids.alternativeId,
          committed_prefix: [],
          effects: [
            {
              effect_id: adoptionIds.effectId,
              index: 0,
              kind: ZapIdSchema.parse("milestone.plan-adopted"),
              payload: { codec: 2, canonical_json: [...encodeCanonicalJson(adoptionPayload)] },
              predecessors: [],
              product_event_id: adoptionIds.productEventId,
            },
          ],
          no_op_basis: null,
        },
      ],
      policy: "required",
      capacity: "not_applicable",
      closure: "known_graph",
    },
  });
  if (!comparison.ok) {
    return fail("refused", `Comparison preparation failed: ${comparison.error.kind}`);
  }
  return buildAssessment(
    context,
    parsed.data,
    input.value,
    plan.value,
    planReceipt,
    comparison.value,
    ids,
  );
}

export interface AssessmentIds {
  readonly assessmentId: ZapId;
  readonly alternativeId: ZapId;
  readonly effects: readonly {
    readonly effectId: ZapId;
    readonly productCommandId: ZapId;
    readonly productEventId: ZapId;
  }[];
}

export function buildAssessment(
  context: AuthoringContext,
  planProposal: PreparedPlanProposal | PreparedCompositePlanProposal,
  input: SuccessorPlanAuthoringInput,
  plan: CanonicalJsonInput,
  planReceipt: MetadataReceipt,
  comparison: PreparedEffectComparisonView,
  ids: AssessmentIds,
): PlanAuthoringResult<PreparedAssessmentProposal> {
  const request = preparedRequest(comparison);
  const basis = comparisonBasis(comparison.basis_request);
  const scope = comparison.affected_scopes[0];
  if (!request.ok) return request;
  if (!basis.ok) return basis;
  if (scope === undefined || request.value.effects.length !== ids.effects.length) {
    return fail("refused", "Backend preparation omitted exact successor effect metadata");
  }
  if (
    request.value.effects.some((effect, index) => effect.effect_id !== ids.effects[index]?.effectId)
  ) {
    return fail("refused", "Prepared successor effect identity changed");
  }
  const baseline = context.economics.baseline_selection;
  if (baseline.state !== "unique") {
    return fail("ambiguous_baseline", "A unique economics baseline is required");
  }
  const scopeRoots = sortedSubjects(
    request.value.effects.flatMap((value) => value.declared_subjects),
  );
  const directWork = scopeRoots
    .filter((subject) => subject.kind === "work")
    .map((subject) => subject.id);
  const proposalEffects = request.value.effects.map((effect) => ({
    effect_id: effect.effect_id,
    index: effect.index,
    kind: effect.kind,
    payload: effect.payload.canonical_json,
    payload_digest: canonicalBytesDigest(Uint8Array.from(effect.payload.canonical_json)),
    subjects: effect.declared_subjects,
    predecessors: effect.predecessors,
    basis: effect.basis,
    relevant_before: effect.relevant_before,
    relevant_after: effect.declared_relevant_after,
    product_event_id: effect.product_event_id,
    preflight_digest: null,
  }));
  const assessment = {
    assessment_id: ids.assessmentId,
    change_id: input.economics.change_id,
    baseline_id: baseline.baseline_id,
    summary: input.economics.summary,
    necessity: input.economics.necessity,
    affected_work_ids: scope.affected_work_ids,
    dependent_work_ids: scope.dependent_work_ids,
    affected_subjects: scope.subjects,
    scope_roots: scopeRoots,
    scope_direct_work_ids: directWork,
    affected_scope_digest: null,
    unknown_impact: scope.unknown_boundary,
    comparison_basis_request: basis.value,
    comparison_basis_digest: comparison.relevant_basis,
    policy_id: context.economics.policy.policy_id,
    policy_revision: BigInt(context.economics.policy.record_revision),
    policy_digest: context.economics.policy.digest,
    team_model: input.economics.team_model,
    alternatives: [
      {
        alternative_id: ids.alternativeId,
        kind: "proposal",
        ...input.economics.proposal,
        effects: proposalEffects,
        no_op_basis_request: null,
      },
      {
        alternative_id: derivedId(input.operationId, "no-op-alternative"),
        kind: "no_op",
        summary: input.economics.no_op.summary,
        solves_mandatory_problem: false,
        preserved_obligations: [],
        sacrificed_obligations: [],
        utility: input.economics.no_op.utility,
        cost: input.economics.no_op.cost,
        feasibility: input.economics.no_op.feasibility,
        effects: [],
        no_op_basis_request: basis.value,
        basis: input.economics.no_op.basis,
        evidence_refs: input.economics.no_op.evidence_refs,
      },
    ],
    recommended_alternative_id: ids.alternativeId,
    recommendation: "take_proposal",
    admission: "automatic",
    comparison_reasons: input.economics.comparison_reasons,
    estimation: input.economics.estimation,
    hold_id: null,
    adjudicated: false,
    resolved: false,
    revision: BigInt(planReceipt.revision) + 1n,
  };
  const assessmentCommandId = derivedId(input.operationId, "assessment-command");
  const assessmentCommand = command(
    context.store,
    {
      commandId: assessmentCommandId,
      eventId: derivedId(input.operationId, "assessment-event"),
    },
    BigInt(planReceipt.revision),
    ZapIdSchema.parse("economics.change-assessment-proposed"),
    { assessment },
    { kind: "exact", digest: comparison.relevant_basis },
    "Propose one prepared change assessment",
    input.economics.change_id,
  );
  const assessmentDigest = ZapDigestSchema.parse(protectedCommandDigest(assessmentCommand));
  const durableEffects = [];
  for (const [index, effect] of request.value.effects.entries()) {
    const identity = ids.effects[index];
    if (identity === undefined) {
      return fail("refused", "Prepared effect identity is unavailable");
    }
    durableEffects.push({
      effectId: effect.effect_id,
      index: effect.index,
      kind: effect.kind,
      payload: effect.payload,
      predecessors: effect.predecessors,
      productEventId: identity.productEventId,
      productCommandId: identity.productCommandId,
      subjects: effect.declared_subjects,
    });
  }
  return {
    ok: true,
    value: PreparedAssessmentProposalSchema.parse({
      planProposal,
      planReceipt,
      assessmentId: ids.assessmentId,
      alternativeId: ids.alternativeId,
      planPayload: canonicalQueryInput(plan),
      effects: durableEffects,
      command: {
        stage: "assessment_proposal",
        command: assessmentCommand,
        commandDigest: assessmentDigest,
      },
    }),
  };
}

export async function finishSuccessor(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  rawAssessment: PreparedAssessmentProposal,
  assessmentReceipt: MetadataReceipt,
): Promise<PlanAuthoringResult<PreparedSuccessor>> {
  const parsed = PreparedAssessmentProposalSchema.safeParse(rawAssessment);
  if (!parsed.success || !receiptMatches(parsed.data.command, assessmentReceipt)) {
    return fail("invalid_input", "Assessment receipt does not match its durable command");
  }
  if (context.intentBasis.revision !== assessmentReceipt.revision) {
    return fail("stale_basis", "Assessment receipt is not the current metadata revision");
  }
  const normalized = await readAssessmentDigest(options, context, parsed.data.assessmentId);
  if (!normalized.ok) return normalized;
  const preparedBasis = PlanBasisSchema.parse({
    ...parsed.data.planProposal.intentBasis,
    revision: assessmentReceipt.revision,
  });
  return {
    ok: true,
    value: PreparedSuccessorSchema.parse({
      intentBasis: parsed.data.planProposal.intentBasis,
      preparedBasis,
      operationId: parsed.data.planProposal.operationId,
      assessmentId: parsed.data.assessmentId,
      alternativeId: parsed.data.alternativeId,
      sourceAssessmentDigest: normalized.value.digest,
      assessmentProposalRevision: assessmentReceipt.revision,
      planPayload: parsed.data.planPayload,
      effects: parsed.data.effects,
      planReceipt: parsed.data.planReceipt,
      assessmentReceipt,
    }),
  };
}

function receiptMatches(prepared: PreparedCommand, receipt: MetadataReceipt): boolean {
  return (
    prepared.command.frame.header.command_id === receipt.commandId &&
    prepared.commandDigest === receipt.commandDigest
  );
}

export function decodeCanonical(payload: QueryInputWire): PlanAuthoringResult<CanonicalJsonInput> {
  try {
    return canonicalValue(parseCanonicalJson(Uint8Array.from(payload.canonical_json)));
  } catch {
    return fail("invalid_input", "Durable canonical payload is malformed");
  }
}

export function decodeSuccessorInput(
  payload: QueryInputWire,
): PlanAuthoringResult<SuccessorPlanAuthoringInput> {
  try {
    const parsed = SuccessorPlanAuthoringInputSchema.safeParse(
      parseCanonicalJson(Uint8Array.from(payload.canonical_json)),
    );
    return parsed.success
      ? { ok: true, value: parsed.data }
      : fail("invalid_input", "Durable semantic input is malformed");
  } catch {
    return fail("invalid_input", "Durable semantic input is not canonical JSON");
  }
}

async function preparePlanBasis(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  input: SuccessorPlanAuthoringInput,
): Promise<PlanAuthoringResult<ReturnType<typeof ZapDigestSchema.parse>>> {
  const roots = sortedSubjects([
    { kind: "outcome", id: context.activeOutcomeId },
    ...input.plan.content.obligation_coverage.map((value) => ({
      kind: "obligation",
      id: value.obligation_id,
    })),
    ...input.plan.content.admission_work_ids.map((id) => ({ kind: "work", id })),
  ]);
  const prepared = await options.reader.prepareBundle({
    at: { kind: "current" },
    actor: null,
    draft: {
      alternative_id: derivedId(input.operationId, "plan-basis-alternative"),
      committed_prefix: [],
      effects: [],
      no_op_basis: {
        purpose: { kind: "mutation", subject: "milestone.plan-basis" },
        roots,
        policy: "required",
        capacity: "not_applicable",
        closure: "known_graph",
      },
    },
  });
  if (!prepared.ok) {
    const detail =
      prepared.error.kind === "http_refusal" ? prepared.error.refusal.message : prepared.error.kind;
    return fail("refused", `Plan basis preparation failed: ${detail}`);
  }
  const parsed = noOpBasis(prepared.value.preflight);
  return parsed.ok ? { ok: true, value: parsed.value.initial_basis } : parsed;
}

export async function readAssessmentDigest(
  options: PlanAuthoringOptions,
  context: AuthoringContext,
  assessmentId: ZapId,
): Promise<
  PlanAuthoringResult<{
    readonly digest: ReturnType<typeof ZapDigestSchema.parse>;
    readonly adjudicated: boolean;
  }>
> {
  const projected = await options.reader.prepareProjectedRecord({
    at: { kind: "current" },
    actor: null,
    draft: {
      alternative_id: derivedId(assessmentId, "assessment-read-alternative"),
      committed_prefix: [],
      effects: [],
      no_op_basis: {
        purpose: { kind: "completion" },
        roots: [],
        policy: "not_applicable",
        capacity: "not_applicable",
        closure: "known_graph",
      },
    },
    record: {
      family: ZapIdSchema.parse("zap.economics.assessment"),
      key: [...new TextEncoder().encode(assessmentId)],
    },
  });
  if (!projected.ok || projected.value.canonical_value === null) {
    return fail("unavailable", "Normalized assessment is unavailable through projected record");
  }
  try {
    const assessment = z
      .looseObject({ adjudicated: z.boolean() })
      .parse(parseCanonicalJson(projected.value.canonical_value));
    if (projected.value.store.store_id !== context.store.store_id) {
      return fail("stale_basis", "Projected assessment came from a foreign store");
    }
    return {
      ok: true,
      value: {
        digest: canonicalBytesDigest(projected.value.canonical_value),
        adjudicated: assessment.adjudicated,
      },
    };
  } catch {
    return fail("unavailable", "Normalized assessment bytes are not canonical JSON");
  }
}

function sortedSubjects<T extends { readonly kind: string; readonly id: ZapId }>(values: T[]): T[] {
  const order = [
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
  return [...values].sort((left, right) => {
    const leftRank = order.indexOf(left.kind);
    const rightRank = order.indexOf(right.kind);
    return leftRank === rightRank ? left.id.localeCompare(right.id) : leftRank - rightRank;
  });
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
