/** Deterministic catalog selection after hard authorization and capability filtering. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import type { EffectiveEffort, EffortCapability, EffortRequest } from "../model-policy/index.ts";
import {
  ExecutionCatalogSnapshotSchema,
  ExecutionSelectionRequestSchema,
  ExecutionSelectionSchema,
  TrustedExecutionCatalogContextSchema,
  type AppliedContext,
  type ContextCapability,
  type ContextRequest,
  type ExecutionCatalogError,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
  type ExecutionConfigurationRecord,
  type ExecutionConnectionRecord,
  type ExecutionSelection,
  type ExecutionSelectionRequest,
  type TrustedExecutionCatalogContext,
  type UsageObservation,
  type CandidateExplanation,
} from "./types.ts";
type ResolvedCandidate = {
  readonly configuration: ExecutionConfigurationRecord;
  readonly connection: ExecutionConnectionRecord;
  readonly effort: EffectiveEffort;
  readonly context: AppliedContext;
  readonly explanation: CandidateExplanation;
};

export function resolveExecutionSelection(
  rawSnapshot: ExecutionCatalogSnapshot,
  rawRequest: ExecutionSelectionRequest,
  rawTrusted: TrustedExecutionCatalogContext,
): ExecutionCatalogResult<ExecutionSelection> {
  const snapshot = ExecutionCatalogSnapshotSchema.safeParse(rawSnapshot);
  const request = ExecutionSelectionRequestSchema.safeParse(rawRequest);
  const trusted = TrustedExecutionCatalogContextSchema.safeParse(rawTrusted);
  if (!snapshot.success || !request.success || !trusted.success) {
    return failure("invalid_input", "execution catalog selection input is malformed");
  }
  const connections = new Map(
    snapshot.data.connections.map((connection) => [connection.connectionId, connection]),
  );
  const evaluated = snapshot.data.configurations.map((configuration) =>
    evaluateCandidate(snapshot.data, request.data, trusted.data, configuration, connections),
  );
  const eligible = evaluated
    .filter((candidate): candidate is ResolvedCandidate => "configuration" in candidate)
    .sort(compareCandidates);
  const overrideId = request.data.override?.configurationId;
  const selected =
    overrideId === undefined
      ? eligible[0]
      : eligible.find((candidate) => candidate.configuration.configurationId === overrideId);
  if (selected === undefined) {
    return failure(
      overrideId === undefined ? "no_eligible_configuration" : "override_refused",
      overrideId === undefined
        ? "no authorized execution configuration satisfies the task requirements"
        : "the requested execution configuration is not eligible",
    );
  }
  const result = ExecutionSelectionSchema.safeParse({
    protocol: "zap-execution-selection/1",
    selectionRef: request.data.selectionRef,
    catalogRevision: snapshot.data.catalogRevision,
    preferencesRevision: snapshot.data.preferencesRevision,
    specialization: request.data.specialization,
    configurationId: selected.configuration.configurationId,
    configurationName: selected.configuration.displayName,
    connectionId: selected.connection.connectionId,
    launchBindingId: selected.connection.launchBindingId,
    providerId: selected.configuration.providerId,
    agentProduct: selected.configuration.agentProduct,
    productId: selected.configuration.productId,
    modelVendorId: selected.configuration.modelVendorId,
    modelFamilyId: selected.configuration.modelFamilyId,
    modelId: selected.configuration.modelId,
    requestedEffort: request.data.effort,
    appliedEffort: selected.effort,
    requestedContext: request.data.context,
    appliedContext: selected.context,
    overrideReason: request.data.override?.reason ?? null,
    selectedUsageObservationIds: selected.explanation.usageObservationIds,
    explanations: evaluated.map((candidate) => candidate.explanation),
    application: "future_attempt",
  });
  return result.success
    ? { ok: true, value: result.data }
    : failure("invalid_input", "resolved execution selection is invalid");
}

function evaluateCandidate(
  snapshot: ExecutionCatalogSnapshot,
  request: ExecutionSelectionRequest,
  trusted: TrustedExecutionCatalogContext,
  configuration: ExecutionConfigurationRecord,
  connections: ReadonlyMap<string, ExecutionConnectionRecord>,
): ResolvedCandidate | { readonly explanation: CandidateExplanation } {
  const reasons: string[] = [];
  const connection = connections.get(configuration.connectionId);
  if (connection === undefined) reasons.push("connection is missing");
  else {
    if (!connection.enabled) reasons.push("connection is disabled");
    if (!trusted.allowedConnectionIds.includes(connection.connectionId))
      reasons.push("connection is outside the authorized scope");
    if (!trusted.availableLaunchBindingIds.includes(connection.launchBindingId))
      reasons.push("protected launch binding is unavailable");
    if (
      connection.providerId !== configuration.providerId ||
      connection.agentProduct !== configuration.agentProduct
    )
      reasons.push("configuration does not match its connection");
  }
  if (!configuration.enabled) reasons.push("configuration is disabled");
  if (!trusted.allowedConfigurationIds.includes(configuration.configurationId))
    reasons.push("configuration is outside the authorized scope");
  if (request.executionMode === "native" && configuration.productId !== request.productId)
    reasons.push("native execution must retain the parent agent product");
  if (request.executionMode === "native" && request.invocationScope !== "coordinator") {
    const parent = trusted.parentSelection;
    if (parent === null) reasons.push("native execution requires a parent selection");
    else if (
      parent.connectionId !== configuration.connectionId ||
      parent.launchBindingId !== connection?.launchBindingId
    )
      reasons.push("native execution must retain the parent account binding");
  }
  if (!configuration.executionModes.includes(request.executionMode))
    reasons.push("execution mode is unsupported");
  if (!configuration.invocationScopes.includes(request.invocationScope))
    reasons.push("invocation scope is unsupported");
  if (request.requiredModalities.some((modality) => !configuration.modalities.includes(modality)))
    reasons.push("required modality is unsupported");
  const score = configuration.scores.find(
    (candidate) => candidate.specialization === request.specialization,
  );
  if (score === undefined) reasons.push("specialization has no suitability entry");
  const effort = resolveEffort(request.effort, configuration.effort, trusted);
  if (!effort.ok) reasons.push(effort.reason);
  const context = resolveContext(request.context, configuration.context, trusted);
  if (!context.ok) reasons.push(context.reason);
  const usage = applicableUsage(snapshot, configuration, request.requestedAt);
  const explanationBase = {
    configurationId: configuration.configurationId,
    usageObservationIds: usage.observations.map((observation) => observation.observationId),
  };
  if (
    reasons.length > 0 ||
    connection === undefined ||
    score === undefined ||
    !effort.ok ||
    !context.ok
  ) {
    return {
      explanation: {
        ...explanationBase,
        state: "excluded",
        score: null,
        reasons: reasons.length > 0 ? reasons : ["configuration is ineligible"],
      },
    };
  }
  const blend = Math.round(
    (score.quality * snapshot.preferences.economyQuality +
      score.economy * (100 - snapshot.preferences.economyQuality)) /
      100,
  );
  const quotaPenalty = usage.lowRemaining ? 2_000 : 0;
  const preferencePenalty = Math.min(score.preferenceOrder, 100) * 10;
  const total = score.suitability * 35 + blend * 65 - preferencePenalty - quotaPenalty;
  const positiveReasons = [score.rationale, `deterministic score ${String(total)}`];
  if (usage.lowRemaining)
    positiveReasons.push("fresh applicable quota is below the configured preference threshold");
  else if (usage.status !== null) positiveReasons.push(usage.status);
  return {
    configuration,
    connection,
    effort: effort.value,
    context: context.value,
    explanation: {
      ...explanationBase,
      state: "eligible",
      score: total,
      reasons: positiveReasons,
    },
  };
}

function applicableUsage(
  snapshot: ExecutionCatalogSnapshot,
  configuration: ExecutionConfigurationRecord,
  requestedAt: string,
): {
  readonly observations: readonly UsageObservation[];
  readonly lowRemaining: boolean;
  readonly status: string | null;
} {
  const requestedTime = Date.parse(requestedAt);
  const candidates = snapshot.usage.filter(
    (observation) =>
      observation.connectionId === configuration.connectionId &&
      configuration.usageBucketIds.includes(observation.bucketId) &&
      usageApplies(observation, configuration) &&
      Date.parse(observation.observedAt) <= requestedTime,
  );
  const observations = latestUsage(candidates);
  if (!snapshot.preferences.quota.deprioritizeLowRemaining)
    return { observations, lowRemaining: false, status: null };
  const cutoff = Date.parse(requestedAt) - snapshot.preferences.quota.freshnessSeconds * 1_000;
  const fresh = observations.filter(
    (observation) =>
      observation.status === "observed" &&
      observation.meterKind === "subscription" &&
      Date.parse(observation.observedAt) >= cutoff &&
      (observation.window.resetsAt === null ||
        Date.parse(observation.window.resetsAt) > Date.parse(requestedAt)),
  );
  const percentages = fresh.flatMap((observation) =>
    observation.remainingPercent === null ? [] : [observation.remainingPercent],
  );
  if (percentages.some((remaining) => remaining < snapshot.preferences.quota.thresholdPercent))
    return { observations, lowRemaining: true, status: null };
  if (fresh.length === 0)
    return {
      observations,
      lowRemaining: false,
      status:
        observations.length === 0
          ? "applicable quota is unknown or unsupported"
          : "applicable quota observation is stale",
    };
  if (percentages.length === 0)
    return { observations, lowRemaining: false, status: "quota has no remaining-percent value" };
  return { observations, lowRemaining: false, status: "fresh quota is not below the threshold" };
}

function latestUsage(observations: readonly UsageObservation[]): UsageObservation[] {
  const latest = new Map<string, UsageObservation>();
  for (const observation of observations) {
    const key = `${observation.connectionId}\u0000${observation.bucketId}\u0000${observation.window.kind}`;
    const prior = latest.get(key);
    if (prior === undefined || Date.parse(observation.observedAt) > Date.parse(prior.observedAt))
      latest.set(key, observation);
  }
  return [...latest.values()].sort((left, right) =>
    left.observationId.localeCompare(right.observationId),
  );
}

function usageApplies(
  observation: UsageObservation,
  configuration: ExecutionConfigurationRecord,
): boolean {
  const applicability = observation.applicability;
  if (applicability.kind === "account") return true;
  if (applicability.kind === "exact_model") return applicability.modelId === configuration.modelId;
  if (applicability.kind === "model_family")
    return applicability.modelFamilyId === configuration.modelFamilyId;
  return true;
}

function resolveEffort(
  request: EffortRequest,
  capability: EffortCapability,
  trusted: TrustedExecutionCatalogContext,
):
  | { readonly ok: true; readonly value: EffectiveEffort }
  | { readonly ok: false; readonly reason: string } {
  if (request.mode === "explicit") {
    if (capability.mode !== "configurable" || !capability.allowedValues.includes(request.value))
      return { ok: false, reason: "requested effort is unsupported" };
    return { ok: true, value: { state: "explicit", value: request.value } };
  }
  if (request.mode === "inherit") {
    if (capability.mode !== "configurable" && capability.mode !== "inherited")
      return { ok: false, reason: "requested effort inheritance is unsupported" };
    const parent = trusted.parentSelection;
    if (parent === null) return { ok: false, reason: "effort inheritance has no parent selection" };
    if (
      capability.mode === "configurable" &&
      parent.effectiveEffort !== null &&
      !capability.allowedValues.includes(parent.effectiveEffort)
    )
      return { ok: false, reason: "inherited effort is unsupported" };
    return {
      ok: true,
      value: {
        state: "inherited",
        value: parent.effectiveEffort,
        sourceSelectionRef: parent.selectionRef,
      },
    };
  }
  if (capability.mode === "inherited") {
    const parent = trusted.parentSelection;
    return parent === null
      ? { ok: false, reason: "effort inheritance has no parent selection" }
      : {
          ok: true,
          value: {
            state: "inherited",
            value: parent.effectiveEffort,
            sourceSelectionRef: parent.selectionRef,
          },
        };
  }
  if (capability.mode === "configurable")
    return capability.defaultValue === null
      ? { ok: true, value: { state: "unknown", reason: "effort default is not configured" } }
      : { ok: true, value: { state: "configured_default", value: capability.defaultValue } };
  if (capability.mode === "unsupported") return { ok: true, value: { state: "unsupported" } };
  return { ok: true, value: { state: "unknown", reason: capability.reason } };
}

function resolveContext(
  request: ContextRequest,
  capability: ContextCapability,
  trusted: TrustedExecutionCatalogContext,
):
  | { readonly ok: true; readonly value: AppliedContext }
  | { readonly ok: false; readonly reason: string } {
  if (request.mode === "explicit") {
    if (
      capability.mode !== "configurable" ||
      !capability.allowedTokens.includes(request.tokens) ||
      (capability.documentedMaximumTokens !== null &&
        request.tokens > capability.documentedMaximumTokens)
    )
      return { ok: false, reason: "requested context preset is unsupported" };
    return { ok: true, value: { state: "configured", tokens: request.tokens } };
  }
  if (request.mode === "inherit") {
    if (capability.mode !== "configurable" && capability.mode !== "inherited")
      return { ok: false, reason: "requested context inheritance is unsupported" };
    const parent = trusted.parentSelection;
    if (parent === null)
      return { ok: false, reason: "context inheritance has no parent selection" };
    const tokens = parent.appliedContextTokens;
    if (
      capability.mode === "configurable" &&
      tokens !== null &&
      !capability.allowedTokens.includes(tokens)
    )
      return { ok: false, reason: "inherited context preset is unsupported" };
    return { ok: true, value: { state: "inherited", tokens } };
  }
  if (capability.mode === "inherited") {
    const parent = trusted.parentSelection;
    return parent === null
      ? { ok: false, reason: "context inheritance has no parent selection" }
      : {
          ok: true,
          value: { state: "inherited", tokens: parent.appliedContextTokens },
        };
  }
  if (capability.mode === "configurable")
    return capability.defaultTokens === null
      ? { ok: true, value: { state: "unknown", reason: "context default is not configured" } }
      : { ok: true, value: { state: "configured", tokens: capability.defaultTokens } };
  if (capability.mode === "fixed")
    return { ok: true, value: { state: "fixed", tokens: capability.tokens } };
  if (capability.mode === "unsupported") return { ok: true, value: { state: "unsupported" } };
  return { ok: true, value: { state: "unknown", reason: capability.reason } };
}

function compareCandidates(left: ResolvedCandidate, right: ResolvedCandidate): number {
  const leftScore = left.explanation.score ?? Number.MIN_SAFE_INTEGER;
  const rightScore = right.explanation.score ?? Number.MIN_SAFE_INTEGER;
  return (
    rightScore - leftScore ||
    left.configuration.configurationId.localeCompare(right.configuration.configurationId)
  );
}

function failure(
  code: ExecutionCatalogError["code"],
  message: string,
): ExecutionCatalogResult<never> {
  return { ok: false, error: { code, message } };
}
