import assert from "node:assert/strict";
import test from "node:test";

import {
  ManagedTerminalKernel,
  type ManagedTerminalFactory,
  type ManagedTerminalProcess,
} from "./index.ts";

test("managed terminal enforces one controller, epochs and bounded output gaps", async () => {
  const process = new FakeProcess();
  const factory: ManagedTerminalFactory = { spawn: async () => ({ ok: true, value: process }) };
  const kernel = new ManagedTerminalKernel(factory, 2);
  const started = await kernel.start({
    terminalId: "terminal.fixture",
    projectId: "project.fixture",
    contextId: "context.fixture",
    sessionId: "session.fixture",
    runId: "run.fixture",
    executable: "C:/placeholder/tool.exe",
    args: [],
    cwd: "C:/placeholder",
  });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const lease = kernel.acquire("terminal.fixture", "client.one", 1);
  assert.equal(lease.ok, true);
  if (!lease.ok) return;
  const denied = kernel.acquire("terminal.fixture", "client.two", 1);
  assert.equal(denied.ok, false);
  const stale = kernel.write("terminal.fixture", lease.value.leaseId, 2, "echo stale\n");
  assert.equal(stale.ok, false);
  process.emit("one\n");
  process.emit("two\n");
  process.emit("three\n");
  const snapshot = kernel.snapshot("terminal.fixture");
  assert.equal(snapshot.ok, true);
  if (snapshot.ok) {
    assert.deepEqual(
      snapshot.value.output.map((item) => item.data),
      ["two\n", "three\n"],
    );
    assert.equal(snapshot.value.lease?.clientId, "client.one");
  }
  const taken = kernel.acquire("terminal.fixture", "client.two", 1, true);
  assert.equal(taken.ok, true);
  if (taken.ok) {
    const old = kernel.write("terminal.fixture", lease.value.leaseId, 1, "old\n");
    assert.equal(old.ok, false);
    assert.equal(
      kernel.write("terminal.fixture", taken.value.leaseId, taken.value.controlEpoch, "new\n").ok,
      true,
    );
  }
  kernel.close();
  assert.equal(process.stopped, true);
});

class FakeProcess implements ManagedTerminalProcess {
  readonly processId = 101;
  stopped = false;
  lastWrite = "";
  lastSize = { columns: 0, rows: 0 };
  readonly #data = new Set<(data: string) => void>();
  readonly #exits = new Set<(code: number | null) => void>();
  onData(listener: (data: string) => void): () => void {
    this.#data.add(listener);
    return () => this.#data.delete(listener);
  }
  onExit(listener: (code: number | null) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }
  write(data: string): void {
    this.lastWrite = data;
  }
  resize(columns: number, rows: number): void {
    this.lastSize = { columns, rows };
  }
  interrupt(): void {}
  stop(): void {
    this.stopped = true;
    for (const listener of this.#exits) listener(0);
  }
  emit(data: string): void {
    for (const listener of this.#data) listener(data);
  }
}
