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
import { StreamControlClient, streamHostControlRequest } from "./stream-control.ts";
import { providerStderrDiagnostic, type ProviderProcessDiagnostic } from "./process-diagnostic.ts";
import { isolatedExecutionEnvironment } from "../execution-accounts/index.ts";

export interface OwnedLineProcess {
  readonly pid: number;
  write(input: string): void;
  onLine(listener: (line: string) => void): () => void;
  onDiagnostic(listener: (diagnostic: ProviderProcessDiagnostic) => void): () => void;
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
  readonly #control = new StreamControlClient();
  readonly #activeChildren = new Set<string>();
  #pendingRootCompletion: unknown;
  #process: OwnedLineProcess | undefined;
  #session: ProviderCoordinatorSession | undefined;
  #activity: "idle" | "busy" | "unknown" = "unknown";
  #unsubscribeLine: (() => void) | undefined;
  #unsubscribeDiagnostic: (() => void) | undefined;
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
    const requestedSessionId = resumeId ?? randomUUID();
    const args = [
      ...(this.#profile.argumentPrefix ?? []),
      "--print",
      "--input-format",
      "stream-json",
      "--output-format",
      "stream-json",
      "--verbose",
      "--setting-sources",
      "",
      "--model",
      scope.modelId ?? this.#profile.modelId,
      ...(selectedEffort(scope, this.#profile) === null
        ? []
        : ["--effort", selectedEffort(scope, this.#profile) ?? ""]),
      ...(resumeId === undefined ? [] : ["--resume", resumeId]),
      ...(resumeId === undefined ? ["--session-id", requestedSessionId] : []),
      ...(mcpConfigPath === undefined || mcpConfigPath === null
        ? []
        : ["--strict-mcp-config", "--mcp-config", mcpConfigPath]),
      ...(prepared === undefined
        ? []
        : [
            "--allowedTools",
            zapPreauthorizedToolNames(true)
              .map((tool) => `mcp__${ZAP_MCP_SERVER_NAME}__${tool}`)
              .join(","),
          ]),
    ];
    const preparedEnvironment = prepared?.value.environment ?? {};
    const ambient =
      this.#profile.accountBindingId === undefined
        ? { ...process.env, ...preparedEnvironment }
        : isolatedExecutionEnvironment(process.env, preparedEnvironment);
    const environment = resolveProxyEnvironment({
      ambient,
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
    this.#activeChildren.clear();
    this.#pendingRootCompletion = undefined;
    const processEpoch = `claude:${String(childProcess.pid)}:${randomUUID()}`;
    this.#observeProcess(childProcess);
    const initialized = await this.#control.request(childProcess, "initialize", this.#timeoutMs);
    if (!initialized.ok) {
      childProcess.kill();
      this.#process = undefined;
      return initialized;
    }
    const started: ProviderCoordinatorSession = {
      coordinatorSessionId: scope.coordinatorSessionId,
      nativeSessionId: requestedSessionId,
      nativeThreadId: requestedSessionId,
      processEpoch,
      modelId: scope.modelId ?? this.#profile.modelId,
      effort: selectedEffort(scope, this.#profile),
    };
    this.#session = started;
    this.#activity = "idle";
    if (bootstrap !== undefined) {
      if (!this.#writeUser(bootstrap))
        return failure("transport_lost", "Claude input stream rejected bootstrap delivery");
      this.#activity = "busy";
    }
    return { ok: true as const, value: started };
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
    this.#activity = "busy";
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
    return this.#control
      .request(this.#process, "interrupt", this.#timeoutMs)
      .then((result) => (result.ok ? { ok: true as const, value: null } : result));
  }

  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }) {
    if (
      this.#session?.nativeSessionId !== input.session.nativeSessionId ||
      this.#session.processEpoch !== input.answer.processEpoch ||
      !this.#process
    )
      return Promise.resolve(failure("not_found", "Claude permission session is not active"));
    return Promise.resolve(
      this.#control.respond(this.#process, String(input.answer.requestId), input.answer.answer).ok
        ? { ok: true as const, value: null }
        : failure("transport_lost", "Claude permission response was not written"),
    );
  }

  async pause(input: { readonly session: ProviderCoordinatorSession }) {
    if (this.#session?.nativeSessionId !== input.session.nativeSessionId || !this.#process)
      return failure("not_found", "Claude session is not active");
    if (this.#activity === "idle")
      return { ok: true as const, value: { observation: "settled" as const } };
    const interrupted = await this.#control.request(this.#process, "interrupt", this.#timeoutMs);
    return interrupted.ok
      ? { ok: true as const, value: { observation: "requested" as const } }
      : interrupted;
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
    this.#control.close();
    this.#unsubscribeLine?.();
    this.#unsubscribeDiagnostic?.();
    this.#unsubscribeExit?.();
    this.#process?.kill();
    this.#process = undefined;
    this.#session = undefined;
    this.#listeners.clear();
  }

  #observeProcess(process: OwnedLineProcess): void {
    this.#unsubscribeDiagnostic = process.onDiagnostic((diagnostic) => {
      const observed = { type: "process_diagnostic", ...diagnostic };
      this.#history.push(observed);
      for (const listener of this.#listeners) listener(observed);
    });
    this.#unsubscribeLine = process.onLine((line) => {
      const raw: unknown = parseJson(line);
      if (this.#control.handle(raw)) return;
      const childrenBefore = this.#activeChildren.size;
      const nextActivity = streamActivity(raw, this.#activeChildren);
      let observed = publicClaudeEvent(raw);
      if (rootCompletion(raw) && this.#activeChildren.size > 0) {
        this.#pendingRootCompletion = observed;
        observed = {
          kind: "session_status",
          data: { type: "busy", reason: "native_children_active" },
        };
      }
      if (nextActivity !== undefined) this.#activity = nextActivity;
      this.#history.push(observed);
      for (const listener of this.#listeners) listener(observed);
      if (
        childrenBefore > 0 &&
        this.#activeChildren.size === 0 &&
        this.#pendingRootCompletion !== undefined
      ) {
        const completion = this.#pendingRootCompletion;
        this.#pendingRootCompletion = undefined;
        this.#activity = "idle";
        this.#history.push(completion);
        for (const listener of this.#listeners) listener(completion);
      }
    });
    this.#unsubscribeExit = process.onExit((code) => {
      this.#control.close();
      this.#process = undefined;
      this.#activeChildren.clear();
      this.#pendingRootCompletion = undefined;
      this.#activity = "unknown";
      for (const listener of this.#listeners) listener({ type: "process_exited", exitCode: code });
    });
  }

  #writeUser(text: string): boolean {
    if (this.#process === undefined) return false;
    try {
      this.#process.write(
        `${JSON.stringify({
          type: "user",
          session_id: this.#session?.nativeSessionId,
          message: { role: "user", content: text },
          parent_tool_use_id: null,
        })}\n`,
      );
      return true;
    } catch {
      return false;
    }
  }
}

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
  const parentToolUseId =
    typeof record.data["parent_tool_use_id"] === "string"
      ? record.data["parent_tool_use_id"]
      : null;
  if (parentToolUseId !== null)
    return {
      kind: type === "stream_event" ? "native_child_observed" : "native_message_observed",
      nativeItemId: parentToolUseId,
      data: { parentToolUseId, type, sessionId },
    };
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
    if (
      event.success &&
      (event.data["type"] === "message_start" || event.data["type"] === "message_stop")
    )
      return { type, session_id: sessionId, event: { type: event.data["type"] } };
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
    return {
      kind: "item_completed",
      nativeItemId: typeof record.data["uuid"] === "string" ? record.data["uuid"] : null,
      data: {
        type: "agentMessage",
        phase: "final_answer",
        text: content.map((part) => part.text).join("\n"),
      },
    };
  }
  if (type === "result")
    return {
      kind: "turn_completed",
      data: {
        status: record.data["is_error"] === true ? "failed" : "completed",
        subtype: typeof record.data["subtype"] === "string" ? record.data["subtype"] : "unknown",
      },
    };
  const control = streamHostControlRequest(raw);
  if (control !== undefined)
    return {
      kind: "host_request_pending",
      nativeItemId:
        typeof control.request["tool_use_id"] === "string"
          ? control.request["tool_use_id"]
          : control.requestId,
      data: {
        requestId: control.requestId,
        kind: "permission_approval",
        body: control.request,
      },
    };
  return {
    type: "provider_event",
    eventType: type,
    session_id: sessionId,
    subtype: typeof record.data["subtype"] === "string" ? record.data["subtype"] : null,
  };
}

function streamActivity(raw: unknown, activeChildren: Set<string>): "idle" | "busy" | undefined {
  const parsed = z
    .looseObject({
      type: z.string(),
      event: z.unknown().optional(),
      parent_tool_use_id: z.string().nullable().optional(),
    })
    .safeParse(raw);
  if (!parsed.success) return undefined;
  const parent = parsed.data.parent_tool_use_id;
  if (typeof parent === "string") {
    const event = z.looseObject({ type: z.string() }).safeParse(parsed.data.event);
    if (parsed.data.type === "result" || (event.success && event.data.type === "message_stop"))
      activeChildren.delete(parent);
    else activeChildren.add(parent);
    return undefined;
  }
  if (parsed.data.type === "assistant") return "busy";
  if (parsed.data.type === "result") return activeChildren.size === 0 ? "idle" : "busy";
  return undefined;
}

function rootCompletion(raw: unknown): boolean {
  const parsed = z
    .looseObject({
      type: z.string(),
      event: z.unknown().optional(),
      parent_tool_use_id: z.string().nullable().optional(),
    })
    .safeParse(raw);
  if (!parsed.success || typeof parsed.data.parent_tool_use_id === "string") return false;
  return parsed.data.type === "result";
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
  const diagnosticListeners = new Set<(diagnostic: ProviderProcessDiagnostic) => void>();
  const exitListeners = new Set<(code: number | null) => void>();
  let buffer = "";
  child.stdout.on("data", (chunk: Buffer | string) => {
    buffer += String(chunk);
    const lines = buffer.split(/\r?\n/);
    buffer = lines.pop() ?? "";
    for (const line of lines) for (const listener of lineListeners) listener(line);
  });
  child.stderr.on("data", (chunk: Buffer | string) => {
    const diagnostic = providerStderrDiagnostic(chunk);
    for (const listener of diagnosticListeners) listener(diagnostic);
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
    onDiagnostic(listener) {
      diagnosticListeners.add(listener);
      return () => diagnosticListeners.delete(listener);
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
