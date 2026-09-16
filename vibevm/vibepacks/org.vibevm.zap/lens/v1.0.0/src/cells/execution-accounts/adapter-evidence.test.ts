import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { CodexCoordinatorProfileSchema } from "../codex-coordinator/index.ts";
import { EXECUTION_MODEL_REFERENCE } from "../execution-catalog/index.ts";
import { codexAdapterEvidence, providerCoordinatorAdapterEvidence } from "./index.ts";
import { ProviderCoordinatorProfileSchema } from "../provider-coordinators/index.ts";

test("adapter evidence exposes only installed profile choices and verified tools", () => {
  const reference = EXECUTION_MODEL_REFERENCE.find((entry) => entry.modelId === "gpt-5.6-sol");
  assert.notEqual(reference, undefined);
  if (reference === undefined) return;
  const profile = CodexCoordinatorProfileSchema.parse({
    profileId: "profile.codex.evidence",
    executablePath: resolve("codex.exe"),
    requestTimeoutMs: 5_000,
    model: reference.modelId,
    effort: "low",
    contextWindowTokens: 200_000,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
  });
  const plain = codexAdapterEvidence(profile, reference, "2026-09-16T12:00:00.000Z");
  assert.deepEqual(plain.effort, {
    mode: "configurable",
    allowedValues: ["low"],
    defaultValue: "low",
  });
  assert.deepEqual(plain.modalities, ["text"]);
  assert.deepEqual(plain.context, {
    mode: "configurable",
    allowedTokens: [128_000, 200_000, 272_000, 512_000, 1_050_000],
    defaultTokens: 200_000,
    documentedMaximumTokens: 1_050_000,
  });
  const image = codexAdapterEvidence(profile, reference, "2026-09-16T12:00:00.000Z", [
    "image_generation_tool",
  ]);
  assert.deepEqual(image.modalities, ["text", "image_output"]);
  const observedProfile = CodexCoordinatorProfileSchema.parse({
    ...profile,
    observedModelCapabilities: [
      {
        modelId: "gpt-5.6-sol",
        displayName: "GPT-5.6-Sol",
        supportedEfforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
        defaultEffort: "low",
      },
    ],
    capabilityObservedAt: "2026-09-16T12:00:00.000Z",
  });
  assert.deepEqual(
    codexAdapterEvidence(observedProfile, reference, "2026-09-16T12:00:00.000Z").effort,
    {
      mode: "configurable",
      allowedValues: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultValue: "low",
    },
  );
  const qwen = ProviderCoordinatorProfileSchema.parse({
    profileId: "profile.qwen.evidence",
    provider: "qwen_code",
    executablePath: resolve("qwen.exe"),
    cwd: resolve("."),
    modelId: "qwen3.8-max",
    effort: null,
    endpoint: null,
  });
  const qwenReference = EXECUTION_MODEL_REFERENCE.find((entry) => entry.modelId === "qwen3.8-max");
  assert.notEqual(qwenReference, undefined);
  if (qwenReference === undefined) return;
  assert.deepEqual(
    providerCoordinatorAdapterEvidence(qwen, qwenReference, "2026-09-16T12:00:00.000Z").effort,
    { mode: "unsupported" },
  );
});
