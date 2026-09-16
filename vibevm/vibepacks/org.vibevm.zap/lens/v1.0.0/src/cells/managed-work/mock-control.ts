/** Deterministic ZapMock managed PTY sideband.
 * @scope spec://org.vibevm.zap/lens/PROP-013#agent
 */
import {
  appendFileSync,
  mkdirSync,
  readFileSync,
  unwatchFile,
  watchFile,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import {
  ManagedControlTargetSchema,
  type ManagedControlTarget,
  type ManagedProviderControlAdapter,
  type ManagedSessionControlEvent,
} from "./control.ts";
import type { ManagedWorkResult } from "./contracts.ts";

export const ZapMockManagedInputSchema = z.discriminatedUnion("kind", [
  z
    .object({ kind: z.literal("offer"), deliveryId: z.string().min(3), bodyMarkdown: z.string() })
    .strict(),
  z
    .object({
      kind: z.literal("interrupt"),
      reason: z.enum(["project_pause", "user_interrupt"]),
    })
    .strict(),
]);
export const ZapMockManagedAssignmentSchema = ManagedControlTargetSchema.extend({
  protocol: z.literal("zap-mock-managed-assignment/1"),
}).strict();
export const ZapMockManagedSidebandEventSchema = z
  .object({
    kind: z.enum([
      "session_ready",
      "turn_started",
      "permission_required",
      "turn_settled",
      "session_exited",
      "delivery_rejected",
    ]),
    providerSessionId: z.string().min(1).max(512).nullable(),
    providerTurnId: z.string().min(1).max(512).nullable(),
    transportCorrelation: z.string().min(3).max(512).nullable(),
  })
  .strict();

interface MockControlState {
  readonly target: ManagedControlTarget;
  readonly inputPath: string;
  readonly sidebandPath: string;
  readonly publish: (event: Omit<ManagedSessionControlEvent, "sourceSequence">) => void;
  lines: number;
}

export function createZapMockManagedControlAdapter(input: {
  readonly directory: string;
}): ManagedProviderControlAdapter {
  mkdirSync(input.directory, { recursive: true });
  const records = new WeakMap<object, MockControlState>();
  return {
    provider: "zap_mock",
    canQueueWhileBusy: true,
    async prepare(prepared) {
      await Promise.resolve();
      const stem = join(input.directory, `zap-mock.${digest(prepared.target.runId)}`);
      const inputPath = `${stem}.input.jsonl`;
      const sidebandPath = `${stem}.sideband.jsonl`;
      writeFileSync(inputPath, "", { encoding: "utf8", mode: 0o600 });
      writeFileSync(sidebandPath, "", { encoding: "utf8", mode: 0o600 });
      const state: MockControlState = {
        target: prepared.target,
        inputPath,
        sidebandPath,
        publish: prepared.publish,
        lines: 0,
      };
      const key = {};
      records.set(key, state);
      watch(state);
      const instruction = prepared.launch.args.indexOf("--instructions");
      const sidebandArgs = [
        "--assignment",
        JSON.stringify(
          ZapMockManagedAssignmentSchema.parse({
            protocol: "zap-mock-managed-assignment/1",
            ...prepared.target,
          }),
        ),
        "--sideband",
        sidebandPath,
        "--input",
        inputPath,
        "--session",
        prepared.target.sessionId,
      ];
      return {
        ok: true,
        value: {
          state: key,
          launch: {
            ...prepared.launch,
            args:
              instruction < 0
                ? [...prepared.launch.args, ...sidebandArgs]
                : [
                    ...prepared.launch.args.slice(0, instruction),
                    ...sidebandArgs,
                    ...prepared.launch.args.slice(instruction),
                  ],
          },
        },
      };
    },
    async interrupt(raw, reason) {
      await Promise.resolve();
      const state = record(records, raw);
      if (!state.ok) return state;
      return append(state.value, { kind: "interrupt", reason });
    },
    async offer(raw, deliveryId, bodyMarkdown) {
      await Promise.resolve();
      const state = record(records, raw);
      if (!state.ok) return state;
      const written = append(state.value, { kind: "offer", deliveryId, bodyMarkdown });
      return written.ok ? { ok: true, value: { queued: true, correlation: deliveryId } } : written;
    },
    close(raw) {
      if (!object(raw)) return;
      const state = records.get(raw);
      if (state === undefined) return;
      unwatchFile(state.sidebandPath);
      records.delete(raw);
    },
  };
}

function watch(state: MockControlState): void {
  watchFile(state.sidebandPath, { interval: 50 }, () => {
    let text: string;
    try {
      text = readFileSync(state.sidebandPath, "utf8");
    } catch {
      return;
    }
    const lines = text.split(/\r?\n/);
    if (!text.endsWith("\n")) lines.pop();
    else lines.pop();
    if (lines.length < state.lines) state.lines = 0;
    for (const line of lines.slice(state.lines)) publish(state, line);
    state.lines = lines.length;
  });
}

function publish(state: MockControlState, line: string): void {
  let raw: unknown;
  try {
    raw = JSON.parse(line);
  } catch {
    return;
  }
  const parsed = ZapMockManagedSidebandEventSchema.safeParse(raw);
  if (!parsed.success) return;
  state.publish({
    eventId: `mock-control-event.${randomUUID().replaceAll("-", "")}`,
    runId: state.target.runId,
    actorId: state.target.actorId,
    sessionId: state.target.sessionId,
    terminalId: state.target.terminalId,
    provider: "zap_mock",
    processEpoch: state.target.expectedProcessEpoch,
    kind: parsed.data.kind,
    providerSessionId: parsed.data.providerSessionId,
    providerTurnId: parsed.data.providerTurnId,
    transportCorrelation: parsed.data.transportCorrelation,
    occurredAt: new Date().toISOString(),
  });
}

function append(
  state: MockControlState,
  raw: z.input<typeof ZapMockManagedInputSchema>,
): ManagedWorkResult<"requested"> {
  const command = ZapMockManagedInputSchema.safeParse(raw);
  if (!command.success) return fail("invalid_input", "ZapMock managed input is invalid");
  try {
    appendFileSync(state.inputPath, `${JSON.stringify(command.data)}\n`, "utf8");
    return { ok: true, value: "requested" };
  } catch {
    return fail("uncertain", "ZapMock managed input acceptance is uncertain");
  }
}

function record(
  records: WeakMap<object, MockControlState>,
  raw: unknown,
): ManagedWorkResult<MockControlState> {
  const state = object(raw) ? records.get(raw) : undefined;
  return state === undefined
    ? fail("conflict", "ZapMock managed control state is stale")
    : { ok: true, value: state };
}

function object(value: unknown): value is object {
  return typeof value === "object" && value !== null;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}

function fail(
  code: "invalid_input" | "conflict" | "uncertain",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
