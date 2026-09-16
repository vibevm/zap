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
import { ExecutionCatalogIdSchema } from "../execution-catalog/index.ts";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import { normalizeProviderEvent } from "./events.ts";
import {
  guardedProviderTransportCall as transportCall,
  providerFailure as failure,
} from "./result.ts";
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
    accountBindingId: ExecutionCatalogIdSchema.optional(),
    executionHostId: ExecutionHostIdSchema.optional(),
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
  pause(input: {
    readonly session: ProviderCoordinatorSession;
  }): Promise<ProviderResult<{ readonly observation: "settled" | "requested" | "uncertain" }>>;
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
  const stopState = { observed: false };
  const stopWasObserved = () => stopState.observed;
  let pauseRequested = false;
  let paused = false;
  let activity: "idle" | "busy" | "unknown" = "unknown";
  let pendingCorrelation: Exclude<
    CoordinatorEvent["transportCorrelation"],
    null | undefined
  > | null = null;
  let observationEpoch: string | undefined;
  let observationSequence = 0n;
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
    if (observationEpoch !== current.processEpoch) {
      observationEpoch = current.processEpoch;
      observationSequence = 0n;
    }
    observationSequence += 1n;
    const event = normalizeProviderEvent(
      profile.provider,
      scope.coordinatorSessionId,
      current,
      raw,
      pendingCorrelation,
      observationSequence.toString(),
    );
    acceptEvent(event);
  };
  const acceptEvent = (event: CoordinatorEvent | null) => {
    if (event?.kind === "turn_started") activity = "busy";
    if (event?.kind === "turn_completed" || idleStatus(event)) activity = "idle";
    const delivered =
      event?.kind === "process_exited" && stopRequested
        ? { ...event, kind: "session_stopped" as const }
        : event;
    if (event?.kind === "process_exited") {
      stopState.observed = true;
      activity = "unknown";
      pendingCorrelation = null;
    }
    if (delivered !== null && listener !== undefined) listener(delivered);
    if (event?.kind === "turn_completed") pendingCorrelation = null;
    if (pauseRequested && !stopRequested && activity === "idle") settlePause();
  };
  const settlePause = () => {
    pauseRequested = false;
    paused = true;
    activity = "idle";
    emit({ kind: "session_paused", data: { observation: "settled" } });
  };
  const flushPending = () => {
    for (const raw of pendingRaw.splice(0)) emit(raw);
  };
  const adapter: CoordinatorAdapter = {
    capabilities,
    lifecycleCapabilities,
    async start(startInput) {
      const started = await transportCall(() => input.transport.start({ scope: startInput }));
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
      stopState.observed = false;
      pauseRequested = false;
      paused = false;
      activity = startInput.bootstrapText === "" ? "idle" : "busy";
      observationEpoch = current.processEpoch;
      observationSequence = 0n;
      flushPending();
      return { ok: true, value: descriptor };
    },
    async resume(resumeInput) {
      const resumed = await transportCall(() => input.transport.resume({ scope: resumeInput }));
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
      stopState.observed = false;
      pauseRequested = false;
      paused = false;
      activity = "idle";
      observationEpoch = current.processEpoch;
      observationSequence = 0n;
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
      return lifecycleAction(lifecycle);
    },
    async stop(lifecycle) {
      if (!current || !scope || lifecycle.coordinatorSessionId !== scope.coordinatorSessionId)
        return failure("not_found", "provider session is not active");
      stopRequested = true;
      stopState.observed = false;
      const stopped = await input.transport.stop({ session: current });
      if (!stopped.ok) return stopped;
      return lifecycleReceipt(
        "stop",
        lifecycle,
        current,
        stopWasObserved() ? "settled" : "requested",
      );
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
      if (paused) {
        paused = false;
        pauseRequested = false;
        emit({ kind: "session_continued", data: { resumedNativeThread: false } });
        return lifecycleReceipt("continue", lifecycle, current, "settled");
      }
      if (pauseRequested) return lifecycleReceipt("continue", lifecycle, current, "uncertain");
      if (!stopRequested || !stopState.observed)
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
      stopState.observed = false;
      activity = "idle";
      observationEpoch = current.processEpoch;
      observationSequence = 0n;
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
      observationEpoch = undefined;
      observationSequence = 0n;
    },
  };
  return { ok: true, value: adapter };
  async function send(
    turn: CoordinatorTurnInput,
  ): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    if (!current || !scope || scope.coordinatorSessionId !== turn.coordinatorSessionId)
      return failure("not_found", "provider session is not active");
    if (paused || pauseRequested || stopRequested)
      return failure("busy", "provider dispatch is paused or stopped");
    if (activity === "busy") return failure("busy", "provider session has active work");
    const sent = await input.transport.send({
      session: current,
      text: turn.text,
      clientMessageId: turn.clientMessageId,
    });
    if (sent.ok) {
      activity = "busy";
      pendingCorrelation = sent.value.transportCorrelation ?? null;
      acceptEvent({
        coordinatorSessionId: scope.coordinatorSessionId,
        processEpoch: current.processEpoch,
        nativeThreadId: current.nativeThreadId,
        nativeTurnId: sent.value.nativeTurnId,
        nativeItemId: null,
        kind: "turn_started",
        sourceEventId: `${profile.provider}:${current.processEpoch}:accepted:${createHash("sha256")
          .update(turn.clientMessageId)
          .digest("hex")
          .slice(0, 24)}`,
        ...(sent.value.transportCorrelation === null
          ? {}
          : { transportCorrelation: sent.value.transportCorrelation }),
        data: { type: "busy", reason: "addressed_turn_accepted" },
      });
    }
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
  async function lifecycleAction(
    lifecycle: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    if (!current || !scope || lifecycle.coordinatorSessionId !== scope.coordinatorSessionId)
      return failure("not_found", "provider session is not active");
    if (lifecycle.expectedProcessEpoch !== current.processEpoch)
      return failure("stale_epoch", "provider process epoch changed before pause");
    if (stopRequested) return lifecycleReceipt("pause", lifecycle, current, "unsupported");
    if (paused) return lifecycleReceipt("pause", lifecycle, current, "settled");
    pauseRequested = true;
    if (activity === "idle") {
      settlePause();
      return lifecycleReceipt("pause", lifecycle, current, "settled");
    }
    const result = await input.transport.pause({ session: current });
    if (!result.ok) {
      pauseRequested = false;
      return result;
    }
    if (result.value.observation === "settled") settlePause();
    return lifecycleReceipt("pause", lifecycle, current, result.value.observation);
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
    pause: "interrupt_owned_session",
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

function idleStatus(event: CoordinatorEvent | null): boolean {
  return (
    event?.kind === "session_status" &&
    z
      .object({ type: z.literal("idle") })
      .loose()
      .safeParse(event.data).success
  );
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

export { createClaudeStreamJsonTransportFactory } from "./claude.ts";
export type { ClaudeProcessFactory, OwnedLineProcess } from "./claude.ts";
export { createOpenCodeHttpTransportFactory, resolveProviderProxyEnvironment } from "./http.ts";
export type { ProviderHttpOptions } from "./http.ts";
export { createOpenCodeOwnedTransportFactory } from "./opencode.ts";
export type { OpenCodeDaemonFactory, OpenCodeDaemonProcess } from "./opencode.ts";
export { createQwenStreamJsonTransportFactory } from "./qwen.ts";
export type { QwenProcessFactory } from "./qwen.ts";
export { qwenZapMcpPermissionArguments } from "./qwen-permissions.ts";
