/** @verifies spec://org.vibevm.zap/lens/PROP-002#interaction */
import assert from "node:assert/strict";
import test from "node:test";

import { ConversationIdSchema, DecimalSchema, WorkspaceIdSchema } from "../protocol/index.ts";
import { InvalidationMonitor } from "./invalidation.ts";

const workspaceId = WorkspaceIdSchema.parse("workspace.invalidation");
const conversationId = ConversationIdSchema.parse("conversation.invalidation");

test("first subscriber observes a question arriving before its first poll completes", async () => {
  let release = (): void => undefined;
  const events = new Promise<EventResult>((resolve) => {
    release = () => resolve(questionEvents("1"));
  });
  const monitor = monitorWith(() => events);
  const received = new Promise<string>((resolve) => {
    monitor.start(resolve);
  });
  release();
  assert.equal(await received, "questions");
  monitor.stop();
});

test("rapid stop and restart fences the old poll and schedules only the new generation", async () => {
  const pending: Array<(value: EventResult) => void> = [];
  let calls = 0;
  const monitor = monitorWith(
    () =>
      new Promise<EventResult>((resolve) => {
        calls += 1;
        pending.push(resolve);
      }),
  );
  const first: string[] = [];
  const second: string[] = [];
  monitor.start((reason) => first.push(reason));
  await until(() => pending.length === 1);
  monitor.stop();
  monitor.start((reason) => second.push(reason));
  await until(() => pending.length === 2);
  pending[0]?.(questionEvents("1"));
  pending[1]?.(questionEvents("1"));
  await until(() => second.length === 1);
  monitor.stop();
  await new Promise((resolve) => setTimeout(resolve, 60));
  assert.deepEqual(first, []);
  assert.deepEqual(second, ["questions"]);
  assert.equal(calls, 2);
});

type EventResult = {
  readonly ok: true;
  readonly value: {
    readonly observationCursor: ReturnType<typeof DecimalSchema.parse>;
    readonly events: readonly { readonly kind: string }[];
  };
};

function questionEvents(cursor: string): EventResult {
  return {
    ok: true,
    value: {
      observationCursor: DecimalSchema.parse(cursor),
      events: [{ kind: "question.created" }],
    },
  };
}

function monitorWith(events: () => Promise<EventResult>): InvalidationMonitor {
  return new InvalidationMonitor({
    broker: { events },
    zap: {
      activeContext: async () => ({ ok: true, value: { revision: "7" } }),
    },
    workspaceId,
    conversationId,
    intervalMilliseconds: 25,
  });
}

async function until(condition: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  assert.fail(
    "violates REQ spec://org.vibevm.zap/lens/PROP-002#interaction: monitor condition did not become observable; fix surface: preserve bounded invalidation progress",
  );
}
