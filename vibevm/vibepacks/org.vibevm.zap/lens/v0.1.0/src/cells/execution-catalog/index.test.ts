/** @scope spec://org.vibevm.zap/lens/PROP-015#root */
import assert from "node:assert/strict";
import test from "node:test";
import {
  ExecutionCatalogSnapshotSchema,
  ExecutionConfigurationRecordSchema,
  ExecutionConnectionRecordSchema,
  ExecutionSelectionRequestSchema,
  TrustedExecutionCatalogContextSchema,
  UsageObservationSchema,
  generateExecutionConfigurationName,
  resolveExecutionSelection,
} from "./index.ts";

const NOW = "2026-09-16T12:00:00.000Z";

test("economy and quality endpoints select meaningfully different configurations", () => {
  const strong = configuration("config.strong", "connection.strong", 90, 100, 10);
  const cheap = configuration("config.cheap", "connection.cheap", 85, 65, 100);
  const economy = resolveExecutionSelection(
    snapshot([strong, cheap], 0, []),
    request(),
    trusted([strong, cheap]),
  );
  const quality = resolveExecutionSelection(
    snapshot([strong, cheap], 100, []),
    request(),
    trusted([strong, cheap]),
  );
  assert.equal(economy.ok && economy.value.configurationId, "config.cheap");
  assert.equal(quality.ok && quality.value.configurationId, "config.strong");
});

test("fresh selected subscription bucket deprioritizes and refreshed/reset usage replaces the basis", () => {
  const low = configuration("config.low", "connection.low", 90, 90, 90, ["bucket.selected"]);
  const other = configuration("config.other", "connection.other", 85, 85, 85);
  const lowUsage = usage(5, "2026-09-16T11:59:00.000Z", "2026-09-16T13:00:00.000Z");
  const selected = resolveExecutionSelection(
    snapshot([low, other], 50, [lowUsage], true),
    request(),
    trusted([low, other]),
  );
  assert.equal(selected.ok && selected.value.configurationId, "config.other");
  const refreshed = usage(80, "2026-09-16T12:01:00.000Z", "2026-09-16T14:00:00.000Z");
  const afterRefresh = resolveExecutionSelection(
    snapshot([low, other], 50, [lowUsage, refreshed], true),
    { ...request(), requestedAt: "2026-09-16T12:02:00.000Z" },
    trusted([low, other]),
  );
  assert.equal(afterRefresh.ok && afterRefresh.value.configurationId, "config.low");
  const futureLow = usage(0, "2026-09-16T12:10:00.000Z", "2026-09-16T15:00:00.000Z");
  const beforeFuture = resolveExecutionSelection(
    snapshot([low, other], 50, [refreshed, futureLow], true),
    { ...request(), requestedAt: "2026-09-16T12:02:00.000Z" },
    trusted([low, other]),
  );
  assert.equal(beforeFuture.ok && beforeFuture.value.configurationId, "config.low");
  const afterReset = resolveExecutionSelection(
    snapshot([low, other], 50, [lowUsage], true),
    { ...request(), requestedAt: "2026-09-16T13:01:00.000Z" },
    trusted([low, other]),
  );
  assert.equal(afterReset.ok && afterReset.value.configurationId, "config.low");
});

test("native selection retains the exact parent account binding", () => {
  const parent = configuration("config.parent", "connection.parent", 80, 80, 80);
  const foreign = configuration("config.foreign", "connection.foreign", 100, 100, 100);
  const context = trusted([parent, foreign], {
    selectionRef: "selection.parent",
    configurationId: parent.configurationId,
    connectionId: parent.connectionId,
    launchBindingId: "binding.parent",
    productId: parent.productId,
    effectiveEffort: "low",
    appliedContextTokens: 100_000,
  });
  const selected = resolveExecutionSelection(
    snapshot([parent, foreign], 100, []),
    {
      ...request(),
      productId: parent.productId,
      executionMode: "native",
      invocationScope: "native_subagent",
    },
    context,
  );
  assert.equal(selected.ok && selected.value.configurationId, "config.parent");
});

test("initial native coordinator does not require a parent selection", () => {
  const coordinator = configuration("config.coordinator", "connection.coordinator", 90, 90, 90);
  const selected = resolveExecutionSelection(
    snapshot([coordinator], 50, []),
    {
      ...request(),
      productId: coordinator.productId,
      executionMode: "native",
      invocationScope: "coordinator",
      role: "coordinator",
    },
    trusted([coordinator]),
  );
  assert.equal(selected.ok && selected.value.configurationId, coordinator.configurationId);
});

test("generated human aliases disambiguate every existing secondary collision", () => {
  const first = generateExecutionConfigurationName(nameInput("config.same"), []);
  const second = generateExecutionConfigurationName(nameInput("config.same"), [first]);
  const third = generateExecutionConfigurationName(nameInput("config.same"), [first, second]);
  assert.equal(new Set([first, second, third]).size, 3);
  assert.match(first, /^[A-Z][a-z]+ [A-Z][a-z]+$/);
});

function request() {
  return ExecutionSelectionRequestSchema.parse({
    selectionRef: "selection.test",
    specialization: "backend",
    purpose: "development_implementation",
    taskClass: "change",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    productId: "product.initiator",
    productVersion: "1",
    requiredModalities: ["text"],
    effort: { mode: "explicit", value: "low" },
    context: { mode: "explicit", tokens: 100_000 },
    override: null,
    requestedAt: NOW,
  });
}

function configuration(
  configurationId: string,
  connectionId: string,
  suitability: number,
  quality: number,
  economy: number,
  usageBucketIds: readonly string[] = [],
) {
  const suffix = connectionId.split(".").at(-1) ?? "unknown";
  return ExecutionConfigurationRecordSchema.parse({
    configurationId,
    displayName: `Maya ${suffix}`,
    connectionId,
    providerId: "provider.codex",
    agentProduct: "codex",
    productId: "profile.codex.template",
    modelVendorId: "vendor.openai",
    modelFamilyId: "family.openai",
    modelId: `model-${suffix}`,
    executionModes: ["native", "managed"],
    invocationScopes: ["coordinator", "native_subagent", "managed_agent"],
    modalities: ["text"],
    adapterEffort: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
    effort: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
    adapterContext: {
      mode: "configurable",
      allowedTokens: [100_000],
      defaultTokens: 100_000,
      documentedMaximumTokens: 200_000,
    },
    context: {
      mode: "configurable",
      allowedTokens: [100_000],
      defaultTokens: 100_000,
      documentedMaximumTokens: 200_000,
    },
    scores: [
      {
        specialization: "backend",
        suitability,
        quality,
        economy,
        preferenceOrder: 0,
        rationale: "deterministic fixture",
        provenance: "synthetic_fixture",
      },
    ],
    usageBucketIds,
    enabled: true,
    synthetic: true,
    evidence: { source: "fixture", observedAt: NOW },
    createdAt: NOW,
    updatedAt: NOW,
  });
}

function snapshot(
  configurations: readonly ReturnType<typeof configuration>[],
  economyQuality: number,
  usageRows: readonly ReturnType<typeof usage>[],
  quota = false,
) {
  return ExecutionCatalogSnapshotSchema.parse({
    protocol: "zap-execution-catalog/1",
    catalogRevision: "1",
    preferencesRevision: "1",
    connections: configurations.map((configuration) =>
      ExecutionConnectionRecordSchema.parse({
        connectionId: configuration.connectionId,
        displayName: configuration.connectionId,
        providerId: configuration.providerId,
        agentProduct: configuration.agentProduct,
        launchBindingId: configuration.connectionId.replace("connection", "binding"),
        enabled: true,
        synthetic: true,
        setupGuidance: "fixture only",
        createdAt: NOW,
        updatedAt: NOW,
      }),
    ),
    configurations,
    preferences: {
      economyQuality,
      quota: { deprioritizeLowRemaining: quota, thresholdPercent: 10, freshnessSeconds: 300 },
    },
    usage: usageRows,
    updatedAt: NOW,
  });
}

function trusted(
  configurations: readonly ReturnType<typeof configuration>[],
  parentSelection: Parameters<typeof TrustedExecutionCatalogContextSchema.parse>[0] extends never
    ? never
    : {
        selectionRef: string;
        configurationId: string;
        connectionId: string;
        launchBindingId: string;
        productId: string;
        effectiveEffort: "low" | null;
        appliedContextTokens: number | null;
      } | null = null,
) {
  return TrustedExecutionCatalogContextSchema.parse({
    allowedConnectionIds: configurations.map((configuration) => configuration.connectionId),
    allowedConfigurationIds: configurations.map((configuration) => configuration.configurationId),
    availableLaunchBindingIds: configurations.map((configuration) =>
      configuration.connectionId.replace("connection", "binding"),
    ),
    parentSelection,
  });
}

function usage(remainingPercent: number, observedAt: string, resetsAt: string) {
  return UsageObservationSchema.parse({
    observationId: `usage.${remainingPercent}.${observedAt.slice(11, 16).replace(":", "")}`,
    connectionId: "connection.low",
    bucketId: "bucket.selected",
    bucketLabel: "Selected subscription",
    applicability: { kind: "unknown", reason: "provider did not label the model" },
    meterKind: "subscription",
    unit: "percent",
    remainingPercent,
    exactRemainingTokens: null,
    window: { kind: "primary", resetsAt },
    observedAt,
    source: "fixture",
    status: "observed",
    detail: "fixture",
  });
}

function nameInput(configurationId: string) {
  return {
    agentProductName: "Codex",
    modelName: "Sol",
    accountName: "Work",
    qualifier: null,
    configurationId,
  };
}
