/** Owner-specified Codex tier bindings and deliberate baseline rules. @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import { ModelPolicySchema, type ModelPolicy } from "./types.ts";

export function createDefaultCodexModelPolicy(input: {
  readonly policyId: string;
  readonly revision: string;
}): ModelPolicy {
  return ModelPolicySchema.parse({
    protocol: "lens-model-policy/1",
    policyId: input.policyId,
    revision: input.revision,
    tierBindings: [
      {
        tier: "ultra",
        profileId: "codex.ultra",
        productId: "codex",
        providerId: "openai",
        modelId: "gpt-6-astra",
      },
      {
        tier: "big",
        profileId: "codex.big",
        productId: "codex",
        providerId: "openai",
        modelId: "gpt-5.6-sol",
      },
      {
        tier: "medium",
        profileId: "codex.medium",
        productId: "codex",
        providerId: "openai",
        modelId: "gpt-5.6-terra",
      },
      {
        tier: "small",
        profileId: "codex.small",
        productId: "codex",
        providerId: "openai",
        modelId: "gpt-5.6-luna",
      },
    ],
    taskRules: [
      {
        ruleId: "codex.test-agent.small-low",
        priority: 200,
        match: { purposes: ["test_agent"] },
        tier: "small",
        effort: { mode: "explicit", value: "low" },
        selectionReason: "Agents invoked to test Lens protocols use the configured small tier.",
      },
      {
        ruleId: "codex.development.big-high",
        priority: 100,
        match: { purposes: ["development_implementation"] },
        tier: "big",
        effort: { mode: "explicit", value: "high" },
        selectionReason:
          "Development implementation uses the configured big tier unless a more specific task rule is configured.",
      },
    ],
  });
}
