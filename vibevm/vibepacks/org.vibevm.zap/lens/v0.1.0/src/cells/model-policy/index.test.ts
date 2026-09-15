/** Pure model policy resolution tests. @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import assert from "node:assert/strict";
import test from "node:test";
import {
  ModelCapabilityProfileSchema,
  ModelPolicySchema,
  ModelSelectionRequestSchema,
  TrustedModelContextSchema,
  createDefaultCodexModelPolicy,
  preserveRunningModelSelection,
  resolveModelSelection,
  type EffortRequest,
  type ModelCapabilityProfile,
  type ModelObservation,
  type ModelPolicy,
  type ParentModelSelection,
} from "./index.ts";

test("default test-agent rule selects small Luna with explicit low effort", () => {
  const policy = createDefaultCodexModelPolicy({ policyId: "policy.default", revision: "7" });
  const result = resolveModelSelection(
    policy,
    request("selection.test", "test_agent"),
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["low", "medium"]))]),
  );
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.requestedTier, "small");
  assert.equal(result.value.profileId, "codex.small");
  assert.equal(result.value.modelId, "gpt-5.6-luna");
  assert.deepEqual(result.value.requestedEffort, { mode: "explicit", value: "low" });
  assert.deepEqual(result.value.effectiveEffort, { state: "explicit", value: "low" });
  assert.equal(result.value.actualObservation, null);
  assert.equal(result.value.policyRevision, "7");

  const partial = resolveModelSelection(
    policy,
    request("selection.partial-observation", "test_agent"),
    trusted(
      [capability("capability.luna", "gpt-5.6-luna", configurable(["low"]))],
      null,
      defaultAllowedProfiles,
      observation(null),
    ),
  );
  assert.equal(partial.ok, true);
  if (partial.ok) assert.equal(partial.value.observationMatchesSelection, null);
});

test("task-class override selects its configured tier without model-name interpretation", () => {
  const policy = withRule(
    createDefaultCodexModelPolicy({ policyId: "policy.custom", revision: "1" }),
    {
      ruleId: "custom.decision.ultra",
      priority: 500,
      match: { purposes: ["development_implementation"], taskClasses: ["decision"] },
      tier: "ultra",
      effort: { mode: "explicit", value: "xhigh" },
      selectionReason: "Owner mapped architectural decisions to the configured ultra tier.",
    },
  );
  const result = resolveModelSelection(
    policy,
    request("selection.decision", "development_implementation", "decision"),
    trusted([capability("capability.astra", "gpt-6-astra", configurable(["high", "xhigh"]))]),
  );
  assert.equal(result.ok, true);
  if (result.ok) {
    assert.equal(result.value.modelId, "gpt-6-astra");
    assert.equal(result.value.ruleId, "custom.decision.ultra");
    assert.match(result.value.selectionReason, /architectural decisions/);
  }
});

test("unsupported explicit effort refuses instead of falling back to another tier", () => {
  const policy = createDefaultCodexModelPolicy({ policyId: "policy.no-fallback", revision: "1" });
  const result = resolveModelSelection(
    policy,
    request("selection.refuse", "test_agent"),
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["medium"]))]),
  );
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.error.code, "effort_unsupported");
});

test("records an explicit per-task override and enforces the trusted profile allowlist", () => {
  const policy = createDefaultCodexModelPolicy({ policyId: "policy.override", revision: "8" });
  const baseRequest = request("selection.override", "development_implementation", "change");
  const overridden = ModelSelectionRequestSchema.parse({
    ...baseRequest,
    override: {
      overrideRef: "override.user-choice",
      tier: "small",
      effort: { mode: "explicit", value: "low" },
      reason: "Use the bounded low-cost profile for this reversible edit.",
    },
  });
  const allowed = resolveModelSelection(
    policy,
    overridden,
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["low"]))]),
  );
  assert.equal(allowed.ok, true);
  if (allowed.ok) {
    assert.equal(allowed.value.ruleId, "codex.development.big-high");
    assert.equal(allowed.value.overrideRef, "override.user-choice");
    assert.equal(allowed.value.requestedTier, "small");
    assert.match(allowed.value.overrideReason ?? "", /low-cost/);
  }
  const denied = resolveModelSelection(
    policy,
    overridden,
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["low"]))], null, [
      "codex.big",
    ]),
  );
  assert.equal(denied.ok, false);
  if (!denied.ok) assert.equal(denied.error.code, "profile_disallowed");
});

test("inherited effort keeps the configured child model and rejects a child override", () => {
  const base = createDefaultCodexModelPolicy({ policyId: "policy.inherit", revision: "2" });
  const inherited = replaceTestEffort(base, { mode: "inherit" });
  const context = trusted(
    [
      capability(
        "capability.coordinator-configurable",
        "gpt-5.6-luna",
        configurable(["low", "high"]),
        "configurable",
        "coordinator",
      ),
      capability(
        "capability.native-inherited",
        "gpt-5.6-luna",
        { mode: "inherited" },
        "inherited",
        "native_subagent",
      ),
    ],
    {
      selectionRef: "selection.parent",
      profileId: "codex.big",
      modelId: "gpt-5.6-sol",
      effectiveEffort: { state: "explicit", value: "high" },
    },
  );
  const selected = resolveModelSelection(
    inherited,
    request("selection.child", "test_agent"),
    context,
  );
  assert.equal(selected.ok, true);
  if (selected.ok) {
    assert.equal(selected.value.modelId, "gpt-5.6-luna", "effort inheritance never changes model");
    assert.deepEqual(selected.value.effectiveEffort, {
      state: "inherited",
      value: "high",
      sourceSelectionRef: "selection.parent",
    });
    assert.equal(selected.value.extendedThinking, "inherited");
  }

  const coordinatorRequest = ModelSelectionRequestSchema.parse({
    ...request("selection.coordinator", "test_agent"),
    role: "coordinator",
    invocationScope: "coordinator",
  });
  const coordinator = resolveModelSelection(base, coordinatorRequest, context);
  assert.equal(coordinator.ok, true);
  if (coordinator.ok) {
    assert.deepEqual(coordinator.value.effectiveEffort, { state: "explicit", value: "low" });
    assert.equal(coordinator.value.capabilityId, "capability.coordinator-configurable");
  }

  const explicit = replaceTestEffort(base, { mode: "explicit", value: "low" });
  const refused = resolveModelSelection(
    explicit,
    request("selection.child-explicit", "test_agent"),
    context,
  );
  assert.equal(refused.ok, false);
  if (!refused.ok) assert.equal(refused.error.code, "effort_unsupported");
});

test("missing capability refuses an explicit effort and same-priority rules are ambiguous", () => {
  const policy = createDefaultCodexModelPolicy({ policyId: "policy.missing", revision: "1" });
  const missing = resolveModelSelection(
    policy,
    request("selection.missing", "test_agent"),
    trusted([]),
  );
  assert.equal(missing.ok, false);
  if (!missing.ok) assert.equal(missing.error.code, "capability_unknown");

  const unspecified = resolveModelSelection(
    replaceTestEffort(policy, { mode: "unspecified" }),
    request("selection.unknown", "test_agent"),
    trusted([]),
  );
  assert.equal(unspecified.ok, true);
  if (unspecified.ok) {
    assert.equal(unspecified.value.capabilityId, null);
    assert.equal(unspecified.value.effectiveEffort.state, "unknown");
  }

  const ambiguous = withRule(policy, {
    ruleId: "codex.test-agent.also-small",
    priority: 200,
    match: { purposes: ["test_agent"] },
    tier: "small",
    effort: { mode: "explicit", value: "low" },
    selectionReason: "Synthetic ambiguous rule.",
  });
  const result = resolveModelSelection(
    ambiguous,
    request("selection.ambiguous", "test_agent"),
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["low"]))]),
  );
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.error.code, "ambiguous_rule");
});

test("running selection survives later policy changes and observation stays separate", () => {
  const original = createDefaultCodexModelPolicy({ policyId: "policy.original", revision: "3" });
  const resolved = resolveModelSelection(
    original,
    request("selection.running", "test_agent"),
    trusted([capability("capability.luna", "gpt-5.6-luna", configurable(["low"]))]),
  );
  assert.equal(resolved.ok, true);
  if (!resolved.ok) return;
  const next = createDefaultCodexModelPolicy({ policyId: "policy.next", revision: "4" });
  const observation = {
    source: "trusted_codex_adapter",
    observedAt: "2026-09-15T12:00:00.000Z",
    profileId: "codex.small",
    productId: "codex",
    productVersion: "0.152.1",
    executionMode: "native" as const,
    invocationScope: "native_subagent" as const,
    modelId: "gpt-5.6-luna",
    effort: "low" as const,
  };
  const preserved = preserveRunningModelSelection(resolved.value, next, observation);
  assert.equal(preserved.ok, true);
  if (preserved.ok) {
    assert.equal(preserved.value.selection.policyId, "policy.original");
    assert.equal(preserved.value.selection.policyRevision, "3");
    assert.equal(preserved.value.ignoredPolicy.policyId, "policy.next");
    assert.deepEqual(preserved.value.actualObservation, observation);
  }
});

function request(
  selectionRef: string,
  purpose: "development_implementation" | "test_agent",
  taskClass: "evidence" | "decision" | "change" | "verification" | "integration" = "verification",
) {
  return ModelSelectionRequestSchema.parse({
    selectionRef,
    purpose,
    taskClass,
    role: "worker",
    executionMode: "native",
    invocationScope: "native_subagent",
    productId: "codex",
    productVersion: "0.152.1",
    override: null,
  });
}

function capability(
  capabilityId: string,
  modelId: string,
  effort: ModelCapabilityProfile["effort"],
  extendedThinking: ModelCapabilityProfile["extendedThinking"] = "unknown",
  invocationScope: ModelCapabilityProfile["invocationScope"] = "native_subagent",
) {
  return ModelCapabilityProfileSchema.parse({
    capabilityId,
    productId: "codex",
    productVersion: "0.152.1",
    executionMode: "native",
    invocationScope,
    modelId,
    effort,
    extendedThinking,
    evidence: {
      source: "synthetic documented capability fixture",
      observedAt: "2026-09-15T12:00:00.000Z",
    },
  });
}

function configurable(
  allowedValues: Array<"none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra">,
): ModelCapabilityProfile["effort"] {
  return { mode: "configurable", allowedValues, defaultValue: null };
}

function trusted(
  capabilities: ModelCapabilityProfile[],
  parentSelection: ParentModelSelection | null = null,
  allowedProfileIds = defaultAllowedProfiles,
  actualObservation: ModelObservation | null = null,
) {
  return TrustedModelContextSchema.parse({
    capabilities,
    allowedProfileIds,
    parentSelection,
    actualObservation,
  });
}

const defaultAllowedProfiles = ["codex.ultra", "codex.big", "codex.medium", "codex.small"];

function observation(effort: ModelObservation["effort"]): ModelObservation {
  return {
    source: "synthetic trusted adapter observation",
    observedAt: "2026-09-15T12:00:00.000Z",
    profileId: "codex.small",
    productId: "codex",
    productVersion: "0.152.1",
    executionMode: "native",
    invocationScope: "native_subagent",
    modelId: "gpt-5.6-luna",
    effort,
  };
}

function withRule(policy: ModelPolicy, rule: ModelPolicy["taskRules"][number]): ModelPolicy {
  return ModelPolicySchema.parse({ ...policy, taskRules: [...policy.taskRules, rule] });
}

function replaceTestEffort(policy: ModelPolicy, effort: EffortRequest): ModelPolicy {
  return ModelPolicySchema.parse({
    ...policy,
    taskRules: policy.taskRules.map((rule) =>
      rule.ruleId === "codex.test-agent.small-low" ? { ...rule, effort } : rule,
    ),
  });
}
