/** Shared simulation discovery and replay proofs. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import assert from "node:assert/strict";
import test from "node:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  defaultMockSimulationRoot,
  loadMockSimulationCorpus,
  runMockSimulations,
} from "./index.ts";

test("corpus discovers indexed colocated scenarios and exposes honest coverage gaps", () => {
  const loaded = loadMockSimulationCorpus();
  assert.equal(loaded.ok, true);
  if (!loaded.ok) return;
  assert.deepEqual(
    loaded.value.documents.map((document) => document.scenarioId),
    [
      "model.full-lifecycle",
      "managed.question-pause-answer-report-review",
      "coordinator.question-pause-answer-continue",
      "store.additive-plan-context",
      "repository.two-plans-child-clean-integration",
      "repository.conflict-resolution-promotion",
      "repository.stale-and-dirty-target-refusal",
      "repository.restart-idempotent-reconcile",
      "same-project.parallel-plans.fork-merge-assignment",
      "annotations.deferred-trash-restore",
      "model-policy.selection-history-isolation",
      "repository.public-two-plan-integration",
      "repository.managed-isolated-writer-fence",
      "provider.addressed-turn-running-queue-ready",
      "execution-catalog.synthetic-cross-provider-routing",
      "execution-catalog.synthetic-cross-provider-runtime",
      "source-install.bootstrap-build-lifecycle",
    ],
  );
  assert.equal(
    loaded.value.gaps.some((gap) => gap.feature === "coordinator.http-question-lifecycle"),
    false,
  );
  assert.equal(
    loaded.value.gaps.some((gap) => gap.feature.includes("repository")),
    false,
  );
});

test("model scenario selection repeats fresh seeds and returns exact replay evidence", async () => {
  const result = await runMockSimulations({
    ids: ["model.full-lifecycle"],
    repeat: 3,
    seed: "seed.replay.fixture",
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.passed, true);
  assert.equal(result.value.zeroLlmInference, true);
  assert.deepEqual(
    result.value.scenarios.map((receipt) => receipt.seed),
    ["seed.replay.fixture#1", "seed.replay.fixture#2", "seed.replay.fixture#3"],
  );
  assert.equal(
    result.value.scenarios.every((receipt) => receipt.evidenceKind === "pure_model"),
    true,
  );
  assert.equal(result.value.scenarios[0]?.replay.args.includes("model.full-lifecycle"), true);
});

test("unknown and empty selections fail instead of silently passing", async () => {
  const unknownId = await runMockSimulations({ ids: ["scenario.unknown"] });
  assert.equal(unknownId.ok, false);
  const unknownTag = await runMockSimulations({ tags: ["tag.unknown"] });
  assert.equal(unknownTag.ok, false);
  const empty = await runMockSimulations({});
  assert.equal(empty.ok, false);
});

test("discovery rejects unindexed and stale scenario files", () => {
  const source = defaultMockSimulationRoot();
  const root = mkdtempSync(join(tmpdir(), "zap-mock-corpus-"));
  try {
    copy(source, root, "src/cells/mock-simulation/coverage.json");
    copy(source, root, "src/cells/mock-model/full-lifecycle.simulation.json");
    copy(source, root, "src/cells/wayfinder-runtime/mock-managed.simulation.json");
    copy(source, root, "src/cells/mock-agent/question-http.simulation.json");
    copy(source, root, "src/cells/workspace-store/plan-context.simulation.json");
    copy(source, root, "src/cells/repository-workspaces/scenarios/two-plans-clean.simulation.json");
    copy(
      source,
      root,
      "src/cells/repository-workspaces/scenarios/conflict-resolution.simulation.json",
    );
    copy(
      source,
      root,
      "src/cells/repository-workspaces/scenarios/stale-dirty-refusal.simulation.json",
    );
    copy(
      source,
      root,
      "src/cells/repository-workspaces/scenarios/restart-reconcile.simulation.json",
    );
    copy(source, root, "src/cells/quicklens-graph/portfolio-repository.simulation.json");
    copy(source, root, "src/cells/workspace-annotations/deferred-trash-restore.simulation.json");
    copy(
      source,
      root,
      "src/cells/model-policy-service/selection-history-isolation.simulation.json",
    );
    copy(source, root, "src/cells/wayfinder-runtime/repository-product.simulation.json");
    copy(source, root, "src/cells/wayfinder-runtime/repository-managed-product.simulation.json");
    copy(source, root, "src/cells/provider-coordinators/running-projection.simulation.json");
    copy(source, root, "src/cells/execution-catalog/catalog-routing.simulation.json");
    copy(source, root, "src/cells/wayfinder-runtime/execution-catalog-runtime.simulation.json");
    copy(source, root, "tooling/source-install/source-install.simulation.json");
    copy(
      source,
      root,
      "src/cells/mock-model/full-lifecycle.simulation.json",
      "src/cells/mock-model/unindexed.simulation.json",
    );
    const unindexed = loadMockSimulationCorpus(root);
    assert.equal(unindexed.ok, false);
    if (!unindexed.ok) assert.equal(unindexed.error.code, "unindexed_scenario");
    unlinkSync(join(root, "src/cells/mock-model/unindexed.simulation.json"));
    unlinkSync(join(root, "src/cells/wayfinder-runtime/mock-managed.simulation.json"));
    const stale = loadMockSimulationCorpus(root);
    assert.equal(stale.ok, false);
    if (!stale.ok) assert.equal(stale.error.code, "stale_scenario");
  } finally {
    rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

function copy(
  sourceRoot: string,
  targetRoot: string,
  sourcePath: string,
  targetPath = sourcePath,
): void {
  const destination = join(targetRoot, targetPath);
  mkdirSync(dirname(destination), { recursive: true });
  cpSync(join(sourceRoot, sourcePath), destination);
}
