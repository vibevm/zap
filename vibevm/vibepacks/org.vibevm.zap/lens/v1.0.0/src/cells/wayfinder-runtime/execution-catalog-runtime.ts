/** Open the trusted local execution catalog and protected account boundary. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { dirname, resolve } from "node:path";
import { createHash } from "node:crypto";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import {
  createExecutionAccountIsolation,
  type ExecutionAccountIsolationPort,
} from "../execution-accounts/index.ts";
import {
  createExecutionCatalogService,
  type ExecutionCatalogCaller,
  type ExecutionCatalogService,
} from "../execution-catalog-service/index.ts";
import type { ExecutionCatalogSnapshot } from "../execution-catalog/index.ts";
import {
  openExecutionCatalogStore,
  type ExecutionCatalogStore,
} from "../execution-catalog-store/index.ts";
import type { ManagedAgentProfile } from "../managed-work/index.ts";
import { ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";
import {
  ClientIdSchema,
  type ExecutionHostId,
  type ProductProviderProfile,
} from "../workspace-model/index.ts";
import type { ProductExecutionBinding } from "../product-app/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import { createRuntimeExecutionCatalogAuthority } from "./execution-catalog.ts";
import { createRuntimeExecutionUsage } from "./execution-catalog-usage.ts";
import type { WayfinderRuntimeConfig } from "./runtime-config.ts";

export type RuntimeExecutionCatalogResult =
  | {
      readonly ok: true;
      readonly value: {
        readonly accounts: ExecutionAccountIsolationPort;
        readonly store: ExecutionCatalogStore;
        readonly service: ExecutionCatalogService;
        readonly productCaller: ExecutionCatalogCaller;
      };
    }
  | { readonly ok: false; readonly message: string };

export function openRuntimeExecutionCatalog(input: {
  readonly databasePath: string;
  readonly configuredDatabasePath?: string;
  readonly executionHostId: ExecutionHostId;
  readonly bindings: readonly ProductExecutionBinding[];
  readonly codexProfiles: readonly CodexCoordinatorProfile[];
  readonly providerProfiles: readonly ProviderCoordinatorProfile[];
  readonly managedProfiles: readonly ManagedAgentProfile[];
  readonly proxyPolicy: ProxyPolicy;
}): RuntimeExecutionCatalogResult {
  const openedAccounts = createExecutionAccountIsolation(input.bindings);
  if (!openedAccounts.ok) return { ok: false, message: openedAccounts.error.message };
  const store = openExecutionCatalogStore({
    databasePath: resolve(
      input.configuredDatabasePath ?? `${input.databasePath}.execution-catalog`,
    ),
    hostId: input.executionHostId,
  });
  if (!store.ok) return { ok: false, message: "execution catalog store could not be opened" };
  const service = createExecutionCatalogService({
    store: store.value,
    access: {
      resolve(access) {
        return {
          ok: true,
          value: {
            ...access,
            hostId: input.executionHostId,
            catalogAdministrator: access.catalogAdministrator ?? false,
          },
        };
      },
    },
    authority: createRuntimeExecutionCatalogAuthority({
      accounts: openedAccounts.value,
      codexProfiles: input.codexProfiles,
      providerProfiles: input.providerProfiles,
      managedProfiles: input.managedProfiles,
    }),
    usage: createRuntimeExecutionUsage({
      accounts: openedAccounts.value,
      hostId: input.executionHostId,
      codexProfiles: input.codexProfiles,
      launchCwd: dirname(input.databasePath),
      proxyPolicy: input.proxyPolicy,
    }),
  });
  const productCaller: ExecutionCatalogCaller = {
    principalId: PrincipalIdSchema.parse("principal.wayfinder.product"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.wayfinder.product"),
    authorizedProjectIds: [],
    catalogAdministrator: true,
  };
  return {
    ok: true,
    value: { accounts: openedAccounts.value, store: store.value, service, productCaller },
  };
}

export function openConfiguredRuntimeExecutionCatalog(input: {
  readonly config: WayfinderRuntimeConfig;
  readonly databasePath: string;
  readonly profiles: CodexCoordinatorProfile[];
}):
  | {
      readonly ok: true;
      readonly value: {
        readonly runtime: Extract<RuntimeExecutionCatalogResult, { readonly ok: true }>["value"];
        readonly codexProfiles: CodexCoordinatorProfile[];
        readonly providerProfiles: ProviderCoordinatorProfile[];
        readonly productProviders: ProductProviderProfile[];
      };
    }
  | { readonly ok: false; readonly message: string } {
  const codexProfiles = uniqueProfiles([
    ...input.profiles,
    ...input.config.executionLaunchTemplates
      .filter((template) => template.agentProduct === "codex")
      .map((template) => template.profile),
  ]);
  const providerProfiles = uniqueProfiles([
    ...input.config.providerCoordinatorProfiles,
    ...input.config.executionLaunchTemplates
      .filter((template) => template.agentProduct === "claude_code")
      .map((template) => template.profile),
  ]);
  const runtime = openRuntimeExecutionCatalog({
    databasePath: input.databasePath,
    ...(input.config.state.executionCatalogDatabasePath === undefined
      ? {}
      : { configuredDatabasePath: input.config.state.executionCatalogDatabasePath }),
    executionHostId: input.config.executionHostId,
    bindings: input.config.executionBindings,
    codexProfiles,
    providerProfiles,
    managedProfiles: input.config.managedAgents,
    proxyPolicy: input.config.proxy,
  });
  return runtime.ok
    ? {
        ok: true,
        value: {
          runtime: runtime.value,
          codexProfiles,
          providerProfiles,
          productProviders: [...input.config.productProviders],
        },
      }
    : runtime;
}

export async function seedConfiguredExecutionCatalog(input: {
  readonly service: ExecutionCatalogService;
  readonly caller: ExecutionCatalogCaller;
  readonly codexProfiles: readonly CodexCoordinatorProfile[];
  readonly providerProfiles: readonly ProviderCoordinatorProfile[];
  readonly managedProfiles: readonly ManagedAgentProfile[];
}): Promise<{ readonly ok: true } | { readonly ok: false; readonly message: string }> {
  const configured = [
    ...input.codexProfiles.map((profile) => ({
      bindingId: profile.accountBindingId,
      modelId: profile.model,
    })),
    ...input.providerProfiles.map((profile) => ({
      bindingId: profile.accountBindingId,
      modelId: profile.modelId,
    })),
    ...input.managedProfiles.map((profile) => ({
      bindingId: profile.accountBindingId,
      modelId: profile.modelId,
    })),
  ].filter(
    (profile): profile is { readonly bindingId: string; readonly modelId: string } =>
      profile.bindingId !== undefined,
  );
  for (const profile of configured) {
    let view = await input.service.get(input.caller, {});
    if (!view.ok) return { ok: false, message: view.error.message };
    let connection = view.value.snapshot.connections.find(
      (candidate) => candidate.launchBindingId === profile.bindingId,
    );
    if (connection === undefined) {
      const binding = view.value.availableBindings.find(
        (candidate) => candidate.bindingId === profile.bindingId,
      );
      if (binding === undefined) continue;
      const created = await input.service.createConnection(input.caller, {
        ...mutationIdentity(view.value.snapshot, "connection", profile.bindingId),
        bindingId: profile.bindingId,
        displayName: binding.displayName,
      });
      if (!created.ok) {
        if (ignorableSeedError(created.error.code)) continue;
        return { ok: false, message: created.error.message };
      }
      connection = created.value.snapshot.connections.find(
        (candidate) => candidate.launchBindingId === profile.bindingId,
      );
      view = await input.service.get(input.caller, {});
      if (!view.ok) return { ok: false, message: view.error.message };
    }
    if (
      connection === undefined ||
      view.value.snapshot.configurations.some(
        (candidate) =>
          candidate.connectionId === connection.connectionId &&
          candidate.modelId === profile.modelId,
      )
    )
      continue;
    const reference = view.value.modelReferences.find(
      (candidate) => candidate.modelId === profile.modelId && candidate.conversationModel,
    );
    if (reference === undefined) continue;
    const created = await input.service.createConfiguration(input.caller, {
      ...mutationIdentity(
        view.value.snapshot,
        "configuration",
        `${profile.bindingId}.${profile.modelId}`,
      ),
      connectionId: connection.connectionId,
      referenceId: reference.referenceId,
      displayName: null,
    });
    if (!created.ok && !ignorableSeedError(created.error.code))
      return { ok: false, message: created.error.message };
  }
  return { ok: true };
}

function mutationIdentity(snapshot: ExecutionCatalogSnapshot, kind: string, identity: string) {
  const key = createHash("sha256").update(`${kind}\u0000${identity}`).digest("hex").slice(0, 24);
  return {
    clientRequestId: ClientRequestIdSchema.parse(`request.catalog.seed.${key}`),
    sourceEventId: `catalog.seed.${key}`,
    expectedCatalogRevision: snapshot.catalogRevision,
    expectedPreferencesRevision: snapshot.preferencesRevision,
  };
}

function ignorableSeedError(code: string): boolean {
  return code === "invalid_input" || code === "not_found" || code === "conflict";
}

function uniqueProfiles<T extends { readonly profileId: string }>(profiles: readonly T[]): T[] {
  const unique = new Map<string, T>();
  for (const profile of profiles) unique.set(profile.profileId, profile);
  return [...unique.values()];
}
