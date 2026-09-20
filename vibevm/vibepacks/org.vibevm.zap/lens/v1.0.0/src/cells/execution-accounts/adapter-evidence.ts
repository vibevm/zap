/** Installed launch-surface capability evidence. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import type { ExecutionModelReference, ExecutionModality } from "../execution-catalog/index.ts";
import {
  ReasoningEffortSchema,
  type EffortCapability,
  type ReasoningEffort,
} from "../model-policy/index.ts";
import type { ManagedAgentProfile } from "../managed-work/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";
import type { ContextCapability } from "../execution-catalog/index.ts";

export interface VerifiedExecutionAdapterEvidence {
  readonly agentProduct: "codex" | "claude_code" | "opencode" | "qwen_code" | "zap_mock";
  readonly executionModes: readonly ("native" | "managed")[];
  readonly invocationScopes: readonly (
    | "coordinator"
    | "native_subagent"
    | "native_fork"
    | "managed_agent"
  )[];
  readonly modalities: readonly ExecutionModality[];
  readonly effort: EffortCapability;
  readonly context: ContextCapability;
  readonly toolCapabilities: readonly string[];
  readonly evidenceSource: string;
  readonly observedAt: string;
}

export function codexAdapterEvidence(
  profile: CodexCoordinatorProfile,
  reference: ExecutionModelReference,
  observedAt: string,
  verifiedToolCapabilities: readonly string[] = [],
): VerifiedExecutionAdapterEvidence {
  return {
    agentProduct: "codex",
    executionModes: ["native"],
    invocationScopes: ["coordinator", "native_subagent", "native_fork"],
    modalities: modalities(verifiedToolCapabilities),
    effort: codexEffort(
      profile.effort ?? null,
      reference,
      profile.observedModelCapabilities?.find(
        (candidate) => candidate.modelId === reference.modelId,
      ),
    ),
    context: codexContext(profile.contextWindowTokens, reference),
    toolCapabilities: [...verifiedToolCapabilities],
    evidenceSource: "installed Codex app-server profile",
    observedAt,
  };
}

export function providerCoordinatorAdapterEvidence(
  profile: ProviderCoordinatorProfile,
  reference: ExecutionModelReference,
  observedAt: string,
  verifiedToolCapabilities: readonly string[] = [],
): VerifiedExecutionAdapterEvidence {
  return {
    agentProduct: profile.provider,
    executionModes: ["native"],
    invocationScopes: ["coordinator"],
    modalities: modalities(verifiedToolCapabilities),
    effort:
      profile.provider === "claude_code"
        ? referenceControlledEffort(profile.effort, reference, "output_config.effort")
        : { mode: "unsupported" },
    context: {
      mode: "unknown",
      reason: "installed provider coordinator exposes no verified context override",
    },
    toolCapabilities: [...verifiedToolCapabilities],
    evidenceSource: `installed ${profile.provider} coordinator profile`,
    observedAt,
  };
}

export function managedAdapterEvidence(
  profile: ManagedAgentProfile,
  reference: ExecutionModelReference,
  observedAt: string,
  verifiedToolCapabilities: readonly string[] = [],
): VerifiedExecutionAdapterEvidence {
  return {
    agentProduct: profile.provider,
    executionModes: ["managed"],
    invocationScopes: ["managed_agent"],
    modalities: modalities(verifiedToolCapabilities),
    effort: profile.effortSupported
      ? profile.provider === "codex"
        ? referenceControlledEffort(profile.effort, reference, "reasoning.effort")
        : profile.provider === "claude_code"
          ? referenceControlledEffort(profile.effort, reference, "output_config.effort")
          : configuredEffort(profile.effort, reference)
      : { mode: "unsupported" },
    context:
      profile.provider === "codex"
        ? codexContext(profile.contextWindowTokens, reference)
        : {
            mode: "unknown",
            reason: "managed provider exposes no verified context override",
          },
    toolCapabilities: [...verifiedToolCapabilities],
    evidenceSource: profile.capabilities.evidence.join("; "),
    observedAt,
  };
}

function configuredEffort(
  value: string | null | undefined,
  reference: ExecutionModelReference,
): EffortCapability {
  const parsed = ReasoningEffortSchema.safeParse(value);
  return parsed.success && reference.effort.values.includes(parsed.data)
    ? { mode: "configurable", allowedValues: [parsed.data], defaultValue: parsed.data }
    : {
        mode: "unknown",
        reason: "installed profile has no verified effective effort value for this model",
      };
}

function codexEffort(
  configured: string | null,
  reference: ExecutionModelReference,
  observed:
    | {
        readonly supportedEfforts: readonly ReasoningEffort[];
        readonly defaultEffort: ReasoningEffort | null;
      }
    | undefined,
): EffortCapability {
  if (observed !== undefined)
    return observed.supportedEfforts.length === 0
      ? { mode: "unknown", reason: "selected Codex account reported no effort choices" }
      : {
          mode: "configurable",
          allowedValues: [...observed.supportedEfforts],
          defaultValue: observed.defaultEffort,
        };
  return referenceControlledEffort(configured, reference, "reasoning.effort");
}

function referenceControlledEffort(
  configured: string | null | undefined,
  reference: ExecutionModelReference,
  control: string,
): EffortCapability {
  const configuredValue = ReasoningEffortSchema.safeParse(configured);
  const supported = reference.effort.values.flatMap((value) => {
    const parsed = ReasoningEffortSchema.safeParse(value);
    return parsed.success ? [parsed.data] : [];
  });
  const referenceDefault = ReasoningEffortSchema.safeParse(reference.effort.defaultValue);
  if (reference.effort.control === control && supported.length > 0)
    return {
      mode: "configurable",
      allowedValues: [...supported],
      defaultValue:
        configuredValue.success && supported.includes(configuredValue.data)
          ? configuredValue.data
          : referenceDefault.success
            ? referenceDefault.data
            : null,
    };
  return configuredEffort(configured, reference);
}

function codexContext(
  configured: number | undefined,
  reference: ExecutionModelReference,
): ContextCapability {
  const maximum = reference.documentedContextMaximumTokens;
  if (maximum === null)
    return { mode: "unknown", reason: "model reference has no documented context maximum" };
  const allowedTokens = [128_000, 200_000, 272_000, 512_000, maximum]
    .filter((tokens) => tokens <= maximum)
    .filter((tokens, index, values) => values.indexOf(tokens) === index)
    .sort((left, right) => left - right);
  const defaultTokens =
    configured !== undefined && allowedTokens.includes(configured) ? configured : maximum;
  return {
    mode: "configurable",
    allowedTokens,
    defaultTokens,
    documentedMaximumTokens: maximum,
  };
}

function modalities(toolCapabilities: readonly string[]): readonly ExecutionModality[] {
  return toolCapabilities.includes("image_generation_tool") ? ["text", "image_output"] : ["text"];
}
