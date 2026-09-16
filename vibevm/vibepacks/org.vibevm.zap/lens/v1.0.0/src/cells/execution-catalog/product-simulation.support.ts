/** Synthetic catalog scenario expansion. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import {
  ExecutionCatalogSnapshotSchema,
  generateExecutionConfigurationName,
  type ExecutionCatalogSnapshot,
  type ExecutionConfigurationRecord,
  type ExecutionConnectionRecord,
  type ExecutionModelReferenceView,
  type TrustedExecutionCatalogContext,
  type UsageObservation,
} from "./index.ts";
import type { ExecutionCatalogAuthorityPort } from "../execution-catalog-service/index.ts";
import {
  ExecutionCatalogCallerSchema,
  type ExecutionCatalogCaller,
} from "../execution-catalog-service/index.ts";
import type { ExecutionCatalogProductSimulation } from "../mock-simulation/index.ts";

export interface ExpandedCatalogFixture {
  readonly snapshot: ExecutionCatalogSnapshot;
  readonly authority: ExecutionCatalogAuthorityPort;
  readonly caller: ExecutionCatalogCaller;
  readonly foreignCaller: ExecutionCatalogCaller;
}

export function expandCatalogFixture(
  scenario: ExecutionCatalogProductSimulation,
): ExpandedCatalogFixture {
  const input = scenario.inputs;
  const connections = input.connections.map<ExecutionConnectionRecord>((connection) => ({
    connectionId: connection.connectionId,
    displayName: connection.accountName,
    providerId: "provider.zap-mock",
    agentProduct: "zap_mock",
    launchBindingId: `binding.${connection.connectionId}`,
    enabled: true,
    synthetic: true,
    setupGuidance: `Synthetic ${connection.emulatedAgentProduct} identity; no credential is used.`,
    createdAt: input.now,
    updatedAt: input.now,
  }));
  const names: string[] = [];
  const configurations = input.personas.map<ExecutionConfigurationRecord>((persona) => {
    const connection = connections.find(
      (candidate) => candidate.connectionId === persona.connectionId,
    );
    if (connection === undefined)
      throw new Error(
        reqMessage(
          "a synthetic configuration references a missing connection",
          "add the referenced connection to catalog-routing.simulation.json",
        ),
      );
    const displayName = generateExecutionConfigurationName(
      {
        agentProductName: persona.emulatedAgentProduct,
        modelName: persona.modelId,
        accountName: connection.displayName,
        qualifier: null,
        configurationId: persona.configurationId,
      },
      names,
    );
    names.push(displayName);
    const explicitEfforts = persona.efforts.filter((effort) => effort !== "default");
    const effort =
      explicitEfforts.length === 0
        ? ({ mode: "unsupported" } as const)
        : ({
            mode: "configurable",
            allowedValues: explicitEfforts,
            defaultValue: explicitEfforts[0] ?? null,
          } as const);
    const context = {
      mode: "configurable" as const,
      allowedTokens: persona.contexts,
      defaultTokens: persona.contexts[0] ?? null,
      documentedMaximumTokens: Math.max(...persona.contexts),
    };
    return {
      configurationId: persona.configurationId,
      displayName,
      connectionId: persona.connectionId,
      providerId: connection.providerId,
      agentProduct: "zap_mock",
      productId: "product.zap-mock",
      modelVendorId: persona.modelVendorId,
      modelFamilyId: persona.modelFamilyId,
      modelId: persona.modelId,
      executionModes: ["managed"],
      invocationScopes: ["managed_agent"],
      modalities: persona.modalities,
      adapterEffort: effort,
      effort,
      adapterContext: context,
      context,
      scores: persona.scores.map((score) => ({
        specialization: score.specialization,
        suitability: score.suitability,
        quality: score.quality,
        economy: score.economy,
        preferenceOrder: score.order,
        rationale: `Synthetic ${persona.emulatedAgentProduct} ${score.specialization} persona.`,
        provenance: "synthetic_fixture",
      })),
      usageBucketIds: [
        input.connections.find((candidate) => candidate.connectionId === persona.connectionId)
          ?.usage.bucketId ?? "bucket.missing",
      ],
      enabled: true,
      synthetic: true,
      evidence: { source: "synthetic catalog fixture", observedAt: input.now },
      createdAt: input.now,
      updatedAt: input.now,
    };
  });
  const usage = input.connections.map<UsageObservation>((connection) => ({
    observationId: `usage.${connection.connectionId}`,
    connectionId: connection.connectionId,
    bucketId: connection.usage.bucketId,
    bucketLabel: `${connection.accountName} subscription`,
    applicability: { kind: "account" },
    meterKind: "subscription",
    unit: "percent",
    remainingPercent: connection.usage.remainingPercent,
    exactRemainingTokens: null,
    window: { kind: "weekly", resetsAt: "2026-09-23T12:00:00.000Z" },
    observedAt: connection.usage.state === "stale" ? "2026-09-15T12:00:00.000Z" : input.now,
    source: "synthetic usage fixture",
    status:
      connection.usage.state === "unknown"
        ? "unknown"
        : connection.usage.state === "unsupported"
          ? "unsupported"
          : "observed",
    detail: `Synthetic ${connection.usage.state} subscription observation.`,
  }));
  const snapshot = ExecutionCatalogSnapshotSchema.parse({
    protocol: "zap-execution-catalog/1",
    catalogRevision: "1",
    preferencesRevision: "1",
    connections,
    configurations,
    preferences: {
      economyQuality: 50,
      quota: { deprioritizeLowRemaining: true, thresholdPercent: 10, freshnessSeconds: 300 },
    },
    usage,
    updatedAt: input.now,
  });
  const trusted: TrustedExecutionCatalogContext = {
    allowedConnectionIds: connections.map((connection) => connection.connectionId),
    allowedConfigurationIds: configurations.map((configuration) => configuration.configurationId),
    availableLaunchBindingIds: connections.map((connection) => connection.launchBindingId),
    parentSelection: null,
  };
  const references = input.personas.map<ExecutionModelReferenceView>((persona) => ({
    referenceId: `reference.${persona.configurationId}`,
    modelVendorId: persona.modelVendorId,
    modelFamilyId: persona.modelFamilyId,
    modelId: persona.modelId,
    conversationModel: true,
    availability: "catalog_only",
    specializations: persona.scores.map((score) => score.specialization),
    sourceUrls: [],
    note: `Synthetic ${persona.emulatedAgentProduct} model reference.`,
  }));
  const authority = fixtureAuthority(trusted, references);
  return {
    snapshot,
    authority,
    caller: caller("owner", [input.projectId]),
    foreignCaller: caller("foreign", [input.foreignProjectId]),
  };
}

function fixtureAuthority(
  trusted: TrustedExecutionCatalogContext,
  references: readonly ExecutionModelReferenceView[],
): ExecutionCatalogAuthorityPort {
  return {
    availableBindings: () => ({ ok: true, value: [] }),
    modelReferences: () => ({ ok: true, value: references }),
    createConnection: () => refused(),
    createConfiguration: () => refused(),
    validateConnection: ({ connection }) =>
      connection.agentProduct === "zap_mock" && connection.synthetic
        ? { ok: true, value: connection }
        : refused(),
    validateConfiguration: ({ connection, configuration }) =>
      connection.agentProduct === "zap_mock" &&
      configuration.agentProduct === "zap_mock" &&
      configuration.synthetic
        ? { ok: true, value: configuration }
        : refused(),
    trustedContext: () => ({ ok: true, value: trusted }),
  };
}

function caller(name: string, projects: readonly string[]): ExecutionCatalogCaller {
  return ExecutionCatalogCallerSchema.parse({
    principalId: `principal.catalog.${name}`,
    actorId: `actor.catalog.${name}`,
    clientId: `client.catalog.${name}`,
    authorizedProjectIds: [...projects],
  });
}

function refused() {
  return {
    ok: false as const,
    error: { code: "forbidden" as const, message: "synthetic fixture authority refused input" },
  };
}

function reqMessage(why: string, fix: string): string {
  return `violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: ${why}; fix surface: ${fix}`;
}
