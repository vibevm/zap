/** Protected Codex network override mapping. @scope spec://org.vibevm.zap/lens/PROP-010#agent-network */
import assert from "node:assert/strict";
import test from "node:test";
import { ProductLocalSettingsSchema } from "./settings.ts";
import { configuredCodexProfile } from "./providers.ts";

test("Codex defaults inherit the global proxy unless a protected override is configured", () => {
  const inherited = ProductLocalSettingsSchema.parse({
    version: 1,
    proxy: { mode: "explicit", httpsProxy: "http://proxy.example:8080" },
    coordinatorDefaults: { modelId: "gpt-fixture", effort: "low" },
  });
  assert.notEqual(inherited.coordinatorDefaults, undefined);
  if (inherited.coordinatorDefaults === undefined) return;
  assert.deepEqual(
    configuredCodexProfile(
      { ...inherited, coordinatorDefaults: inherited.coordinatorDefaults },
      "C:/fixture/codex.exe",
    ).proxy,
    inherited.proxy,
  );

  const overridden = ProductLocalSettingsSchema.parse({
    version: 1,
    proxy: { mode: "explicit", httpsProxy: "http://proxy.example:8080" },
    coordinatorDefaults: {
      modelId: "gpt-fixture",
      effort: "low",
      proxy: { mode: "direct" },
    },
  });
  assert.notEqual(overridden.coordinatorDefaults, undefined);
  if (overridden.coordinatorDefaults === undefined) return;
  const profile = configuredCodexProfile(
    { ...overridden, coordinatorDefaults: overridden.coordinatorDefaults },
    "C:/fixture/codex.exe",
  );
  assert.deepEqual(profile.proxy, { mode: "direct" });
  assert.equal(profile.model, "gpt-fixture");
  assert.equal(profile.effort, "low");
});
