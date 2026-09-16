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
import { StreamControlClient, streamHostControlRequest } from "./stream-control.ts";
import { providerStderrDiagnostic, type ProviderProcessDiagnostic } from "./process-diagnostic.ts";

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
    const requestedSessionId = resumeId ?? randomUUID();
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
        ...(resumeId === undefined ? ["--session-id", requestedSessionId] : []),
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
    this.#activeChildren.clear();
    this.#pendingRootCompletion = undefined;
    this.#observe(child);
    const initialized = await this.#control.request(child, "initialize", this.#timeoutMs);
    if (!initialized.ok) {
      child.kill();
      this.#process = undefined;
      return initialized;
    }
    const observedSessionId = initialized.value["session_id"];
    const nativeSessionId =
      typeof observedSessionId === "string" ? observedSessionId : requestedSessionId;
    if (resumeId !== undefined && nativeSessionId !== resumeId)
      return failure("protocol_error", "Qwen resumed a different native session");
    const started = session(scope, nativeSessionId, processEpoch(child.pid), modelId);
    this.#session = started;
    this.#activity = "idle";
    if (bootstrap !== undefined) {
      if (!this.#writeUser(bootstrap))
        return failure("transport_lost", "Qwen input stream rejected bootstrap delivery");
      this.#activity = "busy";
    }
    return { ok: true as const, value: started };
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
    if (this.#process === undefined)
      return Promise.resolve(failure("not_found", "Qwen process is not active"));
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
      return Promise.resolve(failure("not_found", "Qwen permission session is not active"));
    return Promise.resolve(
      this.#control.respond(this.#process, String(input.answer.requestId), input.answer.answer).ok
        ? { ok: true as const, value: null }
        : failure("transport_lost", "Qwen permission response was not written"),
    );
  }

  async pause(input: { readonly session: ProviderCoordinatorSession }) {
    if (this.#session?.nativeSessionId !== input.session.nativeSessionId || !this.#process)
      return failure("not_found", "Qwen session is not active");
    if (this.#activity === "idle")
      return { ok: true as const, value: { observation: "settled" as const } };
    const interrupted = await this.#control.request(this.#process, "interrupt", this.#timeoutMs);
    return interrupted.ok
      ? { ok: true as const, value: { observation: "requested" as const } }
      : interrupted;
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
    this.#control.close();
    this.#unsubscribeLine?.();
    this.#unsubscribeDiagnostic?.();
    this.#unsubscribeExit?.();
    this.#process?.kill();
    this.#process = undefined;
    this.#session = undefined;
    this.#listeners.clear();
  }

  #observe(child: OwnedLineProcess): void {
    this.#unsubscribeDiagnostic = child.onDiagnostic((diagnostic) => {
      const observed = { type: "process_diagnostic", ...diagnostic };
      this.#history.push(observed);
      for (const listener of this.#listeners) listener(observed);
    });
    this.#unsubscribeLine = child.onLine((line) => {
      const raw = parseJson(line);
      if (this.#control.handle(raw)) return;
      const childrenBefore = this.#activeChildren.size;
      const nextActivity = streamActivity(raw, this.#activeChildren);
      let observed = publicQwenEvent(raw);
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
    this.#unsubscribeExit = child.onExit((code) => {
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
    if (
      event.success &&
      (event.data["type"] === "message_start" || event.data["type"] === "message_stop")
    )
      return { type, session_id: sessionId, event: { type: event.data["type"] } };
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
  return { type: "provider_event", eventType: type, session_id: sessionId };
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
  const diagnostics = new Set<(diagnostic: ProviderProcessDiagnostic) => void>();
  const exits = new Set<(code: number | null) => void>();
  let buffer = "";
  child.stdout.on("data", (chunk: Buffer | string) => {
    buffer += String(chunk);
    const complete = buffer.split(/\r?\n/);
    buffer = complete.pop() ?? "";
    for (const line of complete) for (const listener of lines) listener(line);
  });
  child.stderr.on("data", (chunk: Buffer | string) => {
    const diagnostic = providerStderrDiagnostic(chunk);
    for (const listener of diagnostics) listener(diagnostic);
  });
  child.on("exit", (code) => {
    for (const listener of exits) listener(code);
  });
  return {
    pid: child.pid ?? 0,
    write: (input) => void child.stdin.write(input),
    onLine: (listener) => (lines.add(listener), () => lines.delete(listener)),
    onDiagnostic: (listener) => (diagnostics.add(listener), () => diagnostics.delete(listener)),
    onExit: (listener) => (exits.add(listener), () => exits.delete(listener)),
    kill: () => void child.kill(),
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
      retry: code === "transport_lost" ? ("after_reconcile" as const) : ("never" as const),
    },
  };
}
