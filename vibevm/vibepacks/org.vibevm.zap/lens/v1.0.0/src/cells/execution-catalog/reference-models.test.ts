import assert from "node:assert/strict";
import test from "node:test";
import { EXECUTION_MODEL_REFERENCE } from "./index.ts";

test("model reference covers ten families and keeps image tools separate from controllers", () => {
  const families = new Set(
    EXECUTION_MODEL_REFERENCE.filter((entry) => entry.status !== "synthetic").map(
      (entry) => entry.familyId,
    ),
  );
  assert.equal(families.size, 10);
  assert.equal(
    EXECUTION_MODEL_REFERENCE.filter(
      (entry) => entry.conversationModel && entry.status !== "synthetic",
    ).every((entry) => entry.presets.length >= 10),
    true,
  );
  assert.equal(
    EXECUTION_MODEL_REFERENCE.some((entry) => entry.modelId === "claude-haiku-4-5-20251001"),
    true,
  );
  const sol = EXECUTION_MODEL_REFERENCE.find((entry) => entry.modelId === "gpt-5.6-sol");
  const imagePreset = sol?.presets.find((preset) => preset.specialization === "image_generation");
  assert.equal(imagePreset?.provenance, "owner");
  assert.equal(sol?.toolCapabilities.includes("image_generation_tool"), true);
  const imageModels = EXECUTION_MODEL_REFERENCE.filter((entry) =>
    entry.modelId.startsWith("gpt-image-"),
  );
  assert.equal(imageModels.length, 2);
  assert.equal(
    imageModels.every((entry) => !entry.conversationModel),
    true,
  );
  assert.equal(
    EXECUTION_MODEL_REFERENCE.every((entry) =>
      entry.sourceUrls.every((url) => url.startsWith("https://") || url.startsWith("spec://")),
    ),
    true,
  );
});
