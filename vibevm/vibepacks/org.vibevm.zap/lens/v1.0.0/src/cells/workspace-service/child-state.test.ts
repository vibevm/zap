/** @scope spec://org.vibevm.zap/lens/PROP-007#agent-network */
import assert from "node:assert/strict";
import test from "node:test";
import { CoordinatorEventSchema } from "../agent-runtime/index.ts";
import { childStateFromEvent } from "./child-state.ts";

test("native child turn facts advance starting to active and stopped", () => {
  const event = (kind: "turn_started" | "turn_completed", status: string) =>
    CoordinatorEventSchema.parse({
      coordinatorSessionId: "session.child-state",
      processEpoch: "epoch.1",
      nativeThreadId: "thread.child",
      nativeTurnId: "turn.child",
      nativeItemId: null,
      kind,
      sourceEventId: `event.${kind}`,
      data: { status },
    });
  assert.equal(childStateFromEvent(event("turn_started", "inProgress")), "active");
  assert.equal(childStateFromEvent(event("turn_completed", "completed")), "stopped");
  assert.equal(childStateFromEvent(event("turn_completed", "failed")), "failed");
});
