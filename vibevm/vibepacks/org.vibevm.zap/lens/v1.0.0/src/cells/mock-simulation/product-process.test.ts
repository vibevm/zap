/** Product runner process-tree timeout proof. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { runOwnedProductProcess } from "./product-process.ts";

test("timeout terminates the exact owned product runner tree", async () => {
  const result = await runOwnedProductProcess({
    root: resolve(import.meta.dirname, "../../.."),
    testPath: resolve(import.meta.dirname, "timeout-product.fixture.ts"),
    environment: safeEnvironment(),
    timeoutMs: 500,
    readyMarker: "OWNED_DESCENDANT_PID ",
    startupTimeoutMs: 15_000,
  });
  assert.equal(result.readyObserved, true);
  assert.equal(result.timedOut, true);
  assert.equal(result.exactTreeTerminated, true);
  assert.equal(result.startError, false);
  const match = /OWNED_DESCENDANT_PID ([0-9]+)/.exec(result.output);
  assert.notEqual(match, null);
  if (match === null) return;
  const pid = Number(match[1]);
  assert.equal(await eventuallyStopped(pid), true);
});

async function eventuallyStopped(pid: number): Promise<boolean> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (!isRunning(pid)) return true;
    await new Promise((resolveWait) => setTimeout(resolveWait, 20));
  }
  return !isRunning(pid);
}

function isRunning(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function safeEnvironment(): Record<string, string> {
  const allowed = new Set(["SystemRoot", "WINDIR", "PATH", "Path", "PATHEXT", "TEMP", "TMP"]);
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined && allowed.has(entry[0]),
    ),
  );
}
