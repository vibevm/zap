/** Owned Qwen Code stream-JSON transport. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import type {
  CoordinatorResumeInput,
  CoordinatorStartInput,
  HostRequestAnswer,
} from "../agent-runtime/index.ts";
import { resolveProxyEnvironment, type ProxyPolicy } from "../proxy-policy/index.ts";
import type {
  ProviderCoordinatorProfile,
  ProviderCoordinatorSession,
  ProviderCoordinatorTransport,
  ProviderCoordinatorTransportFactory,
  ProviderLaunchPreparationPort,
} from "./index.ts";
import type { OwnedLineProcess } from "./claude.ts";
import { qwenZapMcpPermissionArguments } from "./qwen-permissions.ts";

export interface QwenProcessFactory {
  spawn(input: {
    readonly executablePath: string;
    readonly args: readonly string[];
    readonly cwd: string;
    readonly environment: Readonly<Record<string, string>>;
  }): OwnedLineProcess;
}

export function createQwenStreamJsonTransportFactory(
  options: {
    readonly processFactory?: QwenProcessFactory;
    readonly timeoutMs?: number;
    readonly proxyPolicy?: ProxyPolicy;
    readonly prepareLaunch?: ProviderLaunchPreparationPort;
  } = {},
): ProviderCoordinatorTransportFactory {
  return {
    open: (profile) =>
      Promise.resolve({
        ok: true as const,
        value: new QwenStreamJsonTransport(
          profile,
          options.processFactory ?? nodeProcessFactory(),
          options.timeoutMs ?? 10_000,
          options.proxyPolicy,
          options.prepareLaunch,
        ),
      }),
  };
}

class QwenStreamJsonTransport implements ProviderCoordinatorTransport {
  readonly #profile: ProviderCoordinatorProfile;
  readonly #factory: QwenProcessFactory;
  readonly #timeoutMs: number;
  readonly #proxyPolicy: ProxyPolicy | undefined;
  readonly #prepareLaunch: ProviderLaunchPreparationPort | undefined;
  readonly #listeners = new Set<(raw: unknown) => void>();
  readonly #history: unknown[] = [];
  #process: OwnedLineProcess | undefined;
  #session: ProviderCoordinatorSession | undefined;
  #unsubscribeLine: (() => void) | undefined;
  #unsubscribeExit: (() => void) | undefined;

  constructor(
    profile: ProviderCoordinatorProfile,
    factory: QwenProcessFactory,
    timeoutMs: number,
    proxyPolicy: ProxyPolicy | undefined,
    prepareLaunch: ProviderLaunchPreparationPort | undefined,
  ) {
    this.#profile = profile;
    this.#factory = factory;
    this.#timeoutMs = timeoutMs;
    this.#proxyPolicy = proxyPolicy;
    this.#prepareLaunch = prepareLaunch;
  }

  start(input: { readonly scope: CoordinatorStartInput }) {
    return this.#launch(input.scope, undefined, input.scope.bootstrapText);
  }

  resume(input: { readonly scope: CoordinatorResumeInput }) {
    return this.#launch(input.scope, input.scope.nativeThreadId, undefined);
  }

  async #launch(
    scope: CoordinatorStartInput | CoordinatorResumeInput,
    resumeId: string | undefined,
    bootstrap: string | undefined,
  ) {
    if (this.#process !== undefined)
      return failure("busy", "Qwen stream process is already owned by this transport");
    const effort =
      scope.reasoningEffort === undefined ? this.#profile.effort : scope.reasoningEffort;
    if (effort !== null)
      return failure("unsupported", "installed Qwen stream CLI has no verified effort argument");
    const prepared = await this.#prepareLaunch?.prepare({ profile: this.#profile, scope });
    if (prepared !== undefined && !prepared.ok) return prepared;
    const resolved = resolveProxyEnvironment({
      ambient: { ...process.env, ...(prepared?.value.environment ?? {}) },
      ...(this.#proxyPolicy === undefined ? {} : { global: this.#proxyPolicy }),
      ...(this.#profile.proxy === undefined ? {} : { profile: this.#profile.proxy }),
    });
    const environment = Object.fromEntries(
      Object.entries(resolved.environment).filter(
        (entry): entry is [string, string] => entry[1] !== undefined,
      ),
    );
    const proxy =
      resolved.mode === "direct"
        ? undefined
        : (environment["HTTPS_PROXY"] ?? environment["ALL_PROXY"] ?? environment["HTTP_PROXY"]);
    const modelId = scope.modelId ?? this.#profile.modelId;
    const child = this.#factory.spawn({
      executablePath: this.#profile.executablePath,
      args: [
        ...(this.#profile.argumentPrefix ?? []),
        "--bare",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--chat-recording",
        "--model",
        modelId,
        "--approval-mode",
        "default",
        ...(proxy === undefined ? [] : ["--proxy", proxy]),
        ...(resumeId === undefined ? [] : ["--resume", resumeId]),
        ...((prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath) === undefined ||
        (prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath) === null
          ? []
          : ["--mcp-config", prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath ?? ""]),
        ...qwenZapMcpPermissionArguments(prepared !== undefined),
      ],
      cwd: scope.cwd,
      environment,
    });
    this.#process = child;
    this.#observe(child);
    if (resumeId !== undefined) {
      this.#session = session(scope, resumeId, processEpoch(child.pid), modelId);
      return { ok: true as const, value: this.#session };
    }
    const initialized = this.#waitForSession(child);
    if (bootstrap === undefined || !this.#writeUser(bootstrap))
      return failure("transport_lost", "Qwen input stream rejected bootstrap delivery");
    const observed = await initialized;
    if (!observed.ok) {
      child.kill();
      this.#process = undefined;
      return observed;
    }
    this.#session = session(scope, observed.value, processEpoch(child.pid), modelId);
    return { ok: true as const, value: this.#session };
  }

  #waitForSession(child: OwnedLineProcess): Promise<QwenResult<string>> {
    return new Promise((resolve) => {
      let settled = false;
      const finish = (result: QwenResult<string>) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        offLine();
        offExit();
        resolve(result);
      };
      const offLine = child.onLine((line) => {
        const record = z.record(z.string(), z.unknown()).safeParse(parseJson(line));
        const id = record.success ? record.data["session_id"] : undefined;
        if (typeof id === "string") finish({ ok: true, value: id });
      });
      const offExit = child.onExit(() => {
        finish(failure("transport_lost", "Qwen process exited before session observation"));
      });
      const timer = setTimeout(() => {
        finish(failure("transport_lost", "Qwen session observation timed out"));
      }, this.#timeoutMs);
    });
  }

  history(input: ProviderCoordinatorSession) {
    return Promise.resolve({
      ok: true as const,
      value: {
        coordinatorSessionId: input.coordinatorSessionId,
        nativeThreadId: input.nativeThreadId,
        processEpoch: input.processEpoch,
        status: { transport: "qwen_stream_json", active: this.#process !== undefined },
        turns: this.#history.flatMap((raw) => {
          const parsed = z.json().safeParse(raw);
          return parsed.success ? [parsed.data] : [];
        }),
      },
    });
  }

  send(input: {
    readonly session: ProviderCoordinatorSession;
    readonly text: string;
    readonly clientMessageId: string;
  }) {
    if (this.#session?.nativeSessionId !== input.session.nativeSessionId)
      return Promise.resolve(failure("not_found", "Qwen session is not active"));
    if (!this.#writeUser(input.text))
      return Promise.resolve(failure("transport_lost", "Qwen input stream rejected delivery"));
    return Promise.resolve({
      ok: true as const,
      value: {
        nativeTurnId: null,
        transportCorrelation: {
          provenance: "transport_correlation" as const,
          clientMessageId: input.clientMessageId,
          processEpoch: input.session.processEpoch,
        },
      },
    });
  }

  interrupt(input: {
    readonly session: ProviderCoordinatorSession;
    readonly nativeTurnId: string;
  }) {
    void input;
    if (this.#process === undefined)
      return Promise.resolve(failure("not_found", "Qwen process is not active"));
    this.#process.kill();
    return Promise.resolve({ ok: true as const, value: null });
  }

  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }) {
    void input;
    return Promise.resolve(
      failure("unsupported", "Qwen direct stream mode has no verified permission response mapping"),
    );
  }

  stop() {
    if (this.#process === undefined)
      return Promise.resolve(failure("not_found", "Qwen process is not active"));
    this.#process.kill();
    return Promise.resolve({ ok: true as const, value: null });
  }

  subscribe(listener: (raw: unknown) => void) {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  close() {
    this.#unsubscribeLine?.();
    this.#unsubscribeExit?.();
    this.#process?.kill();
    this.#process = undefined;
    this.#session = undefined;
    this.#listeners.clear();
  }

  #observe(child: OwnedLineProcess): void {
    this.#unsubscribeLine = child.onLine((line) => {
      const observed = publicQwenEvent(parseJson(line));
      this.#history.push(observed);
      for (const listener of this.#listeners) listener(observed);
    });
    this.#unsubscribeExit = child.onExit((code) => {
      this.#process = undefined;
      for (const listener of this.#listeners) listener({ type: "process_exited", exitCode: code });
    });
  }

  #writeUser(text: string): boolean {
    if (this.#process === undefined) return false;
    try {
      this.#process.write(
        `${JSON.stringify({ type: "user", message: { role: "user", content: text } })}\n`,
      );
      return true;
    } catch {
      return false;
    }
  }
}

function session(
  scope: CoordinatorStartInput | CoordinatorResumeInput,
  nativeId: string,
  epoch: string,
  modelId: string,
): ProviderCoordinatorSession {
  return {
    coordinatorSessionId: scope.coordinatorSessionId,
    nativeSessionId: nativeId,
    nativeThreadId: nativeId,
    processEpoch: epoch,
    modelId,
    effort: null,
  };
}

function processEpoch(pid: number): string {
  return `qwen:${String(pid)}:${randomUUID()}`;
}

function publicQwenEvent(raw: unknown): unknown {
  const record = z.record(z.string(), z.unknown()).safeParse(raw);
  if (!record.success) return { type: "provider_event", eventType: "invalid_json" };
  const type = typeof record.data["type"] === "string" ? record.data["type"] : "unknown";
  const sessionId =
    typeof record.data["session_id"] === "string" ? record.data["session_id"] : null;
  if (type === "assistant") {
    const message = z.record(z.string(), z.unknown()).safeParse(record.data["message"]);
    const content =
      message.success && Array.isArray(message.data["content"])
        ? message.data["content"].flatMap((block) => {
            const parsed = z.record(z.string(), z.unknown()).safeParse(block);
            return parsed.success &&
              parsed.data["type"] === "text" &&
              typeof parsed.data["text"] === "string"
              ? [{ type: "text", text: parsed.data["text"] }]
              : [];
          })
        : [];
    return { type, session_id: sessionId, message: { role: "assistant", content } };
  }
  if (type === "stream_event") {
    const event = z.record(z.string(), z.unknown()).safeParse(record.data["event"]);
    const delta = event.success
      ? z.record(z.string(), z.unknown()).safeParse(event.data["delta"])
      : { success: false as const };
    if (
      delta.success &&
      delta.data["type"] === "text_delta" &&
      typeof delta.data["text"] === "string"
    )
      return {
        type,
        session_id: sessionId,
        event: { type: "content_block_delta", delta: { text: delta.data["text"] } },
      };
  }
  if (type === "result")
    return {
      type,
      session_id: sessionId,
      subtype: typeof record.data["subtype"] === "string" ? record.data["subtype"] : "unknown",
      is_error: record.data["is_error"] === true,
      result: typeof record.data["result"] === "string" ? record.data["result"] : null,
    };
  return { type: "provider_event", eventType: type, session_id: sessionId };
}

function parseJson(line: string): unknown {
  try {
    return JSON.parse(line);
  } catch {
    return {};
  }
}

function nodeProcessFactory(): QwenProcessFactory {
  return {
    spawn(input) {
      const child = spawn(input.executablePath, [...input.args], {
        cwd: input.cwd,
        env: input.environment,
        stdio: ["pipe", "pipe", "pipe"],
      });
      return ownedNodeProcess(child);
    },
  };
}

function ownedNodeProcess(child: ChildProcessWithoutNullStreams): OwnedLineProcess {
  const lines = new Set<(line: string) => void>();
  const exits = new Set<(code: number | null) => void>();
  let buffer = "";
  child.stdout.on("data", (chunk: Buffer | string) => {
    buffer += String(chunk);
    const complete = buffer.split(/\r?\n/);
    buffer = complete.pop() ?? "";
    for (const line of complete) for (const listener of lines) listener(line);
  });
  child.on("exit", (code) => {
    for (const listener of exits) listener(code);
  });
  return {
    pid: child.pid ?? 0,
    write: (input) => void child.stdin.write(input),
    onLine: (listener) => (lines.add(listener), () => lines.delete(listener)),
    onExit: (listener) => (exits.add(listener), () => exits.delete(listener)),
    kill: () => void child.kill(),
  };
}

type QwenResult<T> = { readonly ok: true; readonly value: T } | ReturnType<typeof failure>;

function failure(code: "busy" | "not_found" | "unsupported" | "transport_lost", message: string) {
  return {
    ok: false as const,
    error: {
      code,
      message,
      retry: code === "transport_lost" ? ("after_reconcile" as const) : ("never" as const),
    },
  };
}
