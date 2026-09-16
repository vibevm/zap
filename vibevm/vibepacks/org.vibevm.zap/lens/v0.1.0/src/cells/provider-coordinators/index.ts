/** Provider-neutral coordinator adapters. @scope spec://org.vibevm.zap/lens/PROP-006#provider-adapters */
import { z } from "zod";
import { createHash } from "node:crypto";
import {
  CoordinatorCapabilitiesSchema,
  CoordinatorLifecycleCapabilitiesSchema,
  CoordinatorSessionDescriptorSchema,
  type AgentHost,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorCapabilities,
  type CoordinatorEvent,
  type CoordinatorHistory,
  type CoordinatorLifecycleInput,
  type CoordinatorLifecycleReceipt,
  type CoordinatorResumeInput,
  type CoordinatorScope,
  type CoordinatorStartInput,
  type CoordinatorTurnInput,
  type CoordinatorTurnReceipt,
  type HostRequestAnswer,
} from "../agent-runtime/index.ts";
import { ExecutionHostIdSchema, NativeRefSchema } from "../workspace-model/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";

export const ProviderCoordinatorIdSchema = z.enum(["claude_code", "opencode", "qwen_code"]);
export type ProviderCoordinatorId = z.infer<typeof ProviderCoordinatorIdSchema>;

export const ProviderCoordinatorProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    provider: ProviderCoordinatorIdSchema,
    executablePath: z.string().min(1).max(32_768),
    cwd: z.string().min(1).max(32_768),
    modelId: z.string().min(1).max(256),
    effort: z
      .enum(["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"])
      .nullable(),
    endpoint: z.url().nullable(),
    proxy: ProxyPolicySchema.optional(),
    argumentPrefix: z.array(z.string().max(32_768)).max(32).optional(),
    environmentRef: z.string().min(3).max(512).nullable().optional(),
    mcpConfigPath: z.string().min(1).max(32_768).nullable().optional(),
    mcpCommandPath: z.string().min(1).max(32_768).optional(),
    mcpArgs: z.array(z.string().max(16_384)).max(64).optional(),
  })
  .strict();
export type ProviderCoordinatorProfile = z.infer<typeof ProviderCoordinatorProfileSchema>;

export interface ProviderCoordinatorSession {
  readonly coordinatorSessionId: CoordinatorScope["coordinatorSessionId"];
  readonly nativeSessionId: string;
  readonly nativeThreadId: string;
  readonly processEpoch: string;
  readonly modelId: string;
  readonly effort: Exclude<CoordinatorStartInput["reasoningEffort"], undefined>;
}

export interface ProviderCoordinatorTransport {
  start(input: {
    readonly scope: CoordinatorStartInput;
  }): Promise<ProviderResult<ProviderCoordinatorSession>>;
  resume(input: {
    readonly scope: CoordinatorResumeInput;
  }): Promise<ProviderResult<ProviderCoordinatorSession>>;
  history(session: ProviderCoordinatorSession): Promise<ProviderResult<CoordinatorHistory>>;
  send(input: {
    readonly session: ProviderCoordinatorSession;
    readonly text: string;
    readonly clientMessageId: string;
  }): Promise<
    ProviderResult<{
      readonly nativeTurnId: string | null;
      readonly transportCorrelation: {
        readonly provenance: "transport_correlation";
        readonly clientMessageId: string;
        readonly processEpoch: string;
      } | null;
    }>
  >;
  interrupt(input: {
    readonly session: ProviderCoordinatorSession;
    readonly nativeTurnId: string;
  }): Promise<ProviderResult<null>>;
  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }): Promise<ProviderResult<null>>;
  stop(input: { readonly session: ProviderCoordinatorSession }): Promise<ProviderResult<null>>;
  subscribe(listener: (raw: unknown) => void): () => void;
  close(): void;
}

export interface ProviderCoordinatorTransportFactory {
  open(profile: ProviderCoordinatorProfile): Promise<ProviderResult<ProviderCoordinatorTransport>>;
}

export interface ProviderLaunchPreparationPort {
  prepare(input: {
    readonly profile: ProviderCoordinatorProfile;
    readonly scope: CoordinatorStartInput | CoordinatorResumeInput;
  }): Promise<
    | {
        readonly ok: true;
        readonly value: {
          readonly environment: Readonly<Record<string, string>>;
          readonly mcpConfigPath: string;
        };
      }
    | {
        readonly ok: false;
        readonly error: {
          readonly code: "policy_denied" | "host_refused";
          readonly message: string;
          readonly retry: "never" | "after_refresh";
        };
      }
  >;
}

export interface ProviderCoordinatorHost extends AgentHost {
  readonly provider: ProviderCoordinatorId;
}

export function createProviderCoordinatorHost(input: {
  readonly hostId: string;
  readonly profile: ProviderCoordinatorProfile;
  readonly transportFactory: ProviderCoordinatorTransportFactory;
}): ProviderCoordinatorHost {
  const hostId = ExecutionHostIdSchema.parse(input.hostId);
  return {
    hostId,
    provider: input.profile.provider,
    profileIds: [input.profile.profileId],
    async openCoordinator(profileId) {
      if (profileId !== input.profile.profileId)
        return failure("not_found", "provider profile is not registered");
      const transport = await input.transportFactory.open(input.profile);
      if (!transport.ok) return transport;
      return createProviderCoordinatorAdapter({
        profile: input.profile,
        hostId,
        transport: transport.value,
      });
    },
  };
}

export function createProviderCoordinatorAdapter(input: {
  readonly profile: ProviderCoordinatorProfile;
  readonly hostId: string;
  readonly transport: ProviderCoordinatorTransport;
}): AgentRuntimeResult<CoordinatorAdapter> {
  const profile = ProviderCoordinatorProfileSchema.parse(input.profile);
  const capabilities = capabilitiesFor(profile.provider);
  const lifecycleCapabilities = lifecycleFor(profile.provider);
  let current: ProviderCoordinatorSession | undefined;
  let scope: CoordinatorScope | undefined;
  let descriptor: z.infer<typeof CoordinatorSessionDescriptorSchema> | undefined;
  let listener: ((event: CoordinatorEvent) => void) | undefined;
  let resumeContext: CoordinatorResumeInput | undefined;
  let stopRequested = false;
  let stoppedObserved = false;
  const pendingRaw: unknown[] = [];
  const unsubscribe = input.transport.subscribe((raw) => {
    if (scope === undefined || current === undefined) {
      if (pendingRaw.length < 1_024) pendingRaw.push(raw);
      return;
    }
    emit(raw);
  });
  const emit = (raw: unknown) => {
    if (scope === undefined || current === undefined) return;
    const event = normalizeProviderEvent(
      profile.provider,
      scope.coordinatorSessionId,
      current,
      raw,
    );
    if (event?.kind === "process_exited") stoppedObserved = true;
    if (event !== null && listener !== undefined) listener(event);
  };
  const flushPending = () => {
    for (const raw of pendingRaw.splice(0)) emit(raw);
  };
  const adapter: CoordinatorAdapter = {
    capabilities,
    lifecycleCapabilities,
    async start(startInput) {
      const started = await input.transport.start({ scope: startInput });
      if (!started.ok) return started;
      scope = startInput;
      current = started.value;
      descriptor = descriptorFor(
        startInput,
        profile,
        input.hostId,
        current,
        capabilities,
        "submitted",
      );
      resumeContext = resumeInputFor(startInput, current);
      stopRequested = false;
      stoppedObserved = false;
      flushPending();
      return { ok: true, value: descriptor };
    },
    async resume(resumeInput) {
      const resumed = await input.transport.resume({ scope: resumeInput });
      if (!resumed.ok) return resumed;
      scope = resumeInput;
      current = resumed.value;
      descriptor = descriptorFor(
        resumeInput,
        profile,
        input.hostId,
        current,
        capabilities,
        "submitted",
      );
      resumeContext = resumeInputFor(resumeInput, current);
      stopRequested = false;
      stoppedObserved = false;
      flushPending();
      return { ok: true, value: descriptor };
    },
    async readHistory(sessionId) {
      if (!current || !scope || scope.coordinatorSessionId !== sessionId)
        return failure("not_found", "provider session is not active");
      return input.transport.history(current);
    },
    readNativeChildHistory() {
      return Promise.resolve(
        failure("unsupported", "provider does not expose a verified native child history surface"),
      );
    },
    async startTurn(turn) {
      return send(turn);
    },
    async steer(turn) {
      return send(turn);
    },
    async send(turn) {
      return send(turn);
    },
    async interrupt(sessionId, nativeTurnId) {
      if (!current || !scope || scope.coordinatorSessionId !== sessionId)
        return failure("not_found", "provider session is not active");
      const interrupted = await input.transport.interrupt({ session: current, nativeTurnId });
      return interrupted.ok ? { ok: true, value: undefined } : interrupted;
    },
    async respondToRequest(answer) {
      if (!current) return failure("not_found", "provider session is not active");
      const responded = await input.transport.respond({ session: current, answer });
      return responded.ok ? { ok: true, value: undefined } : responded;
    },
    restart() {
      return Promise.resolve(
        failure("unsupported", "provider restart requires an explicit host-owned session factory"),
      );
    },
    async pause(lifecycle) {
      return lifecycleAction("pause", lifecycle);
    },
    async stop(lifecycle) {
      if (!current || !scope || lifecycle.coordinatorSessionId !== scope.coordinatorSessionId)
        return failure("not_found", "provider session is not active");
      stopRequested = true;
      stoppedObserved = false;
      const stopped = await input.transport.stop({ session: current });
      if (!stopped.ok) return stopped;
      return lifecycleReceipt("stop", lifecycle, current, "requested");
    },
    async continueSession(lifecycle) {
      if (
        current === undefined ||
        scope === undefined ||
        resumeContext === undefined ||
        lifecycle.coordinatorSessionId !== scope.coordinatorSessionId
      )
        return failure("not_found", "provider session is not active");
      if (lifecycle.expectedProcessEpoch !== current.processEpoch)
        return failure("stale_epoch", "provider process epoch changed before continuation");
      if (!stopRequested || !stoppedObserved)
        return lifecycleReceipt("continue", lifecycle, current, "uncertain");
      const previous = current;
      const resumed = await input.transport.resume({ scope: resumeContext });
      if (!resumed.ok) return resumed;
      current = resumed.value;
      resumeContext = resumeInputFor(resumeContext, current);
      descriptor = descriptorFor(
        resumeContext,
        profile,
        input.hostId,
        current,
        capabilities,
        "not_observed",
      );
      stopRequested = false;
      stoppedObserved = false;
      flushPending();
      return lifecycleReceipt("continue", lifecycle, current, "settled", previous.processEpoch);
    },
    subscribe(next) {
      listener = next;
      return () => {
        if (listener === next) listener = undefined;
      };
    },
    close() {
      unsubscribe();
      input.transport.close();
      current = undefined;
      scope = undefined;
      descriptor = undefined;
    },
  };
  return { ok: true, value: adapter };

  async function send(
    turn: CoordinatorTurnInput,
  ): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    if (!current || !scope || scope.coordinatorSessionId !== turn.coordinatorSessionId)
      return failure("not_found", "provider session is not active");
    const sent = await input.transport.send({
      session: current,
      text: turn.text,
      clientMessageId: turn.clientMessageId,
    });
    return sent.ok
      ? {
          ok: true,
          value: {
            coordinatorSessionId: turn.coordinatorSessionId,
            nativeThreadId: current.nativeThreadId,
            nativeTurnId: sent.value.nativeTurnId,
            transportCorrelation: sent.value.transportCorrelation,
            observation: "host_accepted",
            processEpoch: current.processEpoch,
          },
        }
      : sent;
  }

  function lifecycleAction(
    action: "pause",
    lifecycle: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    if (lifecycleCapabilities.pause === "unsupported")
      return Promise.resolve(lifecycleReceipt(action, lifecycle, current, "unsupported"));
    return Promise.resolve(lifecycleReceipt(action, lifecycle, current, "unsupported"));
  }
}

function capabilitiesFor(provider: ProviderCoordinatorId): CoordinatorCapabilities {
  void provider;
  return CoordinatorCapabilitiesSchema.parse({
    persistentThreads: true,
    turnStart: true,
    activeTurnSteer: false,
    turnInterrupt: true,
    historyRead: true,
    nativeChildObservation: false,
    nativeChildDirectInput: false,
    structuredUserInput: false,
    commandApproval: false,
    managedTerminal: false,
  });
}

function lifecycleFor(provider: ProviderCoordinatorId) {
  void provider;
  return CoordinatorLifecycleCapabilitiesSchema.parse({
    pause: "unsupported",
    stop: "owned_process",
    continue: "saved_thread_resume",
    nativeChildren: "unsupported",
  });
}

function descriptorFor(
  input: CoordinatorStartInput | CoordinatorResumeInput,
  profile: ProviderCoordinatorProfile,
  hostId: string,
  session: ProviderCoordinatorSession,
  capabilities: CoordinatorCapabilities,
  bootstrap: "submitted" | "not_observed",
) {
  return CoordinatorSessionDescriptorSchema.parse({
    coordinatorSessionId: input.coordinatorSessionId,
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: input.conversationId,
    coordinatorActorId: input.coordinatorActorId,
    hostId,
    profileId: profile.profileId,
    productId: profile.provider,
    role: "coordinator",
    launchOrigin: "lens",
    interactionKind: "structured",
    state: "ready",
    nativeThreadRef: NativeRefSchema.parse({
      namespace: `${profile.provider}.thread`,
      value: session.nativeThreadId,
      incarnation: BigInt(
        `0x${createHash("sha256").update(session.processEpoch).digest("hex").slice(0, 16)}`,
      ).toString(),
    }),
    nativeSessionId: session.nativeSessionId,
    cwd: input.cwd,
    processEpoch: session.processEpoch,
    bootstrap,
    instructionSources: ["lens.provider-coordinator.v1"],
    capabilities,
  });
}

function normalizeProviderEvent(
  provider: ProviderCoordinatorId,
  sessionId: CoordinatorScope["coordinatorSessionId"],
  session: ProviderCoordinatorSession,
  raw: unknown,
): CoordinatorEvent | null {
  const parsed = z
    .object({
      kind: z.string().optional(),
      nativeTurnId: z.string().nullable().optional(),
      nativeItemId: z.string().nullable().optional(),
      data: z.unknown().optional(),
    })
    .catchall(z.unknown())
    .safeParse(raw);
  if (!parsed.success) return null;
  const providerEvent = providerEventKind(provider, parsed.data);
  const kind = providerEvent.kind;
  const mapped = z
    .enum([
      "message_delta",
      "item_completed",
      "turn_completed",
      "session_started",
      "session_resumed",
      "host_request_pending",
      "host_request_resolved",
      "process_exited",
      "host_event_unmapped",
    ])
    .catch("host_event_unmapped")
    .parse(kind);
  return {
    coordinatorSessionId: sessionId,
    processEpoch: session.processEpoch,
    nativeThreadId: session.nativeThreadId,
    nativeTurnId: providerEvent.nativeTurnId,
    nativeItemId: providerEvent.nativeItemId,
    kind: mapped,
    sourceEventId: `${provider}:${session.processEpoch}:${JSON.stringify(raw).slice(0, 240)}`,
    data: JsonValueSchema.safeParse(providerEvent.data).success
      ? JsonValueSchema.parse(providerEvent.data)
      : JSON.stringify(raw),
  };
}

function providerEventKind(
  provider: ProviderCoordinatorId,
  raw: Record<string, unknown>,
): {
  readonly kind: string | undefined;
  readonly nativeTurnId: string | null;
  readonly nativeItemId: string | null;
  readonly data: unknown;
} {
  const explicit = stringField(raw, "kind");
  const type = stringField(raw, "type");
  const nativeTurnId = stringField(raw, "nativeTurnId") ?? stringField(raw, "turn_id") ?? null;
  const nativeItemId = stringField(raw, "nativeItemId") ?? stringField(raw, "item_id") ?? null;
  if (explicit !== undefined)
    return { kind: explicit, nativeTurnId, nativeItemId, data: raw["data"] ?? raw };
  if (provider === "claude_code" && type === "stream_event") {
    const event = objectField(raw, "event");
    const eventType = stringField(event, "type");
    return {
      kind:
        eventType === "content_block_delta"
          ? "message_delta"
          : eventType === "message_stop"
            ? "turn_completed"
            : "host_event_unmapped",
      nativeTurnId: stringField(raw, "message_id") ?? nativeTurnId,
      nativeItemId: stringField(event, "index") ?? nativeItemId,
      data: event ?? raw,
    };
  }
  if ((provider === "claude_code" || provider === "qwen_code") && type === "assistant")
    return {
      kind: "message_delta",
      nativeTurnId,
      nativeItemId,
      data: raw["message"] ?? { role: "assistant", content: [] },
    };
  if ((provider === "opencode" || provider === "qwen_code") && type === "text")
    return { kind: "message_delta", nativeTurnId, nativeItemId, data: raw["text"] ?? raw };
  if (
    (provider === "opencode" || provider === "qwen_code") &&
    (type === "result" || type === "turn.completed")
  )
    return { kind: "turn_completed", nativeTurnId, nativeItemId, data: raw };
  return {
    kind: undefined,
    nativeTurnId,
    nativeItemId,
    data: { provider, eventType: type ?? "unknown" },
  };
}

function stringField(value: Record<string, unknown> | undefined, key: string): string | undefined {
  const field = value?.[key];
  return typeof field === "string" ? field : undefined;
}

function objectField(
  value: Record<string, unknown> | undefined,
  key: string,
): Record<string, unknown> | undefined {
  const field = value?.[key];
  return typeof field === "object" && field !== null && !Array.isArray(field)
    ? z.record(z.string(), z.unknown()).parse(field)
    : undefined;
}

function lifecycleReceipt(
  action: "pause" | "stop" | "continue",
  input: CoordinatorLifecycleInput,
  session: ProviderCoordinatorSession | undefined,
  observation: CoordinatorLifecycleReceipt["observation"],
  previousProcessEpoch = input.expectedProcessEpoch,
): AgentRuntimeResult<CoordinatorLifecycleReceipt> {
  return session === undefined
    ? failure("not_found", "provider session is not active")
    : {
        ok: true,
        value: {
          coordinatorSessionId: input.coordinatorSessionId,
          action,
          observation,
          previousProcessEpoch,
          currentProcessEpoch: session.processEpoch,
          nativeThreadId: session.nativeThreadId,
          targets: [],
          message:
            observation === "unsupported"
              ? "Provider does not expose this lifecycle action."
              : "Provider lifecycle action requested.",
        },
      };
}

function resumeInputFor(
  input: CoordinatorStartInput | CoordinatorResumeInput,
  session: ProviderCoordinatorSession,
): CoordinatorResumeInput {
  return {
    coordinatorSessionId: input.coordinatorSessionId,
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: input.conversationId,
    coordinatorActorId: input.coordinatorActorId,
    hostId: input.hostId,
    profileId: input.profileId,
    modelId: session.modelId,
    reasoningEffort: session.effort,
    cwd: input.cwd,
    nativeThreadId: session.nativeThreadId,
    ...(input.agentScope === undefined ? {} : { agentScope: input.agentScope }),
    ...(input.agentBinding === undefined ? {} : { agentBinding: input.agentBinding }),
  };
}

type ProviderResult<T> = AgentRuntimeResult<T>;
function failure(
  code: "invalid_input" | "unsupported" | "not_found" | "protocol_error" | "stale_epoch",
  message: string,
): AgentRuntimeResult<never> {
  return {
    ok: false,
    error: { code, message, retry: code === "protocol_error" ? "after_reconcile" : "never" },
  };
}

export { createClaudeStreamJsonTransportFactory } from "./claude.ts";
export type { ClaudeProcessFactory, OwnedLineProcess } from "./claude.ts";
export { createOpenCodeHttpTransportFactory, resolveProviderProxyEnvironment } from "./http.ts";
export type { ProviderHttpOptions } from "./http.ts";
export { createOpenCodeOwnedTransportFactory } from "./opencode.ts";
export type { OpenCodeDaemonFactory, OpenCodeDaemonProcess } from "./opencode.ts";
export { createQwenStreamJsonTransportFactory } from "./qwen.ts";
export type { QwenProcessFactory } from "./qwen.ts";
export { qwenZapMcpPermissionArguments } from "./qwen-permissions.ts";
