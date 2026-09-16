/** Delayed scoped-response proof. @scope spec://org.vibevm.zap/lens/PROP-010#bounded-acceptance */
import assert from "node:assert/strict";
import test from "node:test";
import { ScopedRequestFence } from "./request-fence.ts";
import { preservesInspection } from "./workspace-helpers.ts";

test("late response cannot replace a newer question or canvas scope", async () => {
  const fence = new ScopedRequestFence();
  const first = fence.begin("project.one\u0000context.one\u0000question.one");
  const delayed = Promise.resolve(first);
  const second = fence.begin("project.two\u0000context.two\u0000question.two");
  assert.equal(fence.isCurrent(await delayed), false);
  assert.equal(fence.isCurrent(second), true);
  fence.cancel();
  assert.equal(fence.isCurrent(second), false);
});

test("background refresh cannot reopen an old context in the same project", () => {
  assert.equal(preservesInspection(true, true, true), true);
  assert.equal(preservesInspection(true, true, false), false);
});
