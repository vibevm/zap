/** Managed execution-catalog selection and revocation fence. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import { createHash } from "node:crypto";
import {
  ExecutionCatalogCallerSchema,
  type ExecutionCatalogService,
} from "../execution-catalog-service/index.ts";
import type { ContextCapability } from "../execution-catalog/index.ts";
import { materializeManagedExecutionProfile } from "../execution-accounts/index.ts";
import {
  ModelSelectionSchema,
  type EffortCapability,
  type ModelSelection,
} from "../model-policy/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { AttemptIdSchema, RunIdSchema } from "../workspace-model/index.ts";
import type {
  ManagedAgentProfile,
  ManagedSelectionPort,
  ManagedWorkClaim,
  ManagedWorkResult,
} from "../managed-work/index.ts";

export interface ManagedCatalogRuntime {
  readonly service: ExecutionCatalogService;
  readonly hostId: string;
}

export async function selectCatalogManaged(input: {
  readonly catalog: ManagedCatalogRuntime;
  readonly access: Parameters<ManagedSelectionPort["resolve"]>[0];
  readonly request: Parameters<ManagedSelectionPort["resolve"]>[1];
  readonly profiles: readonly ManagedAgentProfile[];
  readonly identity: Parameters<ManagedSelectionPort["resolve"]>[3];
  readonly parentSelection: Parameters<
    ExecutionCatalogService["selectAndPin"]
  >[1]["parentSelection"];
}): Promise<
  ManagedWorkResult<{
    readonly profile: ManagedAgentProfile;
    readonly modelSelection: ModelSelection;
    readonly executionSelection: NonNullable<ManagedWorkClaim["executionSelection"]>;
  }>
> {
  const choice = input.request.selection;
  if (choice.mode !== "catalog_policy" && choice.mode !== "catalog_override")
    return fail("invalid_input", "managed request does not select the execution catalog");
  const caller = ExecutionCatalogCallerSchema.parse(input.access);
  const selectionRef = `selection.catalog.${digest(input.request.clientRequestId).slice(0, 24)}`;
  const selected = await input.catalog.service.selectAndPin(caller, {
    projectId: input.request.projectId,
    contextId: input.request.contextId,
    runId: RunIdSchema.parse(input.identity.runId),
    attemptId: AttemptIdSchema.parse(input.identity.attemptId),
    clientRequestId: ClientRequestIdSchema.parse(input.request.clientRequestId),
    sourceEventId: `managed-work.catalog-selection.${digest(input.request.clientRequestId)}`,
    request: {
      selectionRef,
      specialization: input.request.specialization,
      purpose: "development_implementation",
      taskClass: "integration",
      role: "worker",
      executionMode: "managed",
      invocationScope: "managed_agent",
      productId: "product.wayfinder",
      productVersion: "1.0.0",
      requiredModalities:
        input.request.specialization === "image_generation" ? ["text", "image_output"] : ["text"],
      effort: choice.effort,
      context: choice.context,
      override:
        choice.mode === "catalog_override"
          ? {
              configurationId: choice.configurationId,
              reason: choice.reasonMarkdown,
            }
          : null,
    },
    ...(input.parentSelection === null || input.parentSelection === undefined
      ? {}
      : { parentSelection: input.parentSelection }),
  });
  if (!selected.ok) return fail(mapCode(selected.error.code), selected.error.message);
  const execution = selected.value.selection;
  const base =
    input.profiles.find((profile) => profile.profileId === execution.productId) ??
    input.profiles.find(
      (profile) =>
        profile.provider === execution.agentProduct &&
        (profile.accountBindingId === execution.launchBindingId ||
          execution.agentProduct === "codex" ||
          (execution.agentProduct === "claude_code" && profile.environmentRef === null)),
    );
  if (base === undefined)
    return fail("unavailable", "selected catalog launch template is unavailable in this project");
  const materialized = materializeManagedExecutionProfile(base, execution, input.catalog.hostId);
  if (!materialized.ok) return fail("unavailable", materialized.error.message);
  return {
    ok: true,
    value: {
      profile: materialized.value,
      modelSelection: legacySelection(execution, materialized.value),
      executionSelection: execution,
    },
  };
}

export async function revalidateCatalogManaged(input: {
  readonly catalog: ManagedCatalogRuntime | undefined;
  readonly access: Parameters<ManagedSelectionPort["resolve"]>[0];
  readonly claim: ManagedWorkClaim;
}): Promise<ManagedWorkResult<null>> {
  const expected = input.claim.executionSelection;
  if (expected === null) return { ok: true, value: null };
  if (input.catalog === undefined)
    return fail("unavailable", "execution catalog is unavailable for selected managed work");
  const caller = ExecutionCatalogCallerSchema.parse(input.access);
  const [stored, current] = await Promise.all([
    Promise.resolve(
      input.catalog.service.selection(caller, {
        projectId: input.claim.packet.projectId,
        contextId: input.claim.packet.contextId,
        runId: input.claim.runId,
        attemptId: input.claim.attemptId,
      }),
    ),
    input.catalog.service.get(caller, {}),
  ]);
  if (!stored.ok) return fail("unavailable", stored.error.message);
  if (!current.ok) return fail("unavailable", current.error.message);
  const observed = stored.value.selection.selection;
  if (
    observed.selectionRef !== expected.selectionRef ||
    observed.configurationId !== expected.configurationId ||
    observed.connectionId !== expected.connectionId ||
    observed.launchBindingId !== expected.launchBindingId ||
    observed.modelId !== expected.modelId
  )
    return fail("conflict", "stored execution selection changed after managed preparation");
  const configuration = current.value.snapshot.configurations.find(
    (candidate) => candidate.configurationId === expected.configurationId,
  );
  const connection = current.value.snapshot.connections.find(
    (candidate) => candidate.connectionId === expected.connectionId,
  );
  const binding = current.value.availableBindings.find(
    (candidate) => candidate.bindingId === expected.launchBindingId,
  );
  return configuration?.enabled === true &&
    connection?.enabled === true &&
    binding?.enabled === true &&
    configuration.connectionId === connection.connectionId &&
    connection.launchBindingId === binding.bindingId &&
    configuration.modelId === expected.modelId &&
    configurationAllowsEffort(configuration.effort, expected.appliedEffort) &&
    configurationAllowsContext(configuration.context, expected.appliedContext)
    ? { ok: true, value: null }
    : fail("forbidden", "execution configuration, connection or account binding was revoked");
}

function configurationAllowsEffort(
  capability: EffortCapability,
  applied: NonNullable<ManagedWorkClaim["executionSelection"]>["appliedEffort"],
): boolean {
  const value = effortValue(applied);
  if (value !== null)
    return capability.mode === "configurable" && capability.allowedValues.includes(value);
  if (applied.state === "unsupported") return capability.mode === "unsupported";
  if (applied.state === "unknown")
    return (
      capability.mode === "unknown" ||
      (capability.mode === "configurable" && capability.defaultValue === null)
    );
  return capability.mode === "inherited" || capability.mode === "configurable";
}

function configurationAllowsContext(
  capability: ContextCapability,
  applied: NonNullable<ManagedWorkClaim["executionSelection"]>["appliedContext"],
): boolean {
  if (applied.state === "configured")
    return capability.mode === "configurable" && capability.allowedTokens.includes(applied.tokens);
  if (applied.state === "fixed")
    return capability.mode === "fixed" && capability.tokens === applied.tokens;
  if (applied.state === "inherited")
    return (
      capability.mode === "inherited" ||
      (capability.mode === "configurable" &&
        (applied.tokens === null || capability.allowedTokens.includes(applied.tokens)))
    );
  if (applied.state === "unsupported") return capability.mode === "unsupported";
  return (
    capability.mode === "unknown" ||
    (capability.mode === "configurable" && capability.defaultTokens === null)
  );
}

export function parentCatalogSelection(
  claim: ManagedWorkClaim | null,
): Parameters<ExecutionCatalogService["selectAndPin"]>[1]["parentSelection"] {
  const selection = claim?.executionSelection;
  if (selection === undefined || selection === null) return null;
  return {
    selectionRef: selection.selectionRef,
    configurationId: selection.configurationId,
    connectionId: selection.connectionId,
    launchBindingId: selection.launchBindingId,
    productId: selection.productId,
    effectiveEffort: effortValue(selection.appliedEffort),
    appliedContextTokens:
      selection.appliedContext.state === "configured" ||
      selection.appliedContext.state === "fixed" ||
      selection.appliedContext.state === "inherited"
        ? selection.appliedContext.tokens
        : null,
  };
}

function legacySelection(
  selection: NonNullable<ManagedWorkClaim["executionSelection"]>,
  profile: ManagedAgentProfile,
): ModelSelection {
  const explanation = selection.explanations.find(
    (candidate) => candidate.configurationId === selection.configurationId,
  );
  return ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: selection.selectionRef,
    policyId: "policy.execution-catalog",
    policyRevision: selection.catalogRevision,
    ruleId: "rule.execution-catalog",
    overrideRef: selection.overrideReason === null ? null : `override.${selection.configurationId}`,
    selectionReason: explanation?.reasons.join(" ") ?? "Selected by the execution catalog.",
    overrideReason: selection.overrideReason,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: profile.tier ?? "small",
    requestedEffort: selection.requestedEffort,
    profileId: profile.profileId,
    productId: profile.provider,
    productVersion: profile.capabilities.observedVersion ?? "unknown",
    providerId: selection.providerId,
    modelId: selection.modelId,
    capabilityId: selection.configurationId,
    effortCapability: effortCapability(selection.appliedEffort),
    extendedThinking:
      selection.appliedEffort.state === "unsupported"
        ? "unsupported"
        : selection.appliedEffort.state === "inherited"
          ? "inherited"
          : "configurable",
    effectiveEffort: selection.appliedEffort,
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
}

function effortCapability(
  effort: NonNullable<ManagedWorkClaim["executionSelection"]>["appliedEffort"],
) {
  const value = effortValue(effort);
  if (effort.state === "unsupported") return { mode: "unsupported" as const };
  if (effort.state === "unknown" || value === null)
    return { mode: "unknown" as const, reason: "Catalog effort has no observed concrete value" };
  if (effort.state === "inherited") return { mode: "inherited" as const };
  return { mode: "configurable" as const, allowedValues: [value], defaultValue: value };
}

function effortValue(effort: NonNullable<ManagedWorkClaim["executionSelection"]>["appliedEffort"]) {
  return effort.state === "explicit" || effort.state === "configured_default"
    ? effort.value
    : effort.state === "inherited"
      ? effort.value
      : null;
}

function mapCode(code: string): "invalid_input" | "forbidden" | "conflict" | "unavailable" {
  if (code === "invalid_input") return "invalid_input";
  if (code === "forbidden" || code === "unauthorized") return "forbidden";
  if (code === "conflict" || code === "idempotency_conflict") return "conflict";
  return "unavailable";
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function fail(
  code: "invalid_input" | "forbidden" | "conflict" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
