/** Trusted synthetic catalog runtime fixture. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import { join } from "node:path";
import type {
  ExecutionCatalogResult,
  ExecutionConfigurationRecord,
} from "../execution-catalog/index.ts";
import {
  createProductExecutionConfiguration,
  createProductExecutionConnection,
  readProductExecutionCatalog,
} from "../workspace-client/index.ts";
import type { ProductSetupPort } from "../workspace-model/index.ts";
import { mockProductConfig } from "./mock-managed.support.ts";

export const CATALOG_RUNTIME_BINDING_ID = "binding.zap-mock.catalog-runtime";

export function catalogRuntimeConfig(root: string, scenarioPath: string) {
  const base = mockProductConfig(root, scenarioPath);
  const profile = base.managedAgents[0];
  if (profile === undefined)
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: ZapMock profile is missing; fix surface: restore the managed fixture profile",
    );
  return {
    ...base,
    state: { ...base.state, executionCatalogDatabasePath: join(root, "catalog.sqlite") },
    executionBindings: [
      {
        kind: "zap_mock_fixture" as const,
        agentProduct: "zap_mock" as const,
        bindingId: CATALOG_RUNTIME_BINDING_ID,
        hostId: "host.execution.local",
        displayName: "Synthetic ZapMock account",
        enabled: true,
        setupGuidance: "Deterministic zero-LLM runtime fixture; no credential is present.",
      },
    ],
    managedAgents: [
      {
        ...profile,
        accountBindingId: CATALOG_RUNTIME_BINDING_ID,
        executionHostId: "host.execution.local",
      },
    ],
  };
}

export async function ensureCatalogRuntimeConfiguration(
  product: ProductSetupPort,
  modelId = "zap-mock/deterministic-v1",
): Promise<ExecutionCatalogResult<ExecutionConfigurationRecord>> {
  const catalog = await readProductExecutionCatalog(product);
  if (!catalog.ok) return catalog;
  const binding = catalog.value.availableBindings.find(
    (candidate) => candidate.bindingId === CATALOG_RUNTIME_BINDING_ID,
  );
  const reference = catalog.value.modelReferences.find(
    (candidate) => candidate.modelId === modelId,
  );
  if (binding === undefined || reference === undefined)
    return failure("synthetic binding or deterministic model reference is unavailable");
  let snapshot = catalog.value.snapshot;
  let account = snapshot.connections.find(
    (candidate) => candidate.launchBindingId === binding.bindingId,
  );
  if (account === undefined) {
    const connected = await createProductExecutionConnection(product, snapshot, binding.bindingId);
    if (!connected.ok) return connected;
    snapshot = connected.value;
    account = snapshot.connections.find(
      (candidate) => candidate.launchBindingId === binding.bindingId,
    );
  }
  if (account === undefined) return failure("synthetic catalog connection was not materialized");
  let configuration = snapshot.configurations.find(
    (candidate) => candidate.connectionId === account.connectionId && candidate.modelId === modelId,
  );
  if (configuration === undefined) {
    const configured = await createProductExecutionConfiguration(
      product,
      snapshot,
      account.connectionId,
      reference.referenceId,
    );
    if (!configured.ok) return configured;
    configuration = configured.value.configurations.find(
      (candidate) =>
        candidate.connectionId === account.connectionId && candidate.modelId === modelId,
    );
  }
  return configuration === undefined
    ? failure("synthetic catalog configuration was not materialized")
    : { ok: true, value: configuration };
}

function failure(message: string): ExecutionCatalogResult<never> {
  return { ok: false, error: { code: "not_found", message } };
}
