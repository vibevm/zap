import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { ActorIdSchema, ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ExecutionHostIdSchema,
  RunIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import {
  createConfiguredCoordinatorRoutingProvider,
  CoordinatorRoutingConfigSchema,
  createWorkspaceCoordinatorRoutingBridge,
  initializeConfiguredCoordinatorPolicies,
} from "../coordinator-routing/index.ts";
import { openModelPolicyStore } from "../model-policy-store/index.ts";
import { createCoordinatorAdapterRegistry, createWorkspaceService } from "./index.ts";
import { FakeAdapter, access, registration, requestWithId } from "./index.test-support.ts";

test("workspace service deduplicates clients, scopes projects, and projects native output", async () => {
  const storeResult = openWorkspaceStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T10:00:00.000Z"),
    idFactory: (() => {
      let n = 0;
      return (kind: string) => `${kind}.test-${++n}`;
    })(),
  });
  assert.equal(storeResult.ok, true);
  if (!storeResult.ok) return;
  const store = storeResult.value;
  const projectA = registration("a");
  const projectB = registration("b");
  assert.equal(store.registerProject(projectA).ok, true);
  assert.equal(store.registerProject(projectB).ok, true);
  const adapterA = new FakeAdapter();
  const adapterB = new FakeAdapter();
  adapterA.onStart = () => {
    adapterA.emit({
      coordinatorSessionId: "session.service-1",
      processEpoch: "1",
      nativeThreadId: "root.project.a",
      nativeTurnId: null,
      nativeItemId: "item.sync-child",
      kind: "native_child_observed",
      sourceEventId: "host.a.sync-child",
      data: {
        fromNativeThreadId: "root.project.a",
        toNativeThreadId: "child.a",
        relationship: "parent",
        tool: "spawnAgent",
        canAcceptDirectInput: null,
      },
    });
  };
  const hostA = {
    hostId: ExecutionHostIdSchema.parse("host.a"),
    profileIds: ["protected-profile.a"],
    openCoordinator: async () => ({ ok: true as const, value: adapterA }),
  };
  const hostB = {
    hostId: ExecutionHostIdSchema.parse("host.b"),
    profileIds: ["protected-profile.b"],
    openCoordinator: async () => ({ ok: true as const, value: adapterB }),
  };
  const service = createWorkspaceService({
    store,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "protected-profile.a", host: hostA },
      { profileRef: "protected-profile.b", host: hostB },
    ]),
    idFactory: (() => {
      let n = 0;
      return (kind: string) => `${kind}.service-${++n}`;
    })(),
  });
  const actions = ["read", "events", "subscribe", "session.start.v1"] as const;
  const clientA = service.bind({
    access: access("a", [projectA.projectId, projectB.projectId], "client.one"),
    allowedActions: actions,
  });
  const clientASecond = service.bind({
    access: access("a", [projectA.projectId, projectB.projectId], "client.two"),
    allowedActions: actions,
  });
  const clientB = service.bind({
    access: access("b", [projectA.projectId, projectB.projectId], "client.three"),
    allowedActions: actions,
  });
  const startA = WorkspaceCommandRequestSchema.parse({
    operation: "session.start.v1",
    clientRequestId: "request.start.a",
    projectId: projectA.projectId,
    contextId: projectA.context.contextId,
    interactionKind: "structured",
    profileId: "profile.codex-default",
  });
  const startA2 = requestWithId(startA, "request.start.a.second");
  const startB = WorkspaceCommandRequestSchema.parse({
    ...startA,
    clientRequestId: "request.start.b",
    projectId: projectB.projectId,
    contextId: projectB.context.contextId,
  });
  assert.equal((await clientA.command(startA)).ok, true);
  assert.equal((await clientASecond.command(startA2)).ok, true);
  assert.equal((await clientB.command(startB)).ok, true);
  assert.equal(adapterA.starts, 1);
  assert.equal(adapterB.starts, 1);
  const feed = clientA.subscribe({
    cursor: {
      scope: { kind: "project", projectId: projectA.projectId },
      afterGlobalSequence: DecimalSchema.parse("0"),
    },
  });
  const nextEvent = feed[Symbol.asyncIterator]().next();
  adapterA.emit({
    coordinatorSessionId: "session.service-1",
    processEpoch: "stale-epoch",
    nativeThreadId: "child.a",
    nativeTurnId: "turn.a",
    nativeItemId: "item.stale",
    kind: "message_delta",
    sourceEventId: "host.a.stale",
    data: { delta: "Ignored stale output" },
  });
  adapterA.emit({
    coordinatorSessionId: "session.service-1",
    processEpoch: "1",
    nativeThreadId: "root.project.a",
    nativeTurnId: null,
    nativeItemId: "item.child",
    kind: "native_child_observed",
    sourceEventId: "host.a.child",
    data: {
      fromNativeThreadId: "root.project.a",
      toNativeThreadId: "child.a",
      relationship: "parent",
      tool: "spawnAgent",
      canAcceptDirectInput: null,
    },
  });
  adapterA.emit({
    coordinatorSessionId: "session.service-1",
    processEpoch: "1",
    nativeThreadId: "child.a",
    nativeTurnId: "turn.a",
    nativeItemId: "item.delta",
    kind: "message_delta",
    sourceEventId: "host.a.delta",
    data: { delta: "Child output" },
  });
  adapterA.emit({
    coordinatorSessionId: "session.service-1",
    processEpoch: "1",
    nativeThreadId: "child.a",
    nativeTurnId: "turn.a",
    nativeItemId: "item.completed",
    kind: "item_completed",
    sourceEventId: "host.a.completed",
    data: {
      id: "item.completed",
      type: "agentMessage",
      status: "completed",
      text: "Completed child output",
      tool: "sendMessage",
    },
  });
  adapterA.emit({
    coordinatorSessionId: "session.service-1",
    processEpoch: "1",
    nativeThreadId: "child.a",
    nativeTurnId: "turn.a",
    nativeItemId: "item.completed",
    kind: "item_completed",
    sourceEventId: "host.a.completed",
    data: {
      id: "item.completed",
      type: "agentMessage",
      status: "completed",
      text: "Completed child output",
      tool: "sendMessage",
    },
  });
  const observed = await nextEvent;
  assert.equal(observed.done, false);
  const network = await clientA.read({
    operation: "agent.network.v1",
    projectId: projectA.projectId,
    contextId: projectA.context.contextId,
  });
  assert.equal(network.ok, true);
  if (network.ok && network.value.operation === "agent.network.v1")
    assert.equal(network.value.network.agents.length, 2);
  const output = await clientA.read({
    operation: "agent.output.page.v1",
    projectId: projectA.projectId,
    contextId: projectA.context.contextId,
    actorId: ActorIdSchema.parse("actor.service-3"),
    afterSequence: DecimalSchema.parse("0"),
    limit: 10,
  });
  assert.equal(output.ok, true, output.ok ? "" : output.error.message);
  if (output.ok && output.value.operation === "agent.output.page.v1")
    assert.deepEqual(
      output.value.page.items.map((item) => item.bodyMarkdown),
      ["Child output", "Completed child output"],
    );
  const foreign = await clientA.read({
    operation: "agent.network.v1",
    projectId: projectB.projectId,
    contextId: projectB.context.contextId,
  });
  assert.equal(foreign.ok, true);
  service.close();
  store.close();
});

test("session start pins configured policy before adapter launch and passes model effort", async () => {
  const storeResult = openWorkspaceStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T10:00:00.000Z"),
  });
  assert.equal(storeResult.ok, true);
  if (!storeResult.ok) return;
  const project = registration("routing-start");
  assert.equal(storeResult.value.registerProject(project).ok, true);
  const routingAccess = access("routing-start", [project.projectId], "client.routing-start");
  const policyStoreResult = openModelPolicyStore({
    databasePath: ":memory:",
    clock: () => new Date("2026-09-15T10:00:00.000Z"),
  });
  assert.equal(policyStoreResult.ok, true);
  if (!policyStoreResult.ok) return;
  const config = CoordinatorRoutingConfigSchema.parse({
    profiles: [
      {
        scope: { projectId: project.projectId, contextId: project.context.contextId },
        profile: {
          profileId: "codex.small",
          productId: "codex",
          productVersion: "1.0",
          providerId: "openai",
          modelId: "gpt-5.6-luna",
          effort: "ultra",
        },
      },
    ],
    capabilities: [
      {
        profileId: "codex.small",
        capability: {
          capabilityId: "cap.routing-start",
          productId: "codex",
          productVersion: "1.0",
          executionMode: "native",
          invocationScope: "coordinator",
          modelId: "gpt-5.6-luna",
          effort: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
          extendedThinking: "configurable",
          evidence: { source: "fake", observedAt: "2026-09-15T10:00:00.000Z" },
        },
      },
    ],
    policies: [
      {
        scope: { projectId: project.projectId, contextId: project.context.contextId },
        policyId: "policy.routing-start",
        clientRequestId: "request.routing-start.policy",
        sourceEventId: "routing-start.policy.1",
      },
    ],
  });
  assert.equal(
    initializeConfiguredCoordinatorPolicies(policyStoreResult.value, routingAccess, config).ok,
    true,
  );
  const adapter = new FakeAdapter();
  const host = {
    hostId: ExecutionHostIdSchema.parse("host.routing-start"),
    profileIds: ["protected-profile.routing-start"],
    openCoordinator: async () => ({ ok: true as const, value: adapter }),
  };
  const service = createWorkspaceService({
    store: storeResult.value,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "protected-profile.routing-start", host },
      { profileRef: "codex.small", host },
    ]),
    idFactory: (() => {
      let next = 0;
      return (kind: string) => `${kind}.routing-start-${++next}`;
    })(),
    coordinatorRouting: createWorkspaceCoordinatorRoutingBridge({
      store: policyStoreResult.value,
      provider: createConfiguredCoordinatorRoutingProvider(config),
      defaults: {
        policyEnabled: true,
        purpose: "test_agent",
        taskClass: "verification",
        productId: "codex",
        productVersion: "1.0",
      },
    }),
  });
  const client = service.bind({
    access: routingAccess,
    allowedActions: ["read", "session.start.v1"],
  });
  const started = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "session.start.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.routing-start.session"),
      projectId: project.projectId,
      contextId: project.context.contextId,
      interactionKind: "structured",
      profileId: "profile.codex-default",
    }),
  );
  assert.equal(started.ok, true);
  assert.equal(adapter.startInputs[0]?.profileId, "codex.small");
  assert.equal(adapter.startInputs[0]?.modelId, "gpt-5.6-luna");
  assert.equal(adapter.startInputs[0]?.reasoningEffort, "low");
  const pinned = policyStoreResult.value.readSelection(
    routingAccess,
    project.projectId,
    project.context.contextId,
    RunIdSchema.parse("session.routing-start-1"),
    AttemptIdSchema.parse("session.routing-start-1.attempt"),
  );
  assert.equal(pinned.ok, true);
  service.close();
  storeResult.value.close();
  policyStoreResult.value.close();
});

test("durable launch claims survive service restart and force native resume", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "lens-claim-"));
  const databasePath = join(directory, "workspace.sqlite");
  const firstStoreResult = openWorkspaceStore({
    databasePath,
    clock: () => new Date("2026-09-15T10:00:00.000Z"),
  });
  assert.equal(firstStoreResult.ok, true);
  if (!firstStoreResult.ok) return;
  const project = registration("recovery");
  assert.equal(firstStoreResult.value.registerProject(project).ok, true);
  const adapter = new FakeAdapter();
  let claimSeenBeforeHost = false;
  adapter.onStart = () => {
    const claim = firstStoreResult.value.readCoordinatorClaim(
      project.projectId,
      project.context.contextId,
    );
    claimSeenBeforeHost = claim.ok && claim.value?.state === "starting";
  };
  const host = {
    hostId: ExecutionHostIdSchema.parse("host.recovery"),
    profileIds: ["protected-profile.recovery"],
    openCoordinator: async () => ({ ok: true as const, value: adapter }),
  };
  const service = createWorkspaceService({
    store: firstStoreResult.value,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "protected-profile.recovery", host },
    ]),
    idFactory: (() => {
      let next = 0;
      return (kind: string) => `${kind}.recovery-${++next}`;
    })(),
  });
  const actions = ["read", "events", "subscribe", "session.start.v1"] as const;
  const client = service.bind({
    access: access("recovery", [project.projectId], "client.recovery.one"),
    allowedActions: actions,
  });
  const request = WorkspaceCommandRequestSchema.parse({
    operation: "session.start.v1",
    clientRequestId: "request.recovery.start",
    projectId: project.projectId,
    contextId: project.context.contextId,
    interactionKind: "structured",
    profileId: "profile.codex-default",
  });
  assert.equal((await client.command(request)).ok, true);
  assert.equal(claimSeenBeforeHost, true);
  assert.equal(adapter.starts, 1);
  adapter.emit({
    coordinatorSessionId: "session.recovery-1",
    processEpoch: "1",
    nativeThreadId: "root.project.recovery",
    nativeTurnId: null,
    nativeItemId: "item.child",
    kind: "native_child_observed",
    sourceEventId: "recovery.child.initial",
    data: {
      fromNativeThreadId: "root.project.recovery",
      toNativeThreadId: "child.recovery",
      relationship: "parent",
      tool: "spawnAgent",
      canAcceptDirectInput: null,
    },
  });
  service.close();
  firstStoreResult.value.close();

  const secondStoreResult = openWorkspaceStore({
    databasePath,
    clock: () => new Date("2026-09-15T10:00:01.000Z"),
  });
  assert.equal(secondStoreResult.ok, true);
  if (!secondStoreResult.ok) return;
  const persisted = secondStoreResult.value.readCoordinatorClaim(
    project.projectId,
    project.context.contextId,
  );
  assert.equal(persisted.ok && persisted.value?.state, "running");
  const restarted = createWorkspaceService({
    store: secondStoreResult.value,
    adapters: createCoordinatorAdapterRegistry([
      { profileRef: "protected-profile.recovery", host },
    ]),
  });
  const resumedClient = restarted.bind({
    access: access("recovery", [project.projectId], "client.recovery.two"),
    allowedActions: actions,
  });
  assert.equal(
    (await resumedClient.command(requestWithId(request, "request.recovery.resume"))).ok,
    true,
  );
  assert.equal(adapter.resumes, 1);
  adapter.emit({
    coordinatorSessionId: "session.recovery-1",
    processEpoch: "2",
    nativeThreadId: "root.project.recovery",
    nativeTurnId: null,
    nativeItemId: "item.child",
    kind: "native_child_observed",
    sourceEventId: "recovery.child.replayed",
    data: {
      fromNativeThreadId: "root.project.recovery",
      toNativeThreadId: "child.recovery",
      relationship: "parent",
      tool: "spawnAgent",
      canAcceptDirectInput: null,
    },
  });
  adapter.emit({
    coordinatorSessionId: "session.recovery-1",
    processEpoch: "2",
    nativeThreadId: "child.recovery",
    nativeTurnId: "turn.recovery",
    nativeItemId: "item.completed.recovery",
    kind: "item_completed",
    sourceEventId: "recovery.completed.epoch2",
    data: {
      id: "item.completed.recovery",
      type: "agentMessage",
      status: "completed",
      text: "Recovered completed output",
    },
  });
  adapter.emit({
    coordinatorSessionId: "session.recovery-1",
    processEpoch: "2",
    nativeThreadId: "child.recovery",
    nativeTurnId: "turn.recovery",
    nativeItemId: "item.completed.recovery",
    kind: "item_completed",
    sourceEventId: "recovery.completed.epoch2.replayed",
    data: {
      id: "item.completed.recovery",
      type: "agentMessage",
      status: "completed",
      text: "Recovered completed output",
    },
  });
  const restoredNetwork = await resumedClient.read({
    operation: "agent.network.v1",
    projectId: project.projectId,
    contextId: project.context.contextId,
  });
  assert.equal(restoredNetwork.ok, true);
  if (restoredNetwork.ok && restoredNetwork.value.operation === "agent.network.v1") {
    assert.equal(restoredNetwork.value.network.agents.length, 2);
    const child = restoredNetwork.value.network.agents.find((agent) => agent.role === "worker");
    assert.notEqual(child, undefined);
    if (child !== undefined) {
      const restoredOutput = await resumedClient.read({
        operation: "agent.output.page.v1",
        projectId: project.projectId,
        contextId: project.context.contextId,
        actorId: child.actorId,
        afterSequence: DecimalSchema.parse("0"),
        limit: 10,
      });
      assert.equal(restoredOutput.ok, true);
      if (restoredOutput.ok && restoredOutput.value.operation === "agent.output.page.v1")
        assert.equal(restoredOutput.value.page.items.length, 1);
    }
  }
  const denied = restarted.bind({
    access: access("denied", [], "client.recovery.denied"),
    allowedActions: actions,
  });
  assert.equal((await denied.command(requestWithId(request, "request.recovery.denied"))).ok, false);
  restarted.close();
  secondStoreResult.value.close();
  rmSync(directory, { recursive: true, force: true });
});
