import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

import {
  ManagedTerminalKernel,
  type ManagedTerminalFactory,
  type ManagedTerminalProcess,
} from "../managed-terminal/index.ts";
import { createManagedTerminalService } from "./index.ts";
import { openManagedTerminalOutputStore } from "../managed-terminal-store/index.ts";
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";

test("managed terminal service refuses cross-project reads and control", async () => {
  const service = createManagedTerminalService(new ManagedTerminalKernel(fakeFactory()), [
    registration("project.a", "context.a", "terminal.a"),
    registration("project.b", "context.b", "terminal.b"),
  ]);
  await service.start(registration("project.a", "context.a", "terminal.a"));
  await service.start(registration("project.b", "context.b", "terminal.b"));
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.a"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.a"),
    authorizedProjectIds: [ProjectIdSchema.parse("project.a")],
  });
  const read = service.read(access, {
    projectId: "project.a",
    contextId: "context.a",
    terminalId: "terminal.b",
    afterSequence: 0,
    limit: 20,
  });
  assert.equal(read.ok, false);
  const acquire = service.acquire(access, {
    terminalId: "terminal.b",
    expectedControlEpoch: 1,
  });
  assert.equal(acquire.ok, false);
  const listed = service.list(access, "project.a", "context.a");
  assert.equal(listed.ok, true);
  if (listed.ok)
    assert.deepEqual(
      listed.value.map((terminal) => terminal.terminalId),
      ["terminal.a"],
    );
  assert.equal(service.list(access, "project.b", "context.b").ok, false);
  service.close();
});

test("managed terminal lifecycle remains durable without restoring a false running process", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "terminal-events-"));
  const databasePath = join(directory, "terminal.sqlite");
  const factory = new RecordingFactory();
  const store = openManagedTerminalOutputStore(databasePath, 2);
  const service = createManagedTerminalService(new ManagedTerminalKernel(factory), [], store);
  const accessA = access("client.a");
  const accessB = access("client.b");
  const started = await service.start(registration("project.a", "context.a", "terminal.a"));
  assert.equal(started.ok, true);
  factory.process.emitData("one");
  factory.process.emitData("two");
  factory.process.emitData("three");
  const bounded = service.read(accessA, {
    projectId: "project.a",
    contextId: "context.a",
    terminalId: "terminal.a",
    afterSequence: 0,
    limit: 20,
  });
  assert.equal(bounded.ok, true);
  if (bounded.ok) {
    assert.deepEqual(
      bounded.value.events.map((event) => event.data),
      ["two", "three"],
    );
    assert.equal(bounded.value.gap?.firstAvailableSequence, 2);
  }
  const first = service.acquire(accessA, {
    terminalId: "terminal.a",
    expectedControlEpoch: 1,
  });
  assert.equal(first.ok, true);
  if (!first.ok || first.value.lease === null) return;
  const takeover = service.acquire(accessB, {
    terminalId: "terminal.a",
    expectedControlEpoch: first.value.controlEpoch,
    takeover: true,
  });
  assert.equal(takeover.ok, true);
  if (!takeover.ok || takeover.value.lease === null) return;
  const lease = takeover.value.lease;
  assert.equal(
    service.input(accessB, "terminal.a", lease.leaseId, lease.controlEpoch, "echo\r").ok,
    true,
  );
  assert.equal(
    service.resize(accessB, "terminal.a", lease.leaseId, lease.controlEpoch, 100, 30).ok,
    true,
  );
  assert.equal(
    service.interrupt(accessB, "terminal.a", lease.leaseId, lease.controlEpoch).ok,
    true,
  );
  assert.equal(service.stop(accessB, "terminal.a", lease.leaseId, lease.controlEpoch).ok, true);
  factory.process.emitExit(0);
  service.close();

  const reopened = openManagedTerminalOutputStore(databasePath);
  const events = reopened.lifecycle("terminal.a");
  assert.deepEqual(
    events.map((event) => event.operation),
    ["start", "acquire", "takeover", "input", "resize", "interrupt", "stop", "exit"],
  );
  assert.equal(events.find((event) => event.operation === "input")?.inputLength, 5);
  const restarted = createManagedTerminalService(
    new ManagedTerminalKernel(new RecordingFactory()),
    [],
    reopened,
  );
  assert.equal(restarted.snapshot(accessA, "terminal.a").ok, false);
  const recovered = restarted.list(accessA, "project.a", "context.a");
  assert.equal(recovered.ok, true);
  if (recovered.ok) {
    assert.equal(recovered.value[0]?.terminalId, "terminal.a");
    assert.equal(recovered.value[0]?.state, "exited");
  }
  restarted.close();
  rmSync(directory, { recursive: true, force: true });
});

function registration(projectId: string, contextId: string, terminalId: string) {
  return {
    accessProjectId: projectId,
    accessContextId: contextId,
    spec: {
      terminalId,
      projectId,
      contextId,
      sessionId: "session.terminal",
      runId: "run.terminal",
      executable: "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
      args: [],
      cwd: "C:/Windows",
    },
  };
}

function fakeFactory(): ManagedTerminalFactory {
  return { spawn: async () => ({ ok: true, value: new FakeProcess() }) };
}

function access(clientId: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.a"),
    actorId: null,
    clientId: ClientIdSchema.parse(clientId),
    authorizedProjectIds: [ProjectIdSchema.parse("project.a")],
  });
}

class RecordingFactory implements ManagedTerminalFactory {
  readonly process = new RecordingProcess();

  spawn(): Promise<{ readonly ok: true; readonly value: ManagedTerminalProcess }> {
    return Promise.resolve({ ok: true, value: this.process });
  }
}

class RecordingProcess implements ManagedTerminalProcess {
  readonly processId = 102;
  #exit: ((code: number | null) => void) | undefined;
  #data: ((data: string) => void) | undefined;

  onData(listener: (data: string) => void): () => void {
    this.#data = listener;
    return () => {
      this.#data = undefined;
    };
  }
  onExit(listener: (code: number | null) => void): () => void {
    this.#exit = listener;
    return () => {
      this.#exit = undefined;
    };
  }
  emitExit(code: number | null): void {
    this.#exit?.(code);
  }
  emitData(data: string): void {
    this.#data?.(data);
  }
  write(): void {}
  resize(): void {}
  interrupt(): void {}
  stop(): void {}
}

class FakeProcess implements ManagedTerminalProcess {
  readonly processId = 103;
  onData(): () => void {
    return () => undefined;
  }
  onExit(): () => void {
    return () => undefined;
  }
  write(): void {}
  resize(): void {}
  interrupt(): void {}
  stop(): void {}
}
