/** @scope spec://org.vibevm.zap/lens/PROP-006#network-and-control */
/** Scoped managed-terminal service port over the owned PTY kernel. */
import { z } from "zod";
import {
  type ManagedTerminalKernel,
  TerminalLaunchSpecSchema,
  TerminalLeaseSchema,
  type ManagedTerminalResult,
  type ManagedTerminalSnapshot,
  type TerminalLaunchSpec,
} from "../managed-terminal/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { ManagedTerminalOutputStore } from "../managed-terminal-store/index.ts";

export const TerminalOutputPageRequestSchema = z
  .object({
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    terminalId: z.string().min(3).max(160),
    afterSequence: z.number().int().min(0),
    limit: z.number().int().min(1).max(256),
  })
  .strict();
export type TerminalOutputPageRequest = z.infer<typeof TerminalOutputPageRequestSchema>;

export const TerminalOutputPageSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    events: z
      .array(
        z
          .object({
            terminalId: z.string().min(3).max(160),
            projectId: z.string().min(3).max(160),
            contextId: z.string().min(3).max(160),
            controlEpoch: z.number().int().min(1),
            sequence: z.number().int().min(1),
            data: z.string().max(64_000),
            occurredAt: z.iso.datetime(),
          })
          .strict(),
      )
      .max(256),
    afterSequence: z.number().int().min(0),
    nextSequence: z.number().int().min(0).nullable(),
    gap: z
      .object({ firstAvailableSequence: z.number().int().min(1), reason: z.string().min(1) })
      .strict()
      .nullable(),
  })
  .strict();
export type TerminalOutputPage = z.infer<typeof TerminalOutputPageSchema>;

export const TerminalLeaseRequestSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    expectedControlEpoch: z.number().int().min(1),
    takeover: z.boolean().optional(),
  })
  .strict();
export type TerminalLeaseRequest = z.infer<typeof TerminalLeaseRequestSchema>;

export const TerminalLeaseResultSchema = z
  .object({
    terminalId: z.string().min(3).max(160),
    controlEpoch: z.number().int().min(1),
    lease: TerminalLeaseSchema.nullable(),
  })
  .strict();
export type TerminalLeaseResult = z.infer<typeof TerminalLeaseResultSchema>;

export interface ManagedTerminalLaunchRegistration {
  readonly accessProjectId: string;
  readonly accessContextId: string;
  readonly spec: TerminalLaunchSpec;
}

export interface ManagedTerminalListing {
  readonly terminalId: string;
  readonly projectId: string;
  readonly contextId: string;
  readonly sessionId: string;
  readonly runId: string;
  readonly state: ManagedTerminalSnapshot["state"] | "unknown";
  readonly controlEpoch: number;
}

export interface ManagedTerminalServicePort {
  start(
    registration: ManagedTerminalLaunchRegistration,
  ): Promise<ManagedTerminalResult<ManagedTerminalSnapshot>>;
  snapshot(
    access: WorkspaceAccessContext,
    terminalId: string,
  ): ManagedTerminalResult<ManagedTerminalSnapshot>;
  list(
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
  ): ManagedTerminalResult<readonly ManagedTerminalListing[]>;
  read(
    access: WorkspaceAccessContext,
    request: TerminalOutputPageRequest,
  ): ManagedTerminalResult<TerminalOutputPage>;
  acquire(
    access: WorkspaceAccessContext,
    request: TerminalLeaseRequest,
  ): ManagedTerminalResult<TerminalLeaseResult>;
  release(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void>;
  input(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
    data: string,
  ): ManagedTerminalResult<void>;
  resize(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
    columns: number,
    rows: number,
  ): ManagedTerminalResult<void>;
  interrupt(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void>;
  stop(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    expectedControlEpoch: number,
  ): ManagedTerminalResult<void>;
  stopProject(
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
  ): Promise<ManagedTerminalResult<readonly ManagedTerminalSnapshot[]>>;
  close(): void;
}

export function createManagedTerminalService(
  kernel: ManagedTerminalKernel,
  registrations: readonly ManagedTerminalLaunchRegistration[],
  outputStore?: ManagedTerminalOutputStore,
): ManagedTerminalServicePort {
  const byTerminal = new Map(
    registrations.map((registration) => [registration.spec.terminalId, registration]),
  );
  const scope = (access: WorkspaceAccessContext, terminalId: string) => {
    const registration = byTerminal.get(terminalId);
    return registration !== undefined &&
      registration.accessProjectId === registration.spec.projectId &&
      registration.accessContextId === registration.spec.contextId &&
      access.authorizedProjectIds.some((projectId) => projectId === registration.spec.projectId)
      ? registration
      : undefined;
  };
  const owned = (access: WorkspaceAccessContext, terminalId: string) =>
    scope(access, terminalId) === undefined
      ? failure("forbidden", "terminal is outside the authorized project scope")
      : undefined;
  return {
    async start(registration) {
      byTerminal.set(registration.spec.terminalId, registration);
      const started = await kernel.start(TerminalLaunchSpecSchema.parse(registration.spec));
      if (started.ok && outputStore !== undefined) {
        kernel.subscribe(registration.spec.terminalId, (event) => {
          outputStore.append(event);
        });
        kernel.subscribeLifecycle(registration.spec.terminalId, (event) => {
          outputStore.appendLifecycle(event);
        });
      }
      return started;
    },
    snapshot(access, terminalId) {
      const denied = owned(access, terminalId);
      return denied ?? kernel.snapshot(terminalId);
    },
    list(access, projectId, contextId) {
      if (!access.authorizedProjectIds.some((authorized) => authorized === projectId)) {
        return failure("forbidden", "managed terminal list is outside authorized project scope");
      }
      const live = kernel.snapshots(projectId, contextId);
      const byId = new Set(live.map((terminal) => terminal.terminalId));
      const recovered = (outputStore?.latestLifecycle(projectId, contextId) ?? [])
        .filter((event) => !byId.has(event.terminalId))
        .map(
          (event): ManagedTerminalListing => ({
            terminalId: event.terminalId,
            projectId: event.projectId,
            contextId: event.contextId,
            sessionId: event.sessionId,
            runId: event.runId,
            state:
              event.operation === "exit"
                ? event.exitCode === 0
                  ? "exited"
                  : "stopped"
                : "unknown",
            controlEpoch: event.controlEpoch,
          }),
        );
      return { ok: true, value: [...live, ...recovered] };
    },
    read(access, rawRequest) {
      const request = TerminalOutputPageRequestSchema.safeParse(rawRequest);
      if (!request.success) return failure("invalid_input", "terminal output request is invalid");
      const registration = byTerminal.get(request.data.terminalId);
      if (
        registration === undefined ||
        registration.spec.projectId !== request.data.projectId ||
        registration.spec.contextId !== request.data.contextId ||
        scope(access, request.data.terminalId) === undefined
      )
        return failure("forbidden", "terminal scope does not match its registered project context");
      const snapshot = kernel.snapshot(request.data.terminalId);
      if (!snapshot.ok) return snapshot;
      const stored = outputStore?.page(
        request.data.terminalId,
        request.data.afterSequence,
        request.data.limit,
      );
      const events =
        stored?.events ??
        snapshot.value.output
          .filter((event) => event.sequence > request.data.afterSequence)
          .slice(0, request.data.limit);
      const available = events[0]?.sequence ?? snapshot.value.nextSequence;
      const gap =
        stored?.gap ??
        (request.data.afterSequence > 0 && available > request.data.afterSequence + 1
          ? { firstAvailableSequence: available, reason: "output history was bounded" }
          : null);
      return {
        ok: true,
        value: TerminalOutputPageSchema.parse({
          terminalId: request.data.terminalId,
          events: events.map((event) => ({
            terminalId: event.terminalId,
            projectId: registration.spec.projectId,
            contextId: registration.spec.contextId,
            controlEpoch:
              "controlEpoch" in event && typeof event.controlEpoch === "number"
                ? event.controlEpoch
                : snapshot.value.controlEpoch,
            sequence: event.sequence,
            data: event.data,
            occurredAt: event.occurredAt,
          })),
          afterSequence: request.data.afterSequence,
          nextSequence: events.at(-1)?.sequence ?? null,
          gap,
        }),
      };
    },
    acquire(access, request) {
      const denied = owned(access, request.terminalId);
      if (denied !== undefined) return denied;
      const result = kernel.acquire(
        request.terminalId,
        access.clientId,
        request.expectedControlEpoch,
        request.takeover,
      );
      return result.ok
        ? {
            ok: true,
            value: {
              terminalId: request.terminalId,
              controlEpoch: result.value.controlEpoch,
              lease: result.value,
            },
          }
        : result;
    },
    release(access, terminalId, leaseId, epoch) {
      return guarded(scope, access, terminalId, () => kernel.release(terminalId, leaseId, epoch));
    },
    input(access, terminalId, leaseId, epoch, data) {
      return guarded(scope, access, terminalId, () =>
        kernel.write(terminalId, leaseId, epoch, data),
      );
    },
    resize(access, terminalId, leaseId, epoch, columns, rows) {
      return guarded(scope, access, terminalId, () =>
        kernel.resize(terminalId, leaseId, epoch, columns, rows),
      );
    },
    interrupt(access, terminalId, leaseId, epoch) {
      return guarded(scope, access, terminalId, () => kernel.interrupt(terminalId, leaseId, epoch));
    },
    stop(access, terminalId, leaseId, epoch) {
      return guarded(scope, access, terminalId, () => kernel.stop(terminalId, leaseId, epoch));
    },
    stopProject(access, projectId, contextId) {
      if (!access.authorizedProjectIds.some((authorized) => authorized === projectId)) {
        return Promise.resolve(
          failure("forbidden", "managed project stop is outside authorized scope"),
        );
      }
      return kernel.stopProject(projectId, contextId);
    },
    close() {
      kernel.close();
      outputStore?.close();
    },
  };
}

function guarded<T>(
  scope: (access: WorkspaceAccessContext, terminalId: string) => unknown,
  access: WorkspaceAccessContext,
  terminalId: string,
  call: () => ManagedTerminalResult<T>,
): ManagedTerminalResult<T> {
  if (scope(access, terminalId) === undefined)
    return failure("forbidden", "terminal is outside the authorized project scope");
  return call();
}

function failure(
  code: "invalid_input" | "unsupported" | "forbidden" | "conflict" | "unavailable",
  message: string,
): ManagedTerminalResult<never> {
  return { ok: false, error: { code, message } };
}
