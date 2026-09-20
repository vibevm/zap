import assert from "node:assert/strict";
import test from "node:test";
import type { ExecutionCatalogSnapshot } from "../execution-catalog/index.ts";
import {
  archiveConfiguration,
  archiveConnection,
  partitionExecutionCatalog,
  restoreConfiguration,
  restoreConnection,
} from "./execution-catalog-archive.ts";

test("archiving an account hides it and all of its configurations without deleting history", () => {
  const archived = archiveConnection(catalog(), "connection.primary");
  const rows = partitionExecutionCatalog(archived);

  assert.deepEqual(
    rows.activeConnections.map((value) => value.connectionId),
    ["connection.secondary"],
  );
  assert.deepEqual(
    rows.archivedConnections.map((value) => value.connectionId),
    ["connection.primary"],
  );
  assert.deepEqual(
    rows.activeConfigurations.map((value) => value.configurationId),
    ["configuration.secondary"],
  );
  assert.deepEqual(
    rows.archivedConfigurations.map((value) => value.configurationId),
    ["configuration.primary"],
  );
  assert.equal(archived.connections.length, 2);
  assert.equal(archived.configurations.length, 2);
});

test("restore is explicit per account and configuration", () => {
  const archived = archiveConnection(catalog(), "connection.primary");
  const refused = restoreConfiguration(archived, "configuration.primary");
  assert.equal(refused, archived);

  const account = restoreConnection(archived, "connection.primary");
  assert.equal(account.connections[0]?.enabled, true);
  assert.equal(account.configurations[0]?.enabled, false);

  const restored = restoreConfiguration(account, "configuration.primary");
  assert.equal(restored.configurations[0]?.enabled, true);
  assert.equal(
    archiveConfiguration(restored, "configuration.primary").configurations[0]?.enabled,
    false,
  );
});

function catalog(): ExecutionCatalogSnapshot {
  return {
    connections: [
      { connectionId: "connection.primary", enabled: true },
      { connectionId: "connection.secondary", enabled: true },
    ],
    configurations: [
      {
        configurationId: "configuration.primary",
        connectionId: "connection.primary",
        enabled: true,
      },
      {
        configurationId: "configuration.secondary",
        connectionId: "connection.secondary",
        enabled: true,
      },
    ],
  } as unknown as ExecutionCatalogSnapshot;
}
