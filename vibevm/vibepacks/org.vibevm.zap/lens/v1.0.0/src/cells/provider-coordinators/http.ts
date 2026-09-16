/** Loopback provider HTTP session transports. @scope spec://org.vibevm.zap/lens/PROP-006#provider-adapters */
import type {
  CoordinatorHistory,
  CoordinatorResumeInput,
  CoordinatorStartInput,
  HostRequestAnswer,
} from "../agent-runtime/index.ts";
import type {
  ProviderCoordinatorProfile,
  ProviderCoordinatorSession,
  ProviderCoordinatorTransport,
  ProviderCoordinatorTransportFactory,
} from "./index.ts";
import { AgentSessionIdSchema } from "../workspace-model/index.ts";
import { z } from "zod";
import { JsonValueSchema } from "../protocol/index.ts";
import { resolveProxyEnvironment, type ProxyPolicy } from "../proxy-policy/index.ts";
import { createHash, randomUUID } from "node:crypto";

export interface ProviderHttpOptions {
  readonly fetchImpl?: typeof fetch;
  readonly bearerToken?: string;
  readonly basicAuth?: { readonly username: string; readonly password: string };
  readonly proxyPolicy?: ProxyPolicy;
}

export function resolveProviderProxyEnvironment(
  profile: ProviderCoordinatorProfile,
  ambient: Readonly<Record<string, string | undefined>> = process.env,
  globalProxy?: ProxyPolicy,
): Readonly<Record<string, string>> {
  const resolved = resolveProxyEnvironment({
    ambient,
    ...(globalProxy === undefined ? {} : { global: globalProxy }),
    ...(profile.proxy === undefined ? {} : { profile: profile.proxy }),
  }).environment;
  return Object.fromEntries(
    Object.entries(resolved).filter((entry): entry is [string, string] => entry[1] !== undefined),
  );
}

export function createOpenCodeHttpTransportFactory(
  options: ProviderHttpOptions = {},
): ProviderCoordinatorTransportFactory {
  return {
    open: (profile) =>
      Promise.resolve({
        ok: true as const,
        value: new HttpSessionTransport(profile, options, `opencode:${randomUUID()}`),
      }),
  };
}

export function openCodeHttpTransport(
  profile: ProviderCoordinatorProfile,
  options: ProviderHttpOptions,
  processEpoch: string,
): ProviderCoordinatorTransport {
  return new HttpSessionTransport(profile, options, processEpoch);
}

class HttpSessionTransport implements ProviderCoordinatorTransport {
  readonly #profile: ProviderCoordinatorProfile;
  readonly #fetch: typeof fetch;
  readonly #headers: HeadersInit;
  readonly #processEpoch: string;
  readonly #listeners = new Set<(raw: unknown) => void>();
  #session: ProviderCoordinatorSession | undefined;
  #eventAbort: AbortController | undefined;
  #activity: "idle" | "busy" | "unknown" = "unknown";
  #assistantText = "";
  #assistantMessageId: string | null = null;

  constructor(
    profile: ProviderCoordinatorProfile,
    options: ProviderHttpOptions,
    processEpoch: string,
  ) {
    this.#profile = profile;
    this.#fetch = options.fetchImpl ?? fetch;
    this.#processEpoch = processEpoch;
    this.#headers = {
      "content-type": "application/json",
      ...(options.bearerToken === undefined
        ? {}
        : { authorization: `Bearer ${options.bearerToken}` }),
      ...(options.basicAuth === undefined
        ? {}
        : {
            authorization: `Basic ${Buffer.from(`${options.basicAuth.username}:${options.basicAuth.password}`).toString("base64")}`,
          }),
    };
  }

  async start(input: { readonly scope: CoordinatorStartInput }) {
    const effort = selectedEffort(input.scope, this.#profile);
    if (effort !== null)
      return failure("unsupported", "OpenCode server has no verified effort request field");
    const response = await this.call("/session", "POST", {
      title: this.#profile.profileId,
    });
    if (!response.ok) return response;
    const id = stringValue(response.value, "id") ?? stringValue(response.value, "sessionId");
    if (id === undefined)
      return failure("protocol_error", "provider session response omitted native session ID");
    this.#session = {
      coordinatorSessionId: input.scope.coordinatorSessionId,
      nativeSessionId: id,
      nativeThreadId: id,
      processEpoch: this.#processEpoch,
      modelId: input.scope.modelId ?? this.#profile.modelId,
      effort,
    };
    this.#activity = "idle";
    this.#startOpenCodeEvents();
    if (input.scope.bootstrapText !== "") {
      const bootstrapped = await this.prompt(
        input.scope.bootstrapText,
        `bootstrap:${input.scope.coordinatorSessionId}`,
      );
      if (!bootstrapped.ok) return bootstrapped;
      this.#activity = "busy";
    }
    return { ok: true as const, value: this.#session };
  }

  async resume(input: { readonly scope: CoordinatorResumeInput }) {
    const effort = selectedEffort(input.scope, this.#profile);
    if (effort !== null)
      return failure("unsupported", "OpenCode server has no verified effort request field");
    const response = await this.call(
      `/session/${encodeURIComponent(input.scope.nativeThreadId)}`,
      "GET",
      undefined,
    );
    if (!response.ok) return response;
    const id = stringValue(response.value, "id") ?? input.scope.nativeThreadId;
    this.#session = {
      coordinatorSessionId: input.scope.coordinatorSessionId,
      nativeSessionId: id,
      nativeThreadId: id,
      processEpoch: this.#processEpoch,
      modelId: input.scope.modelId ?? this.#profile.modelId,
      effort,
    };
    const status = await this.#status(id);
    this.#activity = status ?? "unknown";
    this.#startOpenCodeEvents();
    return { ok: true as const, value: this.#session };
  }

  async history(session: ProviderCoordinatorSession) {
    const response = await this.call(
      `/session/${encodeURIComponent(session.nativeSessionId)}/message`,
      "GET",
      undefined,
    );
    if (!response.ok) return response;
    return {
      ok: true as const,
      value: {
        coordinatorSessionId: AgentSessionIdSchema.parse(session.coordinatorSessionId),
        nativeThreadId: session.nativeThreadId,
        processEpoch: session.processEpoch,
        status: JsonValueSchema.parse(response.value),
        turns: [],
      } satisfies CoordinatorHistory,
    };
  }

  async send(input: {
    readonly session: ProviderCoordinatorSession;
    readonly text: string;
    readonly clientMessageId: string;
  }) {
    if (this.#session?.nativeSessionId !== input.session.nativeSessionId)
      return failure("not_found", "provider session is not active");
    const sent = await this.prompt(input.text, input.clientMessageId);
    if (sent.ok) this.#activity = "busy";
    return sent;
  }

  async prompt(text: string, clientMessageId = "") {
    const path = `/session/${encodeURIComponent(this.#session?.nativeSessionId ?? "")}/prompt_async`;
    const model = openCodeModel(this.#session?.modelId ?? this.#profile.modelId);
    if (model === undefined)
      return failure("invalid_input", "OpenCode model must use provider/model format");
    const response = await this.call(path, "POST", {
      model,
      parts: [{ type: "text", text }],
    });
    if (!response.ok) return response;
    if (clientMessageId !== "")
      return {
        ok: true as const,
        value: {
          nativeTurnId: null,
          transportCorrelation: {
            provenance: "transport_correlation" as const,
            clientMessageId,
            processEpoch: this.#session?.processEpoch ?? this.#processEpoch,
          },
        },
      };
    const turnId =
      stringValue(response.value, "turnId") ?? stringValue(response.value, "messageId");
    return turnId === undefined
      ? failure("unsupported", "provider HTTP response did not expose a native turn ID")
      : { ok: true as const, value: { nativeTurnId: turnId, transportCorrelation: null } };
  }

  async interrupt(input: {
    readonly session: ProviderCoordinatorSession;
    readonly nativeTurnId: string;
  }) {
    void input.nativeTurnId;
    const result = await this.call(
      `/session/${encodeURIComponent(input.session.nativeSessionId)}/abort`,
      "POST",
      {},
    );
    return result.ok ? { ok: true as const, value: null } : result;
  }

  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }) {
    void input;
    return Promise.resolve(
      failure("unsupported", "provider permission response route is not verified"),
    );
  }

  async pause(input: { readonly session: ProviderCoordinatorSession }) {
    if (this.#session?.nativeSessionId !== input.session.nativeSessionId)
      return failure("not_found", "OpenCode session is not active");
    if (this.#activity === "idle")
      return { ok: true as const, value: { observation: "settled" as const } };
    const observed = await this.#status(input.session.nativeSessionId);
    if (observed === "idle") {
      this.#activity = "idle";
      return { ok: true as const, value: { observation: "settled" as const } };
    }
    const interrupted = await this.call(
      `/session/${encodeURIComponent(input.session.nativeSessionId)}/abort`,
      "POST",
      undefined,
    );
    if (!interrupted.ok) return interrupted;
    const accepted = z.boolean().safeParse(interrupted.value);
    return accepted.success && accepted.data
      ? { ok: true as const, value: { observation: "requested" as const } }
      : { ok: true as const, value: { observation: "uncertain" as const } };
  }

  async stop(input: { readonly session: ProviderCoordinatorSession }) {
    return this.interrupt({ session: input.session, nativeTurnId: "stop" });
  }

  subscribe(listener: (raw: unknown) => void) {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  close() {
    this.#eventAbort?.abort();
    this.#eventAbort = undefined;
    this.#listeners.clear();
    this.#session = undefined;
    this.#activity = "unknown";
  }

  #startOpenCodeEvents(): void {
    this.#eventAbort?.abort();
    const abort = new AbortController();
    this.#eventAbort = abort;
    void this.#runOpenCodeEvents(abort);
  }

  async #runOpenCodeEvents(abort: AbortController): Promise<void> {
    while (!abort.signal.aborted) {
      try {
        const response = await this.#fetch(
          new URL("/event", this.#profile.endpoint ?? "http://127.0.0.1").toString(),
          { method: "GET", headers: this.#headers, signal: abort.signal },
        );
        if (!response.ok || response.body === null) {
          await reconnectDelay(abort.signal);
          continue;
        }
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        for (;;) {
          const next = await reader.read();
          if (next.done) break;
          buffer += decoder.decode(next.value, { stream: true });
          const frames = buffer.split("\n\n");
          buffer = frames.pop() ?? "";
          for (const frame of frames) {
            const data = frame
              .split("\n")
              .find((line) => line.startsWith("data:"))
              ?.slice(5)
              .trim();
            if (data === undefined || data === "") continue;
            try {
              const raw: unknown = JSON.parse(data);
              this.#observeOpenCodeEvent(raw);
            } catch {
              for (const listener of this.#listeners)
                listener({ type: "provider_event", eventType: "invalid_sse_json" });
            }
          }
        }
      } catch {
        // Reconnect after a transient loopback transport failure.
      }
      await reconnectDelay(abort.signal);
    }
  }

  async call(path: string, method: "GET" | "POST", body: unknown) {
    try {
      const response = await this.#fetch(
        new URL(path, this.#profile.endpoint ?? "http://127.0.0.1").toString(),
        {
          method,
          headers: this.#headers,
          ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        },
      );
      const text = await response.text();
      let raw: unknown = null;
      if (text !== "") {
        try {
          raw = JSON.parse(text);
        } catch {
          return response.ok
            ? failure("protocol_error", "provider HTTP returned invalid JSON")
            : failure("protocol_error", safeHttpFailure(response.status, text, null));
        }
      }
      return response.ok
        ? { ok: true as const, value: raw }
        : failure("protocol_error", safeHttpFailure(response.status, text, raw));
    } catch {
      return failure("transport_lost", "provider loopback HTTP request failed");
    }
  }

  async #status(sessionId: string): Promise<"idle" | "busy" | "unknown" | undefined> {
    const result = await this.call("/session/status", "GET", undefined);
    if (!result.ok) return undefined;
    const statuses = z
      .record(
        z.string(),
        z.discriminatedUnion("type", [
          z.object({ type: z.literal("idle") }).loose(),
          z.object({ type: z.literal("busy") }).loose(),
          z.object({ type: z.literal("retry") }).loose(),
        ]),
      )
      .safeParse(result.value);
    const status = statuses.success ? statuses.data[sessionId] : undefined;
    return status?.type === "idle" ? "idle" : status === undefined ? undefined : "busy";
  }

  #observeOpenCodeEvent(raw: unknown): void {
    const text = openCodeText(raw);
    if (text !== undefined) {
      this.#assistantMessageId = text.messageId;
      this.#assistantText = text.append ? this.#assistantText + text.text : text.text;
    }
    const observed = publicOpenCodeEvent(raw, this.#session?.nativeSessionId);
    if (observed === null) return;
    const event = z
      .looseObject({ kind: z.string(), data: z.unknown().optional() })
      .safeParse(observed);
    if (event.success && event.data.kind === "session_status") {
      const status = z.looseObject({ type: z.string() }).safeParse(event.data.data);
      if (status.success) this.#activity = status.data.type === "idle" ? "idle" : "busy";
    }
    if (event.success && event.data.kind === "turn_completed") {
      if (this.#assistantMessageId !== null && this.#assistantText !== "")
        for (const listener of this.#listeners)
          listener({
            kind: "item_completed",
            nativeItemId: this.#assistantMessageId,
            data: { type: "agentMessage", phase: "final_answer", text: this.#assistantText },
          });
      this.#assistantMessageId = null;
      this.#assistantText = "";
      this.#activity = "idle";
    }
    for (const listener of this.#listeners) listener(observed);
  }
}

function safeHttpFailure(status: number, text: string, raw: unknown): string {
  const parsed = z
    .looseObject({
      _tag: z.enum(["BadRequest", "InvalidRequestError"]).optional(),
      kind: z.string().optional(),
      field: z.string().optional(),
    })
    .safeParse(raw);
  const details = parsed.success
    ? [parsed.data._tag, safeToken(parsed.data.kind), safeToken(parsed.data.field)].filter(
        (value): value is string => value !== undefined,
      )
    : [];
  const bytes = Buffer.byteLength(text);
  const digest = createHash("sha256").update(text).digest("hex");
  const classification = details.length === 0 ? "unclassified" : details.join("/");
  return `provider HTTP returned ${String(status)} (${classification}; detail sha256=${digest} bytes=${String(bytes)})`;
}

function safeToken(value: string | undefined): string | undefined {
  return value !== undefined && /^[A-Za-z0-9_.:-]{1,80}$/u.test(value) ? value : undefined;
}

function reconnectDelay(signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(done, 100);
    signal.addEventListener("abort", done, { once: true });
    function done() {
      clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    }
  });
}

function publicOpenCodeEvent(raw: unknown, sessionId: string | undefined): unknown {
  const event = z.record(z.string(), z.unknown()).safeParse(raw);
  if (!event.success) return { type: "provider_event", eventType: "invalid_json" };
  const eventType = typeof event.data["type"] === "string" ? event.data["type"] : "unknown";
  const properties = z.record(z.string(), z.unknown()).safeParse(event.data["properties"]);
  const part = properties.success
    ? z.record(z.string(), z.unknown()).safeParse(properties.data["part"])
    : { success: false as const };
  const observedSession =
    part.success && typeof part.data["sessionID"] === "string"
      ? part.data["sessionID"]
      : properties.success && typeof properties.data["sessionID"] === "string"
        ? properties.data["sessionID"]
        : sessionId;
  if (sessionId !== undefined && observedSession !== undefined && observedSession !== sessionId)
    return null;
  if (
    eventType === "message.part.delta" &&
    properties.success &&
    typeof properties.data["messageID"] === "string" &&
    typeof properties.data["delta"] === "string"
  )
    return {
      kind: "message_delta",
      nativeItemId: properties.data["messageID"],
      data: { text: properties.data["delta"] },
    };
  if (
    eventType === "message.part.updated" &&
    part.success &&
    part.data["type"] === "text" &&
    typeof part.data["text"] === "string"
  )
    return {
      kind: "message_delta",
      nativeItemId:
        typeof part.data["messageID"] === "string"
          ? part.data["messageID"]
          : typeof part.data["id"] === "string"
            ? part.data["id"]
            : null,
      data: { text: part.data["text"] },
    };
  if (eventType === "session.status" && properties.success)
    return { kind: "session_status", data: properties.data["status"] ?? {} };
  if (eventType === "session.idle") return { kind: "turn_completed", data: { status: "idle" } };
  return { type: "provider_event", eventType, session_id: observedSession ?? null };
}

function openCodeText(
  raw: unknown,
): { readonly messageId: string; readonly text: string; readonly append: boolean } | undefined {
  const event = z
    .looseObject({
      type: z.enum(["message.part.updated", "message.part.delta"]),
      properties: z.record(z.string(), z.unknown()),
    })
    .safeParse(raw);
  if (!event.success) return undefined;
  if (event.data.type === "message.part.delta") {
    const messageId = event.data.properties["messageID"];
    const delta = event.data.properties["delta"];
    return typeof messageId === "string" && typeof delta === "string"
      ? { messageId, text: delta, append: true }
      : undefined;
  }
  const part = z.record(z.string(), z.unknown()).safeParse(event.data.properties["part"]);
  if (!part.success || part.data["type"] !== "text" || typeof part.data["text"] !== "string")
    return undefined;
  const messageId = part.data["messageID"];
  return typeof messageId === "string"
    ? { messageId, text: part.data["text"], append: false }
    : undefined;
}

function openCodeModel(
  modelId: string,
): { readonly providerID: string; readonly modelID: string } | undefined {
  const separator = modelId.indexOf("/");
  return separator > 0 && separator < modelId.length - 1
    ? { providerID: modelId.slice(0, separator), modelID: modelId.slice(separator + 1) }
    : undefined;
}

function stringValue(value: unknown, key: string): string | undefined {
  const parsed = z.record(z.string(), z.unknown()).safeParse(value);
  if (!parsed.success) return undefined;
  const candidate = parsed.data[key];
  return typeof candidate === "string" ? candidate : undefined;
}

function selectedEffort(
  scope: CoordinatorStartInput | CoordinatorResumeInput,
  profile: ProviderCoordinatorProfile,
): string | null {
  return scope.reasoningEffort === undefined ? profile.effort : scope.reasoningEffort;
}

function failure(
  code: "invalid_input" | "not_found" | "protocol_error" | "unsupported" | "transport_lost",
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
