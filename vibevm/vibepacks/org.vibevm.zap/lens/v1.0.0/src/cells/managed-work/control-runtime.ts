/** Exact-epoch managed provider control registry.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import { randomUUID } from "node:crypto";
import { DecimalSchema } from "../protocol/index.ts";
import {
  ManagedControlTargetSchema,
  ManagedSessionControlEventSchema,
  type ManagedControlIo,
  type ManagedControlProvisioner,
  type ManagedControlTarget,
  type ManagedProviderControlAdapter,
  type ManagedSessionControlEvent,
  type ManagedSessionReadiness,
} from "./control.ts";
import type { ManagedProviderId } from "./provider-types.ts";
import type { ManagedWorkResult } from "./contracts.ts";

interface SessionState {
  readonly target: ManagedControlTarget;
  readonly provider: ManagedProviderId;
  readonly adapter: ManagedProviderControlAdapter | undefined;
  providerState: unknown;
  readiness: ManagedSessionReadiness;
  providerSessionId: string | null;
  providerTurnId: string | null;
  observationId: string;
  controlEpoch: string | null;
  pauseRequested: boolean;
  io: ManagedControlIo | null;
  sourceSequence: bigint;
}

export function createManagedControlRuntime(input: {
  readonly adapters: readonly ManagedProviderControlAdapter[];
  readonly io: (registration: {
    readonly access: Parameters<ManagedControlProvisioner["activate"]>[0]["access"];
    readonly terminalId: string;
    readonly leaseId: string;
    readonly controlEpoch: number;
  }) => ManagedControlIo;
}): ManagedControlProvisioner {
  const adapters = new Map(input.adapters.map((adapter) => [adapter.provider, adapter]));
  const sessions = new Map<string, SessionState>();
  const listeners = new Set<(event: ManagedSessionControlEvent) => void>();
  const load = (target: ManagedControlTarget): ManagedWorkResult<SessionState> => {
    const checked = ManagedControlTargetSchema.safeParse({
      runId: target.runId,
      actorId: target.actorId,
      sessionId: target.sessionId,
      terminalId: target.terminalId,
      expectedProcessEpoch: target.expectedProcessEpoch,
    });
    const session = checked.success ? sessions.get(checked.data.runId) : undefined;
    return session !== undefined &&
      exactTarget(session.target, checked.success ? checked.data : null)
      ? { ok: true, value: session }
      : fail("conflict", "managed control target or process epoch is stale");
  };
  return {
    async prepare(raw) {
      const processEpoch = `process.${randomUUID().replaceAll("-", "")}`;
      const target = ManagedControlTargetSchema.safeParse({
        runId: raw.runId,
        actorId: raw.actorId,
        sessionId: raw.sessionId,
        terminalId: raw.terminalId,
        expectedProcessEpoch: processEpoch,
      });
      if (!target.success) return fail("invalid_input", "managed control registration is invalid");
      const previous = sessions.get(target.data.runId);
      if (previous !== undefined && previous.readiness !== "stopped")
        return fail("conflict", "managed control run is already active");
      if (previous !== undefined) previous.adapter?.close(previous.providerState);
      const adapter = adapters.get(raw.provider);
      const session: SessionState = {
        target: target.data,
        provider: raw.provider,
        adapter,
        providerState: null,
        readiness: "starting",
        providerSessionId: null,
        providerTurnId: null,
        observationId: `observation.${randomUUID().replaceAll("-", "")}`,
        controlEpoch: null,
        pauseRequested: false,
        io: null,
        sourceSequence: 0n,
      };
      sessions.set(target.data.runId, session);
      if (adapter === undefined)
        return { ok: true, value: { target: target.data, launch: raw.launch } };
      const prepared = await adapter.prepare({
        target: target.data,
        launch: raw.launch,
        publish: (event) => {
          publish(session, event);
        },
      });
      if (!prepared.ok) {
        sessions.delete(target.data.runId);
        return prepared;
      }
      session.providerState = prepared.value.state;
      return { ok: true, value: { target: target.data, launch: prepared.value.launch } };
    },
    async activate(raw) {
      const found = load(raw.target);
      if (!found.ok) return found;
      const epoch = Number(raw.automationControlEpoch);
      if (!Number.isSafeInteger(epoch) || epoch < 1)
        return fail("invalid_input", "managed automation control epoch is invalid");
      found.value.controlEpoch = raw.automationControlEpoch;
      found.value.io = input.io({
        access: raw.access,
        terminalId: raw.target.terminalId,
        leaseId: raw.leaseId,
        controlEpoch: epoch,
      });
      if (found.value.adapter?.activate !== undefined) {
        const activated = await found.value.adapter.activate(
          found.value.providerState,
          found.value.io,
        );
        if (!activated.ok) {
          found.value.io = null;
          return activated;
        }
      }
      return { ok: true, value: null };
    },
    discard(target) {
      const found = load(target);
      if (!found.ok) return;
      found.value.adapter?.close(found.value.providerState);
      sessions.delete(target.runId);
    },
    settleStopped(target) {
      const found = load(target);
      if (!found.ok) return;
      found.value.readiness = "stopped";
    },
    inspect(target) {
      const found = load(target);
      return found.ok
        ? {
            ok: true,
            value: {
              readiness: found.value.readiness,
              providerSessionId: found.value.providerSessionId,
              providerTurnId: found.value.providerTurnId,
              observationId: found.value.observationId,
              automationControlEpoch: found.value.controlEpoch,
              pauseRequested: found.value.pauseRequested,
            },
          }
        : found;
    },
    async interrupt(raw) {
      const found = load(raw);
      if (!found.ok) return found;
      if (raw.reason === "project_pause") found.value.pauseRequested = true;
      if (
        found.value.readiness === "idle" ||
        (found.value.readiness === "starting" &&
          found.value.providerSessionId !== null &&
          found.value.providerTurnId === null)
      )
        return { ok: true, value: { observation: "already_idle" } };
      if (found.value.adapter === undefined || found.value.io === null)
        return { ok: true, value: { observation: "unsupported" } };
      found.value.readiness = "interrupting";
      const interrupted = await found.value.adapter.interrupt(
        found.value.providerState,
        raw.reason,
        found.value.io,
      );
      if (!interrupted.ok) {
        found.value.readiness = "unknown";
        return interrupted;
      }
      if (interrupted.value === "idle") found.value.readiness = "idle";
      return {
        ok: true,
        value: { observation: interrupted.value === "idle" ? "already_idle" : "requested" },
      };
    },
    continueSession(raw) {
      const found = load(raw);
      if (!found.ok) return found;
      if (
        found.value.readiness === "permission_required" ||
        found.value.readiness === "unknown" ||
        found.value.readiness === "stopped" ||
        found.value.controlEpoch === null ||
        found.value.io === null
      )
        return fail("unavailable", "managed session is not ready to continue");
      found.value.pauseRequested = false;
      return { ok: true, value: { readiness: found.value.readiness } };
    },
    async offer(raw) {
      const found = load(raw);
      if (!found.ok) return found;
      if (
        found.value.controlEpoch !== raw.expectedAutomationControlEpoch ||
        found.value.io === null
      )
        return fail("conflict", "managed automation terminal lease is stale");
      if (found.value.readiness === "permission_required")
        return {
          ok: true,
          value: { observation: "permission_required", transportCorrelation: null },
        };
      if (
        found.value.adapter === undefined ||
        (found.value.readiness !== "idle" && !found.value.adapter.canQueueWhileBusy)
      )
        return { ok: true, value: { observation: "busy", transportCorrelation: null } };
      const offered = await found.value.adapter.offer(
        found.value.providerState,
        raw.deliveryId,
        raw.bodyMarkdown,
        found.value.io,
      );
      if (!offered.ok) return offered;
      found.value.readiness = "busy";
      return {
        ok: true,
        value: {
          observation: offered.value.queued ? "provider_queued" : "host_accepted",
          transportCorrelation: offered.value.correlation,
        },
      };
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    close() {
      for (const session of sessions.values()) session.adapter?.close(session.providerState);
      sessions.clear();
      listeners.clear();
    },
  };

  function publish(
    session: SessionState,
    raw: Omit<ManagedSessionControlEvent, "sourceSequence">,
  ): void {
    if (!exactEvent(session, raw)) return;
    session.sourceSequence += 1n;
    const event = ManagedSessionControlEventSchema.safeParse({
      ...raw,
      sourceSequence: DecimalSchema.parse(String(session.sourceSequence)),
    });
    if (!event.success) return;
    session.readiness = readiness(event.data.kind);
    session.providerSessionId = event.data.providerSessionId;
    session.providerTurnId = event.data.providerTurnId;
    session.observationId = event.data.eventId;
    for (const listener of listeners) listener(event.data);
  }
}

function exactTarget(left: ManagedControlTarget, right: ManagedControlTarget | null): boolean {
  return (
    right !== null &&
    left.runId === right.runId &&
    left.actorId === right.actorId &&
    left.sessionId === right.sessionId &&
    left.terminalId === right.terminalId &&
    left.expectedProcessEpoch === right.expectedProcessEpoch
  );
}

function exactEvent(
  session: SessionState,
  event: Omit<ManagedSessionControlEvent, "sourceSequence">,
): boolean {
  return (
    event.runId === session.target.runId &&
    event.actorId === session.target.actorId &&
    event.sessionId === session.target.sessionId &&
    event.terminalId === session.target.terminalId &&
    event.provider === session.provider &&
    event.processEpoch === session.target.expectedProcessEpoch
  );
}

function readiness(kind: ManagedSessionControlEvent["kind"]): ManagedSessionReadiness {
  if (kind === "session_ready") return "starting";
  if (kind === "turn_settled") return "idle";
  if (kind === "turn_started") return "busy";
  if (kind === "permission_required") return "permission_required";
  if (kind === "session_exited") return "stopped";
  return "unknown";
}

function fail(
  code: "invalid_input" | "conflict" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
