import assert from "node:assert/strict";
import test from "node:test";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { materializeExecutionConfiguration } from "./index.ts";

test("verified configuration requires adapter evidence and image tool capability", () => {
  const base = {
    configurationId: "configuration.sol.image",
    displayName: "Codex · Sol · Image workflow",
    connectionId: "connection.codex.primary",
    providerId: "provider.openai",
    productId: "product.codex",
    modelVendorId: "vendor.openai",
    referenceModelId: "gpt-5.6-sol",
    binding: {
      bindingId: "binding.codex.primary",
      hostId: ExecutionHostIdSchema.parse("host.local.test"),
      displayName: "Primary Codex account",
      agentProduct: "codex" as const,
      enabled: true,
      setupGuidance: "Run Codex sign-in with the protected CODEX_HOME.",
    },
    usageBucketIds: ["bucket.codex.primary"],
    now: "2026-09-16T12:00:00.000Z",
  };
  const adapter: Parameters<typeof materializeExecutionConfiguration>[0]["adapter"] = {
    agentProduct: "codex" as const,
    executionModes: ["native", "managed"] as const,
    invocationScopes: ["coordinator", "managed_agent"] as const,
    modalities: ["text", "image_input", "image_output"] as const,
    effort: {
      mode: "configurable",
      allowedValues: ["low", "medium", "high"],
      defaultValue: "medium",
    },
    context: {
      mode: "configurable" as const,
      allowedTokens: [200_000, 1_050_000],
      defaultTokens: 1_050_000,
      documentedMaximumTokens: 1_050_000,
    },
    toolCapabilities: ["image_generation_tool"],
    evidenceSource: "installed Codex adapter fixture",
    observedAt: "2026-09-16T11:59:00.000Z",
  };
  const created = materializeExecutionConfiguration({ ...base, adapter });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  assert.equal(created.value.modelId, "gpt-5.6-sol");
  assert.equal(created.value.modalities.includes("image_output"), true);
  assert.equal(
    created.value.scores.find((score) => score.specialization === "image_generation")?.provenance,
    "owner",
  );
  const withoutTool = materializeExecutionConfiguration({
    ...base,
    adapter: { ...adapter, toolCapabilities: [] },
  });
  assert.equal(withoutTool.ok, false);
});
