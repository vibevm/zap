/** @scope spec://org.vibevm.zap/lens/PROP-001#verification */
import assert from "node:assert/strict";
import test from "node:test";
import {
  ActorDescriptorSchema,
  ActorHandleSchema,
  ActorIdSchema,
  BindingIdSchema,
  ConversationIdSchema,
  DeliveryIdSchema,
  HostBindingSchema,
  MessageIdSchema,
  PrincipalIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import type { HostKind } from "../protocol/index.ts";
import {
  ActorRouteBindingSchema,
  buildSafePointOffer,
  dispatchOpenCode,
  getHostCapabilities,
  routeSafePointHook,
} from "./index.ts";
import type { ActorRouteBinding } from "./index.ts";

function binding(
  actor: string,
  host: HostKind,
  session: string,
  subagent?: string,
  generation = "1",
): ActorRouteBinding {
  const actorId = ActorIdSchema.parse(actor);
  const workspaceId = WorkspaceIdSchema.parse("workspace.fixture");
  const conversationId = ConversationIdSchema.parse("conversation.fixture");
  return ActorRouteBindingSchema.parse({
    actor: ActorDescriptorSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.fixture"),
      actorId,
      workspaceId,
      conversationId,
      parentActorId: actor === "actor.root" ? null : ActorIdSchema.parse("actor.root"),
      state: "active",
      capabilities: ["inbox:read", "inbox:ack"],
      hostKind: host,
      hostProvenance: "attested",
    }),
    handle: ActorHandleSchema.parse({
      actorId,
      bindingId: BindingIdSchema.parse(`binding.${actor}`),
      workspaceId,
      conversationId,
      generation,
    }),
    host: HostBindingSchema.parse({
      kind: host,
      sessionId: session,
      ...(subagent === undefined ? {} : { subagentId: subagent }),
      provenance: "attested",
    }),
  });
}

function persisted(actor: string, generation = "1") {
  return {
    deliveryId: DeliveryIdSchema.parse(`delivery.${actor}`),
    messageId: MessageIdSchema.parse(`message.${actor}`),
    recipientActorId: ActorIdSchema.parse(actor),
    bindingGeneration: generation,
    content: `answer for ${actor}`,
    persisted: true,
  } as const;
}

test("routes interleaved children by exact native subagent identity", () => {
  const bindings = [
    binding("actor.root", "codex", "thread.shared"),
    binding("actor.child.a", "codex", "thread.shared", "agent.a"),
    binding("actor.child.b", "codex", "thread.shared", "agent.b"),
  ];
  const a = routeSafePointHook(
    "codex",
    {
      session_id: "thread.shared",
      hook_event_name: "SubagentStart",
      agent_id: "agent.a",
      agent_type: "worker",
      turn_id: "turn.1",
    },
    bindings,
  );
  const b = routeSafePointHook(
    "codex",
    {
      session_id: "thread.shared",
      hook_event_name: "SubagentStart",
      agent_id: "agent.b",
      agent_type: "worker",
      turn_id: "turn.1",
    },
    bindings,
  );
  assert.equal(a.ok && a.value.actorId, "actor.child.a");
  assert.equal(b.ok && b.value.actorId, "actor.child.b");
});

test("refuses parent-only ambiguity but accepts an agreeing authenticated child handle", () => {
  const root = binding("actor.root", "claude_code", "session.shared");
  const child = binding("actor.child", "claude_code", "session.shared", "agent.child");
  const ambiguous = routeSafePointHook(
    "claude_code",
    { session_id: "session.shared", hook_event_name: "PostToolUse" },
    [root, child],
  );
  assert.equal(ambiguous.ok, false);
  if (!ambiguous.ok) assert.equal(ambiguous.error.code, "ambiguous_binding");

  const explicit = routeSafePointHook(
    "claude_code",
    { session_id: "session.shared", hook_event_name: "PostToolUse" },
    [root, child],
    {
      actorId: child.handle.actorId,
      bindingId: child.handle.bindingId,
      workspaceId: child.handle.workspaceId,
      conversationId: child.handle.conversationId,
      generation: child.handle.generation,
      nativeAttested: true,
    },
  );
  assert.equal(explicit.ok && explicit.value.actorId, "actor.child");
});

test("Codex parent-only child hook refuses inherited parent identity and display-name guessing", () => {
  const root = binding("actor.root", "codex", "thread.parent");
  const childA = binding("actor.child.a", "codex", "thread.parent", "agent.a");
  const childB = binding("actor.child.b", "codex", "thread.parent", "agent.b");
  const input = {
    session_id: "thread.parent",
    hook_event_name: "PostToolUse",
    agent_type: "display-name-child-a",
  };
  const inherited = routeSafePointHook("codex", input, [root, childA, childB], {
    ...root.handle,
    nativeAttested: false,
  });
  assert.equal(inherited.ok, false);
  if (!inherited.ok) assert.equal(inherited.error.code, "ambiguous_binding");
  const attestedChild = routeSafePointHook("codex", input, [root, childA, childB], {
    ...childA.handle,
    nativeAttested: true,
  });
  assert.equal(attestedChild.ok && attestedChild.value.actorId, "actor.child.a");
});

test("Claude subagent tool hook routes only by its documented agent_id", () => {
  const bindings = [
    binding("actor.child.a", "claude_code", "session.claude", "agent.claude.a"),
    binding("actor.child.b", "claude_code", "session.claude", "agent.claude.b"),
  ];
  const routed = routeSafePointHook(
    "claude_code",
    {
      session_id: "session.claude",
      hook_event_name: "PostToolUse",
      agent_id: "agent.claude.b",
    },
    bindings,
  );
  assert.equal(routed.ok && routed.value.actorId, "actor.child.b");
});

test("Qwen subagent hook routes by common session_id plus agent_id", () => {
  const bindings = [
    binding("actor.child.a", "qwen_code", "session.qwen", "agent.qwen.a"),
    binding("actor.child.b", "qwen_code", "session.qwen", "agent.qwen.b"),
  ];
  const routed = routeSafePointHook(
    "qwen_code",
    {
      session_id: "session.qwen",
      hook_event_name: "SubagentStop",
      agent_id: "agent.qwen.a",
      agent_type: "Explorer",
    },
    bindings,
  );
  assert.equal(routed.ok && routed.value.actorId, "actor.child.a");
});

test("OpenCode dispatch preserves its separate exact child session binding", async () => {
  const target = binding("actor.open.child", "opencode", "session.open.child");
  let offeredSession = "";
  const result = await dispatchOpenCode(persisted("actor.open.child"), target, {
    observeStatus: async (sessionId) => {
      offeredSession = sessionId;
      return { state: "idle", observationId: "status.child" };
    },
    sendWhenIdle: async (request) => ({
      kind: "completed",
      statusObservationId: request.expectedStatusObservationId,
      hostReceiptId: "receipt.child",
    }),
  });
  assert.equal(result.ok, true);
  assert.equal(offeredSession, "session.open.child");
});

test("rejects stale and contradicting authenticated handles", () => {
  const child = binding("actor.child", "qwen_code", "session.q", "agent.q", "3");
  const stale = routeSafePointHook(
    "qwen_code",
    { session_id: "session.q", hook_event_name: "SubagentStart", agent_id: "agent.q" },
    [child],
    {
      actorId: child.handle.actorId,
      bindingId: child.handle.bindingId,
      workspaceId: child.handle.workspaceId,
      conversationId: child.handle.conversationId,
      generation: "2",
    },
  );
  assert.equal(stale.ok, false);
  if (!stale.ok) assert.equal(stale.error.code, "stale_binding");

  const mismatch = routeSafePointHook(
    "qwen_code",
    { session_id: "session.q", hook_event_name: "SubagentStart", agent_id: "other.agent" },
    [child],
    {
      actorId: child.handle.actorId,
      bindingId: child.handle.bindingId,
      workspaceId: child.handle.workspaceId,
      conversationId: child.handle.conversationId,
      generation: "3",
    },
  );
  assert.equal(mismatch.ok, false);
  if (!mismatch.ok) assert.equal(mismatch.error.code, "binding_mismatch");
});

test("safe-point output escapes content and never claims host or actor acknowledgement", () => {
  const target = binding("actor.child", "codex", "thread.one", "agent.one");
  const route = routeSafePointHook(
    "codex",
    {
      session_id: "thread.one",
      hook_event_name: "SubagentStart",
      agent_id: "agent.one",
    },
    [target],
  );
  assert.equal(route.ok, true);
  if (!route.ok) return;
  const message = { ...persisted("actor.child"), content: "<answer>&ok</answer>" };
  const offered = buildSafePointOffer(route.value, [message]);
  assert.equal(offered.ok, true);
  if (!offered.ok) return;
  assert.match(
    offered.value.hostOutput.hookSpecificOutput.additionalContext,
    /&lt;answer&gt;&amp;ok&lt;\/answer&gt;/,
  );
  const observations = offered.value.deliveries[0]?.observations;
  assert.equal(observations?.persisted.kind, "observed");
  assert.equal(observations?.hostAccepted.kind, "not_observed");
  assert.equal(observations?.actorAcknowledged.kind, "not_observed");
});

test("OpenCode holds while busy and does not call send", async () => {
  const target = binding("actor.open", "opencode", "session.open");
  let sends = 0;
  const result = await dispatchOpenCode(persisted("actor.open"), target, {
    observeStatus: async () => ({ state: "busy", observationId: "status.1" }),
    sendWhenIdle: async () => {
      sends += 1;
      return { kind: "completed", statusObservationId: "status.1", hostReceiptId: "receipt.1" };
    },
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.outcome, "held");
  assert.deepEqual(result.value.observations.offered, {
    kind: "observed",
    value: false,
    evidence: "status busy; send not attempted",
  });
  assert.equal(result.value.observations.hostAccepted.kind, "not_observed");
  assert.equal(sends, 0);
});

test("OpenCode thrown send prohibits blind retry until reconciliation", async () => {
  const target = binding("actor.open", "opencode", "session.open");
  const result = await dispatchOpenCode(persisted("actor.open"), target, {
    observeStatus: async () => ({ state: "idle", observationId: "status.1" }),
    sendWhenIdle: async () =>
      Promise.reject(
        new Error(
          "violates REQ spec://org.vibevm.zap/lens/PROP-001#adapters: response was lost; fix surface: reconcile the host message before retry",
        ),
      ),
  });
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.equal(result.error.retryable, false);
  assert.match(result.error.message, /reconcile/);
});

test("OpenCode surfaces the idle-to-busy race as uncertain", async () => {
  const target = binding("actor.open", "opencode", "session.open");
  const result = await dispatchOpenCode(persisted("actor.open"), target, {
    observeStatus: async () => ({ state: "idle", observationId: "status.1" }),
    sendWhenIdle: async () => ({ kind: "busy", statusObservationId: "status.2" }),
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.outcome, "uncertain");
  assert.equal(result.value.reconcileRequired, true);
  assert.equal(result.value.observations.hostAccepted.kind, "uncertain");
  assert.equal(result.value.observations.actorAcknowledged.kind, "not_observed");
});

test("OpenCode completion proves host acceptance only", async () => {
  const target = binding("actor.open", "opencode", "session.open");
  const result = await dispatchOpenCode(persisted("actor.open"), target, {
    observeStatus: async () => ({ state: "idle", observationId: "status.1" }),
    sendWhenIdle: async () => ({
      kind: "completed",
      statusObservationId: "status.1",
      hostReceiptId: "receipt.1",
    }),
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.outcome, "host_accepted");
  assert.deepEqual(result.value.observations.hostAccepted, {
    kind: "observed",
    value: true,
    evidence: "receipt receipt.1",
  });
  assert.equal(result.value.observations.actorAcknowledged.kind, "not_observed");
});

test("OpenCode asynchronous acceptance stays uncertain and unacknowledged", async () => {
  const target = binding("actor.open", "opencode", "session.open");
  const result = await dispatchOpenCode(persisted("actor.open"), target, {
    observeStatus: async () => ({ state: "idle", observationId: "status.1" }),
    sendWhenIdle: async () => ({
      kind: "accepted_without_result",
      statusObservationId: "status.1",
    }),
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value.outcome, "uncertain");
  assert.equal(result.value.reconcileRequired, true);
  assert.equal(result.value.observations.hostAccepted.kind, "uncertain");
  assert.equal(result.value.observations.actorAcknowledged.kind, "not_observed");
});

test("Qwen Windows advertises file queue wake without claiming peer support", () => {
  const capability = getHostCapabilities("qwen_code", "win32");
  assert.equal(capability.idleWake, "conditional");
  assert.deepEqual(
    capability.inputPaths.find((path) => path.kind === "regular_file_queue"),
    {
      kind: "regular_file_queue",
      availability: "conditional",
      idleWake: true,
      activeTurnSteering: false,
    },
  );
  assert.equal(
    capability.inputPaths.find((path) => path.kind === "native_peer")?.availability,
    "unsupported",
  );
  assert.match(capability.limitations.join(" "), /unavailable on Windows/);
  assert.match(capability.limitations.join(" "), /must never be generated automatically/);
});
