/** Dynamic project agent-channel preparation. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  type ModelPolicyStore,
} from "../model-policy-store/index.ts";
import { ClientRequestIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import { ClientIdSchema, type ProductProviderProfile } from "../workspace-model/index.ts";
import type { ManagedWorkerTemplate } from "../product-app/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";
import type { ProviderLaunchPreparationPort } from "../provider-coordinators/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import type { ProtectedEnvironmentPort } from "../managed-work/index.ts";
import type { ExecutionAccountIsolationPort } from "../execution-accounts/index.ts";
import type { TrustedProjectRegistration } from "../workspace-store/index.ts";
import type { WayfinderAgentFoundation } from "./agent.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import { registerProductManagedProfiles } from "./product-managed.ts";

export async function prepareAgentScope(
  foundation: WayfinderAgentFoundation | null,
  profiles: CodexCoordinatorProfile[],
  registration: TrustedProjectRegistration,
  policyStore: ModelPolicyStore,
  knownProfileIds: readonly string[],
  managedBackend: ManagedAgentBackend | undefined,
  productProviders: readonly ProductProviderProfile[],
  workerTemplates: readonly ManagedWorkerTemplate[],
  providerProfiles: readonly ProviderCoordinatorProfile[],
  managedMcpRoot: string,
  globalProxy: ProxyPolicy,
): Promise<
  | { readonly ok: true; readonly value: null }
  | {
      readonly ok: false;
      readonly error: { readonly code: "unavailable"; readonly message: string };
    }
> {
  const policyKey = createHash("sha256")
    .update(`${registration.projectId}\u0000${registration.context.contextId}`)
    .digest("hex")
    .slice(0, 24);
  const policyAccess = ModelPolicyStoreAccessSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.wayfinder.product"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.wayfinder.product"),
    authorizedProjectIds: [registration.projectId],
  });
  const policy = policyStore.initializeDefaultPolicy(policyAccess, {
    projectId: registration.projectId,
    contextId: registration.context.contextId,
    clientRequestId: ClientRequestIdSchema.parse(`request.product.policy.${policyKey}`),
    sourceEventId: `product.policy.${policyKey}`,
    policyId: `policy.product.${policyKey}`,
  });
  if (!policy.ok && policy.error.code !== "conflict")
    return { ok: false, error: { code: "unavailable", message: policy.error.message } };
  const scope = registration.context.brokerScope;
  if (scope === undefined) return { ok: true, value: null };
  if (foundation === null)
    return {
      ok: false,
      error: { code: "unavailable", message: "agent communication gateway is not configured" },
    };
  const ensured = await foundation.ensureScope({
    projectId: registration.projectId,
    contextId: registration.context.contextId,
    workspaceId: scope.workspaceId,
    conversationId: scope.conversationId,
  });
  if (!ensured.ok) return { ok: false, error: { code: "unavailable", message: ensured.message } };
  const profile = profiles.find(
    (candidate) => candidate.profileId === registration.protected.launchProfileRef,
  );
  const protectedProfileId = registration.protected.launchProfileRef;
  const providerProfile = providerProfiles.find(
    (candidate) => candidate.profileId === protectedProfileId,
  );
  if (
    profile === undefined &&
    providerProfile === undefined &&
    !knownProfileIds.includes(protectedProfileId)
  )
    return {
      ok: false,
      error: { code: "unavailable", message: "protected coordinator profile is unavailable" },
    };
  if (profile !== undefined) augmentCodexProfile(profile, scope, ensured.value);
  if (managedBackend !== undefined) {
    const registered = registerProductManagedProfiles({
      backend: managedBackend,
      policyKey,
      registration,
      codexProfiles: profiles,
      providerProfiles,
      products: productProviders,
      templates: workerTemplates,
      mcpRoot: managedMcpRoot,
      globalProxy,
    });
    if (!registered.ok)
      return { ok: false, error: { code: "unavailable", message: registered.message } };
    const bindings = new Map(
      registered.value
        .filter((candidate) => candidate.tier !== null)
        .map((candidate) => [candidate.tier, candidate]),
    );
    if (policy.ok && bindings.size > 0) {
      const preferred = registered.value.find((candidate) => candidate.tier !== null);
      const updated = policyStore.updatePolicy(policyAccess, {
        projectId: registration.projectId,
        contextId: registration.context.contextId,
        clientRequestId: ClientRequestIdSchema.parse(`request.product.policy-bind.${policyKey}`),
        sourceEventId: `product.policy-bind.${policyKey}`,
        expectedRevision: policy.value.policy.revision,
        policy: {
          ...policy.value.policy,
          revision: DecimalSchema.parse(String(BigInt(policy.value.policy.revision) + 1n)),
          tierBindings: policy.value.policy.tierBindings.map((binding) => {
            const managed = bindings.get(binding.tier);
            return managed === undefined
              ? binding
              : {
                  tier: binding.tier,
                  profileId: managed.profileId,
                  productId: managed.provider,
                  providerId: managed.provider,
                  modelId: managed.modelId,
                };
          }),
          taskRules: policy.value.policy.taskRules.map((rule) =>
            preferred !== undefined &&
            preferred.tier !== null &&
            rule.match.purposes?.includes("development_implementation") === true &&
            (rule.match.taskClasses === undefined || rule.match.taskClasses.includes("integration"))
              ? {
                  ...rule,
                  tier: preferred.tier,
                  effort:
                    preferred.effort === null
                      ? { mode: "unspecified" }
                      : { mode: "explicit", value: preferred.effort },
                  selectionReason:
                    "Use the protected local managed worker default for new implementation runs.",
                }
              : rule,
          ),
        },
      });
      if (!updated.ok)
        return { ok: false, error: { code: "unavailable", message: updated.error.message } };
    }
  }
  return { ok: true, value: null };
}

function augmentCodexProfile(
  profile: CodexCoordinatorProfile,
  scope: NonNullable<TrustedProjectRegistration["context"]["brokerScope"]>,
  ensured: { readonly brokerUrl: string; readonly agentCredentialFile: string },
): void {
  const existing = profile.lensMcp;
  const credential = {
    workspaceId: scope.workspaceId,
    conversationId: scope.conversationId,
    credentialFile: ensured.agentCredentialFile,
  };
  profile.lensMcp = {
    serverName: existing?.serverName ?? "zap",
    commandPath: existing?.commandPath ?? process.execPath,
    args: existing?.args ?? [mcpEntrypoint()],
    brokerUrl: ensured.brokerUrl,
    scopeCredentials: [
      ...(existing?.scopeCredentials ?? []).filter(
        (candidate) =>
          candidate.workspaceId !== credential.workspaceId ||
          candidate.conversationId !== credential.conversationId,
      ),
      credential,
    ],
    communicationPreauthorization: existing?.communicationPreauthorization ?? {
      allowDelegation: true,
    },
    ...(existing?.planConfigFile === undefined ? {} : { planConfigFile: existing.planConfigFile }),
  };
}

function mcpEntrypoint(): string {
  const installed = resolve(import.meta.dirname, "../../mcp.js");
  return existsSync(installed) ? installed : resolve(import.meta.dirname, "../../mcp.ts");
}

export function createProviderLaunchPreparation(options: {
  readonly foundation: WayfinderAgentFoundation | null;
  readonly environment: ProtectedEnvironmentPort | undefined;
  readonly accounts?: ExecutionAccountIsolationPort;
  readonly mcpRoot: string;
}): ProviderLaunchPreparationPort {
  return {
    async prepare({ profile, scope }) {
      if (
        options.foundation === null ||
        scope.agentScope === undefined ||
        scope.agentScope === null ||
        scope.agentBinding === undefined ||
        scope.agentBinding === null
      )
        return providerPolicyFailure("provider launch has no authenticated agent scope");
      if (
        options.environment === undefined &&
        profile.environmentRef !== null &&
        profile.environmentRef !== undefined
      )
        return providerPolicyFailure("protected environment resolver is not configured");
      const account =
        profile.accountBindingId === undefined
          ? { ok: true as const, value: { environment: {}, environmentRef: null } }
          : options.accounts === undefined || profile.executionHostId === undefined
            ? providerPolicyFailure("provider account binding has no trusted host resolver")
            : await options.accounts.resolve({
                bindingId: profile.accountBindingId,
                hostId: profile.executionHostId,
                agentProduct: profile.provider,
              });
      if (!account.ok) return providerPolicyFailure(account.error.message);
      if (
        account.value.environmentRef !== null &&
        account.value.environmentRef !== profile.environmentRef
      )
        return providerPolicyFailure("provider environment does not match its account binding");
      const environment: Awaited<ReturnType<ProtectedEnvironmentPort["resolve"]>> =
        options.environment === undefined
          ? { ok: true, value: {} }
          : await options.environment.resolve(profile.environmentRef ?? null);
      if (!environment.ok) return providerPolicyFailure(environment.message);
      const stem = createHash("sha256")
        .update(`${profile.profileId}\u0000${scope.coordinatorSessionId}`)
        .digest("hex")
        .slice(0, 24);
      const prepared = await options.foundation.prepareOwnedCoordinatorLaunch({
        provider: profile.provider,
        projectId: scope.projectId,
        contextId: scope.contextId,
        coordinatorSessionId: scope.coordinatorSessionId,
        actorId: scope.agentBinding.actorId,
        adapterSessionId: AdapterSessionIdSchema.parse(scope.agentBinding.adapterSessionId),
        mcpConfigPath: profile.mcpConfigPath ?? join(options.mcpRoot, `${stem}.json`),
        mcpCommandPath: profile.mcpCommandPath ?? process.execPath,
        mcpArgs: profile.mcpArgs ?? [mcpEntrypoint()],
      });
      return prepared.ok
        ? {
            ok: true,
            value: {
              environment: {
                ...environment.value,
                ...account.value.environment,
                ...prepared.value.environment,
              },
              mcpConfigPath: prepared.value.mcpConfigPath,
            },
          }
        : providerPreparationFailure(prepared.message);
    },
  };
}

function providerPolicyFailure(message: string): {
  readonly ok: false;
  readonly error: {
    readonly code: "policy_denied";
    readonly message: string;
    readonly retry: "never";
  };
} {
  return { ok: false, error: { code: "policy_denied", message, retry: "never" } };
}

function providerPreparationFailure(message: string): {
  readonly ok: false;
  readonly error: {
    readonly code: "host_refused";
    readonly message: string;
    readonly retry: "after_refresh";
  };
} {
  return {
    ok: false,
    error: { code: "host_refused", message, retry: "after_refresh" },
  };
}
