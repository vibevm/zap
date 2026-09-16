/** Installed Codex envelope fixtures. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import assert from "node:assert/strict";
import test from "node:test";
import { CodexErrorNotificationSchema, CodexWireMessageSchema } from "./protocol.ts";

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

test("validates the installed app-server error notification without retaining private details", () => {
  const parsed = CodexErrorNotificationSchema.parse({
    method: "error",
    params: {
      error: {
        message: "upstream unavailable",
        codexErrorInfo: { httpConnectionFailed: { httpStatusCode: 503 } },
        additionalDetails: "not projected",
        misalignment: null,
      },
      willRetry: true,
      threadId: "thread.fixture",
      turnId: "turn.fixture",
    },
  });
  assert.equal(parsed.params.willRetry, true);
  assert.deepEqual(parsed.params.error.codexErrorInfo, {
    httpConnectionFailed: { httpStatusCode: 503 },
  });
  assert.equal(
    CodexErrorNotificationSchema.safeParse({
      ...parsed,
      params: { ...parsed.params, willRetry: "yes" },
    }).success,
    false,
  );
});
