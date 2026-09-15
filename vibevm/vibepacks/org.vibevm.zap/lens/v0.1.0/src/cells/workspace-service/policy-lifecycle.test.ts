/**
 * End-to-end model-policy propagation through Wayfinder and the real Codex adapter.
 * @scope spec://org.vibevm.zap/lens/PROP-008#verification
 * @scope spec://org.vibevm.zap/lens/PROP-009#verification
 */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { z } from "zod";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { CoordinatorStartInputSchema } from "../agent-runtime/index.ts";
import {
  createConfiguredCoordinatorRoutingProvider,
  createWorkspaceCoordinatorRoutingBridge,
  initializeConfiguredCoordinatorPolicies,
  type CoordinatorRoutingConfig,
} from "../coordinator-routing/index.ts";
import { openModelPolicyStore, type ModelPolicyStore } from "../model-policy-store/index.ts";
import {
  AttemptIdSchema,
  RunIdSchema,
  WorkspaceCommandRequestSchema,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { createCoordinatorAdapterRegistry, createWorkspaceService } from "./index.ts";
import {
  RecordingFactory,
  RecordingProcess,
  access,
  codexAdapter,
  contextId,
  host,
  policyCwd,
  projectId,
  registration,
  sessionId,
} from "./policy-lifecycle.support.ts";

test("policy-selected Codex model and effort survive stop, policy drift, recreation, and Continue", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "lens-policy-life-"));
  const workspacePath = join(directory, "workspace.sqlite");
  const policyPath = join(directory, "model-policy.sqlite");
  const workspace = openWorkspaceStore({
    databasePath: workspacePath,
    clock: () => new Date("2026-09-15T12:00:00.000Z"),
  });
  const policies = openModelPolicyStore({
    databasePath: policyPath,
    clock: () => new Date("2026-09-15T12:00:00.000Z"),
  });
  assert.equal(workspace.ok, true);
  assert.equal(policies.ok, true);
  if (!workspace.ok || !policies.ok) return;
  const workspaceStore = workspace.value;
  const policyStore = policies.value;
  assert.equal(workspaceStore.registerProject(registration()).ok, true);
  const routingAccess = access("client.policy-initial");
  const initialConfig = routingConfig("gpt-5.6-sol", "medium", true);
  assert.equal(initialConfig.profiles.length, 2);
  assert.equal(
    initializeConfiguredCoordinatorPolicies(policyStore, routingAccess, initialConfig).ok,
    true,
  );

  const firstProcess = new RecordingProcess("process-policy-1", "gpt-5.6-sol");
  const firstFactory = new RecordingFactory([firstProcess]);
  const firstAdapter = codexAdapter(firstFactory, {
    model: "gpt-profile-default",
    effort: "low",
  });
  const firstOpenedProfiles: string[] = [];
  const firstHost = host(firstAdapter, firstOpenedProfiles);
  const firstService = createWorkspaceService({
    store: workspaceStore,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "legacy.protected", host: firstHost },
      { profileRef: "codex.big", host: firstHost },
    ]),
    coordinatorRouting: routingBridge(policyStore, initialConfig),
    idFactory: sequencedIds(),
  });
  const firstClient = firstService.bind({
    access: routingAccess,
    allowedActions: ["read", "chat.post.v1", "session.start.v1", "project.stop.v1"],
  });
  const started = await firstClient.command(startRequest("request.policy.start"));
  assert.equal(started.ok, true);
  assert.deepEqual(firstOpenedProfiles, ["codex.big"]);
  const pinned = policyStore.readSelection(
    routingAccess,
    projectId,
    contextId,
    RunIdSchema.parse(sessionId),
    AttemptIdSchema.parse(`${sessionId}.attempt`),
  );
  assert.equal(pinned.ok, true);
  if (pinned.ok) {
    assert.equal(pinned.value.selection.policyRevision, "1");
    assert.equal(pinned.value.selection.profileId, "codex.big");
    assert.equal(pinned.value.selection.modelId, "gpt-5.6-sol");
    assert.deepEqual(pinned.value.selection.effectiveEffort, {
      state: "explicit",
      value: "high",
    });
  }
  assert.match(firstFactory.profiles[0]?.executablePath ?? "", /selected-codex\.exe$/);
  assert.deepEqual(requestParams(firstProcess, "thread/start"), {
    model: "gpt-5.6-sol",
    config: { model_reasoning_effort: "high" },
    allowProviderModelFallback: false,
    cwd: policyCwd,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "policy-selected",
    ephemeral: false,
    historyMode: "legacy",
  });
  const bootstrap = requests(firstProcess, "turn/start");
  assert.equal(bootstrap.length, 1);
  const bootstrapParams = z
    .object({
      threadId: z.string(),
      input: z.array(z.object({ type: z.literal("text"), text: z.string() }).passthrough()),
      clientUserMessageId: z.string(),
      effort: z.string(),
    })
    .passthrough()
    .parse(bootstrap[0]?.params);
  assert.equal(bootstrapParams.threadId, "thread-policy");
  assert.equal(bootstrapParams.clientUserMessageId, `bootstrap:${sessionId}`);
  assert.equal(bootstrapParams.effort, "high");
  assert.equal(bootstrapParams.input.length, 1);
  assert.match(bootstrapParams.input[0]?.text ?? "", /long-lived project coordinator/);
  assert.match(bootstrapParams.input[0]?.text ?? "", /Read AGENTS\.md/);
  assert.match(bootstrapParams.input[0]?.text ?? "", /workspace\.policy-lifecycle/);

  firstProcess.emitTurnCompleted("turn-bootstrap");
  const firstChat = await firstClient.command(chatRequest("request.policy.chat", "First goal"));
  assert.equal(firstChat.ok, true);
  await nextDispatch();
  const initialTurns = requests(firstProcess, "turn/start");
  assert.equal(initialTurns.length, 2);
  const initialMessageId = parameterString(initialTurns[1]?.params, "clientUserMessageId");
  assert.match(initialMessageId, /^message\./);
  assert.deepEqual(initialTurns[1]?.params, {
    threadId: "thread-policy",
    input: [{ type: "text", text: "First goal" }],
    clientUserMessageId: initialMessageId,
    effort: "high",
  });
  firstProcess.emitTurnCompleted("turn-chat-2");

  const beforeStop = await execution(firstClient);
  const stopped = await firstClient.command(
    lifecycleRequest("project.stop.v1", "request.policy.stop", beforeStop.revision),
  );
  assert.equal(stopped.ok, true);
  assert.equal(firstProcess.terminateCalls, 1);
  const stoppedExecution = await execution(firstClient);
  assert.equal(stoppedExecution.state, "stopped");

  const current = policyStore.readPolicy(routingAccess, projectId, contextId);
  assert.equal(current.ok, true);
  if (!current.ok) return;
  const update = WorkspaceCommandRequestSchema.parse({
    operation: "model-policy.update.v1",
    clientRequestId: "request.policy.update",
    sourceEventId: "policy.update.after-stop",
    projectId,
    contextId,
    expectedRevision: current.value.policy.revision,
    policy: {
      ...current.value.policy,
      revision: "2",
      tierBindings: current.value.policy.tierBindings.map((binding) =>
        binding.tier === "big" ? { ...binding, modelId: "gpt-policy-changed" } : binding,
      ),
      taskRules: current.value.policy.taskRules.map((rule) =>
        rule.ruleId === "codex.development.big-high"
          ? { ...rule, effort: { mode: "explicit", value: "medium" } }
          : rule,
      ),
    },
  });
  if (update.operation !== "model-policy.update.v1") {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle: policy update narrowed to the wrong operation; fix surface: parse the exact model-policy update command",
    );
  }
  const { operation: ignoredOperation, ...updateInput } = update;
  assert.equal(ignoredOperation, "model-policy.update.v1");
  const updated = policyStore.updatePolicy(routingAccess, updateInput);
  assert.equal(updated.ok, true);
  if (updated.ok) {
    assert.equal(updated.value.policy.revision, "2");
    assert.equal(
      updated.value.policy.tierBindings.find((binding) => binding.tier === "big")?.modelId,
      "gpt-policy-changed",
    );
    assert.deepEqual(
      updated.value.policy.taskRules.find((rule) => rule.ruleId === "codex.development.big-high")
        ?.effort,
      { mode: "explicit", value: "medium" },
    );
  }
  firstService.close();

  const secondProcess = new RecordingProcess("process-policy-2", "gpt-5.6-sol");
  const secondFactory = new RecordingFactory([secondProcess]);
  const secondAdapter = codexAdapter(secondFactory, {
    model: "gpt-profile-changed",
    effort: "medium",
  });
  const secondOpenedProfiles: string[] = [];
  const secondHost = host(secondAdapter, secondOpenedProfiles);
  const changedConfig = routingConfig("gpt-policy-changed", "medium", false);
  const secondService = createWorkspaceService({
    store: workspaceStore,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "legacy.protected", host: secondHost },
      { profileRef: "codex.big", host: secondHost },
    ]),
    coordinatorRouting: routingBridge(policyStore, changedConfig),
  });
  const secondClient = secondService.bind({
    access: access("client.policy-restored"),
    allowedActions: ["read", "chat.post.v1", "project.continue.v1"],
  });
  const continued = await secondClient.command(
    lifecycleRequest("project.continue.v1", "request.policy.continue", stoppedExecution.revision),
  );
  assert.equal(continued.ok, true);
  assert.deepEqual(secondOpenedProfiles, ["codex.big"]);
  assert.match(secondFactory.profiles[0]?.executablePath ?? "", /selected-codex\.exe$/);
  assert.equal(requests(secondProcess, "thread/start").length, 0);
  assert.deepEqual(requestParams(secondProcess, "thread/resume"), {
    threadId: "thread-policy",
    cwd: policyCwd,
    model: "gpt-5.6-sol",
    config: { model_reasoning_effort: "high" },
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
  });
  assert.equal(requests(secondProcess, "turn/start").length, 0, "Continue must not reboot");

  const resumedChat = await secondClient.command(
    chatRequest("request.policy.after-continue", "After continue"),
  );
  assert.equal(resumedChat.ok, true);
  await nextDispatch();
  const resumedTurn = requests(secondProcess, "turn/start")[0];
  const resumedMessageId = parameterString(resumedTurn?.params, "clientUserMessageId");
  assert.match(resumedMessageId, /^message\./);
  assert.deepEqual(resumedTurn?.params, {
    threadId: "thread-policy",
    input: [{ type: "text", text: "After continue" }],
    clientUserMessageId: resumedMessageId,
    effort: "high",
  });

  secondService.close();
  workspaceStore.close();
  policyStore.close();
  rmSync(directory, { recursive: true, force: true });
});

test("explicit Codex model mismatch refuses before bootstrap", async () => {
  const process = new RecordingProcess("process-model-mismatch", "gpt-substituted");
  const factory = new RecordingFactory([process]);
  const adapter = codexAdapter(factory, { model: "gpt-profile-default", effort: "low" });
  const started = await adapter.start(
    CoordinatorStartInputSchema.parse({
      coordinatorSessionId: "session.model-mismatch",
      projectId,
      contextId,
      conversationId: "conversation.model-mismatch",
      coordinatorActorId: "actor.model-mismatch",
      hostId: "host.policy-lifecycle",
      profileId: "codex.big",
      cwd: policyCwd,
      bootstrapText: "MODEL MISMATCH BOOTSTRAP MUST NOT RUN",
      bootstrapBasis: "policy-lifecycle.mismatch",
      modelId: "gpt-requested",
      reasoningEffort: "high",
    }),
  );
  assert.equal(started.ok, false);
  if (!started.ok) assert.equal(started.error.code, "host_refused");
  assert.equal(requests(process, "turn/start").length, 0);
  assert.equal(
    parameterBoolean(requestParams(process, "thread/start"), "allowProviderModelFallback"),
    false,
  );
  adapter.close();
});

function routingConfig(
  selectedModel: string,
  defaultEffort: "medium",
  initialize: boolean,
): CoordinatorRoutingConfig {
  return {
    profiles: [
      {
        scope: { projectId, contextId },
        profile: {
          profileId: "legacy.protected",
          productId: "codex",
          productVersion: "1.0",
          providerId: "openai",
          modelId: "gpt-legacy-default",
          effort: "low",
        },
      },
      {
        scope: { projectId, contextId },
        profile: {
          profileId: "codex.big",
          productId: "codex",
          productVersion: "1.0",
          providerId: "openai",
          modelId: selectedModel,
          effort: defaultEffort,
        },
      },
    ],
    capabilities: [
      capability("capability.legacy", "gpt-legacy-default", "legacy.protected", "low"),
      capability("capability.selected", selectedModel, "codex.big", defaultEffort),
    ],
    policies: initialize
      ? [
          {
            scope: { projectId, contextId },
            policyId: "policy.policy-lifecycle",
            clientRequestId: ClientRequestIdSchema.parse("request.policy.initialize"),
            sourceEventId: "policy.initialize",
          },
        ]
      : [],
  };
}

function capability(
  capabilityId: string,
  modelId: string,
  profileId: string,
  defaultValue: "low" | "medium",
): CoordinatorRoutingConfig["capabilities"][number] {
  return {
    profileId,
    capability: {
      capabilityId,
      productId: "codex",
      productVersion: "1.0",
      executionMode: "native",
      invocationScope: "coordinator",
      modelId,
      effort: { mode: "configurable", allowedValues: ["low", "medium", "high"], defaultValue },
      extendedThinking: "configurable",
      evidence: { source: "policy lifecycle fixture", observedAt: "2026-09-15T12:00:00.000Z" },
    },
  };
}

function routingBridge(store: ModelPolicyStore, config: CoordinatorRoutingConfig) {
  return createWorkspaceCoordinatorRoutingBridge({
    store,
    provider: createConfiguredCoordinatorRoutingProvider(config),
    defaults: {
      policyEnabled: true,
      purpose: "development_implementation",
      taskClass: "verification",
      productId: "codex",
      productVersion: "1.0",
    },
  });
}

function startRequest(clientRequestId: string) {
  return WorkspaceCommandRequestSchema.parse({
    operation: "session.start.v1",
    clientRequestId,
    projectId,
    contextId,
    interactionKind: "structured",
    profileId: "profile.codex-default",
  });
}

function chatRequest(clientRequestId: string, bodyMarkdown: string) {
  return WorkspaceCommandRequestSchema.parse({
    operation: "chat.post.v1",
    clientRequestId,
    projectId,
    contextId,
    conversationId: "conversation.policy-lifecycle",
    bodyMarkdown,
    artifactRefs: [],
    correlationId: null,
    causationMessageId: null,
  });
}

function lifecycleRequest(
  operation: "project.stop.v1" | "project.continue.v1",
  clientRequestId: string,
  expectedRevision: string,
) {
  return WorkspaceCommandRequestSchema.parse({
    operation,
    clientRequestId,
    projectId,
    contextId,
    sessionId,
    expectedRevision,
    reasonMarkdown: `Policy lifecycle ${operation}`,
  });
}

async function execution(client: WorkspaceClientPort) {
  const result = await client.read({
    operation: "project.execution.get.v1",
    projectId,
    contextId,
  });
  if (!result.ok || result.value.operation !== "project.execution.get.v1") {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-009#verification: project execution fixture read failed; fix surface: repair the policy lifecycle project execution fixture",
    );
  }
  return result.value.execution;
}

function requests(process: RecordingProcess, method: string) {
  return process.requests.filter((request) => request.method === method);
}

function requestParams(process: RecordingProcess, method: string) {
  const request = process.requests.find((candidate) => candidate.method === method);
  if (request === undefined) throw new Error(`Missing ${method} request`);
  return request.params;
}

function parameterString(value: unknown, key: string): string {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`Expected object parameters for ${key}`);
  }
  const field = z.record(z.string(), z.unknown()).parse(value)[key];
  if (typeof field !== "string") throw new Error(`Expected string parameter ${key}`);
  return field;
}

function parameterBoolean(value: unknown, key: string): boolean {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(
      `violates REQ spec://org.vibevm.zap/lens/PROP-009#verification: expected object parameters for ${key}; fix surface: repair the recorded request fixture`,
    );
  }
  const field = z.record(z.string(), z.unknown()).parse(value)[key];
  if (typeof field !== "boolean") {
    throw new Error(
      `violates REQ spec://org.vibevm.zap/lens/PROP-009#verification: expected boolean parameter ${key}; fix surface: repair the adapter request shape`,
    );
  }
  return field;
}

function sequencedIds() {
  const counts = new Map<string, number>();
  return (kind: string) => {
    const next = (counts.get(kind) ?? 0) + 1;
    counts.set(kind, next);
    return `${kind}.policy-lifecycle-${next}`;
  };
}

function nextDispatch(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}
