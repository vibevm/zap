/** Owned Claude Code stream-JSON transport. @scope spec://org.vibevm.zap/lens/PROP-006#provider-adapters */
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import type {
  CoordinatorResumeInput,
  CoordinatorStartInput,
  HostRequestAnswer,
} from "../agent-runtime/index.ts";
import type {
  ProviderCoordinatorProfile,
  ProviderCoordinatorSession,
  ProviderCoordinatorTransport,
  ProviderCoordinatorTransportFactory,
  ProviderLaunchPreparationPort,
} from "./index.ts";
import { resolveProxyEnvironment } from "../proxy-policy/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import { ZAP_MCP_SERVER_NAME, zapPreauthorizedToolNames } from "../protocol/index.ts";

export interface OwnedLineProcess {
  readonly pid: number;
  write(input: string): void;
  onLine(listener: (line: string) => void): () => void;
  onExit(listener: (code: number | null) => void): () => void;
  kill(): void;
}

export interface ClaudeProcessFactory {
  spawn(input: {
    readonly executablePath: string;
    readonly args: readonly string[];
    readonly cwd: string;
    readonly environment: Readonly<Record<string, string>>;
  }): OwnedLineProcess;
}

export function createClaudeStreamJsonTransportFactory(
  options: {
    readonly processFactory?: ClaudeProcessFactory;
    readonly timeoutMs?: number;
    readonly proxyPolicy?: ProxyPolicy;
    readonly prepareLaunch?: ProviderLaunchPreparationPort;
  } = {},
): ProviderCoordinatorTransportFactory {
  return {
    open(profile) {
      return Promise.resolve({
        ok: true,
        value: new ClaudeStreamJsonTransport(
          profile,
          options.processFactory ?? nodeProcessFactory(),
          options.timeoutMs ?? 10_000,
          options.proxyPolicy,
          options.prepareLaunch,
        ),
      } as const);
    },
  };
}

class ClaudeStreamJsonTransport implements ProviderCoordinatorTransport {
  readonly #profile: ProviderCoordinatorProfile;
  readonly #factory: ClaudeProcessFactory;
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
    factory: ClaudeProcessFactory,
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

  async start(input: { readonly scope: CoordinatorStartInput }) {
    return this.#launch(input.scope, undefined, input.scope.bootstrapText);
  }

  async resume(input: { readonly scope: CoordinatorResumeInput }) {
    return this.#launch(input.scope, input.scope.nativeThreadId, undefined);
  }

  async #launch(
    scope: CoordinatorStartInput | CoordinatorResumeInput,
    resumeId: string | undefined,
    bootstrap: string | undefined,
  ) {
    if (this.#process !== undefined)
      return failure("busy", "Claude stream process is already owned by this transport");
    const prepared = await this.#prepareLaunch?.prepare({ profile: this.#profile, scope });
    if (prepared !== undefined && !prepared.ok) return prepared;
    const mcpConfigPath = prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath;
    const args = [
      ...(this.#profile.argumentPrefix ?? []),
      "--print",
      "--input-format",
      "stream-json",
      "--output-format",
      "stream-json",
      "--verbose",
      "--model",
      scope.modelId ?? this.#profile.modelId,
      ...(selectedEffort(scope, this.#profile) === null
        ? []
        : ["--effort", selectedEffort(scope, this.#profile) ?? ""]),
      ...(resumeId === undefined ? [] : ["--resume", resumeId]),
      ...(mcpConfigPath === undefined || mcpConfigPath === null
        ? []
        : ["--mcp-config", mcpConfigPath]),
      ...(prepared === undefined
        ? []
        : [
            "--allowedTools",
            zapPreauthorizedToolNames(true)
              .map((tool) => `mcp__${ZAP_MCP_SERVER_NAME}__${tool}`)
              .join(","),
          ]),
    ];
    const environment = resolveProxyEnvironment({
      ambient: { ...process.env, ...(prepared?.value.environment ?? {}) },
      ...(this.#proxyPolicy === undefined ? {} : { global: this.#proxyPolicy }),
      ...(this.#profile.proxy === undefined ? {} : { profile: this.#profile.proxy }),
    }).environment;
    const processEnvironment = Object.fromEntries(
      Object.entries(environment).filter(
        (entry): entry is [string, string] => entry[1] !== undefined,
      ),
    );
    const childProcess = this.#factory.spawn({
      executablePath: this.#profile.executablePath,
      args,
      cwd: scope.cwd,
      environment: processEnvironment,
    });
    this.#process = childProcess;
    const processEpoch = `claude:${String(childProcess.pid)}:${randomUUID()}`;
    this.#observeProcess(childProcess);
    if (resumeId !== undefined) {
      const resumed: ProviderCoordinatorSession = {
        coordinatorSessionId: scope.coordinatorSessionId,
        nativeSessionId: resumeId,
        nativeThreadId: resumeId,
        processEpoch,
        modelId: scope.modelId ?? this.#profile.modelId,
        effort: selectedEffort(scope, this.#profile),
      };
      this.#session = resumed;
      return { ok: true as const, value: resumed };
    }
    const initPromise = this.#waitForInit(childProcess);
    if (bootstrap !== undefined && !this.#writeUser(bootstrap))
      return failure("transport_lost", "Claude input stream rejected bootstrap delivery");
    const init = await initPromise;
    if (!init.ok) {
      childProcess.kill();
      this.#process = undefined;
      return init;
    }
    const started: ProviderCoordinatorSession = {
      coordinatorSessionId: scope.coordinatorSessionId,
      nativeSessionId: init.value.sessionId,
      nativeThreadId: init.value.threadId,
      processEpoch,
      modelId: scope.modelId ?? this.#profile.modelId,
      effort: selectedEffort(scope, this.#profile),
    };
    this.#session = started;
    return { ok: true as const, value: started };
  }

  async #waitForInit(process: OwnedLineProcess): Promise<
    | {
        readonly ok: true;
        readonly value: { readonly sessionId: string; readonly threadId: string };
      }
    | {
        readonly ok: false;
        readonly error: {
          readonly code: "transport_lost" | "protocol_error";
          readonly message: string;
          readonly retry: "after_reconcile";
        };
      }
  > {
    return new Promise((resolve) => {
      let settled = false;
      const finish = (result: Parameters<typeof resolve>[0]) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(result);
      };
      const unsubscribe = process.onLine((line) => {
        const raw: unknown = parseJson(line);
        const system = InitSchema.safeParse(raw);
        if (!system.success || system.data.subtype !== "init") return;
        const sessionId = system.data.session_id;
        if (sessionId === undefined) {
          finish({
            ok: false,
            error: {
              code: "protocol_error",
              message: "Claude init omitted session_id",
              retry: "after_reconcile",
            },
          });
          return;
        }
        finish({ ok: true, value: { sessionId, threadId: system.data.thread_id ?? sessionId } });
      });
      const unsubscribeExit = process.onExit((code) => {
        unsubscribe();
        unsubscribeExit();
        finish({
          ok: false,
          error: {
            code: "transport_lost",
            message: `Claude process exited before init (${String(code)})`,
            retry: "after_reconcile",
          },
        });
      });
      const timer = setTimeout(() => {
        unsubscribe();
        unsubscribeExit();
        finish({
          ok: false,
          error: {
            code: "transport_lost",
            message: "Claude init timed out",
            retry: "after_reconcile",
          },
        });
      }, this.#timeoutMs);
    });
  }

  history(session: ProviderCoordinatorSession) {
    return Promise.resolve({
      ok: true as const,
      value: {
        coordinatorSessionId: session.coordinatorSessionId,
        nativeThreadId: session.nativeThreadId,
        processEpoch: session.processEpoch,
        status: { transport: "claude_stream_json", active: this.#process !== undefined },
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
    if (!this.#session || input.session.nativeSessionId !== this.#session.nativeSessionId)
      return Promise.resolve(failure("not_found", "Claude session is not active"));
    if (!this.#writeUser(input.text))
      return Promise.resolve(failure("transport_lost", "Claude input stream rejected delivery"));
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
    void input.nativeTurnId;
    if (!this.#process)
      return Promise.resolve(failure("not_found", "Claude process is not active"));
    this.#process.kill();
    return Promise.resolve({ ok: true as const, value: null });
  }

  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }) {
    void input;
    return Promise.resolve(
      failure("unsupported", "Claude permission response mapping is not enabled in this adapter"),
    );
  }

  stop() {
    if (!this.#process)
      return Promise.resolve(failure("not_found", "Claude process is not active"));
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

  #observeProcess(process: OwnedLineProcess): void {
    this.#unsubscribeLine = process.onLine((line) => {
      const raw: unknown = parseJson(line);
      const observed = publicClaudeEvent(raw);
      this.#history.push(observed);
      for (const listener of this.#listeners) listener(observed);
    });
    this.#unsubscribeExit = process.onExit((code) => {
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

const InitSchema = z.looseObject({
  type: z.string().optional(),
  subtype: z.string().optional(),
  session_id: z.string().optional(),
  thread_id: z.string().optional(),
});

function selectedEffort(
  scope: CoordinatorStartInput | CoordinatorResumeInput,
  profile: ProviderCoordinatorProfile,
): ProviderCoordinatorSession["effort"] {
  return scope.reasoningEffort === undefined ? profile.effort : scope.reasoningEffort;
}

function parseJson(line: string): unknown {
  try {
    return JSON.parse(line);
  } catch {
    return {};
  }
}

function publicClaudeEvent(raw: unknown): unknown {
  const record = z.record(z.string(), z.unknown()).safeParse(raw);
  if (!record.success) return { type: "provider_event", eventType: "invalid_json" };
  const type = typeof record.data["type"] === "string" ? record.data["type"] : "unknown";
  const sessionId =
    typeof record.data["session_id"] === "string" ? record.data["session_id"] : null;
  if (type === "stream_event") {
    const event = z.record(z.string(), z.unknown()).safeParse(record.data["event"]);
    const delta = event.success
      ? z.record(z.string(), z.unknown()).safeParse(event.data["delta"])
      : { success: false as const };
    if (
      event.success &&
      event.data["type"] === "content_block_delta" &&
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
  if (type === "result")
    return {
      type,
      session_id: sessionId,
      subtype: typeof record.data["subtype"] === "string" ? record.data["subtype"] : "unknown",
      is_error: record.data["is_error"] === true,
      result: typeof record.data["result"] === "string" ? record.data["result"] : null,
    };
  return {
    type: "provider_event",
    eventType: type,
    session_id: sessionId,
    subtype: typeof record.data["subtype"] === "string" ? record.data["subtype"] : null,
  };
}

function nodeProcessFactory(): ClaudeProcessFactory {
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
  const lineListeners = new Set<(line: string) => void>();
  const exitListeners = new Set<(code: number | null) => void>();
  let buffer = "";
  child.stdout.on("data", (chunk: Buffer | string) => {
    buffer += String(chunk);
    const lines = buffer.split(/\r?\n/);
    buffer = lines.pop() ?? "";
    for (const line of lines) for (const listener of lineListeners) listener(line);
  });
  child.on("exit", (code) => {
    for (const listener of exitListeners) listener(code);
  });
  return {
    pid: child.pid ?? 0,
    write(input) {
      child.stdin.write(input);
    },
    onLine(listener) {
      lineListeners.add(listener);
      return () => lineListeners.delete(listener);
    },
    onExit(listener) {
      exitListeners.add(listener);
      return () => exitListeners.delete(listener);
    },
    kill() {
      child.kill();
    },
  };
}

function failure(
  code: "busy" | "not_found" | "unsupported" | "transport_lost" | "protocol_error",
  message: string,
) {
  return {
    ok: false as const,
    error: {
      code,
      message,
      retry:
        code === "transport_lost" || code === "protocol_error"
          ? ("after_reconcile" as const)
          : ("never" as const),
    },
  };
}
