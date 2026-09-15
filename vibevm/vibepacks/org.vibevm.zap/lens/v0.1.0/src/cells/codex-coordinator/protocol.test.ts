/** Installed Codex envelope fixtures. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import assert from "node:assert/strict";
import test from "node:test";
import { CodexWireMessageSchema } from "./protocol.ts";

test("accepts emittedAtMs notifications without loosening other envelopes", () => {
  assert.equal(
    CodexWireMessageSchema.safeParse({
      method: "remoteControl/status/changed",
      params: { status: "ready" },
      emittedAtMs: 1_789_470_000_000,
    }).success,
    true,
  );
  assert.equal(
    CodexWireMessageSchema.safeParse({
      method: "thread/started",
      params: {},
      emittedAtMs: "not-an-int64",
    }).success,
    false,
  );
  assert.equal(
    CodexWireMessageSchema.safeParse({ id: 1, result: {}, emittedAtMs: 1 }).success,
    false,
  );
});
