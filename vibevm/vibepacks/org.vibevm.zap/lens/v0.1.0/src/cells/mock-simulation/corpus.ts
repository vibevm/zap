/** Corpus discovery and coverage validation. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { readdirSync, readFileSync } from "node:fs";
import { relative, resolve, sep } from "node:path";
import {
  WorkspaceCommandRequestSchema,
  WorkspaceReadRequestSchema,
} from "../workspace-model/index.ts";
import {
  MockSimulationDocumentSchema,
  MockSimulationRegistrySchema,
  type MockSimulationDocument,
  type MockSimulationRegistry,
} from "./schemas.ts";

export type MockCorpusResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: { readonly code: string; readonly message: string } };

export interface MockSimulationCorpus {
  readonly root: string;
  readonly registry: MockSimulationRegistry;
  readonly documents: readonly MockSimulationDocument[];
  readonly gaps: readonly { readonly feature: string; readonly gap: string }[];
}

export function defaultMockSimulationRoot(): string {
  return resolve(import.meta.dirname, "../../..");
}

export function loadMockSimulationCorpus(
  root = defaultMockSimulationRoot(),
): MockCorpusResult<MockSimulationCorpus> {
  const registry = readJson(resolve(root, "src/cells/mock-simulation/coverage.json"));
  const checkedRegistry = MockSimulationRegistrySchema.safeParse(registry);
  if (!checkedRegistry.success) return failure("invalid_registry", "coverage registry is invalid");
  const discovered = [
    ...discover(resolve(root, "src/cells"), root),
    ...discover(resolve(root, "tooling"), root),
  ].toSorted();
  const indexed = checkedRegistry.data.documents.map((entry) => normalized(entry.path));
  const unindexed = discovered.filter((path) => !indexed.includes(path));
  const missing = indexed.filter((path) => !discovered.includes(path));
  const firstUnindexed = unindexed[0];
  if (firstUnindexed !== undefined)
    return failure(
      "unindexed_scenario",
      `colocated scenario is absent from registry: ${firstUnindexed}`,
    );
  const firstMissing = missing[0];
  if (firstMissing !== undefined)
    return failure("stale_scenario", `registry scenario path is missing: ${firstMissing}`);
  const documents: MockSimulationDocument[] = [];
  const ids = new Set<string>();
  for (const entry of checkedRegistry.data.documents) {
    const raw = readJson(resolve(root, entry.path));
    const document = MockSimulationDocumentSchema.safeParse(raw);
    if (!document.success)
      return failure("invalid_scenario", `scenario document is invalid: ${entry.path}`);
    if (
      document.data.scenarioId !== entry.scenarioId ||
      document.data.runnerId !== entry.runnerId ||
      ids.has(document.data.scenarioId)
    )
      return failure("stale_scenario", `scenario registry identity drifted: ${entry.scenarioId}`);
    ids.add(document.data.scenarioId);
    documents.push(document.data);
  }
  const coverage = new Map<string, Set<string>>();
  const gaps: Array<{ feature: string; gap: string }> = [];
  for (const row of checkedRegistry.data.coverage) {
    if (coverage.has(row.feature) || gaps.some((gap) => gap.feature === row.feature))
      return failure("invalid_registry", `coverage feature is duplicated: ${row.feature}`);
    if ("gap" in row) {
      gaps.push(row);
      continue;
    }
    if (row.scenarioIds.some((id) => !ids.has(id)))
      return failure("stale_scenario", `coverage references an unknown scenario: ${row.feature}`);
    coverage.set(row.feature, new Set(row.scenarioIds));
  }
  for (const document of documents)
    for (const feature of document.coverage)
      if (!coverage.get(feature)?.has(document.scenarioId))
        return failure("unmapped_coverage", `scenario coverage is not indexed: ${feature}`);
  const inventory = [
    ...checkedRegistry.data.featureInventory,
    ...workspaceOperations().map((operation) => `workspace.operation.${operation}`),
  ];
  for (const feature of new Set(inventory))
    if (!coverage.has(feature) && !gaps.some((gap) => gap.feature === feature))
      gaps.push({
        feature,
        gap: feature.startsWith("workspace.operation.")
          ? "Public workspace operation has no registered simulation scenario."
          : "Public product feature has no registered simulation scenario.",
      });
  return {
    ok: true,
    value: { root, registry: checkedRegistry.data, documents, gaps },
  };
}

function workspaceOperations(): string[] {
  return [
    ...WorkspaceReadRequestSchema.options.map((option) => option.shape.operation.value),
    ...WorkspaceCommandRequestSchema.options.map((option) => option.shape.operation.value),
  ].toSorted();
}

function discover(directory: string, root: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) found.push(...discover(path, root));
    else if (entry.isFile() && entry.name.endsWith(".simulation.json"))
      found.push(normalized(relative(root, path)));
  }
  return found.toSorted();
}

function readJson(path: string): unknown {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

function normalized(path: string): string {
  return path.split(sep).join("/");
}

function failure(code: string, message: string): MockCorpusResult<never> {
  return { ok: false, error: { code, message } };
}
