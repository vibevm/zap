/** Managed model and execution-catalog selection. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import {
  ModelSelectionRequestSchema,
  ModelSelectionSchema,
  ReasoningEffortSchema,
  TrustedModelContextSchema,
  type EffortCapability,
  type ModelSelection,
} from "../model-policy/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  type ModelPolicyStore,
} from "../model-policy-store/index.ts";
import type { CoordinatorRoutingProvider } from "../coordinator-routing/index.ts";
import type {
  ManagedAgentProfile,
  ManagedSelectionPort,
  ManagedWorkResult,
  ManagedWorkStore,
} from "../managed-work/index.ts";
import { AttemptIdSchema, RunIdSchema } from "../workspace-model/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  parentCatalogSelection,
  revalidateCatalogManaged,
  selectCatalogManaged,
  type ManagedCatalogRuntime,
} from "./managed-catalog-selection.ts";

export function createManagedSelectionPort(
  policyStore: ModelPolicyStore,
  routingProvider: CoordinatorRoutingProvider | undefined,
  workStore: ManagedWorkStore,
  executionCatalog: ManagedCatalogRuntime | undefined,
): ManagedSelectionPort {
  return {
    async resolve(access, request, profiles, identity) {
      if (
        request.selection.mode === "catalog_policy" ||
        request.selection.mode === "catalog_override"
      ) {
        if (executionCatalog === undefined)
          return {
            ok: false,
            error: {
              code: "unavailable",
              message: "execution catalog selection is not configured for this runtime",
            },
          };
        const parent = request.parentRunId === null ? null : workStore.load(request.parentRunId);
        const selected = await selectCatalogManaged({
          catalog: executionCatalog,
          access,
          request,
          profiles,
          identity,
          parentSelection: parentCatalogSelection(parent?.ok === true ? parent.value : null),
        });
        if (!selected.ok) return selected;
        const stored = storeSelection(
          policyStore,
          ModelPolicyStoreAccessSchema.parse(access),
          request,
          identity,
          selected.value.modelSelection,
        );
        return stored.ok ? selected : stored;
      }
      const selected =
        request.selection.mode === "profile_override"
          ? explicitProfileSelection(policyStore, workStore, access, request, profiles, identity)
          : await projectPolicySelection(
              policyStore,
              routingProvider,
              workStore,
              access,
              request,
              profiles,
              identity,
            );
      if (!selected.ok) return selected;
      const profile = profiles.find(
        (candidate) => candidate.profileId === selected.value.profileId,
      );
      return profile === undefined
        ? {
            ok: false,
            error: { code: "forbidden", message: "selected managed profile is unavailable" },
          }
        : {
            ok: true,
            value: { profile, modelSelection: selected.value, executionSelection: null },
          };
    },
    revalidate(access, claim) {
      return revalidateCatalogManaged({ catalog: executionCatalog, access, claim });
    },
  };
}

async function projectPolicySelection(
  policyStore: ModelPolicyStore,
  routingProvider: CoordinatorRoutingProvider | undefined,
  workStore: ManagedWorkStore,
  access: Parameters<ManagedSelectionPort["resolve"]>[0],
  request: Parameters<ManagedSelectionPort["resolve"]>[1],
  profiles: Parameters<ManagedSelectionPort["resolve"]>[2],
  identity: Parameters<ManagedSelectionPort["resolve"]>[3],
): Promise<ManagedWorkResult<ModelSelection>> {
  const policyAccess = ModelPolicyStoreAccessSchema.parse(access);
  const currentPolicy = policyStore.readPolicy(policyAccess, request.projectId, request.contextId);
  if (!currentPolicy.ok)
    return { ok: false, error: { code: "unavailable", message: currentPolicy.error.message } };
  const contexts = new Map(
    profiles.map((profile) => [
      `${profile.provider}\u0000${profile.capabilities.observedVersion ?? "unknown"}`,
      {
        productId: profile.provider,
        productVersion: profile.capabilities.observedVersion ?? "unknown",
      },
    ]),
  );
  const localTrusted = trustedContext(
    profiles,
    workStore,
    request.parentRunId,
    currentPolicy.value.policy.tierBindings.map((binding) => binding.profileId),
  );
  const selections: ModelSelection[] = [];
  const errors: string[] = [];
  for (const product of contexts.values()) {
    const modelRequest = selectionRequest(request, product.productId, product.productVersion, null);
    const trusted =
      routingProvider === undefined
        ? { ok: true as const, value: localTrusted }
        : await routingProvider.trustedContext({
            access: policyAccess,
            projectId: request.projectId,
            contextId: request.contextId,
            request: modelRequest,
          });
    if (!trusted.ok) {
      errors.push(trusted.error.message);
      continue;
    }
    const preview = policyStore.resolvePreview(policyAccess, {
      projectId: request.projectId,
      contextId: request.contextId,
      request: modelRequest,
      trustedContext: trusted.value,
    });
    if (!preview.ok) errors.push(preview.error.message);
    else if (!preview.value.ok) errors.push(preview.value.error.message);
    else {
      const concrete = concreteSelection(preview.value.value, profiles);
      if (concrete.ok) selections.push(concrete.value);
      else errors.push(concrete.error.message);
    }
  }
  if (selections.length !== 1)
    return {
      ok: false,
      error: {
        code: selections.length > 1 ? "conflict" : "unavailable",
        message:
          selections.length > 1
            ? "project policy resolves more than one managed worker product"
            : (errors[0] ?? "project policy has no compatible managed worker profile"),
      },
    };
  const selected = selections[0];
  if (selected === undefined)
    return { ok: false, error: { code: "unavailable", message: "policy selection is missing" } };
  return storeSelection(policyStore, policyAccess, request, identity, selected);
}

function explicitProfileSelection(
  policyStore: ModelPolicyStore,
  workStore: ManagedWorkStore,
  access: Parameters<ManagedSelectionPort["resolve"]>[0],
  request: Parameters<ManagedSelectionPort["resolve"]>[1],
  profiles: Parameters<ManagedSelectionPort["resolve"]>[2],
  identity: Parameters<ManagedSelectionPort["resolve"]>[3],
): ManagedWorkResult<ModelSelection> {
  if (request.selection.mode !== "profile_override")
    return { ok: false, error: { code: "invalid_input", message: "profile override is missing" } };
  const override = request.selection;
  const profile = profiles.find((candidate) => candidate.profileId === override.profileId);
  if (profile === undefined)
    return { ok: false, error: { code: "forbidden", message: "override profile is unavailable" } };
  const policyAccess = ModelPolicyStoreAccessSchema.parse(access);
  const storedPolicy = policyStore.readPolicy(policyAccess, request.projectId, request.contextId);
  if (!storedPolicy.ok)
    return { ok: false, error: { code: "unavailable", message: storedPolicy.error.message } };
  const inherited = parentSelection(workStore, request.parentRunId);
  const effort = overrideEffort(profile, inherited);
  const tier =
    profile.tier ??
    storedPolicy.value.policy.tierBindings.find(
      (binding) => binding.profileId === profile.profileId || binding.modelId === profile.modelId,
    )?.tier ??
    "small";
  const selection = ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: `selection.${request.clientRequestId}`,
    policyId: storedPolicy.value.policy.policyId,
    policyRevision: storedPolicy.value.policy.revision,
    ruleId: "rule.explicit-profile-override",
    overrideRef: `override.${request.clientRequestId}`,
    selectionReason: "Use the explicit managed worker profile selected for this task.",
    overrideReason: override.reasonMarkdown,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: tier,
    requestedEffort: effort.requested,
    profileId: profile.profileId,
    productId: profile.provider,
    productVersion: profile.capabilities.observedVersion ?? "unknown",
    providerId: profile.provider,
    modelId: profile.modelId,
    capabilityId: null,
    effortCapability: effort.capability,
    extendedThinking: effort.extendedThinking,
    effectiveEffort: effort.effective,
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
  return storeSelection(policyStore, policyAccess, request, identity, selection);
}

function storeSelection(
  policyStore: ModelPolicyStore,
  policyAccess: ModelPolicyStoreAccessSchemaType,
  request: Parameters<ManagedSelectionPort["resolve"]>[1],
  identity: Parameters<ManagedSelectionPort["resolve"]>[3],
  selection: ModelSelection,
): ManagedWorkResult<ModelSelection> {
  const stored = policyStore.storeSelection(policyAccess, {
    projectId: request.projectId,
    contextId: request.contextId,
    runId: RunIdSchema.parse(identity.runId),
    attemptId: AttemptIdSchema.parse(identity.attemptId),
    clientRequestId: ClientRequestIdSchema.parse(request.clientRequestId),
    sourceEventId: `managed-work.selection.${request.clientRequestId}`,
    selection,
  });
  return stored.ok
    ? { ok: true as const, value: selection }
    : { ok: false as const, error: { code: "unavailable", message: stored.error.message } };
}

type ModelPolicyStoreAccessSchemaType = ReturnType<typeof ModelPolicyStoreAccessSchema.parse>;

function selectionRequest(
  request: Parameters<ManagedSelectionPort["resolve"]>[1],
  productId: string,
  productVersion: string,
  override: null,
) {
  return ModelSelectionRequestSchema.parse({
    selectionRef: `selection.${request.clientRequestId}`,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    productId,
    productVersion,
    override,
  });
}

function trustedContext(
  profiles: readonly ManagedAgentProfile[],
  workStore: ManagedWorkStore,
  parentRunId: string | null,
  policyProfileIds: readonly string[],
) {
  const capabilities = new Map<string, ReturnType<typeof capabilityFor>>();
  for (const profile of profiles) {
    const capability = capabilityFor(profile);
    capabilities.set(
      `${capability.productId}\u0000${capability.productVersion}\u0000${capability.modelId}`,
      capability,
    );
  }
  return TrustedModelContextSchema.parse({
    capabilities: [...capabilities.values()],
    allowedProfileIds: [
      ...new Set([...profiles.map((profile) => profile.profileId), ...policyProfileIds]),
    ],
    parentSelection: parentSelection(workStore, parentRunId),
    actualObservation: null,
  });
}

function concreteSelection(
  selection: ModelSelection,
  profiles: readonly ManagedAgentProfile[],
): ManagedWorkResult<ModelSelection> {
  const exact = profiles.filter((profile) => profile.profileId === selection.profileId);
  const compatible =
    exact.length > 0
      ? exact
      : profiles.filter(
          (profile) =>
            profile.tier === selection.requestedTier &&
            profile.provider === selection.productId &&
            profile.modelId === selection.modelId,
        );
  if (compatible.length !== 1)
    return {
      ok: false,
      error: {
        code: compatible.length === 0 ? "unavailable" : "conflict",
        message:
          compatible.length === 0
            ? "policy tier has no compatible registered worker profile"
            : "policy tier matches more than one registered worker profile",
      },
    };
  const profile = compatible[0];
  return profile === undefined
    ? { ok: false, error: { code: "unavailable", message: "worker profile is missing" } }
    : {
        ok: true,
        value: ModelSelectionSchema.parse({
          ...selection,
          profileId: profile.profileId,
          selectionReason:
            selection.selectionReason + " Resolved to a project-scoped protected worker profile.",
        }),
      };
}

function capabilityFor(profile: ManagedAgentProfile) {
  return {
    capabilityId: `capability.${profile.profileId}`,
    productId: profile.provider,
    productVersion: profile.capabilities.observedVersion ?? "unknown",
    executionMode: "managed" as const,
    invocationScope: "managed_agent" as const,
    modelId: profile.modelId,
    effort: effortCapability(profile),
    extendedThinking:
      profile.provider === "codex" || profile.provider === "claude_code"
        ? ("configurable" as const)
        : ("unsupported" as const),
    evidence: {
      source: profile.capabilities.evidence[0] ?? "Protected managed worker profile",
      observedAt: new Date().toISOString(),
    },
  };
}

function effortCapability(profile: ManagedAgentProfile): EffortCapability {
  if (["opencode", "qwen_code", "zap_mock"].includes(profile.provider))
    return { mode: "unsupported" };
  const effort = ReasoningEffortSchema.safeParse(profile.effort);
  return profile.effortSupported && effort.success
    ? { mode: "configurable", allowedValues: [effort.data], defaultValue: effort.data }
    : { mode: "unknown", reason: "Managed profile effort capability is not observed" };
}

function parentSelection(workStore: ManagedWorkStore, parentRunId: string | null) {
  if (parentRunId === null) return null;
  const loaded = workStore.load(parentRunId);
  if (!loaded.ok) return null;
  return {
    selectionRef: loaded.value.modelSelection.selectionRef,
    profileId: loaded.value.modelSelection.profileId,
    modelId: loaded.value.modelSelection.modelId,
    effectiveEffort: loaded.value.modelSelection.effectiveEffort,
  };
}

function overrideEffort(profile: ManagedAgentProfile, parent: ReturnType<typeof parentSelection>) {
  const capability = effortCapability(profile);
  if (capability.mode === "inherited")
    return {
      requested: { mode: "inherit" as const },
      capability,
      extendedThinking: "inherited" as const,
      effective:
        parent === null
          ? {
              state: "unknown" as const,
              reason: "Anthropic effort remains inherited but no parent selection is recorded.",
            }
          : {
              state: "inherited" as const,
              value:
                parent.effectiveEffort.state === "unsupported" ||
                parent.effectiveEffort.state === "unknown"
                  ? null
                  : parent.effectiveEffort.value,
              sourceSelectionRef: parent.selectionRef,
            },
    };
  if (capability.mode === "configurable" && profile.effort !== null)
    return {
      requested: { mode: "explicit" as const, value: capability.allowedValues[0] ?? "low" },
      capability,
      extendedThinking: "configurable" as const,
      effective: {
        state: "explicit" as const,
        value: capability.allowedValues[0] ?? "low",
      },
    };
  return {
    requested: { mode: "unspecified" as const },
    capability,
    extendedThinking: "unsupported" as const,
    effective:
      capability.mode === "unsupported"
        ? { state: "unsupported" as const }
        : { state: "unknown" as const, reason: "Profile effort capability is unknown" },
  };
}
