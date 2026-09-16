import assert from "node:assert/strict";
import test from "node:test";
import {
  createProviderCoordinatorAdapter,
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

test("provider adapter converts thrown launch failures into a bounded host refusal", async () => {
  const rejected = () =>
    Promise.reject(
      new Error(
        reqMessage("synthetic provider launch threw", "convert the exception at the adapter port"),
      ),
    );
  const transport: ProviderCoordinatorTransport = {
    start: rejected,
    resume: rejected,
    history: rejected,
    send: rejected,
    interrupt: rejected,
    respond: rejected,
    pause: rejected,
    stop: rejected,
    subscribe: () => () => undefined,
    close: () => undefined,
  };
  const profile: ProviderCoordinatorProfile = {
    profileId: "profile.claude.throwing",
    provider: "claude_code",
    executablePath: "C:/fixture/claude.cmd",
    cwd: "C:/fixture",
    modelId: "fixture-model",
    effort: null,
    endpoint: null,
  };
  const created = createProviderCoordinatorAdapter({
    profile,
    hostId: "host.claude.throwing",
    transport,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const scope = {
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.throwing"),
    projectId: ProjectIdSchema.parse("project.fixture"),
    contextId: WorkContextIdSchema.parse("context.fixture"),
    conversationId: ConversationIdSchema.parse("conversation.fixture"),
    coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
    hostId: ExecutionHostIdSchema.parse("host.claude.throwing"),
    profileId: profile.profileId,
    cwd: profile.cwd,
  };
  const started = await created.value.start({
    ...scope,
    bootstrapText: "",
    bootstrapBasis: "fixture",
  });
  assert.equal(started.ok, false);
  if (!started.ok) {
    assert.equal(started.error.code, "host_refused");
    assert.doesNotMatch(started.error.message, /fixture path/u);
  }
  const resumed = await created.value.resume({
    ...scope,
    nativeThreadId: "thread.claude.throwing",
  });
  assert.equal(resumed.ok, false);
  if (!resumed.ok) assert.equal(resumed.error.code, "host_refused");
});

function reqMessage(why: string, fix: string): string {
  return `violates REQ spec://org.vibevm.zap/lens/PROP-006#root: ${why}; fix surface: ${fix}`;
}
