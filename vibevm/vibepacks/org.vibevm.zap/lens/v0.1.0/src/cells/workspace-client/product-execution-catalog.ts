/** Browser-safe global execution catalog client. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import type {
  ExecutionBindingChoice,
  ExecutionCatalogResult,
  ExecutionCatalogSnapshot,
  ExecutionModelReferenceView,
} from "../execution-catalog/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import type { ProductSetupPort } from "../workspace-model/index.ts";

export interface ProductExecutionCatalogView {
  readonly administrator: boolean;
  readonly snapshot: ExecutionCatalogSnapshot;
  readonly availableBindings: readonly ExecutionBindingChoice[];
  readonly modelReferences: readonly ExecutionModelReferenceView[];
}

export async function readProductExecutionCatalog(
  port: ProductSetupPort,
): Promise<ExecutionCatalogResult<ProductExecutionCatalogView>> {
  const response = await port.request({ operation: "product.execution-catalog.get.v1" });
  if (!response.ok) return failed(response.error.code, response.error.message);
  return response.value.operation === "product.execution-catalog.get.v1"
    ? {
        ok: true,
        value: {
          administrator: response.value.administrator,
          snapshot: response.value.snapshot,
          availableBindings: response.value.availableBindings,
          modelReferences: response.value.modelReferences,
        },
      }
    : mismatch("product.execution-catalog.get.v1");
}

export async function createProductExecutionConnection(
  port: ProductSetupPort,
  snapshot: ExecutionCatalogSnapshot,
  bindingId: string,
): Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>> {
  const response = await port.request({
    operation: "product.execution-catalog.connection.create.v1",
    ...identity(snapshot, "connection-create"),
    bindingId,
    displayName: null,
  });
  if (!response.ok) return failed(response.error.code, response.error.message);
  return response.value.operation === "product.execution-catalog.connection.create.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("product.execution-catalog.connection.create.v1");
}

export async function createProductExecutionConfiguration(
  port: ProductSetupPort,
  snapshot: ExecutionCatalogSnapshot,
  connectionId: string,
  referenceId: string,
): Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>> {
  const response = await port.request({
    operation: "product.execution-catalog.configuration.create.v1",
    ...identity(snapshot, "configuration-create"),
    connectionId,
    referenceId,
    displayName: null,
  });
  if (!response.ok) return failed(response.error.code, response.error.message);
  return response.value.operation === "product.execution-catalog.configuration.create.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("product.execution-catalog.configuration.create.v1");
}

export async function refreshProductExecutionUsage(
  port: ProductSetupPort,
  snapshot: ExecutionCatalogSnapshot,
  connectionId: string,
): Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>> {
  const response = await port.request({
    operation: "product.execution-catalog.usage.refresh.v1",
    ...identity(snapshot, "usage-refresh"),
    connectionId,
  });
  if (!response.ok) return failed(response.error.code, response.error.message);
  return response.value.operation === "product.execution-catalog.usage.refresh.v1"
    ? { ok: true, value: response.value.snapshot }
    : mismatch("product.execution-catalog.usage.refresh.v1");
}

export async function saveProductExecutionCatalog(
  port: ProductSetupPort,
  current: ExecutionCatalogSnapshot,
  draft: ExecutionCatalogSnapshot,
): Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>> {
  let snapshot = current;
  for (const connection of changed(
    current.connections,
    draft.connections,
    (value) => value.connectionId,
  )) {
    const response = await port.request({
      operation: "product.execution-catalog.connection.upsert.v1",
      ...identity(snapshot, "connection"),
      connection,
    });
    if (!response.ok) return failed(response.error.code, response.error.message);
    if (response.value.operation !== "product.execution-catalog.connection.upsert.v1")
      return mismatch("product.execution-catalog.connection.upsert.v1");
    snapshot = response.value.snapshot;
  }
  for (const configuration of changed(
    current.configurations,
    draft.configurations,
    (value) => value.configurationId,
  )) {
    const response = await port.request({
      operation: "product.execution-catalog.configuration.upsert.v1",
      ...identity(snapshot, "configuration"),
      configuration,
    });
    if (!response.ok) return failed(response.error.code, response.error.message);
    if (response.value.operation !== "product.execution-catalog.configuration.upsert.v1")
      return mismatch("product.execution-catalog.configuration.upsert.v1");
    snapshot = response.value.snapshot;
  }
  if (JSON.stringify(current.preferences) !== JSON.stringify(draft.preferences)) {
    const response = await port.request({
      operation: "product.execution-catalog.preferences.update.v1",
      ...identity(snapshot, "preferences"),
      preferences: draft.preferences,
    });
    if (!response.ok) return failed(response.error.code, response.error.message);
    if (response.value.operation !== "product.execution-catalog.preferences.update.v1")
      return mismatch("product.execution-catalog.preferences.update.v1");
    snapshot = response.value.snapshot;
  }
  return { ok: true, value: snapshot };
}

function identity(snapshot: ExecutionCatalogSnapshot, purpose: string) {
  const nonce = crypto.randomUUID();
  return {
    clientRequestId: ClientRequestIdSchema.parse("request.catalog." + purpose + "." + nonce),
    sourceEventId: "catalog.product-ui." + purpose + "." + nonce,
    expectedCatalogRevision: snapshot.catalogRevision,
    expectedPreferencesRevision: snapshot.preferencesRevision,
  };
}

function changed<T>(current: readonly T[], next: readonly T[], key: (value: T) => string) {
  const byId = new Map(current.map((value) => [key(value), value]));
  return next.filter((value) => JSON.stringify(byId.get(key(value))) !== JSON.stringify(value));
}

function mismatch(operation: string): ExecutionCatalogResult<never> {
  return failed("invalid_input", "Product response did not match " + operation + ".");
}

function failed(code: string, message: string): ExecutionCatalogResult<never> {
  const mapped =
    code === "invalid_input" ||
    code === "forbidden" ||
    code === "not_found" ||
    code === "conflict" ||
    code === "stale_revision" ||
    code === "idempotency_conflict"
      ? code
      : "storage_failure";
  return { ok: false, error: { code: mapped, message } };
}
