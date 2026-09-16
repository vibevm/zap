import assert from "node:assert/strict";
import test from "node:test";
import {
  createProviderCoordinatorAdapter,
  createProviderCoordinatorHost,
  type ProviderCoordinatorProfile,
  type ProviderCoordinatorTransport,
} from "./index.ts";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema } from "../protocol/index.ts";

test("provider host opens an independent adapter for each coordinator scope", async () => {
  let opened = 0;
  const host = createProviderCoordinatorHost({
    hostId: "host.opencode.independent",
    profile: {
      profileId: "profile.opencode.independent",
      provider: "opencode",
      executablePath: "C:/fixture/opencode.exe",
      cwd: "C:/fixture",
      modelId: "fixture-model",
      effort: null,
      endpoint: "http://127.0.0.1:4096",
    },
    transportFactory: {
      open: () => {
        opened += 1;
        return Promise.resolve({ ok: true, value: fixtureTransport("opencode") });
      },
    },
  });
  const first = await host.openCoordinator("profile.opencode.independent");
  const second = await host.openCoordinator("profile.opencode.independent");
  assert.equal(first.ok, true);
  assert.equal(second.ok, true);
  assert.equal(opened, 2);
  if (first.ok && second.ok) assert.notEqual(first.value, second.value);
});

for (const provider of ["claude_code", "opencode", "qwen_code"] as const) {
  test(`${provider} fixture adapter preserves host IDs and honest capabilities`, async () => {
    const transport = fixtureTransport(provider);
    const profile: ProviderCoordinatorProfile = {
      profileId: `profile.${provider}.fixture`,
      provider,
      executablePath: `C:/fixture/${provider}.exe`,
      cwd: "C:/fixture",
      modelId: "fixture-model",
      effort: "low",
      endpoint: provider === "opencode" ? "http://127.0.0.1:4096" : null,
    };
    const created = createProviderCoordinatorAdapter({
      profile,
      hostId: `host.${provider}.fixture`,
      transport,
    });
    assert.equal(created.ok, true);
    if (!created.ok) return;
    const events: unknown[] = [];
    const scope = {
      coordinatorSessionId: AgentSessionIdSchema.parse(`session.${provider}.fixture`),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse(`host.${provider}.fixture`),
    };
    const started = await created.value.start({
      ...scope,
      profileId: profile.profileId,
      cwd: profile.cwd,
      bootstrapText: "fixture bootstrap",
      bootstrapBasis: "fixture",
    });
    assert.equal(started.ok, true);
    if (!started.ok) return;
    created.value.subscribe((event) => events.push(event));
    const sent = await created.value.send({
      coordinatorSessionId: scope.coordinatorSessionId,
      text: "fixture turn",
      clientMessageId: "message.fixture",
    });
    assert.equal(sent.ok, true);
    transport.emit(
      provider === "claude_code"
        ? {
            type: "stream_event",
            message_id: "turn.fixture",
            event: {
              type: "content_block_delta",
              index: "item.fixture",
              delta: { text: "fixture output" },
            },
          }
        : {
            type: "text",
            turn_id: "turn.fixture",
            item_id: "item.fixture",
            text: "fixture output",
          },
    );
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(events.length, 1);
    assert.equal(created.value.capabilities.nativeChildObservation, false);
    assert.equal(created.value.capabilities.commandApproval, false);
    if (provider === "opencode") {
      const stopped = await created.value.stop?.({
        coordinatorSessionId: scope.coordinatorSessionId,
        expectedProcessEpoch: started.value.processEpoch,
      });
      assert.equal(stopped?.ok && stopped.value.observation, "requested");
      const continued = await created.value.continueSession?.({
        coordinatorSessionId: scope.coordinatorSessionId,
        expectedProcessEpoch: started.value.processEpoch,
      });
      assert.equal(continued?.ok && continued.value.observation, "settled");
      assert.equal(continued?.ok && continued.value.previousProcessEpoch, "1");
      assert.equal(continued?.ok && continued.value.currentProcessEpoch, "2");
      assert.equal(continued?.ok && continued.value.nativeThreadId, "opencode.thread.fixture");
    }
    created.value.close();
  });
}

function fixtureTransport(
  provider: "claude_code" | "opencode" | "qwen_code",
): ProviderCoordinatorTransport & { emit(raw: unknown): void } {
  let listener: ((raw: unknown) => void) | undefined;
  let epoch = 1;
  const session = () => ({
    coordinatorSessionId: AgentSessionIdSchema.parse(`session.${provider}.fixture`),
    nativeSessionId: `${provider}.session.fixture`,
    nativeThreadId: `${provider}.thread.fixture`,
    processEpoch: String(epoch),
    modelId: "fixture-model",
    effort: "low" as const,
  });
  return {
    async start() {
      return { ok: true, value: session() };
    },
    async resume() {
      epoch += 1;
      return { ok: true, value: session() };
    },
    async history() {
      return {
        ok: true,
        value: {
          coordinatorSessionId: AgentSessionIdSchema.parse("session.fixture"),
          nativeThreadId: session().nativeThreadId,
          processEpoch: String(epoch),
          status: {},
          turns: [],
        },
      };
    },
    async send() {
      return {
        ok: true,
        value: { nativeTurnId: "turn.fixture", transportCorrelation: null },
      };
    },
    async interrupt() {
      return { ok: true, value: null };
    },
    async respond() {
      return { ok: true, value: null };
    },
    async stop() {
      listener?.({ kind: "process_exited", data: { exitCode: 0 } });
      return { ok: true, value: null };
    },
    subscribe(next) {
      listener = next;
      return () => {
        listener = undefined;
      };
    },
    emit(raw) {
      listener?.(raw);
    },
    close() {
      listener = undefined;
    },
  };
}
