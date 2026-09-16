import assert from "node:assert/strict";
import test from "node:test";
import { createExecutionAccountIsolation } from "../execution-accounts/index.ts";
import { ExecutionCatalogStoreAccessSchema } from "../execution-catalog-store/index.ts";
import { PrincipalIdSchema } from "../protocol/index.ts";
import { ClientIdSchema, ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";
import { createRuntimeExecutionCatalogAuthority } from "./execution-catalog.ts";

test("host-configured provider model remains exact and editable without public metadata", async () => {
  const bindingId = "binding.opencode.laguna";
  const access = ExecutionCatalogStoreAccessSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.catalog.authority"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.catalog.authority"),
    authorizedProjectIds: [],
    hostId: "host.catalog.test",
    catalogAdministrator: true,
  });
  const accounts = createExecutionAccountIsolation([
    {
      bindingId,
      hostId: ExecutionHostIdSchema.parse("host.catalog.test"),
      displayName: "Laguna OpenCode account",
      enabled: true,
      setupGuidance: "Use the existing protected OpenCode environment.",
      kind: "environment_reference",
      agentProduct: "opencode",
      environmentRef: "environment.opencode.laguna",
    },
  ]);
  assert.equal(accounts.ok, true);
  if (!accounts.ok) return;
  const profile = ProviderCoordinatorProfileSchema.parse({
    profileId: "profile.opencode.laguna",
    provider: "opencode",
    executablePath: "C:/fixture/opencode.exe",
    cwd: "C:/fixture",
    modelId: "openrouter/poolside/laguna-s-2.1:free",
    effort: null,
    endpoint: null,
    accountBindingId: bindingId,
    executionHostId: ExecutionHostIdSchema.parse("host.catalog.test"),
    environmentRef: "environment.opencode.laguna",
  });
  const authority = createRuntimeExecutionCatalogAuthority({
    accounts: accounts.value,
    codexProfiles: [],
    providerProfiles: [profile],
    managedProfiles: [],
    observedAt: "2026-09-16T12:00:00.000Z",
  });
  const references = await authority.modelReferences(access);
  assert.equal(references.ok, true);
  if (!references.ok) return;
  const reference = references.value.find((entry) => entry.modelId === profile.modelId);
  assert.notEqual(reference, undefined);
  if (reference === undefined) return;
  assert.match(reference.modelFamilyId, /^host_configured_opencode_/u);
  assert.equal(reference.sourceUrls.length, 0);
  const connection = await authority.createConnection({
    access,
    bindingId,
    connectionId: "connection.opencode.laguna",
    displayName: "Laguna OpenCode account",
    now: "2026-09-16T12:00:00.000Z",
  });
  assert.equal(connection.ok, true);
  if (!connection.ok) return;
  const unrelated = references.value.find((entry) => entry.modelId === "qwen3.8-max");
  assert.notEqual(unrelated, undefined);
  if (unrelated === undefined) return;
  const unavailable = await authority.createConfiguration({
    access,
    connection: connection.value,
    referenceId: unrelated.referenceId,
    configurationId: "configuration.opencode.unavailable",
    displayName: "Unavailable generic model",
    now: "2026-09-16T12:00:00.000Z",
  });
  assert.equal(unavailable.ok, false);
  const configuration = await authority.createConfiguration({
    access,
    connection: connection.value,
    referenceId: reference.referenceId,
    configurationId: "configuration.opencode.laguna",
    displayName: "OpenCode · Laguna",
    now: "2026-09-16T12:00:00.000Z",
  });
  assert.equal(configuration.ok, true, configuration.ok ? undefined : configuration.error.message);
  if (!configuration.ok) return;
  assert.equal(configuration.value.modelId, profile.modelId);
  assert.equal(configuration.value.modelFamilyId, reference.modelFamilyId);
  const edited = await authority.validateConfiguration({
    access,
    connection: connection.value,
    configuration: {
      ...configuration.value,
      displayName: "OpenCode · Laguna exact",
      enabled: false,
    },
  });
  assert.equal(edited.ok, true, edited.ok ? undefined : edited.error.message);
  if (edited.ok) {
    assert.equal(edited.value.modelId, profile.modelId);
    assert.equal(edited.value.enabled, false);
    assert.equal(edited.value.displayName, "OpenCode · Laguna exact");
  }
});
