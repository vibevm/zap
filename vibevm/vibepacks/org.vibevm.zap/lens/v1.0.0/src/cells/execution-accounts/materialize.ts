/** Trusted selection-to-launch profile materialization. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import {
  CodexCoordinatorProfileSchema,
  type CodexCoordinatorProfile,
} from "../codex-coordinator/index.ts";
import { ExecutionSelectionSchema, type ExecutionSelection } from "../execution-catalog/index.ts";
import { ManagedAgentProfileSchema, type ManagedAgentProfile } from "../managed-work/index.ts";
import {
  ProviderCoordinatorProfileSchema,
  type ProviderCoordinatorProfile,
} from "../provider-coordinators/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import type { ExecutionAccountResult } from "./types.ts";

export function materializeCodexExecutionProfile(
  base: CodexCoordinatorProfile,
  rawSelection: ExecutionSelection,
): ExecutionAccountResult<CodexCoordinatorProfile> {
  const selection = ExecutionSelectionSchema.safeParse(rawSelection);
  if (!selection.success || selection.data.agentProduct !== "codex")
    return failure("selection is not a valid Codex execution assignment");
  const { accountBindingId: _account, contextWindowTokens: _context, ...rest } = base;
  void _account;
  void _context;
  const parsed = CodexCoordinatorProfileSchema.safeParse({
    ...rest,
    profileId: selection.data.configurationId,
    model: selection.data.modelId,
    effort: effectiveEffort(selection.data),
    accountBindingId: selection.data.launchBindingId,
    ...(selection.data.appliedContext.state === "configured"
      ? { contextWindowTokens: selection.data.appliedContext.tokens }
      : {}),
  });
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("selected Codex launch profile is invalid");
}

export function materializeProviderExecutionProfile(
  base: ProviderCoordinatorProfile,
  rawSelection: ExecutionSelection,
  hostId: string,
): ExecutionAccountResult<ProviderCoordinatorProfile> {
  const selection = ExecutionSelectionSchema.safeParse(rawSelection);
  const host = ExecutionHostIdSchema.safeParse(hostId);
  if (
    !selection.success ||
    !host.success ||
    selection.data.agentProduct !== base.provider ||
    selection.data.appliedContext.state === "configured"
  )
    return failure("selected provider launch cannot apply this product or context override");
  const parsed = ProviderCoordinatorProfileSchema.safeParse({
    ...base,
    profileId: selection.data.configurationId,
    modelId: selection.data.modelId,
    effort: effectiveEffort(selection.data),
    accountBindingId: selection.data.launchBindingId,
    executionHostId: host.data,
  });
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("selected provider launch profile is invalid");
}

export function materializeManagedExecutionProfile(
  base: ManagedAgentProfile,
  rawSelection: ExecutionSelection,
  hostId: string,
): ExecutionAccountResult<ManagedAgentProfile> {
  const selection = ExecutionSelectionSchema.safeParse(rawSelection);
  const host = ExecutionHostIdSchema.safeParse(hostId);
  if (!selection.success || !host.success || selection.data.agentProduct !== base.provider)
    return failure("selected managed launch does not match its provider profile");
  if (selection.data.appliedContext.state === "configured" && base.provider !== "codex")
    return failure("configured context is not supported by this managed provider launch");
  const parsed = ManagedAgentProfileSchema.safeParse({
    ...base,
    profileId: selection.data.configurationId,
    modelId: selection.data.modelId,
    effort: effectiveEffort(selection.data),
    accountBindingId: selection.data.launchBindingId,
    executionHostId: host.data,
    ...(selection.data.appliedContext.state === "configured"
      ? { contextWindowTokens: selection.data.appliedContext.tokens }
      : {}),
  });
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("selected managed launch profile is invalid");
}

function effectiveEffort(selection: ExecutionSelection) {
  const effort = selection.appliedEffort;
  return effort.state === "explicit" || effort.state === "configured_default"
    ? effort.value
    : effort.state === "inherited"
      ? effort.value
      : null;
}

function failure(message: string): ExecutionAccountResult<never> {
  return { ok: false, error: { code: "invalid_input", message } };
}
