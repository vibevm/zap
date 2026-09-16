import assert from "node:assert/strict";
import test from "node:test";
import { ModelSelectionSchema } from "../model-policy/index.ts";
import { createManagedProviderDrivers, ManagedAgentProfileSchema } from "./providers.ts";

test("managed provider launch uses the pinned selection rather than mutable profile defaults", () => {
  const profile = ManagedAgentProfileSchema.parse({
    profileId: "profile.codex.fixture",
    projectId: "project.fixture",
    contextId: "context.fixture",
    provider: "codex",
    executablePath: "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
    cwd: "C:/Windows",
    modelId: "profile-model-must-not-win",
    effort: "high",
    environmentRef: null,
    mcpConfigPath: "C:/temp/wayfinder-mcp.json",
    capabilities: {
      provider: "codex",
      observedVersion: "fixture",
      installed: true,
      launchable: true,
      authenticated: "not_observed",
      structuredConversation: "unsupported",
      nativeChildren: "unsupported",
      nativeQuestions: "unsupported",
      nativeApprovals: "unsupported",
      interrupt: "supported",
      resume: "unsupported",
      interactiveTerminal: "supported",
      evidence: ["synthetic scripted PTY fixture"],
    },
    proxy: { mode: "direct" },
  });
  const selection = ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: "selection.fixture",
    policyId: "policy.fixture",
    policyRevision: "1",
    ruleId: "rule.fixture",
    overrideRef: null,
    selectionReason: "pinned test selection",
    overrideReason: null,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: "small",
    requestedEffort: { mode: "explicit", value: "low" },
    profileId: "profile.codex.fixture",
    productId: "codex",
    productVersion: "fixture",
    providerId: "codex",
    modelId: "selection-model-wins",
    capabilityId: null,
    effortCapability: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
    extendedThinking: "unsupported",
    effectiveEffort: { state: "explicit", value: "low" },
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
  const driver = createManagedProviderDrivers().get("codex");
  assert.ok(driver);
  const launch = driver.launch({
    profile,
    selection,
    instructions: "fixture",
    environment: { SYNTHETIC_MANAGED_ENV: "preserved", HTTPS_PROXY: "http://ambient.invalid" },
    trustedZapMcp: false,
  });
  assert.equal(launch.args.includes("selection-model-wins"), true);
  assert.equal(launch.args.includes("profile-model-must-not-win"), false);
  assert.equal(launch.args.includes('model_reasoning_effort="low"'), true);
  assert.equal(launch.env["SYNTHETIC_MANAGED_ENV"], "preserved");
  assert.equal(launch.env["HTTPS_PROXY"], undefined);
  assert.equal(
    Object.keys(launch.env).some((key) => key.toUpperCase() === "PATH"),
    true,
  );
  const qwenProfile = ManagedAgentProfileSchema.parse({
    ...profile,
    profileId: "profile.qwen.fixture",
    provider: "qwen_code",
    modelId: "qwen-profile-model-must-not-win",
    effort: null,
    effortSupported: false,
    argumentPrefix: ["C:/fixture/qwen-cli-entry.js"],
    proxy: { mode: "explicit", httpsProxy: "http://proxy.fixture:8080" },
    capabilities: {
      ...profile.capabilities,
      provider: "qwen_code",
      structuredConversation: "supported",
      evidence: ["installed Qwen stream-JSON CLI fixture"],
    },
  });
  const qwenSelection = ModelSelectionSchema.parse({
    ...selection,
    profileId: qwenProfile.profileId,
    productId: "qwen_code",
    providerId: "qwen_code",
    modelId: "qwen-selected-model",
    requestedEffort: { mode: "unspecified" },
    effortCapability: { mode: "unsupported" },
    effectiveEffort: { state: "unknown", reason: "provider effort is unsupported" },
  });
  const qwen = createManagedProviderDrivers().get("qwen_code");
  assert.ok(qwen);
  const qwenLaunch = qwen.launch({
    profile: qwenProfile,
    selection: qwenSelection,
    instructions: "fixture",
    environment: {},
    trustedZapMcp: false,
  });
  assert.equal(qwenLaunch.args.includes("qwen-selected-model"), true);
  assert.equal(qwenLaunch.args.includes("qwen-profile-model-must-not-win"), false);
  assert.equal(qwenLaunch.args[0], "C:/fixture/qwen-cli-entry.js");
  assert.deepEqual(
    qwenLaunch.args.slice(
      qwenLaunch.args.indexOf("--proxy"),
      qwenLaunch.args.indexOf("--proxy") + 2,
    ),
    ["--proxy", "http://proxy.fixture:8080"],
  );
  assert.deepEqual(
    qwenLaunch.args.slice(
      qwenLaunch.args.indexOf("--approval-mode"),
      qwenLaunch.args.indexOf("--approval-mode") + 2,
    ),
    ["--approval-mode", "default"],
  );
  assert.equal(qwenLaunch.args.includes("--allowed-tools"), false);
  const trustedQwenLaunch = qwen.launch({
    profile: qwenProfile,
    selection: qwenSelection,
    instructions: "fixture",
    environment: {},
    trustedZapMcp: true,
  });
  assert.equal(
    trustedQwenLaunch.args.includes("mcp__zap-wayfinder__codlens_assigned_context"),
    true,
  );
  assert.equal(trustedQwenLaunch.args.includes("mcp__zap-wayfinder__codlens_inbox_wait"), true);
  assert.equal(
    trustedQwenLaunch.args.includes("mcp__zap-wayfinder__codlens_managed_work_start"),
    true,
  );
  assert.equal(
    trustedQwenLaunch.args.includes("mcp__zap-wayfinder__codlens_native_work_before"),
    true,
  );
  assert.equal(trustedQwenLaunch.args.includes("mcp__zap-wayfinder__codlens_plan_apply"), false);
});
