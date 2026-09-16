/** Claude and Qwen control sidebands for real owned PTYs.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import { createHash, randomUUID } from "node:crypto";
import {
  appendFileSync,
  mkdirSync,
  readFileSync,
  unwatchFile,
  watchFile,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { z } from "zod";
import type {
  ManagedControlTarget,
  ManagedProviderControlAdapter,
  ManagedSessionControlEvent,
} from "./control.ts";
import {
  openManagedHookServer,
  type ManagedHookEvent,
  type ManagedHookRegistration,
  type ManagedHookServer,
} from "./hook-server.ts";
import type { ManagedProviderId } from "./provider-types.ts";
import type { ManagedWorkResult } from "./contracts.ts";

interface ProviderState {
  readonly target: ManagedControlTarget;
  readonly provider: ManagedProviderId;
  readonly publish: (event: Omit<ManagedSessionControlEvent, "sourceSequence">) => void;
  readonly hook: ManagedHookRegistration | null;
  readonly settingsPath: string;
  readonly inputPath: string | null;
  readonly outputPath: string | null;
  outputLines: number;
  providerSessionId: string | null;
  interruptPending: boolean;
}

export function createManagedProviderControlAdapters(input: {
  readonly directory: string;
}): readonly ManagedProviderControlAdapter[] {
  mkdirSync(input.directory, { recursive: true });
  let hookServer: Promise<ManagedHookServer> | undefined;
  const hooks = (): Promise<ManagedHookServer> => {
    hookServer ??= openManagedHookServer();
    return hookServer;
  };
  const records = new WeakMap<object, ProviderState>();
  const active = new Set<ProviderState>();
  const adapter = (provider: "claude_code" | "qwen_code"): ManagedProviderControlAdapter => ({
    provider,
    canQueueWhileBusy: provider === "qwen_code",
    async prepare(prepared) {
      const holder: { state?: ProviderState } = {};
      const hook =
        provider === "claude_code"
          ? (await hooks()).register((event) => {
              if (holder.state !== undefined) observeHook(holder.state, event);
            })
          : null;
      const state = createState(input.directory, provider, prepared.target, prepared.publish, hook);
      holder.state = state;
      const launch =
        provider === "claude_code"
          ? claudeLaunch(prepared.launch, state)
          : qwenLaunch(prepared.launch, state);
      const key = {};
      records.set(key, state);
      active.add(state);
      if (provider === "qwen_code") watchQwen(state);
      return { ok: true, value: { launch, state: key } };
    },
    async interrupt(raw, reason, io) {
      await Promise.resolve();
      const state = record(records, raw);
      if (!state.ok) return state;
      if (
        state.value.provider === "claude_code" &&
        reason === "project_pause" &&
        state.value.providerSessionId === null
      )
        return fail("unavailable", "Claude pause has no observed saved session to resume");
      state.value.interruptPending = true;
      const interrupted =
        state.value.provider === "claude_code" && reason === "project_pause"
          ? io.stop()
          : io.interrupt();
      if (!interrupted.ok) state.value.interruptPending = false;
      return interrupted.ok ? { ok: true, value: "requested" } : interrupted;
    },
    async offer(raw, deliveryId, bodyMarkdown, io) {
      await Promise.resolve();
      const state = record(records, raw);
      if (!state.ok) return state;
      if (state.value.provider === "qwen_code" && state.value.inputPath !== null) {
        try {
          appendFileSync(
            state.value.inputPath,
            `${JSON.stringify({ type: "submit", text: bodyMarkdown, delivery_id: deliveryId })}\n`,
            "utf8",
          );
          return { ok: true, value: { queued: true, correlation: deliveryId } };
        } catch {
          return fail("uncertain", "Qwen input sideband acceptance is uncertain");
        }
      }
      const encoded = claudeInput(bodyMarkdown);
      if (!encoded.ok) return encoded;
      const accepted = io.input(encoded.value);
      return accepted.ok
        ? { ok: true, value: { queued: false, correlation: deliveryId } }
        : accepted;
    },
    close(raw) {
      if (!object(raw)) return;
      const state = records.get(raw);
      if (state === undefined) return;
      state.hook?.close();
      if (state.outputPath !== null) unwatchFile(state.outputPath);
      records.delete(raw);
      active.delete(state);
      if (active.size === 0 && hookServer !== undefined) {
        const closing = hookServer;
        hookServer = undefined;
        void closing.then((server) => server.close());
      }
    },
  });
  return [adapter("claude_code"), adapter("qwen_code")];
}

function createState(
  directory: string,
  provider: "claude_code" | "qwen_code",
  target: ManagedControlTarget,
  publish: ProviderState["publish"],
  hook: ManagedHookRegistration | null,
): ProviderState {
  const base = join(directory, `${provider}.${digest(target.runId)}`);
  const inputPath = provider === "qwen_code" ? `${base}.input.jsonl` : null;
  const outputPath = provider === "qwen_code" ? `${base}.events.jsonl` : null;
  const settingsPath = `${base}.settings.json`;
  if (hook !== null) {
    const hookCommand = {
      type: "http",
      url: hook.url,
      headers: { Authorization: hook.authorization },
      timeout: 10,
      name: "zap-managed-control",
    };
    writeFileSync(settingsPath, JSON.stringify(hookSettings(hookCommand)), {
      encoding: "utf8",
      mode: 0o600,
    });
  }
  if (inputPath !== null) writeFileSync(inputPath, "", { encoding: "utf8", mode: 0o600 });
  if (outputPath !== null) writeFileSync(outputPath, "", { encoding: "utf8", mode: 0o600 });
  return {
    target,
    provider,
    publish,
    hook,
    settingsPath,
    inputPath,
    outputPath,
    outputLines: 0,
    providerSessionId: null,
    interruptPending: false,
  };
}

function hookSettings(hook: Record<string, unknown>) {
  const hooks = (matcher?: string) => [
    { ...(matcher === undefined ? {} : { matcher }), hooks: [hook] },
  ];
  return {
    hooks: {
      SessionStart: hooks(),
      UserPromptSubmit: hooks(),
      Stop: hooks(),
      StopFailure: hooks(),
      PermissionRequest: hooks("*"),
      Notification: [...hooks("idle_prompt"), ...hooks("permission_prompt")],
      SessionEnd: hooks(),
    },
  };
}

function claudeLaunch(
  launch: Parameters<ManagedProviderControlAdapter["prepare"]>[0]["launch"],
  state: ProviderState,
) {
  const prompt = launch.args.at(-1);
  return {
    ...launch,
    args:
      prompt === undefined
        ? [...launch.args, "--settings", state.settingsPath]
        : [...launch.args.slice(0, -1), "--settings", state.settingsPath, prompt],
  };
}

function qwenLaunch(
  launch: Parameters<ManagedProviderControlAdapter["prepare"]>[0]["launch"],
  state: ProviderState,
) {
  if (state.inputPath === null || state.outputPath === null) return launch;
  const args = [...launch.args];
  const prompt = args.indexOf("--prompt-interactive");
  const sideband = ["--json-file", state.outputPath, "--input-file", state.inputPath];
  return {
    ...launch,
    args:
      prompt < 0
        ? [...args, ...sideband]
        : [...args.slice(0, prompt), ...sideband, ...args.slice(prompt)],
  };
}

function observeHook(state: ProviderState, event: ManagedHookEvent): void {
  if (event.session_id !== undefined) state.providerSessionId = event.session_id;
  const notification = event.notification_type;
  if (event.hook_event_name === "SessionStart") publish(state, "session_ready", event.prompt_id);
  else if (event.hook_event_name === "UserPromptSubmit")
    publish(state, "turn_started", event.prompt_id);
  else if (
    event.hook_event_name === "PermissionRequest" ||
    (event.hook_event_name === "Notification" && notification === "permission_prompt")
  )
    publish(state, "permission_required", event.tool_use_id);
  else if (event.hook_event_name === "Notification" && notification === "idle_prompt")
    publish(state, "turn_settled", event.prompt_id);
  else if (event.hook_event_name === "SessionEnd") publish(state, "session_exited", null);
}

function watchQwen(state: ProviderState): void {
  if (state.outputPath === null) return;
  watchFile(state.outputPath, { interval: 100 }, () => {
    let text: string;
    try {
      text = readFileSync(state.outputPath ?? "", "utf8");
    } catch {
      return;
    }
    const complete = text.endsWith("\n")
      ? text.split(/\r?\n/).slice(0, -1)
      : text.split(/\r?\n/).slice(0, -1);
    if (complete.length < state.outputLines) state.outputLines = 0;
    for (const line of complete.slice(state.outputLines)) observeQwenLine(state, line);
    state.outputLines = complete.length;
  });
}

function observeQwenLine(state: ProviderState, line: string): void {
  let raw: unknown;
  try {
    raw = JSON.parse(line);
  } catch {
    return;
  }
  const event = z
    .looseObject({
      type: z.string(),
      subtype: z.string().optional(),
      session_id: z.string().optional(),
      uuid: z.string().optional(),
      request_id: z.string().optional(),
      message: z.looseObject({ stop_reason: z.string().nullable().optional() }).optional(),
    })
    .safeParse(raw);
  if (!event.success) return;
  if (event.data.session_id !== undefined) state.providerSessionId = event.data.session_id;
  if (event.data.type === "system" && event.data.subtype === "session_start")
    publish(state, "session_ready", null);
  else if (event.data.type === "user") publish(state, "turn_started", event.data.uuid);
  else if (event.data.type === "control_request")
    publish(state, "permission_required", event.data.request_id);
  else if (
    event.data.type === "result" ||
    (event.data.type === "assistant" && event.data.message?.stop_reason !== "tool_use")
  ) {
    if (state.interruptPending || event.data.type === "result") {
      state.interruptPending = false;
      publish(state, "turn_settled", event.data.uuid);
    }
  }
}

function claudeInput(bodyMarkdown: string): ManagedWorkResult<string> {
  const normalized = bodyMarkdown.replaceAll("\r\n", "\n");
  if (containsForbiddenTerminalCharacter(normalized))
    return fail("invalid_input", "managed Claude notice contains terminal control characters");
  return { ok: true, value: `\u001b[200~${normalized}\u001b[201~\r` };
}

function containsForbiddenTerminalCharacter(value: string): boolean {
  for (const character of value) {
    const code = character.codePointAt(0) ?? 0;
    if (
      character === "\r" ||
      (code < 32 && character !== "\n" && character !== "\t") ||
      code === 127
    )
      return true;
  }
  return false;
}

function publish(
  state: ProviderState,
  kind: ManagedSessionControlEvent["kind"],
  providerTurnId: string | null | undefined,
): void {
  state.publish({
    eventId: `control-event.${randomUUID().replaceAll("-", "")}`,
    runId: state.target.runId,
    actorId: state.target.actorId,
    sessionId: state.target.sessionId,
    terminalId: state.target.terminalId,
    provider: state.provider,
    processEpoch: state.target.expectedProcessEpoch,
    kind,
    providerSessionId: state.providerSessionId,
    providerTurnId: providerTurnId ?? null,
    transportCorrelation: null,
    occurredAt: new Date().toISOString(),
  });
}

function record(
  records: WeakMap<object, ProviderState>,
  state: unknown,
): ManagedWorkResult<ProviderState> {
  const value = object(state) ? records.get(state) : undefined;
  return value === undefined
    ? fail("conflict", "managed provider control state is stale")
    : { ok: true, value };
}

function object(value: unknown): value is object {
  return typeof value === "object" && value !== null;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}

function fail(
  code: "invalid_input" | "conflict" | "uncertain" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
