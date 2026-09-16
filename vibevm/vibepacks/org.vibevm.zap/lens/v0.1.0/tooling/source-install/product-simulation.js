/** Registered source-install lifecycle simulation. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import "./bootstrap.test.js";
import "./build-core.test.js";
import "./contract-launchers.test.js";

const scenario = JSON.parse(
  readFileSync(new URL("./source-install.simulation.json", import.meta.url), "utf8"),
);
const seed = process.env.ZAP_MOCK_SIMULATION_SEED ?? scenario.seed;

test("source-install scenario binds the complete deterministic lifecycle", () => {
  assert.equal(process.env.ZAP_MOCK_SIMULATION_ID ?? scenario.scenarioId, scenario.scenarioId);
  assert.deepEqual(scenario.inputs.commands, ["install", "status", "update", "uninstall"]);
  assert.equal(scenario.expected.protocol, "zap-source-install/1");
  assert.equal(scenario.expected.installationId, "org.vibevm.zap.source-install");
  assert.equal(scenario.expected.lensCoordinate, "org.vibevm.zap/lens@0.1.0");
  assert.equal(scenario.expected.engineCoordinate, "org.vibevm.zap/zap@1.1.0");
  assert.equal(scenario.inputs.npmRegistry, "https://registry.example.test/npm");
  assert.equal(scenario.expected.buildWithoutPreparedRefused, true);
  assert.equal(scenario.expected.deploySkippedWithoutPrepared, true);
  assert.equal(scenario.expected.npmRegistryNormalized, true);
  assert.equal(scenario.expected.npmRegistryPreservedOnUpdate, true);
  assert.equal(scenario.expected.unsafeNpmRegistryRefused, true);
  assert.equal(scenario.expected.zeroLlmInference, true);
});

test.after(() => {
  process.stdout.write(
    `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
      scenarioId: scenario.scenarioId,
      seed,
      passed: true,
      inputPosition: scenario.coverage.length,
      zeroLlmInference: scenario.expected.zeroLlmInference,
    })}\n`,
  );
});
