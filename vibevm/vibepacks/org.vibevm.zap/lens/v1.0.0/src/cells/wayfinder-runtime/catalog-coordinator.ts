/** Materialize named catalog configurations into trusted coordinator hosts. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import { createHash } from "node:crypto";
import type { AgentHost } from "../agent-runtime/index.ts";
import {
  CodexCoordinatorProfileSchema,
  type CodexCoordinatorProfile,
  type CodexProcessFactory,
} from "../codex-coordinator/index.ts";
import type {
  ExecutionCatalogCaller,
  ExecutionCatalogService,
} from "../execution-catalog-service/index.ts";
import type { ContextCapability } from "../execution-catalog/index.ts";
import type { EffortCapability, ReasoningEffort } from "../model-policy/index.ts";
import type { ProductExecutionConfigurationResolver } from "../product-app/index.ts";
import {
  ProviderCoordinatorProfileSchema,
  type ProviderCoordinatorProfile,
} from "../provider-coordinators/index.ts";
import {
  ProductProviderProfileSchema,
  type ProductProviderProfile,
} from "../workspace-model/index.ts";
import { createCodexHost, type OwnedAgentHost } from "./coordinator-runtime.ts";

export function createDeferredProductExecutionResolver(): ProductExecutionConfigurationResolver & {
  bind(resolver: ProductExecutionConfigurationResolver["resolve"]): void;
} {
  let delegate: ProductExecutionConfigurationResolver["resolve"] | undefined;
  return {
    bind(resolver) {
      delegate = resolver;
    },
    async resolve(input) {
      return delegate === undefined
        ? unavailable("execution catalog hosts are still starting")
        : delegate(input);
    },
  };
}

export function createCatalogCoordinatorResolver(input: {
  readonly catalog: ExecutionCatalogService;
  readonly caller: ExecutionCatalogCaller;
  readonly codexTemplates: readonly CodexCoordinatorProfile[];
  readonly providerTemplates: readonly ProviderCoordinatorProfile[];
  readonly processFactory: CodexProcessFactory;
  readonly providerHost: (profile: ProviderCoordinatorProfile) => AgentHost;
  readonly registerHost: (
    profileId: string,
    host: AgentHost,
  ) => { readonly ok: true } | { readonly ok: false; readonly message: string };
  readonly retainCodexHost: (host: OwnedAgentHost, profile: CodexCoordinatorProfile) => void;
  readonly retainProviderProfile: (profile: ProviderCoordinatorProfile) => void;
}): ProductExecutionConfigurationResolver & {
  validateProfile(
    profileId: string,
  ): Promise<{ readonly ok: true } | { readonly ok: false; readonly message: string }>;
} {
  const resolved = new Map<
    string,
    {
      readonly product: ProductProviderProfile;
      readonly configurationId: string;
      readonly connectionId: string;
      readonly bindingId: string;
      readonly modelId: string;
      readonly effort: ReasoningEffort | null;
      readonly contextWindowTokens: number | null;
    }
  >();
  const validateProfile = async (profileId: string) => {
    const pin = [...resolved.values()].find(
      (candidate) => candidate.product.profileId === profileId,
    );
    if (pin === undefined) return { ok: true as const };
    const view = await input.catalog.get(input.caller, {});
    if (!view.ok) return { ok: false as const, message: view.error.message };
    const configuration = view.value.snapshot.configurations.find(
      (candidate) => candidate.configurationId === pin.configurationId,
    );
    const connection = view.value.snapshot.connections.find(
      (candidate) => candidate.connectionId === pin.connectionId,
    );
    return configuration !== undefined &&
      connection !== undefined &&
      configuration.enabled &&
      connection.enabled &&
      configuration.modelId === pin.modelId &&
      connection.launchBindingId === pin.bindingId &&
      view.value.availableBindings.some(
        (binding) => binding.bindingId === pin.bindingId && binding.enabled,
      ) &&
      effortAllowed(configuration.effort, pin.effort) &&
      contextAllowed(configuration.context, pin.contextWindowTokens)
      ? { ok: true as const }
      : {
          ok: false as const,
          message:
            "catalog configuration is disabled or no longer authorizes the pinned coordinator",
        };
  };
  return {
    validateProfile,
    async resolve(request) {
      const key = `${request.configurationId}\u0000${request.directoryPath}`;
      const prior = resolved.get(key);
      if (prior !== undefined) {
        const valid = await validateProfile(prior.product.profileId);
        return valid.ok ? { ok: true, value: prior.product } : unavailable(valid.message);
      }
      const view = await input.catalog.get(input.caller, {});
      if (!view.ok) return unavailable(view.error.message);
      const configuration = view.value.snapshot.configurations.find(
        (candidate) => candidate.configurationId === request.configurationId,
      );
      const connection = view.value.snapshot.connections.find(
        (candidate) => candidate.connectionId === configuration?.connectionId,
      );
      if (configuration === undefined || connection === undefined)
        return unavailable("execution configuration is not registered");
      if (!configuration.enabled || !connection.enabled)
        return unavailable("execution configuration or account connection is disabled");
      const profileId = `profile.catalog.${digest(key).slice(0, 24)}`;
      const effort = configuredEffort(configuration.effort);
      const contextWindowTokens = configuredContext(configuration.context);
      if (configuration.agentProduct === "codex") {
        const template = input.codexTemplates.find(
          (candidate) => candidate.profileId === configuration.productId,
        );
        if (template === undefined) return unavailable("Codex launch template is unavailable");
        const profile = CodexCoordinatorProfileSchema.parse({
          ...template,
          profileId,
          model: configuration.modelId,
          effort,
          accountBindingId: connection.launchBindingId,
          ...(contextWindowTokens === null ? {} : { contextWindowTokens }),
        });
        const host = createCodexHost(profile, input.processFactory);
        const registered = input.registerHost(profileId, host);
        if (!registered.ok) return unavailable(registered.message);
        input.retainCodexHost(host, profile);
      } else if (
        configuration.agentProduct === "claude_code" ||
        configuration.agentProduct === "opencode" ||
        configuration.agentProduct === "qwen_code"
      ) {
        const template = input.providerTemplates.find(
          (candidate) => candidate.profileId === configuration.productId,
        );
        if (template === undefined) return unavailable("provider launch template is unavailable");
        const profile = ProviderCoordinatorProfileSchema.parse({
          ...template,
          profileId,
          cwd: request.directoryPath,
          modelId: configuration.modelId,
          effort,
          accountBindingId: connection.launchBindingId,
        });
        const host = input.providerHost(profile);
        const registered = input.registerHost(profileId, host);
        if (!registered.ok) return unavailable(registered.message);
        input.retainProviderProfile(profile);
      } else {
        if (!configuration.synthetic)
          return unavailable("ZapMock coordinator configuration must retain synthetic provenance");
      }
      const product = ProductProviderProfileSchema.parse({
        profileId: configuration.agentProduct === "zap_mock" ? configuration.productId : profileId,
        provider: configuration.agentProduct,
        displayName: configuration.displayName,
        modelId: configuration.modelId,
        effort,
        interactionKind: "structured",
        installed: true,
        configured: true,
        authenticated: "not_observed",
        launchable: true,
        evidence: [
          configuration.evidence.source,
          `Catalog configuration ${configuration.configurationId}`,
          `Protected binding ${connection.launchBindingId}`,
        ],
      });
      resolved.set(key, {
        product,
        configurationId: configuration.configurationId,
        connectionId: connection.connectionId,
        bindingId: connection.launchBindingId,
        modelId: configuration.modelId,
        effort,
        contextWindowTokens,
      });
      return { ok: true, value: product };
    },
  };
}

function configuredEffort(capability: EffortCapability): ReasoningEffort | null {
  return capability.mode === "configurable" ? capability.defaultValue : null;
}

function configuredContext(capability: ContextCapability): number | null {
  if (capability.mode === "configurable") return capability.defaultTokens ?? null;
  if (capability.mode === "fixed") return capability.tokens ?? null;
  return null;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function unavailable(message: string) {
  return { ok: false as const, error: { code: "unavailable" as const, message } };
}

function effortAllowed(capability: EffortCapability, effort: ReasoningEffort | null): boolean {
  if (effort === null)
    return capability.mode !== "configurable" || capability.defaultValue === null;
  return capability.mode === "configurable" && capability.allowedValues.includes(effort);
}

function contextAllowed(capability: ContextCapability, tokens: number | null): boolean {
  if (tokens === null)
    return capability.mode !== "configurable" || capability.defaultTokens === null;
  if (capability.mode === "configurable") return capability.allowedTokens.includes(tokens);
  return capability.mode === "fixed" && capability.tokens === tokens;
}
