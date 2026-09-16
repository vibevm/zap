/**
 * Deterministic model policy resolution for future attempts.
 *
 * Capability and observation context is trusted adapter evidence, never user request data.
 * @scope spec://org.vibevm.zap/lens/PROP-008#model-routing
 */
import {
  ModelObservationSchema,
  ModelPolicySchema,
  ModelSelectionRequestSchema,
  ModelSelectionSchema,
  PreservedRunningSelectionSchema,
  TrustedModelContextSchema,
  type EffectiveEffort,
  type EffortCapability,
  type EffortRequest,
  type ModelObservation,
  type ModelPolicy,
  type ModelPolicyError,
  type ModelPolicyResult,
  type ModelSelection,
  type ModelSelectionRequest,
  type ParentModelSelection,
  type PreservedRunningSelection,
  type TaskModelRule,
  type TrustedModelContext,
} from "./types.ts";

export function resolveModelSelection(
  rawPolicy: ModelPolicy,
  rawRequest: ModelSelectionRequest,
  rawTrustedContext: TrustedModelContext,
): ModelPolicyResult<ModelSelection> {
  const policy = ModelPolicySchema.safeParse(rawPolicy);
  if (!policy.success) return failure("invalid_policy", "Model policy is invalid");
  const request = ModelSelectionRequestSchema.safeParse(rawRequest);
  if (!request.success) return failure("invalid_request", "Model selection request is invalid");
  const trusted = TrustedModelContextSchema.safeParse(rawTrustedContext);
  if (!trusted.success) return failure("invalid_request", "Trusted model context is invalid");

  const rules = policy.data.taskRules.filter((rule) => matches(rule, request.data));
  if (rules.length === 0) {
    return failure("rule_not_found", "No model policy rule matches this task purpose and class");
  }
  const priority = Math.max(...rules.map((rule) => rule.priority));
  const selectedRules = rules.filter((rule) => rule.priority === priority);
  if (selectedRules.length !== 1) {
    return failure("ambiguous_rule", "More than one highest-priority model rule matches");
  }
  const rule = selectedRules[0];
  if (rule === undefined) return failure("rule_not_found", "No model policy rule was selected");

  const requestedTier = request.data.override?.tier ?? rule.tier;
  const requestedEffort = request.data.override?.effort ?? rule.effort;
  const binding = policy.data.tierBindings.find((candidate) => candidate.tier === requestedTier);
  if (binding === undefined || binding.productId !== request.data.productId) {
    return failure("binding_not_found", "Requested tier has no binding for the selected product");
  }
  if (!trusted.data.allowedProfileIds.includes(binding.profileId)) {
    return failure("profile_disallowed", "Concrete profile is not allowed in this trusted scope");
  }
  const capabilities = trusted.data.capabilities.filter(
    (candidate) =>
      candidate.productId === binding.productId &&
      candidate.productVersion === request.data.productVersion &&
      candidate.executionMode === request.data.executionMode &&
      candidate.invocationScope === request.data.invocationScope &&
      candidate.modelId === binding.modelId,
  );
  if (capabilities.length > 1) {
    return failure("capability_ambiguous", "Multiple capability profiles match the concrete model");
  }
  const capability = capabilities[0];
  const effortCapability: EffortCapability = capability?.effort ?? {
    mode: "unknown",
    reason: "No verified capability profile matches",
  };
  const effort = resolveEffort(requestedEffort, effortCapability, trusted.data.parentSelection);
  if (!effort.ok) return effort;
  const observation = trusted.data.actualObservation;
  const selection = ModelSelectionSchema.safeParse({
    protocol: "lens-model-selection/1",
    selectionRef: request.data.selectionRef,
    policyId: policy.data.policyId,
    policyRevision: policy.data.revision,
    ruleId: rule.ruleId,
    overrideRef: request.data.override?.overrideRef ?? null,
    selectionReason: rule.selectionReason,
    overrideReason: request.data.override?.reason ?? null,
    purpose: request.data.purpose,
    taskClass: request.data.taskClass,
    role: request.data.role,
    executionMode: request.data.executionMode,
    invocationScope: request.data.invocationScope,
    requestedTier,
    requestedEffort,
    profileId: binding.profileId,
    productId: binding.productId,
    productVersion: request.data.productVersion,
    providerId: binding.providerId,
    modelId: binding.modelId,
    capabilityId: capability?.capabilityId ?? null,
    effortCapability,
    extendedThinking: capability?.extendedThinking ?? "unknown",
    effectiveEffort: effort.value,
    actualObservation: observation,
    observationMatchesSelection:
      observation === null
        ? null
        : observationMatches(observation, binding, request.data, effort.value),
    application: "future_attempt",
  });
  return selection.success
    ? { ok: true, value: selection.data }
    : failure("invalid_policy", "Resolved model selection is invalid");
}

export function preserveRunningModelSelection(
  rawSelection: ModelSelection,
  consideredPolicy: Pick<ModelPolicy, "policyId" | "revision">,
  rawObservation: ModelObservation | null,
): ModelPolicyResult<PreservedRunningSelection> {
  const selection = ModelSelectionSchema.safeParse(rawSelection);
  const observation =
    rawObservation === null
      ? { success: true as const, data: null }
      : ModelObservationSchema.safeParse(rawObservation);
  if (!selection.success || !observation.success) {
    return failure("invalid_request", "Stored selection or runtime observation is invalid");
  }
  const preserved = PreservedRunningSelectionSchema.safeParse({
    state: "running_attempt_preserved",
    selection: selection.data,
    ignoredPolicy: {
      policyId: consideredPolicy.policyId,
      revision: consideredPolicy.revision,
    },
    actualObservation: observation.data,
  });
  return preserved.success
    ? { ok: true, value: preserved.data }
    : failure("invalid_policy", "Considered model policy identity is invalid");
}

function matches(rule: TaskModelRule, request: ModelSelectionRequest): boolean {
  const match = rule.match;
  return (
    (match.purposes === undefined || match.purposes.includes(request.purpose)) &&
    (match.taskClasses === undefined || match.taskClasses.includes(request.taskClass)) &&
    (match.roles === undefined || match.roles.includes(request.role)) &&
    (match.executionModes === undefined || match.executionModes.includes(request.executionMode)) &&
    (match.invocationScopes === undefined ||
      match.invocationScopes.includes(request.invocationScope)) &&
    (match.productIds === undefined || match.productIds.includes(request.productId))
  );
}

function resolveEffort(
  request: EffortRequest,
  capability: EffortCapability,
  parent: ParentModelSelection | null,
): ModelPolicyResult<EffectiveEffort> {
  if (capability.mode === "configurable") {
    if (request.mode === "explicit") {
      return capability.allowedValues.includes(request.value)
        ? { ok: true, value: { state: "explicit", value: request.value } }
        : failure(
            "effort_unsupported",
            "Requested effort is not allowed by this capability profile",
          );
    }
    if (request.mode === "inherit") return inheritEffort(parent, capability.allowedValues);
    return capability.defaultValue === null
      ? { ok: true, value: { state: "unknown", reason: "No effort default is configured" } }
      : { ok: true, value: { state: "configured_default", value: capability.defaultValue } };
  }
  if (capability.mode === "inherited") {
    if (request.mode === "explicit") {
      return failure(
        "effort_unsupported",
        "This execution mode inherits effort and rejects a child override",
      );
    }
    return inheritEffort(parent);
  }
  if (capability.mode === "unsupported") {
    return request.mode === "explicit"
      ? failure("effort_unsupported", "This execution mode does not support an effort setting")
      : { ok: true, value: { state: "unsupported" } };
  }
  return request.mode === "explicit"
    ? failure(
        "capability_unknown",
        "Effort support is unknown; explicit effort cannot be applied safely",
      )
    : { ok: true, value: { state: "unknown", reason: capability.reason } };
}

function inheritEffort(
  parent: ParentModelSelection | null,
  allowedValues?: readonly EffectiveValue[],
): ModelPolicyResult<EffectiveEffort> {
  if (parent === null) {
    return failure(
      "inheritance_unavailable",
      "Effort inheritance requires a recorded parent selection",
    );
  }
  const value = effectiveValue(parent.effectiveEffort);
  if (value !== null && allowedValues !== undefined && !allowedValues.includes(value)) {
    return failure(
      "effort_unsupported",
      "Inherited parent effort is not supported by the child profile",
    );
  }
  return {
    ok: true,
    value: { state: "inherited", value, sourceSelectionRef: parent.selectionRef },
  };
}

type EffectiveValue = Exclude<EffectiveEffort, { state: "unsupported" | "unknown" }>["value"];

function effectiveValue(effort: EffectiveEffort): EffectiveValue | null {
  return effort.state === "unsupported" || effort.state === "unknown" ? null : effort.value;
}

function observationMatches(
  observation: ModelObservation,
  binding: ModelPolicy["tierBindings"][number],
  request: ModelSelectionRequest,
  effort: EffectiveEffort,
): boolean | null {
  const selectedEffort = effectiveValue(effort);
  const modelMatches =
    observation.profileId === binding.profileId &&
    observation.productId === binding.productId &&
    observation.productVersion === request.productVersion &&
    observation.executionMode === request.executionMode &&
    observation.invocationScope === request.invocationScope &&
    observation.modelId === binding.modelId;
  if (!modelMatches) return false;
  return observation.effort === null || selectedEffort === null
    ? null
    : observation.effort === selectedEffort;
}

function failure(code: ModelPolicyError["code"], message: string): ModelPolicyResult<never> {
  return { ok: false, error: { code, message } };
}
