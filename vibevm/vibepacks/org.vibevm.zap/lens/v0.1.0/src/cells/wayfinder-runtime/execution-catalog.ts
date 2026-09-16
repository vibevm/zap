/** Trusted local account and adapter evidence for the execution catalog. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { createHash } from "node:crypto";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import {
  EXECUTION_MODEL_REFERENCE,
  ExecutionBindingChoiceSchema,
  ExecutionConnectionRecordSchema,
  ExecutionModelReferenceViewSchema,
  TaskSpecializationSchema,
  type AgentProduct,
  type ContextCapability,
  type ExecutionCatalogResult,
  type ExecutionConfigurationRecord,
  type ExecutionModelReference,
} from "../execution-catalog/index.ts";
import type { EffortCapability } from "../model-policy/index.ts";
import {
  codexAdapterEvidence,
  managedAdapterEvidence,
  materializeExecutionConfiguration,
  providerCoordinatorAdapterEvidence,
  type ExecutionAccountIsolationPort,
  type SafeExecutionBinding,
  type VerifiedExecutionAdapterEvidence,
} from "../execution-accounts/index.ts";
import type { ExecutionCatalogAuthorityPort } from "../execution-catalog-service/index.ts";
import type { ManagedAgentProfile } from "../managed-work/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";

export function createRuntimeExecutionCatalogAuthority(input: {
  readonly accounts: ExecutionAccountIsolationPort;
  readonly codexProfiles: readonly CodexCoordinatorProfile[];
  readonly providerProfiles: readonly ProviderCoordinatorProfile[];
  readonly managedProfiles: readonly ManagedAgentProfile[];
  readonly observedAt?: string;
}): ExecutionCatalogAuthorityPort {
  const modelReferences = availableReferences(input);
  const references = modelReferences.map(referenceView);
  return {
    availableBindings() {
      return {
        ok: true,
        value: input.accounts.list().map((binding) => ExecutionBindingChoiceSchema.parse(binding)),
      };
    },
    modelReferences() {
      return { ok: true, value: references };
    },
    createConnection(request) {
      const binding = input.accounts
        .list()
        .find((candidate) => candidate.bindingId === request.bindingId);
      return binding === undefined
        ? failure("not_found", "protected execution binding is unavailable")
        : connectionRecord(binding, request.connectionId, request.displayName, request.now);
    },
    async createConfiguration(request) {
      return materialize(input, request.connection, request.referenceId, {
        configurationId: request.configurationId,
        displayName: request.displayName,
        now: request.now,
      });
    },
    validateConnection(request) {
      const binding = input.accounts
        .list()
        .find((candidate) => candidate.bindingId === request.connection.launchBindingId);
      if (binding === undefined)
        return failure("not_found", "protected execution binding is unavailable");
      const canonical = connectionRecord(
        binding,
        request.connection.connectionId,
        request.connection.displayName,
        request.connection.updatedAt,
        request.connection.createdAt,
      );
      return canonical.ok
        ? {
            ok: true,
            value: { ...canonical.value, enabled: request.connection.enabled },
          }
        : canonical;
    },
    async validateConfiguration(request) {
      const reference = referenceForRecord(modelReferences, request.configuration);
      if (reference === undefined)
        return failure("not_found", "execution model reference is unavailable");
      const canonical = await materialize(input, request.connection, referenceId(reference), {
        configurationId: request.configuration.configurationId,
        displayName: request.configuration.displayName,
        now: request.configuration.updatedAt,
        createdAt: request.configuration.createdAt,
      });
      if (!canonical.ok) return canonical;
      if (
        !effortSubset(request.configuration.effort, canonical.value.adapterEffort) ||
        !contextSubset(request.configuration.context, canonical.value.adapterContext)
      )
        return failure(
          "invalid_input",
          "configuration effort or context exceeds installed adapter evidence",
        );
      return {
        ok: true,
        value: {
          ...canonical.value,
          enabled: request.configuration.enabled,
          scores: request.configuration.scores,
          usageBucketIds: request.configuration.usageBucketIds,
          effort: request.configuration.effort,
          context: request.configuration.context,
        },
      };
    },
    trustedContext(request) {
      return {
        ok: true,
        value: {
          allowedConnectionIds: request.snapshot.connections
            .filter((connection) => connection.enabled)
            .map((connection) => connection.connectionId),
          allowedConfigurationIds: request.snapshot.configurations
            .filter((configuration) => configuration.enabled)
            .map((configuration) => configuration.configurationId),
          availableLaunchBindingIds: input.accounts
            .list()
            .filter((binding) => binding.enabled)
            .map((binding) => binding.bindingId),
          parentSelection: request.parentSelection,
        },
      };
    },
  };
}

function connectionRecord(
  binding: SafeExecutionBinding,
  connectionId: string,
  displayName: string,
  updatedAt: string,
  createdAt = updatedAt,
) {
  const parsed = ExecutionConnectionRecordSchema.safeParse({
    connectionId,
    displayName,
    providerId: `provider.${binding.agentProduct}`,
    agentProduct: binding.agentProduct,
    launchBindingId: binding.bindingId,
    enabled: binding.enabled,
    synthetic: binding.agentProduct === "zap_mock",
    setupGuidance: binding.setupGuidance,
    createdAt,
    updatedAt,
  });
  return parsed.success
    ? { ok: true as const, value: parsed.data }
    : failure("invalid_input", "protected execution connection is invalid");
}

async function materialize(
  input: Parameters<typeof createRuntimeExecutionCatalogAuthority>[0],
  connection: Parameters<ExecutionCatalogAuthorityPort["createConfiguration"]>[0]["connection"],
  referenceIdValue: string,
  identity: {
    readonly configurationId: string;
    readonly displayName: string;
    readonly now: string;
    readonly createdAt?: string;
  },
): Promise<ExecutionCatalogResult<ExecutionConfigurationRecord>> {
  await Promise.resolve();
  const reference = availableReferences(input).find(
    (candidate) => referenceId(candidate) === referenceIdValue && candidate.conversationModel,
  );
  const binding = input.accounts
    .list()
    .find((candidate) => candidate.bindingId === connection.launchBindingId);
  if (
    reference === undefined ||
    binding === undefined ||
    !supportsFamily(input, binding, reference)
  )
    return failure("invalid_input", "model is unavailable for this protected agent connection");
  const adapter = adapterEvidence(
    input,
    binding,
    reference,
    input.observedAt ?? new Date().toISOString(),
  );
  if (adapter === null)
    return failure("not_found", "installed agent launch template is unavailable for this binding");
  const created = materializeExecutionConfiguration({
    configurationId: identity.configurationId,
    displayName: identity.displayName,
    connectionId: connection.connectionId,
    providerId: connection.providerId,
    productId: adapter.profileId,
    modelVendorId: catalogId("vendor", reference.vendor),
    referenceModelId: reference.modelId,
    reference,
    binding,
    adapter: adapter.evidence,
    usageBucketIds: [],
    now: identity.now,
  });
  if (!created.ok) return mapAccountResult(created);
  return {
    ok: true as const,
    value: {
      ...created.value,
      synthetic: binding.agentProduct === "zap_mock",
      ...(identity.createdAt === undefined ? {} : { createdAt: identity.createdAt }),
    },
  };
}

function adapterEvidence(
  input: Parameters<typeof createRuntimeExecutionCatalogAuthority>[0],
  binding: SafeExecutionBinding,
  reference: ExecutionModelReference,
  observedAt: string,
): {
  readonly profileId: string;
  readonly evidence: Parameters<typeof materializeExecutionConfiguration>[0]["adapter"];
} | null {
  const managed =
    input.managedProfiles.find(
      (profile) =>
        profile.provider === binding.agentProduct &&
        profile.accountBindingId === binding.bindingId &&
        profileModelCompatible(profile.provider, profile.modelId, reference.modelId),
    ) ??
    input.managedProfiles.find(
      (profile) =>
        profile.provider === binding.agentProduct &&
        profileModelCompatible(profile.provider, profile.modelId, reference.modelId) &&
        (binding.agentProduct === "codex" ||
          (binding.agentProduct === "claude_code" && profile.environmentRef === null)),
    );
  const managedEvidence =
    managed === undefined ? null : managedAdapterEvidence(managed, reference, observedAt);
  if (binding.agentProduct === "codex") {
    const profile =
      input.codexProfiles.find((candidate) => candidate.accountBindingId === binding.bindingId) ??
      input.codexProfiles.find((candidate) => candidate.observedModelCapabilities === undefined);
    if (profile === undefined)
      return managed === undefined || managedEvidence === null
        ? null
        : { profileId: managed.profileId, evidence: managedEvidence };
    if (
      profile.observedModelCapabilities !== undefined &&
      !profile.observedModelCapabilities.some(
        (candidate) => candidate.modelId === reference.modelId,
      )
    )
      return null;
    const coordinator = codexAdapterEvidence(profile, reference, observedAt);
    return {
      profileId: profile.profileId,
      evidence:
        managedEvidence === null ? coordinator : mergedEvidence(coordinator, managedEvidence),
    };
  }
  const profile =
    input.providerProfiles.find(
      (candidate) =>
        candidate.provider === binding.agentProduct &&
        candidate.accountBindingId === binding.bindingId &&
        profileModelCompatible(candidate.provider, candidate.modelId, reference.modelId),
    ) ??
    input.providerProfiles.find(
      (candidate) =>
        candidate.provider === binding.agentProduct &&
        profileModelCompatible(candidate.provider, candidate.modelId, reference.modelId) &&
        binding.agentProduct === "claude_code" &&
        candidate.environmentRef == null,
    );
  if (profile === undefined)
    return managed === undefined || managedEvidence === null
      ? null
      : { profileId: managed.profileId, evidence: managedEvidence };
  const coordinator = providerCoordinatorAdapterEvidence(profile, reference, observedAt);
  return {
    profileId: profile.profileId,
    evidence: managedEvidence === null ? coordinator : mergedEvidence(coordinator, managedEvidence),
  };
}

function mergedEvidence(
  coordinator: VerifiedExecutionAdapterEvidence,
  managed: VerifiedExecutionAdapterEvidence,
): VerifiedExecutionAdapterEvidence {
  const modalities = coordinator.modalities.filter((value) => managed.modalities.includes(value));
  const tools = coordinator.toolCapabilities.filter((value) =>
    managed.toolCapabilities.includes(value),
  );
  return {
    agentProduct: coordinator.agentProduct,
    executionModes: [...new Set([...coordinator.executionModes, ...managed.executionModes])],
    invocationScopes: [...new Set([...coordinator.invocationScopes, ...managed.invocationScopes])],
    modalities,
    effort:
      coordinator.agentProduct === "codex"
        ? coordinator.effort
        : intersectEffort(coordinator.effort, managed.effort),
    context:
      coordinator.agentProduct === "codex"
        ? coordinator.context
        : intersectContext(coordinator.context, managed.context),
    toolCapabilities: tools,
    evidenceSource: `${coordinator.evidenceSource}; ${managed.evidenceSource}`,
    observedAt: coordinator.observedAt,
  };
}

function intersectEffort(left: EffortCapability, right: EffortCapability): EffortCapability {
  if (left.mode !== "configurable" || right.mode !== "configurable")
    return left.mode === right.mode
      ? left
      : { mode: "unknown", reason: "coordinator and managed effort evidence differ" };
  const allowedValues = left.allowedValues.filter((value) => right.allowedValues.includes(value));
  return allowedValues.length === 0
    ? { mode: "unknown", reason: "coordinator and managed effort values do not intersect" }
    : {
        mode: "configurable",
        allowedValues,
        defaultValue:
          left.defaultValue !== null && allowedValues.includes(left.defaultValue)
            ? left.defaultValue
            : (allowedValues[0] ?? null),
      };
}

function intersectContext(left: ContextCapability, right: ContextCapability): ContextCapability {
  if (left.mode !== "configurable" || right.mode !== "configurable")
    return left.mode === right.mode
      ? left
      : { mode: "unknown", reason: "coordinator and managed context evidence differ" };
  const allowedTokens = left.allowedTokens.filter((value) => right.allowedTokens.includes(value));
  return allowedTokens.length === 0
    ? { mode: "unknown", reason: "coordinator and managed context values do not intersect" }
    : {
        mode: "configurable",
        allowedTokens,
        defaultTokens:
          left.defaultTokens !== null && allowedTokens.includes(left.defaultTokens)
            ? left.defaultTokens
            : (allowedTokens[0] ?? null),
        documentedMaximumTokens:
          left.documentedMaximumTokens === null
            ? right.documentedMaximumTokens
            : right.documentedMaximumTokens === null
              ? left.documentedMaximumTokens
              : Math.min(left.documentedMaximumTokens, right.documentedMaximumTokens),
      };
}

function referenceView(reference: ExecutionModelReference) {
  return ExecutionModelReferenceViewSchema.parse({
    referenceId: referenceId(reference),
    modelVendorId: catalogId("vendor", reference.vendor),
    modelFamilyId: reference.familyId,
    modelId: reference.modelId,
    conversationModel: reference.conversationModel,
    availability: reference.launchAvailability,
    specializations: reference.presets.map((preset) => preset.specialization),
    sourceUrls: reference.sourceUrls,
    note: reference.note,
  });
}

function referenceForRecord(
  references: readonly ExecutionModelReference[],
  record: { readonly modelId: string; readonly modelFamilyId: string },
) {
  return references.find(
    (candidate) =>
      candidate.modelId === record.modelId && candidate.familyId === record.modelFamilyId,
  );
}
function referenceId(reference: ExecutionModelReference): string {
  return catalogId("reference", `${reference.familyId}.${reference.modelId}`);
}
function catalogId(kind: string, value: string): string {
  return `${kind}.${createHash("sha256").update(value).digest("hex").slice(0, 24)}`;
}
function supportsFamily(
  input: Parameters<typeof createRuntimeExecutionCatalogAuthority>[0],
  binding: SafeExecutionBinding,
  reference: ExecutionModelReference,
): boolean {
  if (reference.familyId === hostFamily(binding.agentProduct, binding.bindingId)) return true;
  if (!supportsProductFamily(binding.agentProduct, reference)) return false;
  if (binding.agentProduct !== "opencode" && binding.agentProduct !== "qwen_code") return true;
  return [...input.providerProfiles, ...input.managedProfiles].some(
    (profile) =>
      profile.provider === binding.agentProduct &&
      profile.accountBindingId === binding.bindingId &&
      profile.modelId === reference.modelId,
  );
}

function availableReferences(
  input: Parameters<typeof createRuntimeExecutionCatalogAuthority>[0],
): readonly ExecutionModelReference[] {
  const configured: ConfiguredReferenceInput[] = [
    ...input.codexProfiles.map((profile) =>
      configuredReference(
        "codex",
        profile.accountBindingId,
        profile.model,
        profile.effort ?? null,
        profile.contextWindowTokens ?? null,
      ),
    ),
    ...input.providerProfiles.map((profile) =>
      configuredReference(
        profile.provider,
        profile.accountBindingId,
        profile.modelId,
        profile.effort,
        null,
      ),
    ),
    ...input.managedProfiles.map((profile) =>
      configuredReference(
        profile.provider,
        profile.accountBindingId,
        profile.modelId,
        profile.effort,
        profile.contextWindowTokens ?? null,
      ),
    ),
  ];
  const additions = new Map<string, ExecutionModelReference>();
  for (const profile of configured) {
    if (
      profile.bindingId === undefined ||
      EXECUTION_MODEL_REFERENCE.some(
        (reference) =>
          reference.modelId === profile.modelId &&
          supportsProductFamily(profile.product, reference),
      )
    )
      continue;
    const familyId = hostFamily(profile.product, profile.bindingId);
    const key = `${familyId}\u0000${profile.modelId}`;
    additions.set(key, {
      familyId,
      vendor: `Host-configured ${profile.product}`,
      modelId: profile.modelId,
      status: "metadata_incomplete",
      conversationModel: true,
      launchAvailability: "verified_profile_required",
      inputModalities: ["text"],
      outputModalities: ["text"],
      documentedContextMaximumTokens: profile.context,
      maximumOutputTokens: null,
      effort: {
        control: profile.effort === null ? "unknown" : "configured_profile",
        values: profile.effort === null ? [] : [profile.effort],
        defaultValue: profile.effort,
      },
      toolCapabilities: [],
      presets: TaskSpecializationSchema.options
        .filter((specialization) => specialization !== "image_generation")
        .map((specialization) => ({
          specialization,
          suitability: 50,
          quality: 50,
          economy: 50,
          preferenceOrder: 500,
          rationale:
            "Trusted host-configured compatibility entry; task fit is unknown and editable.",
          provenance: "owner",
        })),
      sourceUrls: [],
      note: "Exact protected host profile retained for compatibility; public model metadata is unknown.",
    });
  }
  return [...EXECUTION_MODEL_REFERENCE, ...additions.values()];
}

interface ConfiguredReferenceInput {
  readonly product: AgentProduct;
  readonly bindingId: string | undefined;
  readonly modelId: string;
  readonly effort: string | null;
  readonly context: number | null;
}

function configuredReference(
  product: AgentProduct,
  bindingId: string | undefined,
  modelId: string,
  effort: string | null,
  context: number | null,
): ConfiguredReferenceInput {
  return { product, bindingId, modelId, effort, context };
}

function supportsProductFamily(product: AgentProduct, reference: ExecutionModelReference): boolean {
  return (
    product === "opencode" ||
    (product === "zap_mock" && reference.familyId === "zap_mock") ||
    (product === "codex" && reference.familyId === "openai_gpt") ||
    (product === "claude_code" && reference.familyId === "anthropic_claude") ||
    (product === "qwen_code" && reference.familyId === "alibaba_qwen")
  );
}

function hostFamily(product: string, bindingId: string): string {
  return `host_configured_${product}_${createHash("sha256").update(bindingId).digest("hex").slice(0, 12)}`;
}
function profileModelCompatible(
  product: AgentProduct,
  configuredModelId: string,
  requestedModelId: string,
): boolean {
  return product !== "opencode" && product !== "qwen_code"
    ? true
    : configuredModelId === requestedModelId;
}
function effortSubset(owner: EffortCapability, adapter: EffortCapability): boolean {
  if (owner.mode !== adapter.mode) return false;
  if (owner.mode !== "configurable" || adapter.mode !== "configurable") return true;
  return (
    owner.allowedValues.every((value) => adapter.allowedValues.includes(value)) &&
    (owner.defaultValue === null || owner.allowedValues.includes(owner.defaultValue))
  );
}
function contextSubset(owner: ContextCapability, adapter: ContextCapability): boolean {
  if (owner.mode !== adapter.mode) return false;
  if (owner.mode !== "configurable" || adapter.mode !== "configurable") return true;
  return (
    owner.allowedTokens.every((value) => adapter.allowedTokens.includes(value)) &&
    (owner.defaultTokens === null || owner.allowedTokens.includes(owner.defaultTokens)) &&
    (adapter.documentedMaximumTokens === null ||
      owner.documentedMaximumTokens === null ||
      owner.documentedMaximumTokens <= adapter.documentedMaximumTokens)
  );
}
function mapAccountResult<T>(
  result:
    | { readonly ok: true; readonly value: T }
    | { readonly ok: false; readonly error: { readonly message: string } },
): ExecutionCatalogResult<T> {
  return result.ok ? result : failure("invalid_input", result.error.message);
}
function failure(
  code: "invalid_input" | "not_found",
  message: string,
): ExecutionCatalogResult<never> {
  return { ok: false, error: { code, message } };
}
