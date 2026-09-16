/** Browser-safe execution catalog client. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import type {
  ExecutionCatalogResult,
  ExecutionCatalogSnapshot,
  ExecutionBindingChoice,
  ExecutionModelReferenceView,
  ExecutionSelection,
  ExecutionSelectionRequest,
} from "../execution-catalog/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import type {
  ProjectId,
  WorkContextId,
  WorkspaceClientPort,
  WorkspaceResult,
} from "../workspace-model/index.ts";

export interface ExecutionCatalogWorkspaceView {
  readonly administrator: boolean;
  readonly snapshot: ExecutionCatalogSnapshot;
  readonly availableBindings: readonly ExecutionBindingChoice[];
  readonly modelReferences: readonly ExecutionModelReferenceView[];
}

export async function readWorkspaceExecutionCatalog(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
): Promise<WorkspaceResult<ExecutionCatalogWorkspaceView>> {
  const response = await port.read({
    operation: "execution-catalog.get.v1",
    projectId,
    contextId,
  });
  if (!response.ok) return response;
  return response.value.operation === "execution-catalog.get.v1"
    ? {
        ok: true,
        value: {
          administrator: response.value.administrator,
          snapshot: response.value.snapshot,
          availableBindings: response.value.availableBindings,
          modelReferences: response.value.modelReferences,
        },
      }
    : mismatch("execution-catalog.get.v1");
}

export async function createWorkspaceExecutionConnection(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  snapshot: ExecutionCatalogSnapshot,
  bindingId: string,
): Promise<WorkspaceResult<ExecutionCatalogSnapshot>> {
  const response = await port.command({
    operation: "execution-catalog.connection.create.v1",
    ...identity(snapshot, "connection-create"),
    projectId,
    contextId,
    bindingId,
    displayName: null,
  });
  if (!response.ok) return response;
  return response.value.operation === "execution-catalog.connection.create.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("execution-catalog.connection.create.v1");
}

export async function createWorkspaceExecutionConfiguration(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  snapshot: ExecutionCatalogSnapshot,
  connectionId: string,
  referenceId: string,
): Promise<WorkspaceResult<ExecutionCatalogSnapshot>> {
  const response = await port.command({
    operation: "execution-catalog.configuration.create.v1",
    ...identity(snapshot, "configuration-create"),
    projectId,
    contextId,
    connectionId,
    referenceId,
    displayName: null,
  });
  if (!response.ok) return response;
  return response.value.operation === "execution-catalog.configuration.create.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("execution-catalog.configuration.create.v1");
}

export async function refreshWorkspaceExecutionUsage(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  snapshot: ExecutionCatalogSnapshot,
  connectionId: string,
): Promise<WorkspaceResult<ExecutionCatalogSnapshot>> {
  const response = await port.command({
    operation: "execution-catalog.usage.refresh.v1",
    ...identity(snapshot, "usage-refresh"),
    projectId,
    contextId,
    connectionId,
  });
  if (!response.ok) return response;
  return response.value.operation === "execution-catalog.usage.refresh.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("execution-catalog.usage.refresh.v1");
}

export async function previewWorkspaceExecutionCatalog(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  request: Omit<ExecutionSelectionRequest, "requestedAt">,
): Promise<ExecutionCatalogResult<ExecutionSelection>> {
  const response = await port.read({
    operation: "execution-catalog.preview.v1",
    projectId,
    contextId,
    request,
  });
  if (!response.ok)
    return {
      ok: false,
      error: { code: mapCode(response.error.code), message: response.error.message },
    };
  return response.value.operation === "execution-catalog.preview.v1"
    ? response.value.result
    : { ok: false, error: { code: "invalid_input", message: "catalog preview response changed" } };
}

export async function saveWorkspaceExecutionCatalog(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  current: ExecutionCatalogSnapshot,
  draft: ExecutionCatalogSnapshot,
): Promise<WorkspaceResult<ExecutionCatalogSnapshot>> {
  let snapshot = current;
  for (const connection of changed(
    current.connections,
    draft.connections,
    (value) => value.connectionId,
  )) {
    const response = await port.command({
      operation: "execution-catalog.connection.upsert.v1",
      ...identity(snapshot, "connection"),
      projectId,
      contextId,
      connection,
    });
    if (!response.ok) return response;
    if (response.value.operation !== "execution-catalog.connection.upsert.v1")
      return mismatch("execution-catalog.connection.upsert.v1");
    snapshot = response.value.snapshot;
  }
  for (const configuration of changed(
    current.configurations,
    draft.configurations,
    (value) => value.configurationId,
  )) {
    const response = await port.command({
      operation: "execution-catalog.configuration.upsert.v1",
      ...identity(snapshot, "configuration"),
      projectId,
      contextId,
      configuration,
    });
    if (!response.ok) return response;
    if (response.value.operation !== "execution-catalog.configuration.upsert.v1")
      return mismatch("execution-catalog.configuration.upsert.v1");
    snapshot = response.value.snapshot;
  }
  if (JSON.stringify(current.preferences) !== JSON.stringify(draft.preferences)) {
    const response = await port.command({
      operation: "execution-catalog.preferences.update.v1",
      ...identity(snapshot, "preferences"),
      projectId,
      contextId,
      preferences: draft.preferences,
    });
    if (!response.ok) return response;
    if (response.value.operation !== "execution-catalog.preferences.update.v1")
      return mismatch("execution-catalog.preferences.update.v1");
    snapshot = response.value.snapshot;
  }
  return { ok: true, value: snapshot };
}

function identity(snapshot: ExecutionCatalogSnapshot, purpose: string) {
  const nonce = crypto.randomUUID();
  return {
    clientRequestId: ClientRequestIdSchema.parse("request.catalog." + purpose + "." + nonce),
    sourceEventId: "catalog.ui." + purpose + "." + nonce,
    expectedCatalogRevision: snapshot.catalogRevision,
    expectedPreferencesRevision: snapshot.preferencesRevision,
  };
}

function changed<T>(
  current: readonly T[],
  next: readonly T[],
  key: (value: T) => string,
): readonly T[] {
  const byId = new Map(current.map((value) => [key(value), value]));
  return next.filter((value) => JSON.stringify(byId.get(key(value))) !== JSON.stringify(value));
}

function mismatch(operation: string): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_input",
      message:
        "violates REQ spec://org.vibevm.zap/lens/PROP-015#catalog: workspace response did not match " +
        operation,
    },
  };
}

function mapCode(
  code: string,
):
  | "invalid_input"
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "conflict"
  | "stale_revision"
  | "idempotency_conflict"
  | "no_eligible_configuration"
  | "override_refused"
  | "closed"
  | "storage_failure" {
  return code === "unauthorized" ||
    code === "forbidden" ||
    code === "not_found" ||
    code === "conflict" ||
    code === "stale_revision" ||
    code === "idempotency_conflict" ||
    code === "closed" ||
    code === "invalid_input"
    ? code
    : "storage_failure";
}
