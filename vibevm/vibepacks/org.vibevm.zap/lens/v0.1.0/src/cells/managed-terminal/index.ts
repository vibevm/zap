/** @scope spec://org.vibevm.zap/lens/PROP-006#managed-terminal */
/** Optional owned PTY kernel with lease-fenced input and bounded public output. */
import { randomUUID } from "node:crypto";
import { isAbsolute } from "node:path";
import { z } from "zod";

export const TerminalLaunchSpecSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    sessionId: z.string().min(3).max(160),
    runId: z.string().min(3).max(160),
    executable: z.string().min(1).max(512).refine(isAbsolute, "executable must be absolute"),
    args: z.array(z.string().max(16_384)).max(256),
    cwd: z.string().min(1).max(32_000).refine(isAbsolute, "cwd must be absolute"),
    env: z.record(z.string(), z.string()).optional(),
  })
  .strict();
export type TerminalLaunchSpec = z.infer<typeof TerminalLaunchSpecSchema>;

export const TerminalOutputSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    controlEpoch: z.number().int().min(1),
    sequence: z.number().int().min(1),
    data: z.string().max(64_000),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalOutput = z.infer<typeof TerminalOutputSchema>;

export const TerminalLeaseSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    leaseId: z.string().min(3).max(160),
    clientId: z.string().min(3).max(160),
    controlEpoch: z.number().int().min(1),
    acquiredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalLease = z.infer<typeof TerminalLeaseSchema>;

export const TerminalLifecycleEventSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    sessionId: z.string().min(3).max(160),
    runId: z.string().min(3).max(160),
    processId: z.number().int().positive(),
    operation: z.enum([
      "start",
      "acquire",
      "takeover",
      "release",
      "input",
      "resize",
      "interrupt",
      "stop",
      "exit",
    ]),
    controlEpoch: z.number().int().min(1),
    clientId: z.string().min(3).max(160).nullable(),
    inputLength: z.number().int().min(0).max(64_000).nullable(),
    columns: z.number().int().min(1).max(1_000).nullable(),
    rows: z.number().int().min(1).max(1_000).nullable(),
    exitCode: z.number().int().nullable(),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalLifecycleEvent = z.infer<typeof TerminalLifecycleEventSchema>;

export type ManagedTerminalResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_input" | "unsupported" | "forbidden" | "conflict" | "unavailable";
        readonly message: string;
      };
    };

export interface ManagedTerminalProcess {
  readonly processId: number;
  onData(listener: (data: string) => void): () => void;
  onExit(listener: (code: number | null) => void): () => void;
  write(data: string): void;
  resize(columns: number, rows: number): void;
  interrupt(): void;
  stop(): void;
}

export interface ManagedTerminalFactory {
  spawn(spec: TerminalLaunchSpec): Promise<ManagedTerminalResult<ManagedTerminalProcess>>;
}

export interface ManagedTerminalSnapshot {
  readonly terminalId: string;
  readonly projectId: string;
  readonly contextId: string;
  readonly sessionId: string;
  readonly runId: string;
  readonly processId: number;
  readonly state: "running" | "stopping" | "exited" | "stopped";
  readonly controlEpoch: number;
  readonly lease: TerminalLease | null;
  readonly nextSequence: number;
  readonly output: readonly TerminalOutput[];
}

export class ManagedTerminalKernel {
  readonly #factory: ManagedTerminalFactory;
  readonly #historyLimit: number;
  readonly #terminals = new Map<string, TerminalState>();

  constructor(factory: ManagedTerminalFactory, historyLimit = 2_000) {
    if (!Number.isInteger(historyLimit) || historyLimit < 1 || historyLimit > 100_000)
      throw new RangeError();
    this.#factory = factory;
    this.#historyLimit = historyLimit;
  }

  async start(
    rawSpec: TerminalLaunchSpec,
  ): Promise<ManagedTerminalResult<ManagedTerminalSnapshot>> {
    const spec = TerminalLaunchSpecSchema.safeParse(rawSpec);
    if (!spec.success) return failure("invalid_input", "managed terminal launch spec is invalid");
    if (this.#terminals.has(spec.data.terminalId))
      return failure("conflict", "managed terminal identity is already active");
    const process = await this.#factory.spawn(spec.data);
    if (!process.ok) return process;
    const exit = deferred();
    const state: TerminalState = {
      spec: spec.data,
      process: process.value,
      state: "running",
      controlEpoch: 1,
      lease: null,
      nextSequence: 1,
      output: [],
      listeners: new Set(),
      lifecycleListeners: new Set(),
      exit: exit.promise,
      settleExit: exit.resolve,
    };
    state.unsubscribeData = process.value.onData((data) => {
      this.#output(state, data);
    });
    state.unsubscribeExit = process.value.onExit((code) => {
      state.state = code === 0 ? "exited" : "stopped";
      state.lease = null;
      this.#lifecycle(state, "exit", { exitCode: code });
      state.settleExit();
    });
    this.#terminals.set(spec.data.terminalId, state);
    return { ok: true, value: this.#snapshot(state) };
  }

  subscribe(terminalId: string, listener: (output: TerminalOutput) => void): () => void {
    const state = this.#terminals.get(terminalId);
    if (state === undefined) return () => undefined;
    state.listeners.add(listener);
    return () => {
      state.listeners.delete(listener);
    };
  }

  subscribeLifecycle(
    terminalId: string,
    listener: (event: TerminalLifecycleEvent) => void,
  ): () => void {
    const state = this.#terminals.get(terminalId);
    if (state === undefined) return () => undefined;
    state.lifecycleListeners.add(listener);
    listener(this.#lifecycleEvent(state, "start"));
    if (state.state === "exited" || state.state === "stopped") {
      listener(this.#lifecycleEvent(state, "exit"));
    }
    return () => {
      state.lifecycleListeners.delete(listener);
    };
  }

  snapshot(terminalId: string): ManagedTerminalResult<ManagedTerminalSnapshot> {
    const state = this.#terminals.get(terminalId);
    return state === undefined
      ? failure("unavailable", "managed terminal is not registered")
      : { ok: true, value: this.#snapshot(state) };
  }

  snapshots(projectId?: string, contextId?: string): readonly ManagedTerminalSnapshot[] {
    return [...this.#terminals.values()]
      .filter(
        (state) =>
          (projectId === undefined || state.spec.projectId === projectId) &&
          (contextId === undefined || state.spec.contextId === contextId),
      )
      .map((state) => this.#snapshot(state));
  }

  async stopProject(
    projectId: string,
    contextId: string,
    timeoutMs = 5_000,
  ): Promise<ManagedTerminalResult<readonly ManagedTerminalSnapshot[]>> {
    const states = [...this.#terminals.values()].filter(
      (state) => state.spec.projectId === projectId && state.spec.contextId === contextId,
    );
    for (const state of states) {
      if (state.state === "running") {
        state.state = "stopping";
        state.lease = null;
        state.controlEpoch += 1;
        state.process.stop();
        this.#lifecycle(state, "stop");
      }
    }
    const settled = await Promise.all(
      states.map((state) =>
        state.state === "exited" || state.state === "stopped"
          ? Promise.resolve(true)
          : waitForExit(state.exit, timeoutMs),
      ),
    );
    return settled.every(Boolean)
      ? { ok: true, value: states.map((state) => this.#snapshot(state)) }
      : failure("unavailable", "managed terminal exit was not observed before timeout");
  }

  acquire(
    terminalId: string,
    clientId: string,
    expectedControlEpoch: number,
    takeover = false,
  ): ManagedTerminalResult<TerminalLease> {
    const state = this.#terminals.get(terminalId);
    if (state === undefined) return failure("unavailable", "managed terminal is not registered");
    if (state.controlEpoch !== expectedControlEpoch)
      return failure("conflict", "terminal control epoch is stale");
    if (state.lease !== null && state.lease.clientId !== clientId && !takeover)
      return failure("conflict", "managed terminal is controlled by another client");
    const isTakeover = state.lease !== null && state.lease.clientId !== clientId;
    if (isTakeover) state.controlEpoch += 1;
    const lease = TerminalLeaseSchema.parse({
      terminalId,
      leaseId: `lease.${randomUUID()}`,
      clientId,
      controlEpoch: state.controlEpoch,
      acquiredAt: new Date().toISOString(),
    });
    state.lease = lease;
    this.#lifecycle(state, isTakeover ? "takeover" : "acquire", { clientId });
    return { ok: true, value: lease };
  }

  release(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void> {
    const checked = this.#controlled(terminalId, leaseId, expectedControlEpoch);
    if (!checked.ok) return checked;
    const clientId = checked.value.lease?.clientId ?? null;
    checked.value.lease = null;
    checked.value.controlEpoch += 1;
    this.#lifecycle(checked.value, "release", { clientId });
    return { ok: true, value: undefined };
  }

  write(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
    data: string,
  ): ManagedTerminalResult<void> {
    const checked = this.#controlled(terminalId, leaseId, expectedControlEpoch);
    if (!checked.ok) return checked;
    if (data.length === 0 || data.length > 64_000)
      return failure("invalid_input", "terminal input is out of bounds");
    checked.value.process.write(data);
    this.#lifecycle(checked.value, "input", { inputLength: data.length });
    return { ok: true, value: undefined };
  }

  resize(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
    columns: number,
    rows: number,
  ): ManagedTerminalResult<void> {
    const checked = this.#controlled(terminalId, leaseId, expectedControlEpoch);
    if (!checked.ok) return checked;
    if (
      !Number.isInteger(columns) ||
      !Number.isInteger(rows) ||
      columns < 1 ||
      rows < 1 ||
      columns > 1_000 ||
      rows > 1_000
    )
      return failure("invalid_input", "terminal dimensions are invalid");
    checked.value.process.resize(columns, rows);
    this.#lifecycle(checked.value, "resize", { columns, rows });
    return { ok: true, value: undefined };
  }

  interrupt(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void> {
    const checked = this.#controlled(terminalId, leaseId, expectedControlEpoch);
    if (!checked.ok) return checked;
    checked.value.process.interrupt();
    this.#lifecycle(checked.value, "interrupt");
    return { ok: true, value: undefined };
  }

  stop(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void> {
    const checked = this.#controlled(terminalId, leaseId, expectedControlEpoch);
    if (!checked.ok) return checked;
    checked.value.process.stop();
    checked.value.state = "stopping";
    checked.value.lease = null;
    this.#lifecycle(checked.value, "stop");
    return { ok: true, value: undefined };
  }

  close(): void {
    for (const state of this.#terminals.values()) {
      state.unsubscribeData?.();
      state.unsubscribeExit?.();
      state.process.stop();
      state.lease = null;
      state.state = "stopped";
    }
    this.#terminals.clear();
  }

  #controlled(
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<TerminalState> {
    const state = this.#terminals.get(terminalId);
    if (state === undefined) return failure("unavailable", "managed terminal is not registered");
    if (state.controlEpoch !== expectedControlEpoch)
      return failure("conflict", "terminal control epoch is stale");
    if (state.lease?.leaseId !== leaseId)
      return failure("forbidden", "terminal lease is not owned by this request");
    return { ok: true, value: state };
  }

  #output(state: TerminalState, data: string): void {
    const output = TerminalOutputSchema.parse({
      terminalId: state.spec.terminalId,
      projectId: state.spec.projectId,
      contextId: state.spec.contextId,
      controlEpoch: state.controlEpoch,
      sequence: state.nextSequence++,
      data: data.slice(0, 64_000),
      occurredAt: new Date().toISOString(),
    });
    state.output.push(output);
    if (state.output.length > this.#historyLimit)
      state.output.splice(0, state.output.length - this.#historyLimit);
    for (const listener of state.listeners) listener(output);
  }

  #lifecycle(
    state: TerminalState,
    operation: TerminalLifecycleEvent["operation"],
    detail: Partial<
      Pick<TerminalLifecycleEvent, "clientId" | "inputLength" | "columns" | "rows" | "exitCode">
    > = {},
  ): void {
    const event = this.#lifecycleEvent(state, operation, detail);
    for (const listener of state.lifecycleListeners) listener(event);
  }

  #lifecycleEvent(
    state: TerminalState,
    operation: TerminalLifecycleEvent["operation"],
    detail: Partial<
      Pick<TerminalLifecycleEvent, "clientId" | "inputLength" | "columns" | "rows" | "exitCode">
    > = {},
  ): TerminalLifecycleEvent {
    return TerminalLifecycleEventSchema.parse({
      terminalId: state.spec.terminalId,
      projectId: state.spec.projectId,
      contextId: state.spec.contextId,
      sessionId: state.spec.sessionId,
      runId: state.spec.runId,
      processId: state.process.processId,
      operation,
      controlEpoch: state.controlEpoch,
      clientId: detail.clientId ?? state.lease?.clientId ?? null,
      inputLength: detail.inputLength ?? null,
      columns: detail.columns ?? null,
      rows: detail.rows ?? null,
      exitCode: detail.exitCode ?? null,
      occurredAt: new Date().toISOString(),
    });
  }

  #snapshot(state: TerminalState): ManagedTerminalSnapshot {
    return {
      terminalId: state.spec.terminalId,
      projectId: state.spec.projectId,
      contextId: state.spec.contextId,
      sessionId: state.spec.sessionId,
      runId: state.spec.runId,
      processId: state.process.processId,
      state: state.state,
      controlEpoch: state.controlEpoch,
      lease: state.lease,
      nextSequence: state.nextSequence,
      output: [...state.output],
    };
  }
}

export { createOptionalNodePtyFactory } from "./pty.ts";

interface TerminalState {
  readonly spec: TerminalLaunchSpec;
  readonly process: ManagedTerminalProcess;
  state: "running" | "stopping" | "exited" | "stopped";
  controlEpoch: number;
  lease: TerminalLease | null;
  nextSequence: number;
  readonly output: TerminalOutput[];
  readonly listeners: Set<(output: TerminalOutput) => void>;
  readonly lifecycleListeners: Set<(event: TerminalLifecycleEvent) => void>;
  readonly exit: Promise<void>;
  readonly settleExit: () => void;
  unsubscribeData?: () => void;
  unsubscribeExit?: () => void;
}

function deferred(): { readonly promise: Promise<void>; readonly resolve: () => void } {
  let resolve = (): void => undefined;
  const promise = new Promise<void>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

async function waitForExit(exit: Promise<void>, timeoutMs: number): Promise<boolean> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<false>((resolve) => {
    timer = setTimeout(() => {
      resolve(false);
    }, timeoutMs);
  });
  const observed = await Promise.race([exit.then(() => true), timeout]);
  if (timer !== undefined) clearTimeout(timer);
  return observed;
}

function failure(
  code: "invalid_input" | "unsupported" | "forbidden" | "conflict" | "unavailable",
  message: string,
): ManagedTerminalResult<never> {
  return { ok: false, error: { code, message } };
}
