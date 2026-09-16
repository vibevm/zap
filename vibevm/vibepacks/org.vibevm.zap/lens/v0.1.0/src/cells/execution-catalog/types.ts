/** Browser-safe execution catalog and deterministic selection contracts. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { z } from "zod";
import {
  EffortCapabilitySchema,
  EffortRequestSchema,
  EffectiveEffortSchema,
  ModelInvocationScopeSchema,
  ReasoningEffortSchema,
  TaskClassSchema,
  TaskPurposeSchema,
} from "../model-policy/index.ts";
import { DecimalSchema } from "../protocol/index.ts";

export const ExecutionCatalogIdSchema = z
  .string()
  .min(3)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/);

export const TaskSpecializationSchema = z.enum([
  "general",
  "architecture",
  "backend",
  "web_ui",
  "testing",
  "code_review",
  "research",
  "documentation",
  "data_analysis",
  "image_generation",
  "automation",
]);
export type TaskSpecialization = z.infer<typeof TaskSpecializationSchema>;

export const AgentProductSchema = z.enum([
  "codex",
  "claude_code",
  "opencode",
  "qwen_code",
  "zap_mock",
]);
export type AgentProduct = z.infer<typeof AgentProductSchema>;

export const ExecutionModeSchema = z.enum(["native", "managed"]);
export const ExecutionModalitySchema = z.enum(["text", "image_input", "image_output"]);
export type ExecutionModality = z.infer<typeof ExecutionModalitySchema>;

export const ContextRequestSchema = z.discriminatedUnion("mode", [
  z.object({ mode: z.literal("default") }).strict(),
  z.object({ mode: z.literal("inherit") }).strict(),
  z.object({ mode: z.literal("explicit"), tokens: z.number().int().positive() }).strict(),
]);
export type ContextRequest = z.infer<typeof ContextRequestSchema>;

export const ContextCapabilitySchema = z
  .discriminatedUnion("mode", [
    z
      .object({
        mode: z.literal("configurable"),
        allowedTokens: z.array(z.number().int().positive()).min(1).max(32),
        defaultTokens: z.number().int().positive().nullable(),
        documentedMaximumTokens: z.number().int().positive().nullable(),
      })
      .strict(),
    z
      .object({
        mode: z.literal("fixed"),
        tokens: z.number().int().positive().nullable(),
        source: z.string().min(1).max(1_000),
      })
      .strict(),
    z
      .object({
        mode: z.literal("inherited"),
        source: z.string().min(1).max(1_000),
      })
      .strict(),
    z.object({ mode: z.literal("unsupported") }).strict(),
    z.object({ mode: z.literal("unknown"), reason: z.string().min(1).max(2_000) }).strict(),
  ])
  .superRefine((capability, context) => {
    if (capability.mode !== "configurable") return;
    const maximum = capability.documentedMaximumTokens;
    if (maximum !== null && capability.allowedTokens.some((tokens) => tokens > maximum))
      context.addIssue({ code: "custom", message: "context preset exceeds documented maximum" });
    if (
      capability.defaultTokens !== null &&
      !capability.allowedTokens.includes(capability.defaultTokens)
    )
      context.addIssue({ code: "custom", message: "context default must be an allowed preset" });
  });
export type ContextCapability = z.infer<typeof ContextCapabilitySchema>;

export const AppliedContextSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("configured"), tokens: z.number().int().positive() }).strict(),
  z.object({ state: z.literal("fixed"), tokens: z.number().int().positive().nullable() }).strict(),
  z
    .object({ state: z.literal("inherited"), tokens: z.number().int().positive().nullable() })
    .strict(),
  z.object({ state: z.literal("unsupported") }).strict(),
  z.object({ state: z.literal("unknown"), reason: z.string().min(1).max(2_000) }).strict(),
]);
export type AppliedContext = z.infer<typeof AppliedContextSchema>;

export const SpecializationScoreSchema = z
  .object({
    specialization: TaskSpecializationSchema,
    suitability: z.number().int().min(0).max(100),
    quality: z.number().int().min(0).max(100),
    economy: z.number().int().min(0).max(100),
    preferenceOrder: z.number().int().min(0).max(10_000),
    rationale: z.string().min(1).max(2_000),
    provenance: z.enum(["owner", "reference_preset", "synthetic_fixture"]),
  })
  .strict();
export type SpecializationScore = z.infer<typeof SpecializationScoreSchema>;

export const ExecutionConnectionRecordSchema = z
  .object({
    connectionId: ExecutionCatalogIdSchema,
    displayName: z.string().trim().min(1).max(160),
    providerId: ExecutionCatalogIdSchema,
    agentProduct: AgentProductSchema,
    launchBindingId: ExecutionCatalogIdSchema,
    enabled: z.boolean(),
    synthetic: z.boolean(),
    setupGuidance: z.string().min(1).max(4_000),
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ExecutionConnectionRecord = z.infer<typeof ExecutionConnectionRecordSchema>;

export const ExecutionBindingChoiceSchema = z
  .object({
    bindingId: ExecutionCatalogIdSchema,
    hostId: z.string().min(3).max(160),
    displayName: z.string().trim().min(1).max(160),
    agentProduct: AgentProductSchema,
    enabled: z.boolean(),
    setupGuidance: z.string().min(1).max(4_000),
  })
  .strict();
export type ExecutionBindingChoice = z.infer<typeof ExecutionBindingChoiceSchema>;

export const ExecutionModelReferenceViewSchema = z
  .object({
    referenceId: ExecutionCatalogIdSchema,
    modelVendorId: ExecutionCatalogIdSchema,
    modelFamilyId: ExecutionCatalogIdSchema,
    modelId: z.string().min(1).max(256),
    conversationModel: z.boolean(),
    availability: z.enum([
      "verified_profile_required",
      "catalog_only",
      "deployment_required",
      "tool_only",
    ]),
    specializations: z.array(TaskSpecializationSchema).max(TaskSpecializationSchema.options.length),
    sourceUrls: z.array(z.url()).max(32),
    note: z.string().min(1).max(4_000),
  })
  .strict();
export type ExecutionModelReferenceView = z.infer<typeof ExecutionModelReferenceViewSchema>;

export const ExecutionConfigurationRecordSchema = z
  .object({
    configurationId: ExecutionCatalogIdSchema,
    displayName: z.string().trim().min(1).max(200),
    connectionId: ExecutionCatalogIdSchema,
    providerId: ExecutionCatalogIdSchema,
    agentProduct: AgentProductSchema,
    productId: ExecutionCatalogIdSchema,
    modelVendorId: ExecutionCatalogIdSchema,
    modelFamilyId: ExecutionCatalogIdSchema,
    modelId: z.string().min(1).max(256),
    executionModes: z.array(ExecutionModeSchema).min(1).max(2),
    invocationScopes: z.array(ModelInvocationScopeSchema).min(1).max(4),
    modalities: z.array(ExecutionModalitySchema).min(1).max(3),
    adapterEffort: EffortCapabilitySchema,
    effort: EffortCapabilitySchema,
    adapterContext: ContextCapabilitySchema,
    context: ContextCapabilitySchema,
    scores: z.array(SpecializationScoreSchema).min(1).max(TaskSpecializationSchema.options.length),
    usageBucketIds: z.array(ExecutionCatalogIdSchema).max(32),
    enabled: z.boolean(),
    synthetic: z.boolean(),
    evidence: z
      .object({ source: z.string().min(1).max(1_000), observedAt: z.iso.datetime() })
      .strict(),
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ExecutionConfigurationRecord = z.infer<typeof ExecutionConfigurationRecordSchema>;

export const ExecutionCatalogPreferencesSchema = z
  .object({
    economyQuality: z.number().int().min(0).max(100).default(50),
    quota: z
      .object({
        deprioritizeLowRemaining: z.boolean(),
        thresholdPercent: z.number().int().min(0).max(100).default(10),
        freshnessSeconds: z.number().int().min(30).max(86_400).default(300),
      })
      .strict(),
  })
  .strict();
export type ExecutionCatalogPreferences = z.infer<typeof ExecutionCatalogPreferencesSchema>;

export const UsageObservationSchema = z
  .object({
    observationId: ExecutionCatalogIdSchema,
    connectionId: ExecutionCatalogIdSchema,
    bucketId: ExecutionCatalogIdSchema,
    bucketLabel: z.string().min(1).max(160),
    applicability: z.discriminatedUnion("kind", [
      z.object({ kind: z.literal("account") }).strict(),
      z
        .object({ kind: z.literal("model_family"), modelFamilyId: ExecutionCatalogIdSchema })
        .strict(),
      z.object({ kind: z.literal("exact_model"), modelId: z.string().min(1).max(256) }).strict(),
      z.object({ kind: z.literal("unknown"), reason: z.string().min(1).max(1_000) }).strict(),
    ]),
    meterKind: z
      .enum(["subscription", "api_rate_limit", "api_balance", "consumption", "unknown"])
      .default("unknown"),
    unit: z.enum(["percent", "tokens", "requests", "currency", "unknown"]),
    remainingPercent: z.number().min(0).max(100).nullable(),
    exactRemainingTokens: z
      .string()
      .regex(/^(0|[1-9][0-9]*)$/)
      .nullable(),
    window: z
      .object({ kind: z.string().min(1).max(160), resetsAt: z.iso.datetime().nullable() })
      .strict(),
    observedAt: z.iso.datetime(),
    source: z.string().min(1).max(1_000),
    status: z.enum(["observed", "unsupported", "unknown"]),
    detail: z.string().min(1).max(2_000),
  })
  .strict();
export type UsageObservation = z.infer<typeof UsageObservationSchema>;

export const ExecutionCatalogSnapshotSchema = z
  .object({
    protocol: z.literal("zap-execution-catalog/1"),
    catalogRevision: DecimalSchema,
    preferencesRevision: DecimalSchema,
    connections: z.array(ExecutionConnectionRecordSchema).max(1_000),
    configurations: z.array(ExecutionConfigurationRecordSchema).max(5_000),
    preferences: ExecutionCatalogPreferencesSchema,
    usage: z.array(UsageObservationSchema).max(10_000),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type ExecutionCatalogSnapshot = z.infer<typeof ExecutionCatalogSnapshotSchema>;

export const ExecutionSelectionRequestSchema = z
  .object({
    selectionRef: ExecutionCatalogIdSchema,
    specialization: TaskSpecializationSchema,
    purpose: TaskPurposeSchema,
    taskClass: TaskClassSchema,
    role: z.enum(["coordinator", "worker"]),
    executionMode: ExecutionModeSchema,
    invocationScope: ModelInvocationScopeSchema,
    productId: ExecutionCatalogIdSchema,
    productVersion: z.string().min(1).max(160),
    requiredModalities: z.array(ExecutionModalitySchema).min(1).max(3),
    effort: EffortRequestSchema,
    context: ContextRequestSchema,
    override: z
      .object({
        configurationId: ExecutionCatalogIdSchema,
        reason: z.string().trim().min(1).max(2_000),
      })
      .strict()
      .nullable(),
    requestedAt: z.iso.datetime(),
  })
  .strict();
export type ExecutionSelectionRequest = z.infer<typeof ExecutionSelectionRequestSchema>;

export const TrustedExecutionCatalogContextSchema = z
  .object({
    allowedConnectionIds: z.array(ExecutionCatalogIdSchema).max(1_000),
    allowedConfigurationIds: z.array(ExecutionCatalogIdSchema).max(5_000),
    availableLaunchBindingIds: z.array(ExecutionCatalogIdSchema).max(1_000),
    parentSelection: z
      .object({
        selectionRef: ExecutionCatalogIdSchema,
        configurationId: ExecutionCatalogIdSchema,
        connectionId: ExecutionCatalogIdSchema,
        launchBindingId: ExecutionCatalogIdSchema,
        productId: ExecutionCatalogIdSchema,
        effectiveEffort: ReasoningEffortSchema.nullable(),
        appliedContextTokens: z.number().int().positive().nullable(),
      })
      .strict()
      .nullable(),
  })
  .strict();
export type TrustedExecutionCatalogContext = z.infer<typeof TrustedExecutionCatalogContextSchema>;

export const CandidateExplanationSchema = z
  .object({
    configurationId: ExecutionCatalogIdSchema,
    state: z.enum(["eligible", "excluded"]),
    score: z.number().int().nullable(),
    reasons: z.array(z.string().min(1).max(1_000)).min(1).max(32),
    usageObservationIds: z.array(ExecutionCatalogIdSchema).max(32),
  })
  .strict();
export type CandidateExplanation = z.infer<typeof CandidateExplanationSchema>;

export const ExecutionSelectionSchema = z
  .object({
    protocol: z.literal("zap-execution-selection/1"),
    selectionRef: ExecutionCatalogIdSchema,
    catalogRevision: DecimalSchema,
    preferencesRevision: DecimalSchema,
    specialization: TaskSpecializationSchema,
    configurationId: ExecutionCatalogIdSchema,
    configurationName: z.string().min(1).max(200),
    connectionId: ExecutionCatalogIdSchema,
    launchBindingId: ExecutionCatalogIdSchema,
    providerId: ExecutionCatalogIdSchema,
    agentProduct: AgentProductSchema,
    productId: ExecutionCatalogIdSchema,
    modelVendorId: ExecutionCatalogIdSchema,
    modelFamilyId: ExecutionCatalogIdSchema,
    modelId: z.string().min(1).max(256),
    requestedEffort: EffortRequestSchema,
    appliedEffort: EffectiveEffortSchema,
    requestedContext: ContextRequestSchema,
    appliedContext: AppliedContextSchema,
    overrideReason: z.string().min(1).max(2_000).nullable(),
    selectedUsageObservationIds: z.array(ExecutionCatalogIdSchema).max(32),
    explanations: z.array(CandidateExplanationSchema).min(1).max(5_000),
    application: z.literal("future_attempt"),
  })
  .strict();
export type ExecutionSelection = z.infer<typeof ExecutionSelectionSchema>;

export const ExecutionCatalogErrorSchema = z
  .object({
    code: z.enum([
      "invalid_input",
      "unauthorized",
      "forbidden",
      "not_found",
      "conflict",
      "stale_revision",
      "idempotency_conflict",
      "no_eligible_configuration",
      "override_refused",
      "closed",
      "storage_failure",
    ]),
    message: z.string().min(1).max(4_000),
  })
  .strict();
export type ExecutionCatalogError = z.infer<typeof ExecutionCatalogErrorSchema>;
export type ExecutionCatalogResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: ExecutionCatalogError };

export type { z };
