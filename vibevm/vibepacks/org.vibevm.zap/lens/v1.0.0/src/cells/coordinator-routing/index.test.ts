import assert from "node:assert/strict";
import test from "node:test";
import { ActorIdSchema, PrincipalIdSchema, ClientRequestIdSchema } from "../protocol/index.ts";
import { ClientIdSchema, ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import { ModelCapabilityProfileSchema, TrustedModelContextSchema } from "../model-policy/index.ts";
import { openModelPolicyStore, type ModelPolicyStoreAccess } from "../model-policy-store/index.ts";
import {
  CoordinatorRoutingRequestSchema,
  CoordinatorRoutingConfigSchema,
  createConfiguredCoordinatorRoutingProvider,
  initializeConfiguredCoordinatorPolicies,
  launchWithRoutedParameters,
  resolveCoordinatorLaunch,
  type CoordinatorLaunchProfile,
  type CoordinatorRoutingProvider,
  type CoordinatorRoutingRequest,
} from "./index.ts";

test("policy-enabled coordinator routing resolves test-agent small/low, pins, and passes actual launch parameters", async () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T14:00:00.000Z"),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.routing");
  const contextId = WorkContextIdSchema.parse("context.routing");
  const access = accessFor(projectId);
  assert.equal(
    opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.routing.policy"),
      sourceEventId: "routing.policy.1",
      policyId: "policy.routing",
    }).ok,
    true,
  );
  const provider = providerWith({
    mode: "configurable",
    allowedValues: ["low"],
    defaultValue: "low",
  });
  const request = routingRequest(
    projectId,
    contextId,
    "request.routing.launch",
    "routing.launch.1",
  );
  const routed = await resolveCoordinatorLaunch(access, request, { store: opened.value, provider });
  assert.equal(routed.ok, true);
  if (!routed.ok) return;
  assert.equal(routed.value.pinned, true);
  assert.equal(routed.value.parameters.profileId, "codex.small");
  assert.equal(routed.value.parameters.modelId, "gpt-5.6-luna");
  assert.equal(routed.value.parameters.effectiveEffort, "low");
  let received = "";
  const launched = await launchWithRoutedParameters(
    {
      launch: async (parameters) => {
        received = `${parameters.profileId}/${parameters.modelId}/${parameters.effectiveEffort}`;
        return { ok: true, value: null };
      },
    },
    routed.value,
  );
  assert.equal(launched.ok, true);
  assert.equal(received, "codex.small/gpt-5.6-luna/low");
  const pinned = opened.value.readSelection(
    access,
    projectId,
    contextId,
    request.runId,
    request.attemptId,
  );
  assert.equal(pinned.ok, true);
  if (pinned.ok) assert.equal(pinned.value.selection.policyRevision, "1");
  opened.value.close();
});

test("enabled policy refuses missing or unsupported configuration without fallback; explicit mode remains truthful", async () => {
  const opened = openModelPolicyStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.routing-refusal");
  const contextId = WorkContextIdSchema.parse("context.routing-refusal");
  const access = accessFor(projectId);
  const request = routingRequest(
    projectId,
    contextId,
    "request.routing.missing",
    "routing.missing.1",
  );
  const provider = providerWith({
    mode: "configurable",
    allowedValues: ["low"],
    defaultValue: "low",
  });
  const missing = await resolveCoordinatorLaunch(access, request, {
    store: opened.value,
    provider,
  });
  assert.equal(missing.ok, false);
  assert.equal(missing.ok ? "" : missing.error.code, "not_found");
  assert.equal(
    opened.value.initializeDefaultPolicy(access, {
      projectId,
      contextId,
      clientRequestId: ClientRequestIdSchema.parse("request.routing.policy2"),
      sourceEventId: "routing.policy.2",
      policyId: "policy.routing-refusal",
    }).ok,
    true,
  );
  const unsupported = await resolveCoordinatorLaunch(access, request, {
    store: opened.value,
    provider: providerWith({ mode: "unsupported" }),
  });
  assert.equal(unsupported.ok, false);
  assert.equal(unsupported.ok ? "" : unsupported.error.code, "policy_refused");
  const explicit = await resolveCoordinatorLaunch(
    access,
    { ...request, policyEnabled: false, explicitProfileId: "profile.explicit" },
    { store: opened.value, provider: providerWith({ mode: "unsupported" }) },
  );
  assert.equal(explicit.ok, true);
  if (explicit.ok) {
    assert.equal(explicit.value.pinned, false);
    assert.equal(explicit.value.parameters.profileId, "profile.explicit");
    assert.equal(explicit.value.parameters.effectiveEffort, null);
  }
  opened.value.close();
});

test("trusted runtime config initializes policy and scopes profile capabilities", async () => {
  const opened = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T14:00:00.000Z"),
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const projectId = ProjectIdSchema.parse("project.configured");
  const contextId = WorkContextIdSchema.parse("context.configured");
  const access = accessFor(projectId);
  const config = CoordinatorRoutingConfigSchema.parse({
    profiles: [
      {
        scope: { projectId, contextId },
        profile: {
          profileId: "codex.small",
          productId: "codex",
          productVersion: "1.0",
          providerId: "openai",
          modelId: "gpt-5.6-luna",
          effort: "low",
        },
      },
    ],
    capabilities: [
      {
        profileId: "codex.small",
        capability: {
          capabilityId: "cap.configured",
          productId: "codex",
          productVersion: "1.0",
          executionMode: "native",
          invocationScope: "coordinator",
          modelId: "gpt-5.6-luna",
          effort: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
          extendedThinking: "configurable",
          evidence: { source: "configured-test", observedAt: "2026-09-15T14:00:00.000Z" },
        },
      },
    ],
    policies: [
      {
        scope: { projectId, contextId },
        policyId: "policy.configured",
        clientRequestId: ClientRequestIdSchema.parse("request.configured.policy"),
        sourceEventId: "configured.policy.1",
      },
    ],
  });
  assert.equal(initializeConfiguredCoordinatorPolicies(opened.value, access, config).ok, true);
  const routed = await resolveCoordinatorLaunch(
    access,
    routingRequest(projectId, contextId, "request.configured.route", "configured.route.1"),
    { store: opened.value, provider: createConfiguredCoordinatorRoutingProvider(config) },
  );
  assert.equal(routed.ok, true);
  if (routed.ok) assert.equal(routed.value.parameters.modelId, "gpt-5.6-luna");
  opened.value.close();
});

function routingRequest(
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
  clientRequestId: string,
  sourceEventId: string,
): CoordinatorRoutingRequest {
  return CoordinatorRoutingRequestSchema.parse({
    projectId,
    contextId,
    sessionId: "session.routing",
    runId: "run.routing",
    attemptId: "attempt.routing",
    clientRequestId,
    sourceEventId,
    policyEnabled: true,
    purpose: "test_agent",
    taskClass: "verification",
    role: "coordinator",
    executionMode: "native",
    invocationScope: "coordinator",
    productId: "codex",
    productVersion: "1.0",
    selectionRef: `selection.${clientRequestId}`,
    override: null,
    explicitProfileId: null,
  });
}

function providerWith(effort: unknown): CoordinatorRoutingProvider {
  return {
    trustedContext: () => ({
      ok: true,
      value: TrustedModelContextSchema.parse({
        allowedProfileIds: ["codex.small"],
        parentSelection: null,
        actualObservation: null,
        capabilities: [
          ModelCapabilityProfileSchema.parse({
            capabilityId: "cap.routing",
            productId: "codex",
            productVersion: "1.0",
            executionMode: "native",
            invocationScope: "coordinator",
            modelId: "gpt-5.6-luna",
            effort,
            extendedThinking: "configurable",
            evidence: { source: "fake", observedAt: "2026-09-15T14:00:00.000Z" },
          }),
        ],
      }),
    }),
    explicitProfile: () => ({
      ok: true,
      value: {
        profileId: "profile.explicit",
        productId: "codex",
        productVersion: "1.0",
        providerId: "openai",
        modelId: "gpt-5.6-terra",
        effort: null,
      } satisfies CoordinatorLaunchProfile,
    }),
  };
}

function accessFor(projectId: ReturnType<typeof ProjectIdSchema.parse>): ModelPolicyStoreAccess {
  return {
    principalId: PrincipalIdSchema.parse("principal.routing"),
    actorId: ActorIdSchema.parse("actor.routing"),
    clientId: ClientIdSchema.parse("client.routing"),
    authorizedProjectIds: [projectId],
  };
}
