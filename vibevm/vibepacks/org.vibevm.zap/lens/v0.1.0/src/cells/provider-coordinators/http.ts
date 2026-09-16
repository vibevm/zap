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
import { randomUUID } from "node:crypto";

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
      parentID: null,
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
    this.#startOpenCodeEvents();
    if (input.scope.bootstrapText !== "") {
      const bootstrapped = await this.prompt(
        input.scope.bootstrapText,
        `bootstrap:${input.scope.coordinatorSessionId}`,
      );
      if (!bootstrapped.ok) return bootstrapped;
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
    return this.prompt(input.text, input.clientMessageId);
  }

  async prompt(text: string, clientMessageId = "") {
    const path = `/session/${encodeURIComponent(this.#session?.nativeSessionId ?? "")}/prompt_async`;
    const response = await this.call(path, "POST", {
      messageID: clientMessageId,
      model: this.#session?.modelId ?? this.#profile.modelId,
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
              const observed = publicOpenCodeEvent(raw, this.#session?.nativeSessionId);
              if (observed !== null) for (const listener of this.#listeners) listener(observed);
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
      const raw: unknown = text === "" ? null : JSON.parse(text);
      return response.ok
        ? { ok: true as const, value: raw }
        : failure("protocol_error", `provider HTTP returned ${String(response.status)}`);
    } catch {
      return failure("transport_lost", "provider loopback HTTP request failed");
    }
  }
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
    eventType === "message.part.updated" &&
    part.success &&
    part.data["type"] === "text" &&
    typeof part.data["text"] === "string"
  )
    return { type: "text", session_id: observedSession, text: part.data["text"] };
  if (eventType === "session.idle") return { type: "turn.completed", session_id: observedSession };
  return { type: "provider_event", eventType, session_id: observedSession ?? null };
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
  code: "not_found" | "protocol_error" | "unsupported" | "transport_lost",
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
