/** Trusted managed-agent runtime composition. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { randomUUID } from "node:crypto";
import {
  createManagedAgentBackend,
  createManagedProviderDrivers,
  openManagedWorkStore,
  type ManagedActorBindingPort,
  type ManagedAgentBackend,
  type ManagedAgentProfile,
  type ManagedWorkStore,
  type WorkAttachmentPort,
  type ManagedParentPort,
  type ManagedSelectionPort,
  type ManagedWorkResult,
  type ProtectedEnvironmentPort,
} from "../managed-work/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
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
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { AttemptIdSchema, RunIdSchema } from "../workspace-model/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  createAnnotationWorkAttachmentPort,
  type AnnotationTargetResolver,
  type AnnotationStore,
} from "../workspace-annotations/index.ts";

export interface ConfiguredManagedWorkRuntime {
  readonly backend: ManagedAgentBackend;
  readonly store: ManagedWorkStore;
  close(): void;
}

export function openConfiguredManagedWorkRuntime(input: {
  readonly profiles: readonly ManagedAgentProfile[];
  readonly databasePath: string;
  readonly terminals: ManagedTerminalServicePort;
  readonly bindings: ManagedActorBindingPort;
  readonly policyStore: ModelPolicyStore;
  readonly routingProvider: CoordinatorRoutingProvider | undefined;
  readonly attachments?: WorkAttachmentPort;
  readonly annotationStore?: AnnotationStore;
  readonly annotationResolver?: AnnotationTargetResolver;
  readonly proxyPolicy?: ProxyPolicy;
  readonly environment?: ProtectedEnvironmentPort;
  readonly workspaceStore: WorkspaceStore;
}):
  | { readonly ok: true; readonly value: ConfiguredManagedWorkRuntime }
  | { readonly ok: false; readonly message: string } {
  const opened = openManagedWorkStore(input.databasePath);
  if (!opened.ok) return { ok: false, message: opened.error.message };
  const store = opened.value;
  const backend = createManagedAgentBackend({
    store,
    terminals: input.terminals,
    profiles: input.profiles,
    drivers: createManagedProviderDrivers(
      input.proxyPolicy === undefined ? {} : { proxyPolicy: input.proxyPolicy },
    ),
    environment: input.environment ?? protectedEnvironment(),
    bindings: input.bindings,
    selections: selectionPort(input.policyStore, input.routingProvider, store),
    parents: parentPort(store, input.workspaceStore),
    attachments:
      input.attachments ??
      (input.annotationStore === undefined
        ? attachmentPort()
        : createAnnotationWorkAttachmentPort({
            store: input.annotationStore,
            ...(input.annotationResolver === undefined
              ? {}
              : { resolver: input.annotationResolver }),
          })),
    execution: {
      canStart(access, claim) {
        const execution = input.workspaceStore.readProjectExecution(
          claim.packet.projectId,
          claim.packet.contextId,
        );
        if (!execution.ok)
          return {
            ok: false,
            error: { code: "unavailable", message: execution.error.message },
          };
        return access.authorizedProjectIds.includes(claim.packet.projectId) &&
          (execution.value.state === "uninitialized" || execution.value.state === "running")
          ? { ok: true, value: null }
          : {
              ok: false,
              error: { code: "conflict", message: "project execution gate blocks managed start" },
            };
      },
    },
    id: (kind) => `${kind}.${randomUUID().replaceAll("-", "")}`,
  });
  return {
    ok: true,
    value: {
      backend,
      store,
      close() {
        store.close();
      },
    },
  };
}

function protectedEnvironment() {
  return {
    async resolve(reference: string | null) {
      await Promise.resolve();
      return reference === null
        ? { ok: true as const, value: {} }
        : {
            ok: false as const,
            message: "managed environment references require a trusted runtime provider",
          };
    },
  };
}

function parentPort(store: ManagedWorkStore, workspaceStore: WorkspaceStore): ManagedParentPort {
  return {
    validate(access, request) {
      if (!access.authorizedProjectIds.includes(request.projectId))
        return {
          ok: false,
          error: { code: "forbidden", message: "parent managed work is outside scope" },
        };
      if ((request.parentTaskId === null) !== (request.parentRunId === null))
        return {
          ok: false,
          error: { code: "invalid_input", message: "parent task and run must be paired" },
        };
      if (request.depth > 32)
        return {
          ok: false,
          error: { code: "forbidden", message: "managed work nesting depth is bounded" },
        };
      let parentActorId: string | null = null;
      if (request.parentRunId !== null) {
        const parent = store.load(request.parentRunId);
        if (!parent.ok)
          return {
            ok: false,
            error: { code: "forbidden", message: "parent managed run is unavailable" },
          };
        if (
          parent.value.taskId !== request.parentTaskId ||
          parent.value.packet.projectId !== request.projectId ||
          parent.value.packet.contextId !== request.contextId ||
          request.projectedParentActorId !== parent.value.actorId ||
          ["failed", "stopped", "uncertain"].includes(parent.value.state) ||
          request.depth !== parent.value.packet.depth + 1
        )
          return {
            ok: false,
            error: { code: "forbidden", message: "parent managed run is outside scope" },
          };
        parentActorId = parent.value.actorId;
      } else {
        if (request.depth !== 0)
          return {
            ok: false,
            error: { code: "invalid_input", message: "root managed work must have depth zero" },
          };
        if (request.projectedParentActorId !== null) {
          const network = workspaceStore.read(access, {
            operation: "agent.network.v1",
            projectId: request.projectId,
            contextId: request.contextId,
          });
          if (
            !network.ok ||
            network.value.operation !== "agent.network.v1" ||
            !network.value.network.agents.some(
              (agent) =>
                agent.actorId === request.projectedParentActorId && agent.role === "coordinator",
            )
          )
            return {
              ok: false,
              error: { code: "forbidden", message: "coordinator parent actor is unavailable" },
            };
          parentActorId = request.projectedParentActorId;
        }
      }
      return {
        ok: true,
        value: {
          parentTaskId: request.parentTaskId,
          parentRunId: request.parentRunId,
          parentActorId,
          depth: request.depth,
        },
      };
    },
  };
}

function attachmentPort(): WorkAttachmentPort {
  return {
    async prepareBeforeWork(input) {
      await Promise.resolve();
      return {
        ok: true,
        value: {
          state: input.targets.length === 0 ? ("ready" as const) : ("waiting_for_target" as const),
          instructions: [],
        },
      };
    },
    async acknowledge() {
      await Promise.resolve();
      return { ok: true, value: null };
    },
  };
}

function selectionPort(
  policyStore: ModelPolicyStore,
  routingProvider: CoordinatorRoutingProvider | undefined,
  workStore: ManagedWorkStore,
): ManagedSelectionPort {
  return {
    async resolve(access, request, profiles, identity) {
      return request.selection.mode === "profile_override"
        ? explicitProfileSelection(policyStore, workStore, access, request, profiles, identity)
        : projectPolicySelection(
            policyStore,
            routingProvider,
            workStore,
            access,
            request,
            profiles,
            identity,
          );
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
  if (profile.provider === "opencode" || profile.provider === "qwen_code")
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
