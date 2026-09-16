/** Protected Codex network override mapping. @scope spec://org.vibevm.zap/lens/PROP-010#agent-network */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import test from "node:test";
import { ProductLocalSettingsSchema } from "./settings.ts";
import { configuredCodexProfile, resolveClaudeLaunchExecutable } from "./providers.ts";

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
  assert.equal(profile.accountBindingId, "binding.codex.default");
});

test("Windows Claude discovery replaces the npm command shim with its native binary", async () => {
  const temporaryParent = resolve(tmpdir());
  const root = await mkdtemp(join(temporaryParent, "zap-claude-shim-"));
  assert.equal(root.startsWith(`${temporaryParent}${sep}`), true);
  try {
    const shim = join(root, "claude.cmd");
    const binary = join(root, "node_modules", "@anthropic-ai", "claude-code", "bin", "claude.exe");
    await mkdir(resolve(binary, ".."), { recursive: true });
    await Promise.all([writeFile(shim, "@echo off\r\n"), writeFile(binary, "fixture")]);
    assert.equal(await resolveClaudeLaunchExecutable(shim, "win32"), binary);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
