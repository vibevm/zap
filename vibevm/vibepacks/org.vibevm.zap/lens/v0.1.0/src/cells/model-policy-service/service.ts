/** Authenticated model-policy application feature. @scope spec://org.vibevm.zap/lens/PROP-008#policy-interface */
import {
  type ModelPolicyStore,
  type ModelPolicyStoreAccess,
  type ModelPolicyStoreError,
  type ModelPolicyStoreResult,
} from "../model-policy-store/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import { ModelPolicySchema } from "../model-policy/index.ts";
import {
  ModelPolicyGetRequestSchema,
  ModelPolicyHistoryRequestSchema,
  ModelPolicyPreviewRequestSchema,
  ModelPolicyUpdateRequestSchema,
  ModelSelectionGetRequestSchema,
  type ModelPolicyGetRequest,
  type ModelPolicyGetResponse,
  type ModelPolicyHistoryRequest,
  type ModelPolicyHistoryResponse,
  type ModelPolicyPreviewRequest,
  type ModelPolicyPreviewResponse,
  type ModelPolicyService,
  type ModelPolicyServiceOptions,
  type ModelPolicyUpdateRequest,
  type ModelPolicyUpdateResponse,
  type ModelSelectionGetRequest,
  type ModelSelectionGetResponse,
} from "./types.ts";

function failure(
  code: ModelPolicyStoreError["code"],
  message: string,
): ModelPolicyStoreResult<never> {
  return { ok: false, error: { code, message } };
}

export class AuthenticatedModelPolicyService implements ModelPolicyService {
  readonly #store: ModelPolicyStore;
  readonly #trustedContext: ModelPolicyServiceOptions["trustedContext"];

  constructor(options: ModelPolicyServiceOptions) {
    this.#store = options.store;
    this.#trustedContext = options.trustedContext;
  }

  get(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: ModelPolicyGetRequest,
  ): ModelPolicyStoreResult<ModelPolicyGetResponse> {
    const access = this.#access(rawAccess, rawRequest.projectId);
    if (!access.ok) return access;
    const request = ModelPolicyGetRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "model policy get request is malformed");
    const policy = this.#store.readPolicy(
      access.value,
      request.data.projectId,
      request.data.contextId,
    );
    return policy.ok
      ? { ok: true, value: { feature: "model-policy.get.v1", policy: policy.value } }
      : policy;
  }

  update(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: ModelPolicyUpdateRequest,
  ): ModelPolicyStoreResult<ModelPolicyUpdateResponse> {
    const access = this.#access(rawAccess, rawRequest.projectId);
    if (!access.ok) return access;
    const request = ModelPolicyUpdateRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "model policy update request is malformed");
    const policy = ModelPolicySchema.safeParse(request.data.policy);
    if (!policy.success) return failure("invalid_input", "model policy update body is invalid");
    const parsed = {
      projectId: request.data.projectId,
      contextId: request.data.contextId,
      clientRequestId: ClientRequestIdSchema.parse(request.data.clientRequestId),
      sourceEventId: request.data.sourceEventId,
      expectedRevision: DecimalSchema.parse(request.data.expectedRevision),
      policy: policy.data,
    };
    const updated = this.#store.updatePolicy(access.value, parsed);
    if (!updated.ok) return updated;
    const changes = this.#store.listChanges(access.value, parsed.projectId, parsed.contextId);
    if (!changes.ok) return changes;
    const change = changes.value.find(
      (candidate) =>
        candidate.toRevision === updated.value.policy.revision &&
        candidate.sourceEventId === parsed.sourceEventId,
    );
    return change === undefined
      ? failure("storage_failure", "committed policy change record is missing")
      : { ok: true, value: { feature: "model-policy.update.v1", policy: updated.value, change } };
  }

  async preview(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: ModelPolicyPreviewRequest,
  ): Promise<ModelPolicyStoreResult<ModelPolicyPreviewResponse>> {
    const request = ModelPolicyPreviewRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "model policy preview request is malformed");
    const access = this.#access(rawAccess, request.data.projectId);
    if (!access.ok) return access;
    const trusted = await this.#trustedContext.resolve({
      access: access.value,
      projectId: request.data.projectId,
      contextId: request.data.contextId,
      request: request.data.request,
    });
    if (!trusted.ok) return trusted;
    const preview = this.#store.resolvePreview(access.value, {
      projectId: request.data.projectId,
      contextId: request.data.contextId,
      request: request.data.request,
      trustedContext: trusted.value,
    });
    return preview.ok
      ? { ok: true, value: { feature: "model-policy.preview.v1", result: preview.value } }
      : preview;
  }

  history(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: ModelPolicyHistoryRequest,
  ): ModelPolicyStoreResult<ModelPolicyHistoryResponse> {
    const request = ModelPolicyHistoryRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "model policy history request is malformed");
    const access = this.#access(rawAccess, request.data.projectId);
    if (!access.ok) return access;
    const versions = this.#store.listPolicyVersions(
      access.value,
      request.data.projectId,
      request.data.contextId,
    );
    if (!versions.ok) return versions;
    const changes = this.#store.listChanges(
      access.value,
      request.data.projectId,
      request.data.contextId,
    );
    return changes.ok
      ? {
          ok: true,
          value: {
            feature: "model-policy.history.v1",
            versions: versions.value,
            changes: changes.value,
          },
        }
      : changes;
  }

  selection(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: ModelSelectionGetRequest,
  ): ModelPolicyStoreResult<ModelSelectionGetResponse> {
    const request = ModelSelectionGetRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "model selection get request is malformed");
    const access = this.#access(rawAccess, request.data.projectId);
    if (!access.ok) return access;
    const selection = this.#store.readSelection(
      access.value,
      request.data.projectId,
      request.data.contextId,
      request.data.runId,
      request.data.attemptId,
    );
    return selection.ok
      ? { ok: true, value: { feature: "model-selection.get.v1", selection: selection.value } }
      : selection;
  }

  #access(
    rawAccess: ModelPolicyStoreAccess,
    projectId: ModelPolicyGetRequest["projectId"],
  ): ModelPolicyStoreResult<ModelPolicyStoreAccess> {
    if (!Array.isArray(rawAccess.authorizedProjectIds))
      return failure("unauthorized", "authenticated model policy access is malformed");
    return rawAccess.authorizedProjectIds.includes(projectId)
      ? { ok: true, value: rawAccess }
      : failure("forbidden", "project is outside authorized model policy scope");
  }
}

export function createModelPolicyService(options: ModelPolicyServiceOptions): ModelPolicyService {
  return new AuthenticatedModelPolicyService(options);
}
