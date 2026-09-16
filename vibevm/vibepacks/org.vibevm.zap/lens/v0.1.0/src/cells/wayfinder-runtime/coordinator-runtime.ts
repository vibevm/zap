/** Coordinator host, routing and policy composition. @scope spec://org.vibevm.zap/lens/PROP-006#execution-backends */
import type { AgentHost, CoordinatorAdapter } from "../agent-runtime/index.ts";
import {
  createCodexCoordinatorAdapter,
  createNodeCodexProcessFactory,
  type CodexCoordinatorProfile,
  type CodexProcessFactory,
} from "../codex-coordinator/index.ts";
import {
  initializeConfiguredCoordinatorPolicies,
  resolveCoordinatorLaunch,
  type CoordinatorRoutingConfig,
  type CoordinatorRoutingProvider,
} from "../coordinator-routing/index.ts";
import type { ModelPolicyStore, ModelPolicyStoreAccess } from "../model-policy-store/index.ts";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  ClientIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
  type ExecutionHostId,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import type { CoordinatorRoutingBridge } from "../workspace-service/index.ts";
import type { WayfinderRuntimeConfig } from "./index.ts";

export interface OwnedAgentHost extends AgentHost {
  closeOwned(): void;
}

export function createCodexHost(
  profile: CodexCoordinatorProfile,
  processFactory: CodexProcessFactory = createNodeCodexProcessFactory(),
): OwnedAgentHost {
  const hostId: ExecutionHostId = ExecutionHostIdSchema.parse(
    `host.wayfinder.${profile.profileId}`,
  );
  const adapters = new Set<CoordinatorAdapter>();
  return {
    hostId,
    profileIds: [profile.profileId],
    openCoordinator(profileId) {
      if (profileId !== profile.profileId) {
        return Promise.resolve({
          ok: false,
          error: { code: "not_found", message: "profile is not registered", retry: "never" },
        });
      }
      const created = createCodexCoordinatorAdapter({ profiles: [profile], processFactory });
      if (created.ok) adapters.add(created.value);
      return Promise.resolve(created);
    },
    closeOwned() {
      for (const adapter of adapters) adapter.close();
      adapters.clear();
    },
  };
}

export function disposeOwnedHosts(hosts: readonly OwnedAgentHost[]): void {
  for (const host of hosts) host.closeOwned();
}

export function createRuntimeRoutingBridge(
  config: CoordinatorRoutingConfig,
  store: ModelPolicyStore,
  provider: CoordinatorRoutingProvider,
): CoordinatorRoutingBridge {
  const options = { store, provider };
  const profileFor = (projectId: ProjectId, contextId: WorkContextId, profileId: string) =>
    config.profiles.find(
      (binding) =>
        binding.scope.projectId === projectId &&
        binding.scope.contextId === contextId &&
        binding.profile.profileId === profileId,
    )?.profile;
  return {
    async resolve(access, input) {
      const profile = profileFor(input.projectId, input.contextId, input.explicitProfileId);
      return resolveCoordinatorLaunch(
        access,
        {
          projectId: input.projectId,
          contextId: input.contextId,
          sessionId: AgentSessionIdSchema.parse(input.sessionId),
          runId: RunIdSchema.parse(input.runId),
          attemptId: AttemptIdSchema.parse(input.attemptId),
          clientRequestId: ClientRequestIdSchema.parse(input.clientRequestId),
          sourceEventId: input.sourceEventId,
          policyEnabled: config.policies.length > 0,
          purpose: "development_implementation",
          taskClass: "integration",
          role: "coordinator",
          executionMode: "native",
          invocationScope: "coordinator",
          productId: profile?.productId ?? "codex",
          productVersion: profile?.productVersion ?? "0.152.1",
          selectionRef: `selection.${input.sessionId}`,
          override: null,
          explicitProfileId: input.explicitProfileId,
        },
        options,
      );
    },
    async resume(access, input) {
      const profile = config.profiles.find(
        (binding) =>
          binding.scope.projectId === input.projectId &&
          binding.scope.contextId === input.contextId,
      );
      return this.resolve(access, {
        ...input,
        clientRequestId: `request.resume.${input.sessionId}`,
        sourceEventId: `resume.${input.sessionId}`,
        explicitProfileId: profile?.profile.profileId ?? "profile.unknown",
      });
    },
  };
}

export function initializeRuntimePolicies(
  store: ModelPolicyStore,
  projectIds: readonly ProjectId[],
  config: WayfinderRuntimeConfig,
): RuntimePolicyResult {
  if (projectIds.length === 0) {
    if (config.modelPolicies.length > 0 || (config.routing?.policies.length ?? 0) > 0)
      return {
        ok: false,
        error: { code: "invalid_input", message: "model policy names no registered project" },
      };
    return { ok: true, value: [] };
  }
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.wayfinder.runtime"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.wayfinder.runtime"),
    authorizedProjectIds: projectIds,
  });
  return config.routing === undefined
    ? initializeLegacyPolicies(store, access, config.modelPolicies)
    : initializeConfiguredCoordinatorPolicies(store, access, config.routing);
}

function initializeLegacyPolicies(
  store: ModelPolicyStore,
  access: ModelPolicyStoreAccess,
  policies: readonly {
    readonly projectId: string;
    readonly contextId: string;
    readonly policyId: string;
  }[],
): RuntimePolicyResult {
  const results = [];
  for (const policy of policies) {
    const initialized = store.initializeDefaultPolicy(access, {
      projectId: ProjectIdSchema.parse(policy.projectId),
      contextId: WorkContextIdSchema.parse(policy.contextId),
      clientRequestId: ClientRequestIdSchema.parse(`request.wayfinder.policy.${policy.policyId}`),
      sourceEventId: `wayfinder.policy.${policy.policyId}`,
      policyId: policy.policyId,
    });
    if (!initialized.ok && initialized.error.code !== "conflict") return initialized;
    if (initialized.ok) results.push(initialized.value);
  }
  return { ok: true, value: results };
}

type RuntimePolicyResult =
  | { readonly ok: true; readonly value: readonly unknown[] }
  | { readonly ok: false; readonly error: { readonly code: string; readonly message: string } };
