/** Owned Codex app-server plus remote TUI managed control. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import { randomUUID } from "node:crypto";
import { mkdirSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { z } from "zod";
import type {
  ManagedControlTarget,
  ManagedProviderControlAdapter,
  ManagedSessionControlEvent,
} from "./control.ts";
import type { ManagedWorkResult } from "./contracts.ts";
import type { ProviderLaunch } from "./providers.ts";
import {
  nodeManagedControlProcessFactory,
  reserveLoopbackPort,
  type ManagedControlProcess,
  type ManagedControlProcessFactory,
} from "./native-control-process.ts";
import { connectManagedJsonRpc, type ManagedJsonRpcClient } from "./websocket-rpc.ts";

interface CodexState {
  readonly target: ManagedControlTarget;
  readonly publish: (event: Omit<ManagedSessionControlEvent, "sourceSequence">) => void;
  readonly process: ManagedControlProcess;
  readonly rpc: ManagedJsonRpcClient;
  readonly tokenPath: string;
  readonly activeTurns: Map<string, string>;
  readonly childThreads: Set<string>;
  unsubscribe: () => void;
  rootThreadId: string | null;
}

export function createCodexManagedControlAdapter(options: {
  readonly directory: string;
  readonly processFactory?: ManagedControlProcessFactory;
  readonly reservePort?: () => Promise<number>;
  readonly token?: () => string;
  readonly connect?: typeof connectManagedJsonRpc;
  readonly timeoutMs?: number;
}): ManagedProviderControlAdapter {
  mkdirSync(options.directory, { recursive: true });
  const factory = options.processFactory ?? nodeManagedControlProcessFactory();
  const reservePort = options.reservePort ?? reserveLoopbackPort;
  const token = options.token ?? (() => randomUUID());
  const connect = options.connect ?? connectManagedJsonRpc;
  const records = new WeakMap<object, CodexState>();
  return {
    provider: "codex",
    async prepare(input) {
      const parsed = parseLaunch(input.launch);
      if (!parsed.ok) return parsed;
      const port = await reservePort();
      const url = `ws://127.0.0.1:${String(port)}`;
      const capability = token();
      const tokenPath = join(options.directory, `codex.${digest(input.target.runId)}.token`);
      writeFileSync(tokenPath, capability, { encoding: "utf8", mode: 0o600 });
      const process = factory.spawn({
        executable: input.launch.executable,
        args: [
          ...parsed.value.prefix,
          "app-server",
          "--listen",
          url,
          "--ws-auth",
          "capability-token",
          "--ws-token-file",
          tokenPath,
        ],
        cwd: input.launch.cwd,
        env: input.launch.env,
      });
      const connected = await waitConnect(connect, url, capability, options.timeoutMs ?? 10_000);
      if (!connected.ok) {
        process.kill();
        removeToken(tokenPath);
        return connected;
      }
      const initialized = await connected.value.request("initialize", {
        clientInfo: { name: "zap-wayfinder", title: "Zap Wayfinder", version: "0.1.0" },
        capabilities: { experimentalApi: true },
      });
      if (!initialized.ok) {
        connected.value.close();
        process.kill();
        removeToken(tokenPath);
        return initialized;
      }
      const acknowledged = connected.value.notify("initialized", {});
      if (!acknowledged.ok) {
        connected.value.close();
        process.kill();
        removeToken(tokenPath);
        return acknowledged;
      }
      const state: CodexState = {
        target: input.target,
        publish: input.publish,
        process,
        rpc: connected.value,
        tokenPath,
        activeTurns: new Map(),
        childThreads: new Set(),
        unsubscribe: () => undefined,
        rootThreadId: null,
      };
      const key = {};
      records.set(key, state);
      state.unsubscribe = state.rpc.subscribe((message) => {
        observe(state, message);
      });
      process.onExit(() => {
        publish(state, "session_exited", null, null);
      });
      const tokenName = `ZAP_CODEX_REMOTE_TOKEN_${digest(input.target.runId).toUpperCase()}`;
      return {
        ok: true,
        value: {
          state: key,
          launch: {
            ...input.launch,
            args: injectRemote(parsed.value.tuiArgs, url, tokenName),
            env: { ...input.launch.env, [tokenName]: capability },
          },
        },
      };
    },
    async interrupt(raw) {
      const found = record(records, raw);
      if (!found.ok) return found;
      if (found.value.activeTurns.size === 0) return { ok: true, value: "idle" };
      let accepted = false;
      for (const [threadId, turnId] of found.value.activeTurns) {
        const result = await found.value.rpc.request("turn/interrupt", { threadId, turnId });
        if (!result.ok) return result;
        accepted = true;
      }
      return { ok: true, value: accepted ? "requested" : "idle" };
    },
    async offer(raw, deliveryId, bodyMarkdown, io) {
      await Promise.resolve();
      const found = record(records, raw);
      if (!found.ok) return found;
      if (found.value.activeTurns.size > 0)
        return fail("conflict", "Codex managed session is busy");
      const encoded = terminalInput(bodyMarkdown);
      if (!encoded.ok) return encoded;
      const sent = io.input(encoded.value);
      return sent.ok ? { ok: true, value: { queued: false, correlation: deliveryId } } : sent;
    },
    close(raw) {
      const found = record(records, raw);
      if (!found.ok) return;
      found.value.unsubscribe();
      found.value.rpc.close();
      found.value.process.kill();
      removeToken(found.value.tokenPath);
      if (typeof raw === "object" && raw !== null) records.delete(raw);
    },
  };
}

function parseLaunch(launch: ProviderLaunch): ManagedWorkResult<{
  readonly prefix: readonly string[];
  readonly tuiArgs: readonly string[];
}> {
  const modelAt = launch.args.indexOf("-m");
  if (modelAt < 0) return fail("invalid_input", "Codex managed launch omitted its pinned model");
  return {
    ok: true,
    value: { prefix: launch.args.slice(0, modelAt), tuiArgs: launch.args },
  };
}

function injectRemote(args: readonly string[], url: string, tokenName: string): readonly string[] {
  const resumeAt = args.indexOf("resume");
  const insertAt = resumeAt < 0 ? Math.max(0, args.length - 1) : resumeAt;
  return [
    ...args.slice(0, insertAt),
    "--remote",
    url,
    "--remote-auth-token-env",
    tokenName,
    ...args.slice(insertAt),
  ];
}

async function waitConnect(
  connect: typeof connectManagedJsonRpc,
  url: string,
  bearerToken: string,
  timeoutMs: number,
): Promise<ManagedWorkResult<ManagedJsonRpcClient>> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const connected = await connect({ url, bearerToken, timeoutMs: Math.min(500, timeoutMs) });
    if (connected.ok) return connected;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return fail("unavailable", "owned Codex app-server did not become ready");
}

function observe(state: CodexState, raw: unknown): void {
  const request = z
    .looseObject({
      id: z.union([z.string(), z.number().int()]),
      method: z.enum([
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
        "item/tool/requestUserInput",
      ]),
      params: z.record(z.string(), z.unknown()),
    })
    .safeParse(raw);
  if (request.success) {
    const threadId = request.data.params["threadId"];
    if (typeof threadId === "string" && ownedThread(state, threadId)) {
      const turnId =
        typeof request.data.params["turnId"] === "string" ? request.data.params["turnId"] : null;
      publish(state, "permission_required", turnId, String(request.data.id));
    }
    return;
  }
  const notification = z
    .looseObject({ method: z.string(), params: z.record(z.string(), z.unknown()) })
    .safeParse(raw);
  if (!notification.success) return;
  const params = notification.data.params;
  if (notification.data.method === "thread/started") {
    const thread = z
      .looseObject({ id: z.string().min(1), parentThreadId: z.string().nullable().optional() })
      .safeParse(params["thread"]);
    if (!thread.success) return;
    if (thread.data.parentThreadId === null || thread.data.parentThreadId === undefined) {
      if (state.rootThreadId === null) state.rootThreadId = thread.data.id;
      if (state.rootThreadId === thread.data.id) publish(state, "session_ready", null, null);
    } else if (
      thread.data.parentThreadId === state.rootThreadId ||
      state.childThreads.has(thread.data.parentThreadId)
    ) {
      state.childThreads.add(thread.data.id);
    }
    return;
  }
  const threadId = typeof params["threadId"] === "string" ? params["threadId"] : null;
  if (threadId === null || !ownedThread(state, threadId)) return;
  if (notification.data.method === "turn/started") {
    const turnId = turn(params);
    if (turnId === null) return;
    state.activeTurns.set(threadId, turnId);
    publish(state, "turn_started", turnId, null);
  } else if (notification.data.method === "turn/completed") {
    const turnId = turn(params);
    if (turnId === null) return;
    if (state.activeTurns.get(threadId) === turnId) state.activeTurns.delete(threadId);
    if (state.activeTurns.size === 0) publish(state, "turn_settled", turnId, null);
  } else if (notification.data.method === "thread/status/changed") {
    const status = z.looseObject({ type: z.string() }).safeParse(params["status"]);
    if (status.success && status.data.type === "idle" && state.activeTurns.size === 0)
      publish(state, "turn_settled", null, null);
  } else if (notification.data.method === "item/started") {
    observeChildItem(state, params["item"]);
  }
}

function observeChildItem(state: CodexState, raw: unknown): void {
  const item = z
    .looseObject({
      type: z.literal("collabAgentToolCall"),
      receiverThreadIds: z.array(z.string().min(1)),
    })
    .safeParse(raw);
  if (!item.success) return;
  for (const threadId of item.data.receiverThreadIds) state.childThreads.add(threadId);
}

function turn(params: Record<string, unknown>): string | null {
  const parsed = z.looseObject({ id: z.string().min(1) }).safeParse(params["turn"]);
  return parsed.success ? parsed.data.id : null;
}

function ownedThread(state: CodexState, threadId: string): boolean {
  return threadId === state.rootThreadId || state.childThreads.has(threadId);
}

function terminalInput(bodyMarkdown: string): ManagedWorkResult<string> {
  const normalized = bodyMarkdown.replaceAll("\r\n", "\n");
  if (Array.from(normalized).some(terminalControl))
    return fail("invalid_input", "managed Codex notice contains terminal control characters");
  return { ok: true, value: `\u001b[200~${normalized}\u001b[201~\r` };
}

function terminalControl(character: string): boolean {
  const code = character.charCodeAt(0);
  return code === 13 || code === 127 || code < 9 || (code > 10 && code < 32);
}

function publish(
  state: CodexState,
  kind: ManagedSessionControlEvent["kind"],
  providerTurnId: string | null,
  correlation: string | null,
): void {
  state.publish({
    eventId: `codex-control.${randomUUID().replaceAll("-", "")}`,
    runId: state.target.runId,
    actorId: state.target.actorId,
    sessionId: state.target.sessionId,
    terminalId: state.target.terminalId,
    provider: "codex",
    processEpoch: state.target.expectedProcessEpoch,
    kind,
    providerSessionId: state.rootThreadId,
    providerTurnId,
    transportCorrelation: correlation,
    occurredAt: new Date().toISOString(),
  });
}

function record(records: WeakMap<object, CodexState>, raw: unknown): ManagedWorkResult<CodexState> {
  if (typeof raw !== "object" || raw === null)
    return fail("conflict", "Codex managed control state is stale");
  const state = records.get(raw);
  return state === undefined
    ? fail("conflict", "Codex managed control state is stale")
    : { ok: true, value: state };
}

function digest(value: string): string {
  return Buffer.from(value).toString("hex").slice(0, 40).padEnd(8, "0");
}

function removeToken(path: string): void {
  try {
    unlinkSync(path);
  } catch {
    // The sidecar may already have removed or never opened the token file.
  }
}

function fail(
  code: "invalid_input" | "conflict" | "uncertain" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
