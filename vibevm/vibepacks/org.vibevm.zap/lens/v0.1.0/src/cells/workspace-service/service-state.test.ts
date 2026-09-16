/** Terminal provider error state projection. @scope spec://org.vibevm.zap/lens/PROP-012#events */
import assert from "node:assert/strict";
import test from "node:test";
import { rootSessionStatusState, rootTurnCompletionState } from "./service-state.ts";

test("canonical failed completion cannot overwrite a terminal error with ready", () => {
  assert.deepEqual(
    [
      rootSessionStatusState({ status: { type: "systemError" } }),
      rootTurnCompletionState({ status: "failed", error: { message: "terminal upstream" } }),
    ],
    ["failed", "failed"],
  );
  assert.equal(rootTurnCompletionState({ status: "interrupted" }), "ready");
  assert.equal(rootTurnCompletionState({ status: "completed" }), "ready");
});
