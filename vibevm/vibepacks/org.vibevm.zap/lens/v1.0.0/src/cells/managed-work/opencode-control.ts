/** Owned OpenCode server plus attached TUI managed control. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import { randomUUID } from "node:crypto";
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

interface OpenCodeState {
  readonly target: ManagedControlTarget;
  readonly publish: (event: Omit<ManagedSessionControlEvent, "sourceSequence">) => void;
  readonly process: ManagedControlProcess;
  readonly baseUrl: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly sessionId: string;
  readonly model: { readonly providerID: string; readonly modelID: string };
  readonly initialPrompt: string | null;
  readonly abort: AbortController;
  active: boolean;
}

export function createOpenCodeManagedControlAdapter(
  options: {
    readonly processFactory?: ManagedControlProcessFactory;
    readonly fetchImpl?: typeof fetch;
    readonly reservePort?: () => Promise<number>;
    readonly password?: () => string;
    readonly timeoutMs?: number;
  } = {},
): ManagedProviderControlAdapter {
  const factory = options.processFactory ?? nodeManagedControlProcessFactory();
  const fetchImpl = options.fetchImpl ?? fetch;
  const reservePort = options.reservePort ?? reserveLoopbackPort;
  const password = options.password ?? (() => randomUUID());
  const records = new WeakMap<object, OpenCodeState>();
  return {
    provider: "opencode",
    async prepare(input) {
      const parsed = parseLaunch(input.launch);
      if (!parsed.ok) return parsed;
      const port = await reservePort();
      const baseUrl = `http://127.0.0.1:${String(port)}`;
      const username = "zap";
      const secret = password();
      const headers = {
        authorization: `Basic ${Buffer.from(`${username}:${secret}`).toString("base64")}`,
        "content-type": "application/json",
      };
      const process = factory.spawn({
        executable: input.launch.executable,
        args: [
          ...parsed.value.prefix,
          "serve",
          "--pure",
          "--hostname",
          "127.0.0.1",
          "--port",
          String(port),
        ],
        cwd: input.launch.cwd,
        env: {
          ...input.launch.env,
          OPENCODE_SERVER_USERNAME: username,
          OPENCODE_SERVER_PASSWORD: secret,
        },
      });
      const ready = await waitReady(fetchImpl, baseUrl, headers, options.timeoutMs ?? 10_000);
      if (!ready) {
        process.kill();
        return fail("unavailable", "owned OpenCode server did not become ready");
      }
      const sessionId =
        parsed.value.resumeSessionId ??
        (await createSession(fetchImpl, baseUrl, headers, input.target.runId));
      if (sessionId === null) {
        process.kill();
        return fail("unavailable", "owned OpenCode session could not be created");
      }
      const abort = new AbortController();
      const state: OpenCodeState = {
        target: input.target,
        publish: input.publish,
        process,
        baseUrl,
        headers,
        sessionId,
        model: parsed.value.model,
        initialPrompt: parsed.value.initialPrompt,
        abort,
        active: false,
      };
      const key = {};
      records.set(key, state);
      process.onExit(() => {
        publish(state, "session_exited", null, null);
      });
      void observeEvents(state, fetchImpl);
      publish(state, "session_ready", null, null);
      return {
        ok: true,
        value: {
          state: key,
          launch: {
            ...input.launch,
            env: {
              ...input.launch.env,
              OPENCODE_SERVER_USERNAME: username,
              OPENCODE_SERVER_PASSWORD: secret,
            },
            args: [
              ...parsed.value.prefix,
              "attach",
              baseUrl,
              "--session",
              sessionId,
              "--dir",
              input.launch.cwd,
              "--pure",
            ],
          },
        },
      };
    },
    async activate(raw) {
      const found = record(records, raw);
      if (!found.ok) return found;
      if (found.value.initialPrompt === null) return { ok: true, value: null };
      const sent = await prompt(
        fetchImpl,
        found.value,
        `initial.${found.value.target.runId}`,
        found.value.initialPrompt,
      );
      return sent.ok ? { ok: true, value: null } : sent;
    },
    async interrupt(raw) {
      const found = record(records, raw);
      if (!found.ok) return found;
      if (!found.value.active) return { ok: true, value: "idle" };
      const response = await call(
        fetchImpl,
        found.value,
        `/session/${encodeURIComponent(found.value.sessionId)}/abort`,
        "POST",
        {},
      );
      if (!response.ok) return response;
      return z.boolean().safeParse(response.value).success
        ? { ok: true, value: "requested" }
        : fail("uncertain", "OpenCode abort acceptance was not confirmed");
    },
    async offer(raw, deliveryId, bodyMarkdown) {
      const found = record(records, raw);
      if (!found.ok) return found;
      if (found.value.active) return fail("conflict", "OpenCode session is busy");
      const sent = await prompt(fetchImpl, found.value, deliveryId, bodyMarkdown);
      return sent.ok ? { ok: true, value: { queued: false, correlation: deliveryId } } : sent;
    },
    close(raw) {
      const found = record(records, raw);
      if (!found.ok) return;
      found.value.abort.abort();
      found.value.process.kill();
      if (typeof raw === "object" && raw !== null) records.delete(raw);
    },
  };
}

function parseLaunch(launch: ProviderLaunch): ManagedWorkResult<{
  readonly prefix: readonly string[];
  readonly model: { readonly providerID: string; readonly modelID: string };
  readonly initialPrompt: string | null;
  readonly resumeSessionId: string | null;
}> {
  const modelAt = launch.args.indexOf("--model");
  const modelId = modelAt >= 0 ? launch.args[modelAt + 1] : undefined;
  const separator = modelId?.indexOf("/") ?? -1;
  if (modelId === undefined || separator < 1 || separator === modelId.length - 1)
    return fail("invalid_input", "OpenCode managed model must use provider/model format");
  const commandAt = launch.args.findIndex((argument) => argument === launch.cwd);
  const prefix = commandAt < 0 ? [] : launch.args.slice(0, commandAt);
  const promptAt = launch.args.indexOf("--prompt");
  const sessionAt = launch.args.indexOf("--session");
  return {
    ok: true,
    value: {
      prefix,
      model: { providerID: modelId.slice(0, separator), modelID: modelId.slice(separator + 1) },
      initialPrompt: promptAt < 0 ? null : (launch.args[promptAt + 1] ?? null),
      resumeSessionId: sessionAt < 0 ? null : (launch.args[sessionAt + 1] ?? null),
    },
  };
}

async function waitReady(
  fetchImpl: typeof fetch,
  baseUrl: string,
  headers: Readonly<Record<string, string>>,
  timeoutMs: number,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetchImpl(`${baseUrl}/doc`, { headers });
      if (response.ok) return true;
    } catch {
      // The owned server may still be binding its loopback socket.
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return false;
}

async function createSession(
  fetchImpl: typeof fetch,
  baseUrl: string,
  headers: Readonly<Record<string, string>>,
  title: string,
): Promise<string | null> {
  try {
    const response = await fetchImpl(`${baseUrl}/session`, {
      method: "POST",
      headers,
      body: JSON.stringify({ title }),
    });
    const body: unknown = response.ok ? await response.json() : null;
    const parsed = z.looseObject({ id: z.string().min(1) }).safeParse(body);
    return parsed.success ? parsed.data.id : null;
  } catch {
    return null;
  }
}

async function prompt(
  fetchImpl: typeof fetch,
  state: OpenCodeState,
  messageId: string,
  text: string,
): Promise<ManagedWorkResult<null>> {
  const response = await call(
    fetchImpl,
    state,
    `/session/${encodeURIComponent(state.sessionId)}/prompt_async`,
    "POST",
    { messageID: messageId, model: state.model, parts: [{ type: "text", text }] },
  );
  if (!response.ok) return response;
  state.active = true;
  publish(state, "turn_started", messageId, messageId);
  return { ok: true, value: null };
}

async function call(
  fetchImpl: typeof fetch,
  state: Pick<OpenCodeState, "baseUrl" | "headers">,
  path: string,
  method: "GET" | "POST",
  body: unknown,
): Promise<ManagedWorkResult<unknown>> {
  try {
    const response = await fetchImpl(`${state.baseUrl}${path}`, {
      method,
      headers: state.headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const text = await response.text();
    return response.ok
      ? { ok: true, value: text === "" ? null : JSON.parse(text) }
      : fail("unavailable", `OpenCode control HTTP returned ${String(response.status)}`);
  } catch {
    return fail("uncertain", "OpenCode control HTTP acceptance is uncertain");
  }
}

async function observeEvents(state: OpenCodeState, fetchImpl: typeof fetch): Promise<void> {
  try {
    const response = await fetchImpl(`${state.baseUrl}/event`, {
      headers: state.headers,
      signal: state.abort.signal,
    });
    if (!response.ok || response.body === null) return;
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    while (!state.abort.signal.aborted) {
      const next = await reader.read();
      if (next.done) break;
      buffer += decoder.decode(next.value, { stream: true });
      const frames = buffer.split("\n\n");
      buffer = frames.pop() ?? "";
      for (const frame of frames) observeEvent(state, frame);
    }
  } catch {
    // The terminal remains visible; reconciliation will report lost sidecar observation.
  }
}

function observeEvent(state: OpenCodeState, frame: string): void {
  const data = frame
    .split("\n")
    .find((line) => line.startsWith("data:"))
    ?.slice(5)
    .trim();
  if (data === undefined) return;
  let raw: unknown;
  try {
    raw = JSON.parse(data);
  } catch {
    return;
  }
  const event = z
    .looseObject({ type: z.string(), properties: z.record(z.string(), z.unknown()) })
    .safeParse(raw);
  if (!event.success) return;
  const sessionId = event.data.properties["sessionID"];
  if (typeof sessionId === "string" && sessionId !== state.sessionId) return;
  if (event.data.type === "permission.asked") {
    const permission = z.looseObject({ id: z.string().min(1) }).safeParse(event.data.properties);
    publish(state, "permission_required", permission.success ? permission.data.id : null, null);
  } else if (event.data.type === "session.status") {
    const status = z.looseObject({ type: z.string() }).safeParse(event.data.properties["status"]);
    if (status.success && status.data.type !== "idle") state.active = true;
  } else if (event.data.type === "session.idle") {
    state.active = false;
    publish(state, "turn_settled", null, null);
  }
}

function publish(
  state: OpenCodeState,
  kind: ManagedSessionControlEvent["kind"],
  providerTurnId: string | null,
  correlation: string | null,
): void {
  state.publish({
    eventId: `opencode-control.${randomUUID().replaceAll("-", "")}`,
    runId: state.target.runId,
    actorId: state.target.actorId,
    sessionId: state.target.sessionId,
    terminalId: state.target.terminalId,
    provider: "opencode",
    processEpoch: state.target.expectedProcessEpoch,
    kind,
    providerSessionId: state.sessionId,
    providerTurnId,
    transportCorrelation: correlation,
    occurredAt: new Date().toISOString(),
  });
}

function record(
  records: WeakMap<object, OpenCodeState>,
  raw: unknown,
): ManagedWorkResult<OpenCodeState> {
  if (typeof raw !== "object" || raw === null)
    return fail("conflict", "OpenCode managed control state is stale");
  const state = records.get(raw);
  return state === undefined
    ? fail("conflict", "OpenCode managed control state is stale")
    : { ok: true, value: state };
}

function fail(
  code: "invalid_input" | "conflict" | "uncertain" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
