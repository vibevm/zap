/** Recoverable execution-catalog archive presentation. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import type { ExecutionCatalogSnapshot } from "../execution-catalog/index.ts";

export function archiveConnection(
  catalog: ExecutionCatalogSnapshot,
  connectionId: string,
): ExecutionCatalogSnapshot {
  return {
    ...catalog,
    connections: catalog.connections.map((connection) =>
      connection.connectionId === connectionId ? { ...connection, enabled: false } : connection,
    ),
    configurations: catalog.configurations.map((configuration) =>
      configuration.connectionId === connectionId
        ? { ...configuration, enabled: false }
        : configuration,
    ),
  };
}

export function restoreConnection(
  catalog: ExecutionCatalogSnapshot,
  connectionId: string,
): ExecutionCatalogSnapshot {
  return {
    ...catalog,
    connections: catalog.connections.map((connection) =>
      connection.connectionId === connectionId ? { ...connection, enabled: true } : connection,
    ),
  };
}

export function archiveConfiguration(
  catalog: ExecutionCatalogSnapshot,
  configurationId: string,
): ExecutionCatalogSnapshot {
  return setConfigurationEnabled(catalog, configurationId, false);
}

export function restoreConfiguration(
  catalog: ExecutionCatalogSnapshot,
  configurationId: string,
): ExecutionCatalogSnapshot {
  const configuration = catalog.configurations.find(
    (candidate) => candidate.configurationId === configurationId,
  );
  if (
    configuration === undefined ||
    !catalog.connections.some(
      (connection) => connection.connectionId === configuration.connectionId && connection.enabled,
    )
  )
    return catalog;
  return setConfigurationEnabled(catalog, configurationId, true);
}

export function partitionExecutionCatalog(catalog: ExecutionCatalogSnapshot) {
  const activeConnectionIds = new Set(
    catalog.connections
      .filter((connection) => connection.enabled)
      .map((connection) => connection.connectionId),
  );
  return {
    activeConnections: catalog.connections.filter((connection) => connection.enabled),
    archivedConnections: catalog.connections.filter((connection) => !connection.enabled),
    activeConfigurations: catalog.configurations.filter(
      (configuration) =>
        configuration.enabled && activeConnectionIds.has(configuration.connectionId),
    ),
    archivedConfigurations: catalog.configurations.filter(
      (configuration) =>
        !configuration.enabled || !activeConnectionIds.has(configuration.connectionId),
    ),
  };
}

function setConfigurationEnabled(
  catalog: ExecutionCatalogSnapshot,
  configurationId: string,
  enabled: boolean,
): ExecutionCatalogSnapshot {
  return {
    ...catalog,
    configurations: catalog.configurations.map((configuration) =>
      configuration.configurationId === configurationId
        ? { ...configuration, enabled }
        : configuration,
    ),
  };
}
