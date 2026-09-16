/** Provider idle wake gate proof. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import assert from "node:assert/strict";
import test from "node:test";
import { CoordinatorEventSchema } from "../agent-runtime/index.ts";
import { NativeRefSchema } from "../workspace-model/index.ts";
import { shouldWakeCoordinatorChat } from "./wake.ts";

const launch = {
  descriptor: {
    nativeThreadRef: NativeRefSchema.parse({
      namespace: "test.thread",
      value: "root.thread",
      incarnation: "1",
    }),
  },
};

function event(kind: "turn_completed" | "session_status", nativeThreadId: string | null) {
  return CoordinatorEventSchema.parse({
    coordinatorSessionId: "session.wake",
    processEpoch: "epoch.wake",
    nativeThreadId,
    nativeTurnId: null,
    nativeItemId: null,
    kind,
    sourceEventId: `event.wake.${kind}.${nativeThreadId ?? "root"}`,
    data: kind === "session_status" ? { status: "idle" } : {},
  });
}

test("wake accepts root completion and idle status, including null native thread", () => {
  assert.equal(shouldWakeCoordinatorChat(launch, event("turn_completed", "root.thread")), true);
  assert.equal(shouldWakeCoordinatorChat(launch, event("session_status", null)), true);
});

test("child completion does not wake the coordinator queue", () => {
  assert.equal(shouldWakeCoordinatorChat(launch, event("turn_completed", "child.thread")), false);
});
