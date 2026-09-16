/** Server-validated reference-to-configuration materialization. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import {
  ExecutionConfigurationRecordSchema,
  EXECUTION_MODEL_REFERENCE,
  EXECUTION_MODEL_REFERENCE_AS_OF,
  type ExecutionConfigurationRecord,
  type ContextCapability,
  type ExecutionModality,
  type ExecutionModelReference,
} from "../execution-catalog/index.ts";
import type { EffortCapability } from "../model-policy/index.ts";
import type { SafeExecutionBinding } from "./types.ts";
import type { ExecutionAccountResult } from "./types.ts";
import type { VerifiedExecutionAdapterEvidence } from "./adapter-evidence.ts";

export function materializeExecutionConfiguration(input: {
  readonly configurationId: string;
  readonly displayName: string;
  readonly connectionId: string;
  readonly providerId: string;
  readonly productId: string;
  readonly modelVendorId: string;
  readonly referenceModelId: string;
  readonly reference?: ExecutionModelReference;
  readonly binding: SafeExecutionBinding;
  readonly adapter: VerifiedExecutionAdapterEvidence;
  readonly usageBucketIds: readonly string[];
  readonly now: string;
}): ExecutionAccountResult<ExecutionConfigurationRecord> {
  const reference =
    input.reference?.modelId === input.referenceModelId
      ? input.reference
      : EXECUTION_MODEL_REFERENCE.find((candidate) => candidate.modelId === input.referenceModelId);
  if (reference === undefined || !reference.conversationModel)
    return failure("reference model is unavailable as a conversation model");
  if (!input.binding.enabled || input.binding.agentProduct !== input.adapter.agentProduct)
    return failure("selected protected binding is disabled or belongs to another agent product");
  if (!modalitiesSupported(reference, input.adapter.modalities, input.adapter.toolCapabilities))
    return failure("adapter modalities are not supported by the model and verified tools");
  if (!effortSupported(reference, input.adapter.effort))
    return failure("adapter effort evidence exceeds the official model reference");
  if (!contextSupported(reference, input.adapter.context))
    return failure("adapter context evidence exceeds the documented model maximum");
  const parsed = ExecutionConfigurationRecordSchema.safeParse({
    configurationId: input.configurationId,
    displayName: input.displayName,
    connectionId: input.connectionId,
    providerId: input.providerId,
    agentProduct: input.adapter.agentProduct,
    productId: input.productId,
    modelVendorId: input.modelVendorId,
    modelFamilyId: reference.familyId,
    modelId: reference.modelId,
    executionModes: input.adapter.executionModes,
    invocationScopes: input.adapter.invocationScopes,
    modalities: input.adapter.modalities,
    adapterEffort: input.adapter.effort,
    effort: input.adapter.effort,
    adapterContext: input.adapter.context,
    context: input.adapter.context,
    scores: reference.presets,
    usageBucketIds: input.usageBucketIds,
    enabled: true,
    synthetic: input.binding.agentProduct === "zap_mock",
    evidence: {
      source: `${input.adapter.evidenceSource}; reference ${EXECUTION_MODEL_REFERENCE_AS_OF} ${reference.sourceUrls[0] ?? "unknown"}`,
      observedAt: input.adapter.observedAt,
    },
    createdAt: input.now,
    updatedAt: input.now,
  });
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("verified execution configuration is invalid");
}

function modalitiesSupported(
  reference: ExecutionModelReference,
  modalities: readonly ExecutionModality[],
  tools: readonly string[],
): boolean {
  return modalities.every((modality) => {
    if (modality === "text") return reference.outputModalities.includes("text");
    if (modality === "image_input") return reference.inputModalities.includes("image");
    return (
      reference.outputModalities.includes("image") ||
      (reference.toolCapabilities.includes("image_generation_tool") &&
        tools.includes("image_generation_tool"))
    );
  });
}

function effortSupported(reference: ExecutionModelReference, effort: EffortCapability): boolean {
  if (effort.mode !== "configurable") return true;
  return effort.allowedValues.every((value) => reference.effort.values.includes(value));
}

function contextSupported(reference: ExecutionModelReference, context: ContextCapability): boolean {
  const maximum = reference.documentedContextMaximumTokens;
  if (context.mode === "configurable")
    return (
      maximum !== null &&
      context.allowedTokens.every((tokens) => tokens <= maximum) &&
      (context.documentedMaximumTokens === null || context.documentedMaximumTokens <= maximum)
    );
  return (
    context.mode !== "fixed" ||
    context.tokens === null ||
    maximum === null ||
    context.tokens <= maximum
  );
}

function failure(message: string): ExecutionAccountResult<never> {
  return { ok: false, error: { code: "invalid_input", message } };
}
