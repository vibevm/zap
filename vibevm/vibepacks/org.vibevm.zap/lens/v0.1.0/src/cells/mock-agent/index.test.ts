/** ZapMockAgent public adapter gate. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import assert from "node:assert/strict";
import test from "node:test";
import { ActorIdSchema, ConversationIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createZapMockModel, type ZapMockModel } from "../mock-model/index.ts";
import { createZapMockAgent } from "./index.ts";

test("ZapMockAgent emits public coordinator events from deterministic model effects", async () => {
  const modelResult = createZapMockModel({
    seed: "agent-gate",
    scenario: {
      scenarioId: "scenario.echo",
      steps: [{ kind: "ready" }, { kind: "echo", prefix: "MOCK:" }],
    },
  });
  assert.equal(modelResult.ok, true);
  if (!modelResult.ok) return;
  const model: ZapMockModel = modelResult.value;
  const created = createZapMockAgent({ modelFactory: () => model });
  const opened = await created.host.openCoordinator("profile.zap-mock");
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const events: string[] = [];
  opened.value.subscribe((event) => events.push(event.kind));
  const sessionId = AgentSessionIdSchema.parse("session.mock.agent");
  const started = await opened.value.start({
    coordinatorSessionId: sessionId,
    projectId: ProjectIdSchema.parse("project.mock.agent"),
    contextId: WorkContextIdSchema.parse("context.mock.agent"),
    conversationId: ConversationIdSchema.parse("conversation.mock.agent"),
    coordinatorActorId: ActorIdSchema.parse("actor.mock.agent"),
    hostId: ExecutionHostIdSchema.parse("host.zap-mock"),
    profileId: "profile.zap-mock",
    cwd: "C:\\fixtures\\zap-mock",
    bootstrapText: "Synthetic bootstrap",
    bootstrapBasis: "zap-mock.test",
  });
  assert.equal(started.ok, true);
  const sent = await opened.value.send({
    coordinatorSessionId: sessionId,
    text: "hello",
    clientMessageId: "message.mock.hello",
  });
  assert.equal(sent.ok, true);
  assert.deepEqual(events, ["session_started", "turn_started", "item_completed", "turn_completed"]);
  const history = await opened.value.readHistory(sessionId);
  assert.equal(history.ok, true);
  opened.value.close();
});

test("ZapMock test driver advances busy gates through adapter events", async () => {
  const model = createZapMockModel({
    seed: "gate",
    scenario: {
      scenarioId: "scenario.gate",
      steps: [
        { kind: "ready" },
        { kind: "busy_gate", gateId: "gate.one", completionText: "opened" },
      ],
    },
  });
  assert.equal(model.ok, true);
  if (!model.ok) return;
  const created = createZapMockAgent({ modelFactory: () => model.value });
  const opened = await created.host.openCoordinator("profile.zap-mock");
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const sessionId = AgentSessionIdSchema.parse("session.mock.gate");
  const seen: string[] = [];
  opened.value.subscribe((event) => seen.push(event.kind));
  const started = await opened.value.start({
    coordinatorSessionId: sessionId,
    projectId: ProjectIdSchema.parse("project.mock.gate"),
    contextId: WorkContextIdSchema.parse("context.mock.gate"),
    conversationId: ConversationIdSchema.parse("conversation.mock.gate"),
    coordinatorActorId: ActorIdSchema.parse("actor.mock.gate"),
    hostId: ExecutionHostIdSchema.parse("host.zap-mock"),
    profileId: "profile.zap-mock",
    cwd: "C:\\synthetic\\mock",
    bootstrapText: "start",
    bootstrapBasis: "mock",
  });
  assert.equal(started.ok, true);
  const message = await opened.value.send({
    coordinatorSessionId: sessionId,
    text: "busy",
    clientMessageId: "message.mock.busy",
  });
  assert.equal(message.ok, true);
  assert.equal(
    (await created.driver.dispatch(sessionId, { kind: "tick", inputId: "tick.one", count: 1 })).ok,
    true,
  );
  assert.equal(
    (
      await created.driver.dispatch(sessionId, {
        kind: "open_gate",
        inputId: "gate.open",
        gateId: "gate.one",
      })
    ).ok,
    true,
  );
  assert.equal(seen.includes("item_completed"), true);
});
