/** @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import { ModelSelectionRequestSchema, type ModelSelection } from "../model-policy/index.ts";
import {
  CoordinatorRoutingRequestSchema,
  CoordinatorLaunchParametersSchema,
  type CoordinatorLaunchParameters,
  type CoordinatorRoutingOptions,
  type CoordinatorRoutingRequest,
  type CoordinatorRoutingResult,
  type CoordinatorRoutingResultValue,
  type CoordinatorRoutingError,
  type CoordinatorLaunchAdapter,
} from "./types.ts";
import type { ModelPolicyStoreAccess } from "../model-policy-store/index.ts";

function failure(
  code: CoordinatorRoutingError["code"],
  message: string,
): CoordinatorRoutingResult<never> {
  return { ok: false, error: { code, message } };
}

export async function resolveCoordinatorLaunch(
  rawAccess: ModelPolicyStoreAccess,
  rawRequest: CoordinatorRoutingRequest,
  options: CoordinatorRoutingOptions,
): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>> {
  const request = CoordinatorRoutingRequestSchema.safeParse(rawRequest);
  if (!request.success) return failure("invalid_input", "coordinator routing request is malformed");
  if (
    !Array.isArray(rawAccess.authorizedProjectIds) ||
    !rawAccess.authorizedProjectIds.includes(request.data.projectId)
  )
    return failure("forbidden", "project is outside trusted coordinator routing scope");
  if (!request.data.policyEnabled) return explicitRoute(rawAccess, request.data, options);
  const modelRequest = ModelSelectionRequestSchema.parse({
    selectionRef: request.data.selectionRef,
    purpose: request.data.purpose,
    taskClass: request.data.taskClass,
    role: request.data.role,
    executionMode: request.data.executionMode,
    invocationScope: request.data.invocationScope,
    productId: request.data.productId,
    productVersion: request.data.productVersion,
    override: request.data.override,
  });
  const trusted = await options.provider.trustedContext({
    access: rawAccess,
    projectId: request.data.projectId,
    contextId: request.data.contextId,
    request: modelRequest,
  });
  if (!trusted.ok) return trusted;
  const preview = options.store.resolvePreview(rawAccess, {
    projectId: request.data.projectId,
    contextId: request.data.contextId,
    request: modelRequest,
    trustedContext: trusted.value,
  });
  if (!preview.ok) return mapStoreFailure(preview);
  if (!preview.value.ok)
    return failure("policy_refused", `${preview.value.error.code}: ${preview.value.error.message}`);
  const pinned = options.store.storeSelection(rawAccess, {
    projectId: request.data.projectId,
    contextId: request.data.contextId,
    runId: request.data.runId,
    attemptId: request.data.attemptId,
    clientRequestId: request.data.clientRequestId,
    sourceEventId: request.data.sourceEventId,
    selection: preview.value.value,
  });
  if (!pinned.ok) return mapStoreFailure(pinned);
  return {
    ok: true,
    value: {
      selection: preview.value.value,
      parameters: parametersFromSelection(preview.value.value),
      pinned: true,
    },
  };
}

async function explicitRoute(
  access: ModelPolicyStoreAccess,
  request: CoordinatorRoutingRequest,
  options: CoordinatorRoutingOptions,
): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>> {
  if (request.explicitProfileId === null)
    return failure(
      "invalid_input",
      "policy-disabled coordinator routing requires an explicit registered profile",
    );
  const profile = await options.provider.explicitProfile({
    access,
    projectId: request.projectId,
    contextId: request.contextId,
    profileId: request.explicitProfileId,
    productId: request.productId,
    productVersion: request.productVersion,
  });
  if (!profile.ok) return profile;
  return {
    ok: true,
    value: {
      selection: null,
      parameters: CoordinatorLaunchParametersSchema.parse({
        selectionRef: null,
        policyId: null,
        policyRevision: null,
        profileId: profile.value.profileId,
        productId: profile.value.productId,
        productVersion: profile.value.productVersion,
        providerId: profile.value.providerId,
        modelId: profile.value.modelId,
        requestedEffort: null,
        effectiveEffort: profile.value.effort,
      }),
      pinned: false,
    },
  };
}

export function parametersFromSelection(selection: ModelSelection): CoordinatorLaunchParameters {
  const effort =
    selection.effectiveEffort.state === "explicit" ||
    selection.effectiveEffort.state === "configured_default" ||
    selection.effectiveEffort.state === "inherited"
      ? selection.effectiveEffort.value
      : null;
  return CoordinatorLaunchParametersSchema.parse({
    selectionRef: selection.selectionRef,
    policyId: selection.policyId,
    policyRevision: selection.policyRevision,
    profileId: selection.profileId,
    productId: selection.productId,
    productVersion: selection.productVersion,
    providerId: selection.providerId,
    modelId: selection.modelId,
    requestedEffort: selection.requestedEffort,
    effectiveEffort: effort,
  });
}

function mapStoreFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): CoordinatorRoutingResult<never> {
  return result.error.code === "not_found"
    ? failure("not_found", result.error.message)
    : result.error.code === "forbidden"
      ? failure("forbidden", result.error.message)
      : result.error.code === "conflict"
        ? failure("conflict", result.error.message)
        : failure("storage_failure", result.error.message);
}

export function launchWithRoutedParameters<T>(
  adapter: CoordinatorLaunchAdapter<T>,
  routed: CoordinatorRoutingResultValue,
): Promise<CoordinatorRoutingResult<T>> {
  return adapter.launch(routed.parameters);
}
