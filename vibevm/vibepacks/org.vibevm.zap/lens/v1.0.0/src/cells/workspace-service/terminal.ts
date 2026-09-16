/** Managed-terminal workspace bridge. @scope spec://org.vibevm.zap/lens/PROP-006#network-and-control */
import type { ManagedTerminalResult } from "../managed-terminal/index.ts";
import {
  ManagedTerminalViewSchema,
  TerminalCommandReceiptSchema,
  TerminalLeaseViewSchema,
  TerminalOutputPageSchema,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceReadRequest,
  type WorkspaceReadResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { workspaceFailure } from "./errors.ts";
import type { WorkspaceManagedTerminalPort } from "./types.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { Launch } from "./launch.ts";
import { registerManagedTerminalAgent } from "./managed-agent.ts";

type TerminalRead = Extract<WorkspaceReadRequest, { operation: "terminal.output.page.v1" }>;
type TerminalList = Extract<WorkspaceReadRequest, { operation: "terminal.list.v1" }>;
type TerminalCommand = Extract<
  WorkspaceCommandRequest,
  {
    operation:
      | "terminal.acquire.v1"
      | "terminal.start.v1"
      | "terminal.release.v1"
      | "terminal.input.v1"
      | "terminal.resize.v1"
      | "terminal.interrupt.v1"
      | "terminal.stop.v1";
  }
>;

export function readTerminal(
  terminals: WorkspaceManagedTerminalPort | undefined,
  access: WorkspaceAccessContext,
  request: TerminalRead,
): WorkspaceResult<WorkspaceReadResponse> {
  if (terminals === undefined)
    return workspaceFailure("unavailable", "managed terminal service is not configured");
  const after = safeInteger(request.afterSequence);
  if (after === null)
    return workspaceFailure("invalid_input", "terminal cursor exceeds safe range");
  const result = terminals.read(access, {
    projectId: request.projectId,
    contextId: request.contextId,
    terminalId: request.terminalId,
    afterSequence: after,
    limit: request.limit,
  });
  if (!result.ok) return terminalFailure(result);
  return {
    ok: true,
    value: {
      operation: request.operation,
      page: TerminalOutputPageSchema.parse({
        terminalId: result.value.terminalId,
        events: result.value.events.map((event) => ({
          ...event,
          sequence: String(event.sequence),
          controlEpoch: String(event.controlEpoch),
        })),
        afterSequence: String(result.value.afterSequence),
        nextSequence: result.value.nextSequence === null ? null : String(result.value.nextSequence),
        gap:
          result.value.gap === null
            ? null
            : {
                ...result.value.gap,
                firstAvailableSequence: String(result.value.gap.firstAvailableSequence),
              },
      }),
    },
  };
}

export function listTerminals(
  terminals: WorkspaceManagedTerminalPort | undefined,
  access: WorkspaceAccessContext,
  request: TerminalList,
): WorkspaceResult<WorkspaceReadResponse> {
  if (terminals === undefined)
    return workspaceFailure("unavailable", "managed terminal service is not configured");
  const listed = terminals.list(access, request.projectId, request.contextId);
  if (!listed.ok) return terminalFailure(listed);
  return {
    ok: true,
    value: {
      operation: request.operation,
      terminals: listed.value.map((terminal) =>
        ManagedTerminalViewSchema.parse({
          terminalId: terminal.terminalId,
          projectId: terminal.projectId,
          contextId: terminal.contextId,
          sessionId: terminal.sessionId,
          runId: terminal.runId,
          state: terminal.state,
          controlEpoch: String(terminal.controlEpoch),
        }),
      ),
    },
  };
}

export async function commandTerminal(
  terminals: WorkspaceManagedTerminalPort | undefined,
  access: WorkspaceAccessContext,
  request: TerminalCommand,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  if (terminals === undefined)
    return workspaceFailure("unavailable", "managed terminal service is not configured");
  if (request.operation === "terminal.start.v1") {
    const started = await terminals.startRegistered(access, {
      profileId: request.profileId,
      projectId: request.projectId,
      contextId: request.contextId,
      terminalId: request.terminalId,
      sessionId: request.sessionId,
      runId: request.runId,
    });
    return started.ok
      ? {
          ok: true,
          value: {
            operation: request.operation,
            terminal: ManagedTerminalViewSchema.parse({
              terminalId: started.value.terminalId,
              projectId: started.value.projectId,
              contextId: started.value.contextId,
              sessionId: started.value.sessionId,
              runId: started.value.runId,
              state: started.value.state,
              controlEpoch: String(started.value.controlEpoch),
            }),
          },
        }
      : terminalFailure(started);
  }
  const epoch = safeInteger(request.expectedControlEpoch);
  if (epoch === null)
    return workspaceFailure("invalid_input", "terminal control epoch exceeds safe range");
  if (request.operation === "terminal.acquire.v1") {
    const acquired = terminals.acquire(access, {
      terminalId: request.terminalId,
      expectedControlEpoch: epoch,
      takeover: request.takeover,
    });
    if (!acquired.ok) return terminalFailure(acquired);
    if (acquired.value.lease === null)
      return workspaceFailure("unavailable", "terminal did not return an input lease");
    return {
      ok: true,
      value: {
        operation: request.operation,
        lease: TerminalLeaseViewSchema.parse({
          ...acquired.value.lease,
          controlEpoch: String(acquired.value.lease.controlEpoch),
        }),
      },
    };
  }
  const result = terminalMutation(terminals, access, request, epoch);
  return result.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          receipt: TerminalCommandReceiptSchema.parse({
            terminalId: request.terminalId,
            observation: "accepted",
          }),
        },
      }
    : terminalFailure(result);
}

export async function startManagedTerminal(
  terminals: WorkspaceManagedTerminalPort | undefined,
  store: WorkspaceStore,
  launches: ReadonlyMap<string, Launch>,
  clock: () => Date,
  access: WorkspaceAccessContext,
  request: Extract<WorkspaceCommandRequest, { operation: "terminal.start.v1" }>,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const execution = store.readProjectExecution(request.projectId, request.contextId);
  if (!execution.ok) return execution;
  if (execution.value.state !== "uninitialized" && execution.value.state !== "running")
    return workspaceFailure("conflict", "project dispatch gate blocks managed terminal start");
  const started = await commandTerminal(terminals, access, request);
  if (!started.ok) return started;
  if (started.value.operation !== "terminal.start.v1")
    return workspaceFailure("storage_failure", "managed terminal returned another operation");
  const registered = registerManagedTerminalAgent({
    store,
    request,
    response: started.value,
    coordinator:
      execution.value.sessionId === null ? undefined : launches.get(execution.value.sessionId),
    now: clock().toISOString(),
  });
  return registered.ok ? started : registered;
}

function terminalMutation(
  terminals: WorkspaceManagedTerminalPort,
  access: WorkspaceAccessContext,
  request: Exclude<TerminalCommand, { operation: "terminal.acquire.v1" | "terminal.start.v1" }>,
  epoch: number,
): ManagedTerminalResult<void> {
  if (request.operation === "terminal.release.v1")
    return terminals.release(access, request.terminalId, request.leaseId, epoch);
  if (request.operation === "terminal.input.v1")
    return terminals.input(access, request.terminalId, request.leaseId, epoch, request.data);
  if (request.operation === "terminal.resize.v1")
    return terminals.resize(
      access,
      request.terminalId,
      request.leaseId,
      epoch,
      request.columns,
      request.rows,
    );
  if (request.operation === "terminal.interrupt.v1")
    return terminals.interrupt(access, request.terminalId, request.leaseId, epoch);
  return terminals.stop(access, request.terminalId, request.leaseId, epoch);
}

function safeInteger(value: string): number | null {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function terminalFailure<T>(result: Extract<ManagedTerminalResult<T>, { ok: false }>) {
  const code =
    result.error.code === "unsupported"
      ? "unsupported_operation"
      : result.error.code === "unavailable"
        ? "unavailable"
        : result.error.code;
  return workspaceFailure(code, result.error.message);
}
