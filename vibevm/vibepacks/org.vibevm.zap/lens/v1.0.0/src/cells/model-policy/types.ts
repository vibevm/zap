/**
 * Provider-neutral model tier, effort capability and selection records.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-008#model-routing
 */
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";

const OpaqueIdSchema = z
  .string()
  .min(3)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/);

export const ModelTierSchema = z.enum(["ultra", "big", "medium", "small"]);
export type ModelTier = z.infer<typeof ModelTierSchema>;

export const TaskPurposeSchema = z.enum([
  "development_implementation",
  "test_agent",
  "coordination",
  "research",
  "review",
  "verification",
  "integration",
  "other",
]);
export type TaskPurpose = z.infer<typeof TaskPurposeSchema>;

/** Mirrors the existing ZAP WorkType values without owning ZAP semantics. */
export const TaskClassSchema = z.enum([
  "evidence",
  "decision",
  "change",
  "verification",
  "integration",
]);
export type TaskClass = z.infer<typeof TaskClassSchema>;

export const ReasoningEffortSchema = z.enum([
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
]);
export type ReasoningEffort = z.infer<typeof ReasoningEffortSchema>;

export const ModelInvocationScopeSchema = z.enum([
  "coordinator",
  "native_subagent",
  "native_fork",
  "managed_agent",
]);
export type ModelInvocationScope = z.infer<typeof ModelInvocationScopeSchema>;

export const EffortRequestSchema = z.discriminatedUnion("mode", [
  z.object({ mode: z.literal("explicit"), value: ReasoningEffortSchema }).strict(),
  z.object({ mode: z.literal("inherit") }).strict(),
  z.object({ mode: z.literal("unspecified") }).strict(),
]);
export type EffortRequest = z.infer<typeof EffortRequestSchema>;

export const TierBindingSchema = z
  .object({
    tier: ModelTierSchema,
    profileId: OpaqueIdSchema,
    productId: OpaqueIdSchema,
    providerId: OpaqueIdSchema,
    modelId: z.string().min(1).max(256),
  })
  .strict();
export type TierBinding = z.infer<typeof TierBindingSchema>;

const TaskRuleMatchSchema = z
  .object({
    purposes: z.array(TaskPurposeSchema).min(1).max(32).optional(),
    taskClasses: z.array(TaskClassSchema).min(1).max(32).optional(),
    roles: z
      .array(z.enum(["coordinator", "worker"]))
      .min(1)
      .max(2)
      .optional(),
    executionModes: z
      .array(z.enum(["native", "managed"]))
      .min(1)
      .max(2)
      .optional(),
    invocationScopes: z.array(ModelInvocationScopeSchema).min(1).max(4).optional(),
    productIds: z.array(OpaqueIdSchema).min(1).max(32).optional(),
  })
  .strict();

export const TaskModelRuleSchema = z
  .object({
    ruleId: OpaqueIdSchema,
    priority: z.number().int().min(0).max(1_000_000),
    match: TaskRuleMatchSchema,
    tier: ModelTierSchema,
    effort: EffortRequestSchema,
    selectionReason: z.string().min(1).max(2_000),
  })
  .strict();
export type TaskModelRule = z.infer<typeof TaskModelRuleSchema>;

export const ModelPolicySchema = z
  .object({
    protocol: z.literal("lens-model-policy/1"),
    policyId: OpaqueIdSchema,
    revision: DecimalSchema,
    tierBindings: z.array(TierBindingSchema).length(4),
    taskRules: z.array(TaskModelRuleSchema).min(1).max(1_000),
  })
  .strict()
  .superRefine((policy, context) => {
    const tiers = new Set(policy.tierBindings.map((binding) => binding.tier));
    if (tiers.size !== ModelTierSchema.options.length) {
      context.addIssue({ code: "custom", message: "tierBindings must contain each tier once" });
    }
    const rules = new Set(policy.taskRules.map((rule) => rule.ruleId));
    if (rules.size !== policy.taskRules.length) {
      context.addIssue({ code: "custom", message: "task rule ids must be unique" });
    }
  });
export type ModelPolicy = z.infer<typeof ModelPolicySchema>;

export const EffortCapabilitySchema = z.discriminatedUnion("mode", [
  z
    .object({
      mode: z.literal("configurable"),
      allowedValues: z.array(ReasoningEffortSchema).min(1).max(16),
      defaultValue: ReasoningEffortSchema.nullable(),
    })
    .strict(),
  z.object({ mode: z.literal("inherited") }).strict(),
  z.object({ mode: z.literal("unsupported") }).strict(),
  z.object({ mode: z.literal("unknown"), reason: z.string().min(1).max(2_000) }).strict(),
]);
export type EffortCapability = z.infer<typeof EffortCapabilitySchema>;

export const ModelCapabilityProfileSchema = z
  .object({
    capabilityId: OpaqueIdSchema,
    productId: OpaqueIdSchema,
    productVersion: z.string().min(1).max(160),
    executionMode: z.enum(["native", "managed"]),
    invocationScope: ModelInvocationScopeSchema,
    modelId: z.string().min(1).max(256),
    effort: EffortCapabilitySchema,
    extendedThinking: z.enum(["configurable", "inherited", "unsupported", "unknown"]),
    evidence: z
      .object({
        source: z.string().min(1).max(1_000),
        observedAt: z.iso.datetime(),
      })
      .strict(),
  })
  .strict();
export type ModelCapabilityProfile = z.infer<typeof ModelCapabilityProfileSchema>;

export const ModelSelectionRequestSchema = z
  .object({
    selectionRef: OpaqueIdSchema,
    purpose: TaskPurposeSchema,
    taskClass: TaskClassSchema,
    role: z.enum(["coordinator", "worker"]),
    executionMode: z.enum(["native", "managed"]),
    invocationScope: ModelInvocationScopeSchema,
    productId: OpaqueIdSchema,
    productVersion: z.string().min(1).max(160),
    override: z
      .object({
        overrideRef: OpaqueIdSchema,
        tier: ModelTierSchema,
        effort: EffortRequestSchema,
        reason: z.string().min(1).max(2_000),
      })
      .strict()
      .nullable(),
  })
  .strict();
export type ModelSelectionRequest = z.infer<typeof ModelSelectionRequestSchema>;

export const EffectiveEffortSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("explicit"), value: ReasoningEffortSchema }).strict(),
  z.object({ state: z.literal("configured_default"), value: ReasoningEffortSchema }).strict(),
  z
    .object({
      state: z.literal("inherited"),
      value: ReasoningEffortSchema.nullable(),
      sourceSelectionRef: OpaqueIdSchema,
    })
    .strict(),
  z.object({ state: z.literal("unsupported") }).strict(),
  z.object({ state: z.literal("unknown"), reason: z.string().min(1).max(2_000) }).strict(),
]);
export type EffectiveEffort = z.infer<typeof EffectiveEffortSchema>;

export const ParentModelSelectionSchema = z
  .object({
    selectionRef: OpaqueIdSchema,
    profileId: OpaqueIdSchema,
    modelId: z.string().min(1).max(256),
    effectiveEffort: EffectiveEffortSchema,
  })
  .strict();
export type ParentModelSelection = z.infer<typeof ParentModelSelectionSchema>;

export const ModelObservationSchema = z
  .object({
    source: z.string().min(1).max(1_000),
    observedAt: z.iso.datetime(),
    profileId: OpaqueIdSchema,
    productId: OpaqueIdSchema,
    productVersion: z.string().min(1).max(160),
    executionMode: z.enum(["native", "managed"]),
    invocationScope: ModelInvocationScopeSchema,
    modelId: z.string().min(1).max(256),
    effort: ReasoningEffortSchema.nullable(),
  })
  .strict();
export type ModelObservation = z.infer<typeof ModelObservationSchema>;

export const TrustedModelContextSchema = z
  .object({
    capabilities: z.array(ModelCapabilityProfileSchema).max(1_000),
    allowedProfileIds: z.array(OpaqueIdSchema).max(1_000),
    parentSelection: ParentModelSelectionSchema.nullable(),
    actualObservation: ModelObservationSchema.nullable(),
  })
  .strict();
export type TrustedModelContext = z.infer<typeof TrustedModelContextSchema>;

export const ModelSelectionSchema = z
  .object({
    protocol: z.literal("lens-model-selection/1"),
    selectionRef: OpaqueIdSchema,
    policyId: OpaqueIdSchema,
    policyRevision: DecimalSchema,
    ruleId: OpaqueIdSchema,
    overrideRef: OpaqueIdSchema.nullable(),
    selectionReason: z.string().min(1).max(2_000),
    overrideReason: z.string().min(1).max(2_000).nullable(),
    purpose: TaskPurposeSchema,
    taskClass: TaskClassSchema,
    role: z.enum(["coordinator", "worker"]),
    executionMode: z.enum(["native", "managed"]),
    invocationScope: ModelInvocationScopeSchema,
    requestedTier: ModelTierSchema,
    requestedEffort: EffortRequestSchema,
    profileId: OpaqueIdSchema,
    productId: OpaqueIdSchema,
    productVersion: z.string().min(1).max(160),
    providerId: OpaqueIdSchema,
    modelId: z.string().min(1).max(256),
    capabilityId: OpaqueIdSchema.nullable(),
    effortCapability: EffortCapabilitySchema,
    extendedThinking: z.enum(["configurable", "inherited", "unsupported", "unknown"]),
    effectiveEffort: EffectiveEffortSchema,
    actualObservation: ModelObservationSchema.nullable(),
    observationMatchesSelection: z.boolean().nullable(),
    application: z.literal("future_attempt"),
  })
  .strict();
export type ModelSelection = z.infer<typeof ModelSelectionSchema>;

export const PreservedRunningSelectionSchema = z
  .object({
    state: z.literal("running_attempt_preserved"),
    selection: ModelSelectionSchema,
    ignoredPolicy: z.object({ policyId: OpaqueIdSchema, revision: DecimalSchema }).strict(),
    actualObservation: ModelObservationSchema.nullable(),
  })
  .strict();
export type PreservedRunningSelection = z.infer<typeof PreservedRunningSelectionSchema>;

export const ModelPolicyErrorSchema = z
  .object({
    code: z.enum([
      "invalid_policy",
      "invalid_request",
      "rule_not_found",
      "ambiguous_rule",
      "binding_not_found",
      "profile_disallowed",
      "capability_ambiguous",
      "capability_unknown",
      "effort_unsupported",
      "inheritance_unavailable",
    ]),
    message: z.string().min(1).max(4_000),
  })
  .strict();
export type ModelPolicyError = z.infer<typeof ModelPolicyErrorSchema>;
export type ModelPolicyResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: ModelPolicyError };
