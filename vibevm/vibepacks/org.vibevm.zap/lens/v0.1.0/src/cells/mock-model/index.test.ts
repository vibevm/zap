/** Deterministic Zap mock-model proofs. @scope spec://org.vibevm.zap/lens/PROP-013#root */
import assert from "node:assert/strict";
import test from "node:test";
import { createZapMockModel, ZapMockScenarioSchema, type ZapMockInput } from "./index.ts";

const scenario = ZapMockScenarioSchema.parse({
  scenarioId: "scenario.complete.fixture",
  steps: [
    { kind: "ready" },
    { kind: "echo", prefix: "echo: " },
    { kind: "busy_gate", gateId: "gate.fixture", completionText: "gate complete" },
    {
      kind: "question",
      questionId: "question.fixture",
      prompt: "Choose a route",
      options: ["first", "second"],
    },
    {
      kind: "late_answer",
      questionId: "question.fixture",
      acknowledgementId: "ack.fixture",
    },
    { kind: "report", reportId: "report.fixture" },
    { kind: "pause_continue" },
    { kind: "restart" },
    { kind: "delivery_fault", deliveryId: "delivery.duplicate", fault: "duplicate" },
    { kind: "delivery_fault", deliveryId: "delivery.uncertain", fault: "uncertain" },
  ],
});

const tape: readonly ZapMockInput[] = [
  { kind: "start", inputId: "input.start" },
  { kind: "message", inputId: "input.echo", messageId: "message.echo", text: "hello" },
  { kind: "message", inputId: "input.busy", messageId: "message.busy", text: "work" },
  { kind: "tick", inputId: "input.tick", count: 3 },
  { kind: "open_gate", inputId: "input.gate", gateId: "gate.fixture" },
  {
    kind: "message",
    inputId: "input.question",
    messageId: "message.question",
    text: "ask",
  },
  {
    kind: "answer",
    inputId: "input.answer",
    questionId: "question.fixture",
    answerVersion: "answer.version.1",
    text: "first",
  },
  {
    kind: "ack",
    inputId: "input.ack",
    acknowledgementId: "ack.fixture",
    answerVersion: "answer.version.1",
  },
  {
    kind: "report",
    inputId: "input.report",
    reportId: "report.fixture",
    summary: "fixture work complete",
  },
  { kind: "pause", inputId: "input.pause" },
  { kind: "continue", inputId: "input.continue" },
  { kind: "restart", inputId: "input.restart" },
  {
    kind: "message",
    inputId: "input.duplicate-fault",
    messageId: "message.duplicate-fault",
    text: "duplicate",
  },
  {
    kind: "message",
    inputId: "input.uncertain-fault",
    messageId: "message.uncertain-fault",
    text: "uncertain",
  },
];

test("same seed, scenario and ordered tape produce identical effects, state and trace", () => {
  const left = opened("seed.fixture");
  const right = opened("seed.fixture");
  const leftTransitions = tape.map((input) => dispatched(left, input));
  const rightTransitions = tape.map((input) => dispatched(right, input));
  assert.deepEqual(leftTransitions, rightTransitions);
  assert.deepEqual(left.snapshot(), right.snapshot());
  assert.equal(left.state().status, "complete");
  assert.equal(left.state().logicalTick, tape.length - 1 + 3);
  assert.equal(leftTransitions[2]?.state.status, "busy");
  assert.equal(leftTransitions[3]?.state.status, "busy");
  assert.equal(leftTransitions[5]?.state.status, "idle");
  assert.equal(leftTransitions[9]?.state.status, "paused");
  assert.deepEqual(
    leftTransitions
      .flatMap((transition) => transition.effects)
      .filter((effect) => effect.kind === "delivery")
      .map((effect) => (effect.kind === "delivery" ? effect.observation : "unreachable")),
    ["accepted", "duplicate", "uncertain"],
  );
});

test("snapshot restore continues from an awaiting acknowledgement with identical suffix", () => {
  const uninterrupted = opened("seed.snapshot.fixture");
  const split = opened("seed.snapshot.fixture");
  const boundary = tape.findIndex((input) => input.kind === "ack");
  for (const input of tape.slice(0, boundary)) {
    dispatched(uninterrupted, input);
    dispatched(split, input);
  }
  assert.equal(split.state().phase, "awaiting_ack");
  const restored = createZapMockModel({
    seed: "seed.snapshot.fixture",
    scenario,
    snapshot: split.snapshot(),
  });
  assert.equal(restored.ok, true);
  if (!restored.ok) return;
  const uninterruptedSuffix = tape.slice(boundary).map((input) => dispatched(uninterrupted, input));
  const restoredSuffix = tape.slice(boundary).map((input) => dispatched(restored.value, input));
  assert.deepEqual(restoredSuffix, uninterruptedSuffix);
  assert.deepEqual(restored.value.snapshot(), uninterrupted.snapshot());
});

test("duplicate input is explicit and does not advance the scenario", () => {
  const model = opened("seed.duplicate.fixture");
  const start = tape[0];
  const message = tape[1];
  assert.ok(start !== undefined && message !== undefined);
  if (start === undefined || message === undefined) return;
  dispatched(model, start);
  const first = dispatched(model, message);
  const duplicate = dispatched(model, message);
  assert.equal(first.state.cursor, 2);
  assert.equal(duplicate.state.cursor, 2);
  assert.equal(duplicate.effects[0]?.kind, "delivery");
  if (duplicate.effects[0]?.kind === "delivery")
    assert.equal(duplicate.effects[0].observation, "input_duplicate_ignored");
});

test("scenario references and strict input shape fail before model execution", () => {
  const invalidScenario = ZapMockScenarioSchema.safeParse({
    scenarioId: "scenario.invalid",
    steps: [
      { kind: "ready" },
      {
        kind: "late_answer",
        questionId: "question.missing",
        acknowledgementId: "ack.invalid",
      },
    ],
  });
  assert.equal(invalidScenario.success, false);
  const model = opened("seed.strict.fixture");
  const invalidInput = model.dispatch({ kind: "start", inputId: "input.strict", extra: true });
  assert.equal(invalidInput.ok, false);
  if (!invalidInput.ok) assert.equal(invalidInput.error.code, "invalid_input");
});

test("active busy pause retains the gate and Continue restores the same work", () => {
  const activeScenario = ZapMockScenarioSchema.parse({
    scenarioId: "scenario.active-pause",
    steps: [
      { kind: "ready" },
      { kind: "busy_gate", gateId: "gate.active", completionText: "resumed" },
      { kind: "echo", prefix: "after: " },
    ],
  });
  const created = createZapMockModel({ seed: "seed.active-pause", scenario: activeScenario });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  dispatched(created.value, { kind: "start", inputId: "active.start" });
  dispatched(created.value, {
    kind: "message",
    inputId: "active.busy",
    messageId: "active.message",
    text: "work",
  });
  const paused = dispatched(created.value, { kind: "pause", inputId: "active.pause" });
  assert.equal(paused.state.status, "paused");
  assert.equal(paused.state.activeGateId, "gate.active");
  const continued = dispatched(created.value, {
    kind: "continue",
    inputId: "active.continue",
  });
  assert.equal(continued.state.status, "busy");
  assert.equal(continued.state.phase, "busy_started");
  const openedGate = dispatched(created.value, {
    kind: "open_gate",
    inputId: "active.gate",
    gateId: "gate.active",
  });
  assert.equal(openedGate.state.status, "idle");
  assert.equal(openedGate.state.cursor, 2);
});

function opened(seed: string) {
  const result = createZapMockModel({ seed, scenario });
  assert.equal(result.ok, true);
  if (!result.ok) throw new Error();
  return result.value;
}

function dispatched(model: ReturnType<typeof opened>, input: ZapMockInput) {
  const result = model.dispatch(input);
  assert.equal(result.ok, true);
  if (!result.ok) throw new Error();
  return result.value;
}
