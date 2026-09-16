/** No-model Codex/OpenCode managed sidecar proof. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ActorIdSchema } from "../protocol/index.ts";
import { AgentSessionIdSchema, RunIdSchema, TerminalIdSchema } from "../workspace-model/index.ts";
import type { ManagedControlTarget, ManagedSessionControlEvent } from "./control.ts";
import { createCodexManagedControlAdapter } from "./codex-control.ts";
import type {
  ManagedControlProcess,
  ManagedControlProcessFactory,
} from "./native-control-process.ts";
import { createOpenCodeManagedControlAdapter } from "./opencode-control.ts";
import type { ProviderLaunch } from "./providers.ts";
import type { ManagedJsonRpcClient } from "./websocket-rpc.ts";

test("Codex managed sidecar keeps the real TUI, interrupts root and child, and leaves approval on TUI", async () => {
  const directory = await mkdtemp(join(tmpdir(), "zap-codex-control-"));
  try {
    const process = new FixtureProcess(8101);
    const launches: ProviderLaunch[] = [];
    const processFactory: ManagedControlProcessFactory = {
      spawn(input) {
        launches.push(input);
        return process;
      },
    };
    const rpc = new FixtureRpc();
    const adapter = createCodexManagedControlAdapter({
      directory,
      processFactory,
      reservePort: () => Promise.resolve(43111),
      token: () => "fixture-capability-token",
      connect: () => Promise.resolve({ ok: true, value: rpc }),
    });
    const events: Array<Omit<ManagedSessionControlEvent, "sourceSequence">> = [];
    const target = fixtureTarget("codex");
    const prepared = await adapter.prepare({
      target,
      launch: {
        executable: "C:/fixture/node.exe",
        args: [
          "C:/fixture/codex.js",
          "-m",
          "gpt-fixture",
          "-c",
          'model_reasoning_effort="low"',
          "read packet",
        ],
        cwd: "C:/fixture/project",
        env: { FIXTURE: "1" },
      },
      publish: (event) => events.push(event),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    assert.deepEqual(launches[0]?.args.slice(0, 4), [
      "C:/fixture/codex.js",
      "app-server",
      "--listen",
      "ws://127.0.0.1:43111",
    ]);
    assert.equal(launches[0]?.args.includes("--ws-auth"), true);
    assert.equal(prepared.value.launch.args.includes("--remote"), true);
    assert.equal(prepared.value.launch.args.at(-1), "read packet");
    const tokenName = prepared.value.launch.args.at(
      prepared.value.launch.args.indexOf("--remote-auth-token-env") + 1,
    );
    assert.equal(
      tokenName === undefined ? undefined : prepared.value.launch.env[tokenName],
      "fixture-capability-token",
    );
    rpc.emit({
      method: "thread/started",
      params: { thread: { id: "thread.root", parentThreadId: null } },
    });
    rpc.emit({
      method: "turn/started",
      params: { threadId: "thread.root", turn: { id: "turn.root" } },
    });
    rpc.emit({
      method: "thread/started",
      params: { thread: { id: "thread.child", parentThreadId: "thread.root" } },
    });
    rpc.emit({
      method: "turn/started",
      params: { threadId: "thread.child", turn: { id: "turn.child" } },
    });
    const interrupted = await adapter.interrupt(
      prepared.value.state,
      "project_pause",
      fixtureIo([]),
    );
    assert.equal(interrupted.ok && interrupted.value, "requested");
    assert.deepEqual(
      rpc.requests
        .filter((entry) => entry.method === "turn/interrupt")
        .map((entry) => entry.params),
      [
        { threadId: "thread.root", turnId: "turn.root" },
        { threadId: "thread.child", turnId: "turn.child" },
      ],
    );
    rpc.emit({
      method: "turn/completed",
      params: { threadId: "thread.root", turn: { id: "turn.root" } },
    });
    assert.equal(
      events.some((event) => event.kind === "turn_settled"),
      false,
    );
    rpc.emit({
      method: "turn/completed",
      params: { threadId: "thread.child", turn: { id: "turn.child" } },
    });
    assert.equal(
      events.some((event) => event.kind === "turn_settled"),
      true,
    );
    const terminalWrites: string[] = [];
    const offered = await adapter.offer(
      prepared.value.state,
      "delivery.fixture",
      "continue safely",
      fixtureIo(terminalWrites),
    );
    assert.equal(offered.ok && offered.value.correlation, "delivery.fixture");
    assert.match(terminalWrites[0] ?? "", /continue safely/);
    assert.equal(
      rpc.requests.some((entry) => entry.method === "turn/start"),
      false,
    );
    rpc.emit({
      id: 71,
      method: "item/commandExecution/requestApproval",
      params: { threadId: "thread.root", turnId: "turn.next", itemId: "item.command" },
    });
    assert.equal(events.at(-1)?.kind, "permission_required");
    adapter.close(prepared.value.state);
    assert.equal(process.killed, true);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("OpenCode managed sidecar attaches the TUI before exact-model prompts and observes permission", async () => {
  const process = new FixtureProcess(8201);
  const launches: ProviderLaunch[] = [];
  const requests: Array<{ readonly path: string; readonly body: unknown }> = [];
  let controller: ReadableStreamDefaultController<Uint8Array> | undefined;
  const stream = new ReadableStream<Uint8Array>({
    start(next) {
      controller = next;
    },
  });
  const fetchImpl: typeof fetch = async (raw, init) => {
    const path = new URL(raw instanceof Request ? raw.url : raw.toString()).pathname;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : null;
    requests.push({ path, body });
    if (path === "/doc") return Response.json({ openapi: "3.1.0" });
    if (path === "/session") return Response.json({ id: "session.opencode.fixture" });
    if (path === "/event") return new Response(stream, { status: 200 });
    if (path.endsWith("/prompt_async")) return new Response(null, { status: 204 });
    if (path.endsWith("/abort")) return Response.json(true);
    return Response.json({});
  };
  const adapter = createOpenCodeManagedControlAdapter({
    processFactory: {
      spawn(input) {
        launches.push(input);
        return process;
      },
    },
    fetchImpl,
    reservePort: () => Promise.resolve(43112),
    password: () => "fixture-server-password",
  });
  const events: Array<Omit<ManagedSessionControlEvent, "sourceSequence">> = [];
  const prepared = await adapter.prepare({
    target: fixtureTarget("opencode"),
    launch: {
      executable: "C:/fixture/opencode.exe",
      args: [
        "C:/fixture/opencode-prefix",
        "C:/fixture/project",
        "--model",
        "fixture/luna",
        "--prompt",
        "read packet",
      ],
      cwd: "C:/fixture/project",
      env: { OPENCODE_CONFIG: "C:/fixture/generated.json" },
    },
    publish: (event) => events.push(event),
  });
  assert.equal(prepared.ok, true);
  if (!prepared.ok) return;
  assert.deepEqual(launches[0]?.args, [
    "C:/fixture/opencode-prefix",
    "serve",
    "--pure",
    "--hostname",
    "127.0.0.1",
    "--port",
    "43112",
  ]);
  assert.deepEqual(prepared.value.launch.args.slice(0, 5), [
    "C:/fixture/opencode-prefix",
    "attach",
    "http://127.0.0.1:43112",
    "--session",
    "session.opencode.fixture",
  ]);
  assert.equal(prepared.value.launch.args.includes("fixture-server-password"), false);
  assert.equal(prepared.value.launch.env["OPENCODE_SERVER_PASSWORD"], "fixture-server-password");
  const activated = await adapter.activate?.(prepared.value.state, fixtureIo([]));
  assert.equal(activated?.ok, true);
  const initial = requests.find((entry) => entry.path.endsWith("/prompt_async"));
  assert.deepEqual(initial?.body, {
    messageID: "initial.run.managed.opencode",
    model: { providerID: "fixture", modelID: "luna" },
    parts: [{ type: "text", text: "read packet" }],
  });
  controller?.enqueue(
    new TextEncoder().encode(
      `data: ${JSON.stringify({
        type: "permission.asked",
        properties: { sessionID: "session.opencode.fixture", id: "permission.fixture" },
      })}\n\n`,
    ),
  );
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(events.at(-1)?.kind, "permission_required");
  controller?.enqueue(
    new TextEncoder().encode(
      `data: ${JSON.stringify({
        type: "session.idle",
        properties: { sessionID: "session.opencode.fixture" },
      })}\n\n`,
    ),
  );
  await new Promise((resolve) => setImmediate(resolve));
  const offered = await adapter.offer(
    prepared.value.state,
    "delivery.opencode.fixture",
    "next packet",
    fixtureIo([]),
  );
  assert.equal(offered.ok && offered.value.correlation, "delivery.opencode.fixture");
  const interrupted = await adapter.interrupt(
    prepared.value.state,
    "user_interrupt",
    fixtureIo([]),
  );
  assert.equal(interrupted.ok && interrupted.value, "requested");
  assert.equal(
    requests.some((entry) => entry.path.endsWith("/abort")),
    true,
  );
  adapter.close(prepared.value.state);
  assert.equal(process.killed, true);
});

function fixtureTarget(suffix: string): ManagedControlTarget {
  return {
    runId: RunIdSchema.parse(`run.managed.${suffix}`),
    actorId: ActorIdSchema.parse(`actor.managed.${suffix}`),
    sessionId: AgentSessionIdSchema.parse(`session.managed.${suffix}`),
    terminalId: TerminalIdSchema.parse(`terminal.managed.${suffix}`),
    expectedProcessEpoch: `process.managed.${suffix}`,
  };
}

function fixtureIo(writes: string[]) {
  return {
    input(data: string) {
      writes.push(data);
      return { ok: true as const, value: undefined };
    },
    interrupt: () => ({ ok: true as const, value: undefined }),
    stop: () => ({ ok: true as const, value: undefined }),
  };
}

class FixtureProcess implements ManagedControlProcess {
  readonly pid: number;
  killed = false;
  #listener: ((code: number | null) => void) | undefined;
  constructor(pid: number) {
    this.pid = pid;
  }
  onExit(listener: (code: number | null) => void): () => void {
    this.#listener = listener;
    return () => {
      if (this.#listener === listener) this.#listener = undefined;
    };
  }
  kill(): void {
    this.killed = true;
    this.#listener?.(0);
  }
}

class FixtureRpc implements ManagedJsonRpcClient {
  readonly requests: Array<{ readonly method: string; readonly params: unknown }> = [];
  readonly #listeners = new Set<(message: unknown) => void>();
  request(method: string, params: unknown) {
    this.requests.push({ method, params });
    return Promise.resolve({ ok: true as const, value: {} });
  }
  notify(method: string, params: unknown) {
    this.requests.push({ method, params });
    return { ok: true as const, value: undefined };
  }
  subscribe(listener: (message: unknown) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }
  emit(message: unknown): void {
    for (const listener of this.#listeners) listener(message);
  }
  close(): void {
    this.#listeners.clear();
  }
}
