/** Model-policy feature aggregation into the master workspace port. @scope spec://org.vibevm.zap/lens/PROP-008#policy-interface */
import type { ModelPolicyService } from "../model-policy-service/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceReadRequest,
  WorkspaceReadResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { workspaceFailure } from "./errors.ts";

type PolicyRead = Extract<
  WorkspaceReadRequest,
  {
    operation:
      | "model-policy.get.v1"
      | "model-policy.preview.v1"
      | "model-policy.history.v1"
      | "model-selection.get.v1";
  }
>;

export async function readModelPolicy(
  service: ModelPolicyService | undefined,
  access: WorkspaceAccessContext,
  request: PolicyRead,
): Promise<WorkspaceResult<WorkspaceReadResponse>> {
  if (service === undefined) return unavailable();
  if (request.operation === "model-policy.get.v1") {
    const result = service.get(access, {
      projectId: request.projectId,
      contextId: request.contextId,
    });
    return result.ok
      ? { ok: true, value: { operation: request.operation, policy: result.value.policy } }
      : policyFailure(result.error.code, result.error.message);
  }
  if (request.operation === "model-policy.preview.v1") {
    const result = await service.preview(access, {
      projectId: request.projectId,
      contextId: request.contextId,
      request: request.request,
    });
    return result.ok
      ? { ok: true, value: { operation: request.operation, result: result.value.result } }
      : policyFailure(result.error.code, result.error.message);
  }
  if (request.operation === "model-policy.history.v1") {
    const result = service.history(access, {
      projectId: request.projectId,
      contextId: request.contextId,
    });
    return result.ok
      ? {
          ok: true,
          value: {
            operation: request.operation,
            versions: [...result.value.versions],
            changes: [...result.value.changes],
          },
        }
      : policyFailure(result.error.code, result.error.message);
  }
  const result = service.selection(access, {
    projectId: request.projectId,
    contextId: request.contextId,
    runId: request.runId,
    attemptId: request.attemptId,
  });
  return result.ok
    ? { ok: true, value: { operation: request.operation, selection: result.value.selection } }
    : policyFailure(result.error.code, result.error.message);
}

export function updateModelPolicy(
  service: ModelPolicyService | undefined,
  access: WorkspaceAccessContext,
  request: Extract<WorkspaceCommandRequest, { operation: "model-policy.update.v1" }>,
): WorkspaceResult<WorkspaceCommandResponse> {
  if (service === undefined) return unavailable();
  const result = service.update(access, {
    projectId: request.projectId,
    contextId: request.contextId,
    clientRequestId: request.clientRequestId,
    sourceEventId: request.sourceEventId,
    expectedRevision: request.expectedRevision,
    policy: request.policy,
  });
  return result.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          policy: result.value.policy,
          change: result.value.change,
        },
      }
    : policyFailure(result.error.code, result.error.message);
}

function policyFailure(code: string, message: string): WorkspaceResult<never> {
  if (code === "unauthorized") return workspaceFailure("unauthorized", message);
  if (code === "forbidden") return workspaceFailure("forbidden", message);
  if (code === "not_found") return workspaceFailure("not_found", message);
  if (code === "stale_revision") return workspaceFailure("stale_revision", message);
  if (code === "idempotency_conflict") return workspaceFailure("idempotency_conflict", message);
  if (code === "conflict") return workspaceFailure("conflict", message);
  if (code === "closed") return workspaceFailure("closed", message);
  if (code === "invalid_input") return workspaceFailure("invalid_input", message);
  return workspaceFailure("storage_failure", message);
}

function unavailable(): WorkspaceResult<never> {
  return workspaceFailure("unsupported_operation", "model policy service is not configured");
}
