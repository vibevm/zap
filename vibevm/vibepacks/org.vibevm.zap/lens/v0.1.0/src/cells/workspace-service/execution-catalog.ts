/** Workspace projection of the shared execution catalog. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import type { ExecutionCatalogService } from "../execution-catalog-service/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceReadRequest,
  WorkspaceReadResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { workspaceFailure } from "./errors.ts";

type CatalogRead = Extract<WorkspaceReadRequest, { operation: `execution-catalog.${string}` }>;
type CatalogCommand = Extract<
  WorkspaceCommandRequest,
  { operation: `execution-catalog.${string}` }
>;

export function isExecutionCatalogRead(request: WorkspaceReadRequest): request is CatalogRead {
  return request.operation.startsWith("execution-catalog.");
}

export function isExecutionCatalogCommand(
  request: WorkspaceCommandRequest,
): request is CatalogCommand {
  return request.operation.startsWith("execution-catalog.");
}

export async function readExecutionCatalog(
  service: ExecutionCatalogService | undefined,
  access: WorkspaceAccessContext,
  request: CatalogRead,
): Promise<WorkspaceResult<WorkspaceReadResponse>> {
  if (service === undefined) return unavailable();
  if (request.operation === "execution-catalog.get.v1") {
    const result = await service.get(access, {});
    return result.ok
      ? {
          ok: true,
          value: {
            operation: request.operation,
            administrator: result.value.administrator,
            snapshot: result.value.snapshot,
            availableBindings: [...result.value.availableBindings],
            modelReferences: [...result.value.modelReferences],
          },
        }
      : catalogFailure(result.error.code, result.error.message);
  }
  if (request.operation === "execution-catalog.preview.v1") {
    const result = await service.preview(access, {
      projectId: request.projectId,
      contextId: request.contextId,
      request: request.request,
    });
    return result.ok
      ? { ok: true, value: { operation: request.operation, result: result.value.result } }
      : catalogFailure(result.error.code, result.error.message);
  }
  if (request.operation === "execution-catalog.history.v1") {
    const result = service.history(access, {});
    return result.ok
      ? { ok: true, value: { operation: request.operation, changes: [...result.value.changes] } }
      : catalogFailure(result.error.code, result.error.message);
  }
  const result = service.selection(access, {
    projectId: request.projectId,
    contextId: request.contextId,
    runId: request.runId,
    attemptId: request.attemptId,
  });
  return result.ok
    ? { ok: true, value: { operation: request.operation, selection: result.value.selection } }
    : catalogFailure(result.error.code, result.error.message);
}

export async function commandExecutionCatalog(
  service: ExecutionCatalogService | undefined,
  access: WorkspaceAccessContext,
  request: CatalogCommand,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  if (service === undefined) return unavailable();
  const identity = {
    clientRequestId: request.clientRequestId,
    sourceEventId: request.sourceEventId,
    expectedCatalogRevision: request.expectedCatalogRevision,
    expectedPreferencesRevision: request.expectedPreferencesRevision,
  };
  const result =
    request.operation === "execution-catalog.connection.create.v1"
      ? await service.createConnection(access, {
          ...identity,
          bindingId: request.bindingId,
          displayName: request.displayName,
        })
      : request.operation === "execution-catalog.connection.upsert.v1"
        ? await service.upsertConnection(access, { ...identity, connection: request.connection })
        : request.operation === "execution-catalog.configuration.create.v1"
          ? await service.createConfiguration(access, {
              ...identity,
              connectionId: request.connectionId,
              referenceId: request.referenceId,
              displayName: request.displayName,
            })
          : request.operation === "execution-catalog.configuration.upsert.v1"
            ? await service.upsertConfiguration(access, {
                ...identity,
                configuration: request.configuration,
              })
            : request.operation === "execution-catalog.preferences.update.v1"
              ? service.updatePreferences(access, {
                  ...identity,
                  preferences: request.preferences,
                })
              : await service.refreshUsage(access, {
                  ...identity,
                  connectionId: request.connectionId,
                });
  return result.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          snapshot: result.value.snapshot,
          change: result.value.change,
        },
      }
    : catalogFailure(result.error.code, result.error.message);
}

function catalogFailure(code: string, message: string): WorkspaceResult<never> {
  if (
    code === "unauthorized" ||
    code === "forbidden" ||
    code === "not_found" ||
    code === "conflict" ||
    code === "stale_revision" ||
    code === "idempotency_conflict" ||
    code === "closed" ||
    code === "invalid_input"
  )
    return workspaceFailure(code, message);
  return workspaceFailure("storage_failure", message);
}
function unavailable(): WorkspaceResult<never> {
  return workspaceFailure("unsupported_operation", "execution catalog service is not configured");
}
