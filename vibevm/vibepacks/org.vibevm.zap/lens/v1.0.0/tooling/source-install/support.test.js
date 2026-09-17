/** Process-boundary regressions. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { createCommandRunner } from "./support.mjs";

test("command runner turns stdout and stderr stream errors into one bounded failure", async (t) => {
  for (const channel of ["stdout", "stderr"]) {
    await t.test(channel, async () => {
      const child = fakeChild();
      const runner = createCommandRunner({ spawn: () => child });
      const running = runner.run(command());
      child[channel].emit("error", new Error(`fixture ${channel} disconnected`));
      child.emit("close", 1, null);
      await assert.rejects(
        running,
        new RegExp(`command ${channel} stream failed: fixture ${channel} disconnected`, "u"),
      );
      assert.equal(child.kills, 1);
    });
  }
});

test("command runner settles on close after streams drain", async () => {
  const child = fakeChild();
  const runner = createCommandRunner({ spawn: () => child });
  let settled = false;
  const running = runner.run(command()).finally(() => {
    settled = true;
  });
  child.stdout.emit("data", "complete output");
  child.emit("exit", 0, null);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, false);
  child.emit("close", 0, null);
  assert.deepEqual(await running, {
    code: 0,
    signal: null,
    stdout: "complete output",
    stderr: "",
  });
  child.stdout.emit("error", new Error("late ignored error"));
  assert.equal(child.kills, 0);
});

function command() {
  return { executable: "fixture", args: [], cwd: ".", environment: {} };
}

function fakeChild() {
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kills = 0;
  child.kill = () => {
    child.kills += 1;
  };
  return child;
}
