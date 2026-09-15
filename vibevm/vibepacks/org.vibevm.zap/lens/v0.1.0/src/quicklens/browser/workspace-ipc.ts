/** @scope spec://org.vibevm.zap/lens/PROP-005#shared-code */
/** Renderer-owned WorkspaceClientPort over clone-safe Electron IPC calls. */
import { z } from "zod";
import {
  HistoryPageSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceErrorSchema,
  WorkspaceReadResponseSchema,
  type HistoryEvent,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceEventsRequest,
  type WorkspaceReadRequest,
  type WorkspaceReadResponse,
  type WorkspaceResult,
} from "../../cells/workspace-model/index.ts";

export interface WorkspaceIpcBridge {
  read(request: unknown): Promise<unknown>;
  command(request: unknown): Promise<unknown>;
  events(request: unknown): Promise<unknown>;
}

export function createWorkspaceIpcClient(
  bridge: WorkspaceIpcBridge,
  pollMilliseconds = 500,
): WorkspaceClientPort {
  const read = async (
    request: WorkspaceReadRequest,
  ): Promise<WorkspaceResult<WorkspaceReadResponse>> =>
    envelope(await bridge.read(request), WorkspaceReadResponseSchema);
  const command = async (
    request: WorkspaceCommandRequest,
  ): Promise<WorkspaceResult<WorkspaceCommandResponse>> =>
    envelope(await bridge.command(request), WorkspaceCommandResponseSchema);
  const events = async (request: WorkspaceEventsRequest) =>
    envelope(await bridge.events(request), HistoryPageSchema);
  return {
    read,
    command,
    events,
    subscribe: (request) => subscribe(request.cursor, request.signal, events, pollMilliseconds),
  };
}

async function* subscribe(
  initial: Parameters<WorkspaceClientPort["subscribe"]>[0]["cursor"],
  signal: AbortSignal | undefined,
  events: WorkspaceClientPort["events"],
  pollMilliseconds: number,
): AsyncIterable<WorkspaceResult<HistoryEvent>> {
  let cursor = initial;
  while (signal?.aborted !== true) {
    const page = await events({ cursor, limit: 256 });
    if (!page.ok) {
      yield page;
      return;
    }
    for (const event of page.value.events) yield { ok: true, value: event };
    cursor = page.value.next ?? page.value.resume;
    if (page.value.events.length === 0) await waitForPoll(pollMilliseconds, signal);
  }
}

function envelope<T>(value: unknown, schema: z.ZodType<T>): WorkspaceResult<T> {
  const parsed = z
    .union([
      z.object({ ok: z.literal(true), value: schema }).strict(),
      z.object({ ok: z.literal(false), error: WorkspaceErrorSchema }).strict(),
    ])
    .safeParse(value);
  return parsed.success
    ? parsed.data
    : {
        ok: false,
        error: {
          code: "unavailable",
          message:
            "violates REQ spec://org.vibevm.zap/lens/PROP-005#transport: workspace IPC response is invalid.",
        },
      };
}

function waitForPoll(milliseconds: number, signal: AbortSignal | undefined): Promise<void> {
  if (signal?.aborted === true) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, milliseconds);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}
