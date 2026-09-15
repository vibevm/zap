/** Codex stdio coordinator adapter. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { isAbsolute, resolve } from "node:path";
import type { z } from "zod";
import {
  CoordinatorResumeInputSchema,
  CoordinatorStartInputSchema,
  CoordinatorSteerInputSchema,
  CoordinatorTurnInputSchema,
  HostRequestAnswerSchema,
  jsonValue,
  runtimeFailure,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorEvent,
  type CoordinatorHistory,
  type CoordinatorLifecycleInput,
  type CoordinatorLifecycleReceipt,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorSteerInput,
  type CoordinatorTurnInput,
  type CoordinatorTurnReceipt,
  type HostRequestAnswer,
} from "../agent-runtime/index.ts";
import { AgentSessionIdSchema } from "../workspace-model/index.ts";
import { CodexEventRouter } from "./events.ts";
import { CodexLifecycleController } from "./lifecycle.ts";
import {
  activeTurn,
  coordinatorBootstrap,
  descriptor,
  lensCredentialFile,
  modelParameters,
  parseThread,
  processFailure,
  processProfile,
  publicThread,
  receipt,
  requestKey,
  scopeOf,
  validateHostAnswer,
  turnStartParams,
  validateExplicitThreadModel,
} from "./helpers.ts";
import {
  CodexTurnStartResponseSchema,
  CodexTurnSteerResponseSchema,
  publicItem,
} from "./protocol.ts";
import type { CodexProcessFactory } from "./process.ts";
import {
  CODEX_COORDINATOR_CAPABILITIES,
  CODEX_LIFECYCLE_CAPABILITIES,
  type CodexCoordinatorProfile,
} from "./profile.ts";
import type { SessionState, WorkerState } from "./state.ts";
import { exclusive, makeSession } from "./session.ts";

export class CodexAdapter implements CoordinatorAdapter {
  readonly capabilities = CODEX_COORDINATOR_CAPABILITIES;
  readonly lifecycleCapabilities = CODEX_LIFECYCLE_CAPABILITIES;
  readonly #profiles: ReadonlyMap<string, CodexCoordinatorProfile>;
  readonly #factory: CodexProcessFactory;
  readonly #workers = new Map<string, WorkerState>();
  readonly #sessions = new Map<string, SessionState>();
  readonly #events = new CodexEventRouter(this.#sessions, this.#workers);
  readonly #lifecycle: CodexLifecycleController;
  readonly #incarnations = new Map<string, number>();
  #closed = false;

  constructor(
    profiles: ReadonlyMap<string, CodexCoordinatorProfile>,
    factory: CodexProcessFactory,
  ) {
    this.#profiles = profiles;
    this.#factory = factory;
    this.#lifecycle = new CodexLifecycleController({
      sessions: this.#sessions,
      workers: this.#workers,
      events: this.#events,
      openWorker: (ownerId, profileId, cwd) => this.#worker(ownerId, profileId, cwd),
      resume: (input, worker, existing) => this.#resumeInto(input, worker, existing),
      discardWorker: (ownerId) => {
        this.#dropWorker(ownerId);
      },
    });
  }

  async start(
    raw: CoordinatorStartInput,
  ): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    const input = CoordinatorStartInputSchema.safeParse(raw);
    if (!input.success) {
      return runtimeFailure("invalid_input", "Coordinator start input does not match its schema");
    }
    if (!isAbsolute(input.data.cwd)) {
      return runtimeFailure(
        "invalid_input",
        "Coordinator start input must contain an absolute cwd",
      );
    }
    const normalized = { ...input.data, cwd: resolve(input.data.cwd) };
    if (this.#sessions.has(normalized.coordinatorSessionId)) {
      return runtimeFailure("already_exists", "Coordinator session is already registered");
    }
    const worker = await this.#worker(
      normalized.coordinatorSessionId,
      normalized.profileId,
      normalized.cwd,
    );
    if (!worker.ok) return worker;
    const profile = worker.value.profile;
    if (profile.lensMcp !== undefined && normalized.agentScope == null) {
      this.#dropWorker(normalized.coordinatorSessionId);
      return runtimeFailure("policy_denied", "Lens MCP profile requires a trusted agent scope");
    }
    if (
      profile.lensMcp !== undefined &&
      lensCredentialFile(profile, normalized.agentScope) === undefined
    ) {
      this.#dropWorker(normalized.coordinatorSessionId);
      return runtimeFailure("policy_denied", "Lens MCP has no credential for this project scope");
    }
    const started = await worker.value.process.request("thread/start", {
      ...modelParameters(
        normalized.modelId,
        normalized.reasoningEffort,
        profile,
        normalized.agentScope,
        normalized.agentBinding,
      ),
      allowProviderModelFallback: false,
      cwd: normalized.cwd,
      approvalPolicy: profile.approvalPolicy,
      sandbox: profile.sandbox,
      personality: profile.personality,
      serviceName: profile.serviceName,
      ephemeral: false,
      historyMode: "legacy",
    });
    const thread = parseThread(started, input.data.cwd);
    if (!thread.ok) {
      this.#dropWorker(normalized.coordinatorSessionId);
      return thread;
    }
    const model = validateExplicitThreadModel(
      thread.value.thread,
      normalized.modelId ?? profile.model,
    );
    if (!model.ok) {
      this.#dropWorker(normalized.coordinatorSessionId);
      return model;
    }
    const session = makeSession(normalized, worker.value, thread.value.thread, "submitted");
    this.#sessions.set(normalized.coordinatorSessionId, session);
    this.#events.emit(
      session,
      worker.value,
      "session_started",
      thread.value.thread.id,
      null,
      null,
      {
        thread: publicThread(thread.value.thread),
      },
    );

    const boot = await this.#startTurn(session, worker.value, {
      coordinatorSessionId: normalized.coordinatorSessionId,
      clientMessageId: `bootstrap:${normalized.coordinatorSessionId}`,
      text: coordinatorBootstrap(
        normalized.bootstrapText,
        normalized,
        lensCredentialFile(profile, normalized.agentScope) !== undefined,
      ),
    });
    if (!boot.ok) {
      session.descriptor = { ...session.descriptor, bootstrap: "not_observed" };
    }
    return { ok: true as const, value: session.descriptor };
  }

  async resume(
    raw: CoordinatorResumeInput,
  ): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    const input = CoordinatorResumeInputSchema.safeParse(raw);
    if (!input.success) {
      return runtimeFailure("invalid_input", "Coordinator resume input does not match its schema");
    }
    if (!isAbsolute(input.data.cwd)) {
      return runtimeFailure(
        "invalid_input",
        "Coordinator resume input must contain an absolute cwd",
      );
    }
    const normalized = { ...input.data, cwd: resolve(input.data.cwd) };
    if (this.#sessions.has(normalized.coordinatorSessionId)) {
      return runtimeFailure("already_exists", "Coordinator session is already registered");
    }
    const worker = await this.#worker(
      normalized.coordinatorSessionId,
      normalized.profileId,
      normalized.cwd,
    );
    if (!worker.ok) return worker;
    const resumed = await this.#resumeInto(normalized, worker.value);
    if (!resumed.ok) this.#dropWorker(normalized.coordinatorSessionId);
    return resumed;
  }

  async readHistory(sessionId: z.infer<typeof AgentSessionIdSchema>) {
    const session = this.#session(sessionId);
    if (!session.ok) return session;
    return this.#readThread(session.value, session.value.descriptor.nativeThreadRef.value);
  }

  async readNativeChildHistory(
    sessionId: z.infer<typeof AgentSessionIdSchema>,
    nativeThreadId: string,
  ) {
    const session = this.#session(sessionId);
    if (!session.ok) return session;
    if (!session.value.childThreads.has(nativeThreadId)) {
      return runtimeFailure(
        "not_found",
        "Native child thread has not been observed for this coordinator",
      );
    }
    return this.#readThread(session.value, nativeThreadId);
  }

  async startTurn(raw: CoordinatorTurnInput) {
    const input = CoordinatorTurnInputSchema.safeParse(raw);
    if (!input.success) return runtimeFailure("invalid_input", "Coordinator turn input is invalid");
    const session = this.#session(input.data.coordinatorSessionId);
    if (!session.ok) return session;
    if (session.value.lifecycle !== "active") {
      return runtimeFailure("busy", "Coordinator dispatch is paused or stopped", "after_refresh");
    }
    return exclusive(session.value, async () => {
      if (session.value.activeTurnId !== null) {
        return runtimeFailure("busy", "Coordinator already has an active turn", "after_refresh");
      }
      const worker = this.#workerFor(session.value);
      return worker.ok
        ? this.#startTurn(session.value, worker.value, input.data)
        : Promise.resolve(worker);
    });
  }

  async steer(raw: CoordinatorSteerInput) {
    const input = CoordinatorSteerInputSchema.safeParse(raw);
    if (!input.success) {
      return runtimeFailure("invalid_input", "Coordinator steering input is invalid");
    }
    const session = this.#session(input.data.coordinatorSessionId);
    if (!session.ok) return session;
    if (session.value.lifecycle !== "active") {
      return runtimeFailure("busy", "Coordinator dispatch is paused or stopped", "after_refresh");
    }
    return exclusive(session.value, async () => {
      if (session.value.activeTurnId !== input.data.expectedNativeTurnId) {
        return runtimeFailure("stale_epoch", "Active Codex turn does not match", "after_refresh");
      }
      const worker = this.#workerFor(session.value);
      if (!worker.ok) return worker;
      const result = await worker.value.process.request("turn/steer", {
        threadId: session.value.descriptor.nativeThreadRef.value,
        expectedTurnId: input.data.expectedNativeTurnId,
        input: [{ type: "text", text: input.data.text }],
        clientUserMessageId: input.data.clientMessageId,
      });
      if (!result.ok) return processFailure(result.error);
      const parsed = CodexTurnSteerResponseSchema.safeParse(result.value);
      return parsed.success
        ? {
            ok: true as const,
            value: receipt(session.value, parsed.data.turnId, worker.value.epoch),
          }
        : runtimeFailure("protocol_error", "Codex turn/steer response is invalid");
    });
  }

  async send(input: CoordinatorTurnInput) {
    const session = this.#session(input.coordinatorSessionId);
    if (!session.ok) return session;
    return session.value.activeTurnId === null
      ? this.startTurn(input)
      : this.steer({ ...input, expectedNativeTurnId: session.value.activeTurnId });
  }

  async interrupt(sessionId: z.infer<typeof AgentSessionIdSchema>, nativeTurnId: string) {
    const session = this.#session(sessionId);
    if (!session.ok) return session;
    if (session.value.activeTurnId !== nativeTurnId) {
      return runtimeFailure("stale_epoch", "Active Codex turn does not match", "after_refresh");
    }
    const worker = this.#workerFor(session.value);
    if (!worker.ok) return worker;
    const result = await worker.value.process.request("turn/interrupt", {
      threadId: session.value.descriptor.nativeThreadRef.value,
      turnId: nativeTurnId,
    });
    return result.ok ? { ok: true as const, value: undefined } : processFailure(result.error);
  }

  async pause(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    const session = this.#session(input.coordinatorSessionId);
    return session.ok ? exclusive(session.value, () => this.#lifecycle.pause(input)) : session;
  }

  async stop(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    const session = this.#session(input.coordinatorSessionId);
    return session.ok ? exclusive(session.value, () => this.#lifecycle.stop(input)) : session;
  }

  async continueSession(
    input: CoordinatorLifecycleInput,
  ): Promise<AgentRuntimeResult<CoordinatorLifecycleReceipt>> {
    const session = this.#session(input.coordinatorSessionId);
    return session.ok
      ? exclusive(session.value, () => this.#lifecycle.continueSession(input))
      : session;
  }

  async respondToRequest(raw: HostRequestAnswer) {
    await Promise.resolve();
    const input = HostRequestAnswerSchema.safeParse(raw);
    if (!input.success) return runtimeFailure("invalid_input", "Host request answer is invalid");
    const session = this.#session(input.data.coordinatorSessionId);
    if (!session.ok) return session;
    if (session.value.lifecycle !== "active") {
      return runtimeFailure("busy", "Native answers are retained until project continuation");
    }
    const key = requestKey(input.data.requestId);
    const pending = session.value.pending.get(key);
    if (pending === undefined) {
      return session.value.retiredRequests.has(key)
        ? runtimeFailure("stale_epoch", "Host request belongs to an earlier process epoch")
        : runtimeFailure("not_found", "Host request is no longer pending");
    }
    if (session.value.answeredRequests.has(key)) {
      return runtimeFailure("already_exists", "Host request answer was already written");
    }
    if (pending.processEpoch !== input.data.processEpoch) {
      return runtimeFailure("stale_epoch", "Host request belongs to an earlier process epoch");
    }
    const answer = validateHostAnswer(pending.kind, input.data.answer);
    if (!answer.ok) return answer;
    const worker = this.#workerFor(session.value);
    if (!worker.ok) return worker;
    const result = worker.value.process.respond(input.data.requestId, answer.value);
    if (!result.ok) return processFailure(result.error);
    session.value.answeredRequests.add(key);
    return { ok: true as const, value: undefined };
  }

  async restart(profileId: string) {
    const profile = this.#profiles.get(profileId);
    if (profile === undefined)
      return runtimeFailure("not_found", "Codex profile is not registered");
    const prior = [...this.#sessions.values()].filter(
      (session) => session.descriptor.profileId === profileId,
    );
    const restored: CoordinatorSessionDescriptor[] = [];
    for (const session of prior) {
      const ownerId = session.descriptor.coordinatorSessionId;
      this.#dropWorker(ownerId);
      const worker = await this.#worker(ownerId, profileId, session.descriptor.cwd);
      if (!worker.ok) return worker;
      for (const key of session.pending.keys()) session.retiredRequests.add(key);
      session.pending.clear();
      session.answeredRequests.clear();
      session.activeTurnId = null;
      const input: CoordinatorResumeInput = {
        ...scopeOf(session.start),
        profileId,
        cwd: session.descriptor.cwd,
        nativeThreadId: session.descriptor.nativeThreadRef.value,
        modelId: session.modelId,
        reasoningEffort: session.reasoningEffort,
        agentScope: session.start.agentScope,
        agentBinding: session.start.agentBinding,
      };
      const result = await this.#resumeInto(input, worker.value, session);
      if (!result.ok) return result;
      restored.push(result.value);
    }
    return { ok: true as const, value: restored };
  }

  subscribe(listener: (event: CoordinatorEvent) => void): () => void {
    return this.#events.subscribe(listener);
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const ownerId of [...this.#workers.keys()]) this.#dropWorker(ownerId);
  }

  async #worker(
    ownerCoordinatorSessionId: string,
    profileId: string,
    launchCwd: string,
  ): Promise<AgentRuntimeResult<WorkerState>> {
    if (this.#closed) return runtimeFailure("transport_lost", "Codex adapter is closed");
    const current = this.#workers.get(ownerCoordinatorSessionId);
    if (current !== undefined) return { ok: true as const, value: current };
    const profile = this.#profiles.get(profileId);
    if (profile === undefined)
      return runtimeFailure("not_found", "Codex profile is not registered");
    const process = await this.#factory.start(processProfile(profile), launchCwd);
    if (!process.ok) return processFailure(process.error);
    const incarnation = String((this.#incarnations.get(profileId) ?? 0) + 1);
    this.#incarnations.set(profileId, Number(incarnation));
    const worker: WorkerState = {
      ownerCoordinatorSessionId,
      profile,
      process: process.value,
      epoch: `${profileId}:${incarnation}:${process.value.epoch}`,
      incarnation,
      sequence: 0,
      unsubscribe: () => undefined,
      unsubscribeExit: () => undefined,
    };
    worker.unsubscribe = process.value.subscribe((message) => {
      this.#events.message(worker, message);
    });
    worker.unsubscribeExit = process.value.onExit((exit) => {
      this.#events.exited(worker, exit.code);
    });
    this.#workers.set(ownerCoordinatorSessionId, worker);
    const initialized = await process.value.request("initialize", {
      clientInfo: { name: "quicklens", title: "Zap Wayfinder", version: "0.1.0" },
      capabilities: { experimentalApi: true },
    });
    if (!initialized.ok) {
      this.#dropWorker(ownerCoordinatorSessionId);
      return processFailure(initialized.error);
    }
    const acknowledged = process.value.notify("initialized", {});
    if (!acknowledged.ok) {
      this.#dropWorker(ownerCoordinatorSessionId);
      return processFailure(acknowledged.error);
    }
    return { ok: true as const, value: worker };
  }

  async #resumeInto(
    input: CoordinatorResumeInput,
    worker: WorkerState,
    existing?: SessionState,
  ): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    const profile = worker.profile;
    if (profile.lensMcp !== undefined && input.agentScope == null)
      return runtimeFailure("policy_denied", "Lens MCP profile requires a trusted agent scope");
    if (
      profile.lensMcp !== undefined &&
      lensCredentialFile(profile, input.agentScope) === undefined
    )
      return runtimeFailure("policy_denied", "Lens MCP has no credential for this project scope");
    const resumed = await worker.process.request("thread/resume", {
      threadId: input.nativeThreadId,
      cwd: input.cwd,
      ...modelParameters(
        input.modelId,
        input.reasoningEffort,
        profile,
        input.agentScope,
        input.agentBinding,
      ),
      approvalPolicy: profile.approvalPolicy,
      sandbox: profile.sandbox,
      personality: profile.personality,
    });
    const thread = parseThread(resumed, input.cwd, input.nativeThreadId);
    if (!thread.ok) return thread;
    const model = validateExplicitThreadModel(thread.value.thread, input.modelId ?? profile.model);
    if (!model.ok) return model;
    const session = existing ?? makeSession(input, worker, thread.value.thread, "not_observed");
    const bootstrap = existing?.descriptor.bootstrap ?? "not_observed";
    session.descriptor = descriptor(
      input,
      worker,
      {
        thread: thread.value.thread,
        instructionSources: thread.value.instructionSources,
      },
      bootstrap,
    );
    session.activeTurnId = activeTurn(thread.value.thread);
    session.childActiveTurns.clear();
    session.pauseTargets.clear();
    session.lifecycle = "active";
    session.pauseObservation = null;
    session.stopObservation = null;
    this.#sessions.set(input.coordinatorSessionId, session);
    this.#events.observeChildren(
      session,
      thread.value.thread.turns.flatMap((turn) => turn.items),
    );
    this.#events.emit(session, worker, "session_resumed", thread.value.thread.id, null, null, {
      thread: publicThread(thread.value.thread),
    });
    return { ok: true as const, value: session.descriptor };
  }

  async #startTurn(
    session: SessionState,
    worker: WorkerState,
    input: CoordinatorTurnInput,
  ): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    const result = await worker.process.request("turn/start", turnStartParams(session, input));
    if (!result.ok) return processFailure(result.error);
    const parsed = CodexTurnStartResponseSchema.safeParse(result.value);
    if (!parsed.success)
      return runtimeFailure("protocol_error", "Codex turn/start response is invalid");
    session.activeTurnId = parsed.data.turn.id;
    session.descriptor = { ...session.descriptor, state: "running" };
    return { ok: true as const, value: receipt(session, parsed.data.turn.id, worker.epoch) };
  }

  async #readThread(session: SessionState, threadId: string) {
    const worker = this.#workerFor(session);
    if (!worker.ok) return worker;
    const result = await worker.value.process.request("thread/read", {
      threadId,
      includeTurns: true,
    });
    const thread = parseThread(result, session.descriptor.cwd, threadId);
    if (!thread.ok) return thread;
    this.#events.observeChildren(
      session,
      thread.value.thread.turns.flatMap((turn) => turn.items),
    );
    const status = jsonValue(thread.value.thread.status);
    const turns = jsonValue(
      thread.value.thread.turns.map((turn) => ({
        id: turn.id,
        status: turn.status,
        items: turn.items.map(publicItem),
        ...(turn.error === undefined ? {} : { error: turn.error }),
      })),
    );
    if (!status.ok) return status;
    if (!turns.ok || !Array.isArray(turns.value)) {
      return runtimeFailure("protocol_error", "Codex history is not a JSON array");
    }
    const history: CoordinatorHistory = {
      coordinatorSessionId: session.descriptor.coordinatorSessionId,
      nativeThreadId: thread.value.thread.id,
      processEpoch: worker.value.epoch,
      status: status.value,
      turns: turns.value,
    };
    return { ok: true as const, value: history };
  }

  #dropWorker(ownerCoordinatorSessionId: string): void {
    const worker = this.#workers.get(ownerCoordinatorSessionId);
    if (worker === undefined) return;
    this.#workers.delete(ownerCoordinatorSessionId);
    worker.unsubscribe();
    worker.unsubscribeExit();
    worker.process.close();
  }

  #session(sessionId: z.infer<typeof AgentSessionIdSchema>): AgentRuntimeResult<SessionState> {
    const parsed = AgentSessionIdSchema.safeParse(sessionId);
    const session = parsed.success ? this.#sessions.get(parsed.data) : undefined;
    return session === undefined
      ? runtimeFailure("not_found", "Coordinator session is not registered")
      : { ok: true as const, value: session };
  }

  #workerFor(session: SessionState): AgentRuntimeResult<WorkerState> {
    const worker = this.#workers.get(session.descriptor.coordinatorSessionId);
    return worker === undefined
      ? runtimeFailure("transport_lost", "Codex process is not running", "after_reconcile")
      : { ok: true as const, value: worker };
  }
}
