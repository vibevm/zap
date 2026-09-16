/** Claude-compatible stream-JSON control requests. @scope spec://org.vibevm.zap/lens/PROP-012#pause */
import { randomUUID } from "node:crypto";
import { z } from "zod";
import type { AgentRuntimeResult } from "../agent-runtime/index.ts";
import type { JsonValue } from "../protocol/index.ts";

interface WritableLineProcess {
  write(input: string): void;
}

type ControlResult = AgentRuntimeResult<Record<string, unknown>>;

const ControlResponseSchema = z.looseObject({
  type: z.literal("control_response"),
  response: z.looseObject({
    subtype: z.string(),
    request_id: z.string(),
    response: z.unknown().optional(),
    error: z.string().optional(),
  }),
});
const ControlRequestSchema = z.looseObject({
  type: z.literal("control_request"),
  request_id: z.string(),
  request: z.looseObject({ subtype: z.string() }),
});

export function streamHostControlRequest(raw: unknown):
  | {
      readonly requestId: string;
      readonly request: Record<string, unknown>;
    }
  | undefined {
  const parsed = ControlRequestSchema.safeParse(raw);
  return parsed.success
    ? { requestId: parsed.data.request_id, request: parsed.data.request }
    : undefined;
}

export class StreamControlClient {
  readonly #pending = new Map<
    string,
    {
      readonly finish: (result: ControlResult) => void;
      readonly timer: ReturnType<typeof setTimeout>;
    }
  >();

  request(
    process: WritableLineProcess,
    subtype: "initialize" | "interrupt",
    timeoutMs: number,
  ): Promise<ControlResult> {
    const requestId = `control.${subtype}.${randomUUID()}`;
    return new Promise((resolve) => {
      let settled = false;
      const finish = (result: ControlResult) => {
        if (settled) return;
        settled = true;
        const pending = this.#pending.get(requestId);
        if (pending !== undefined) clearTimeout(pending.timer);
        this.#pending.delete(requestId);
        resolve(result);
      };
      const timer = setTimeout(() => {
        finish({
          ok: false,
          error: {
            code: "transport_lost",
            message: `Provider ${subtype} control response timed out`,
            retry: "after_reconcile",
          },
        });
      }, timeoutMs);
      this.#pending.set(requestId, { finish, timer });
      try {
        process.write(
          `${JSON.stringify({
            type: "control_request",
            request_id: requestId,
            request: { subtype },
          })}\n`,
        );
      } catch {
        finish({
          ok: false,
          error: {
            code: "transport_lost",
            message: `Provider ${subtype} control request was not written`,
            retry: "after_reconcile",
          },
        });
      }
    });
  }

  handle(raw: unknown): boolean {
    const parsed = ControlResponseSchema.safeParse(raw);
    if (!parsed.success) return false;
    const pending = this.#pending.get(parsed.data.response.request_id);
    if (pending === undefined) return true;
    if (parsed.data.response.subtype !== "success") {
      pending.finish({
        ok: false,
        error: {
          code: "host_refused",
          message: parsed.data.response.error ?? "Provider control request was refused",
          retry: "after_refresh",
        },
      });
      return true;
    }
    const payload = z.record(z.string(), z.unknown()).safeParse(parsed.data.response.response);
    pending.finish({ ok: true, value: payload.success ? payload.data : {} });
    return true;
  }

  respond(
    process: WritableLineProcess,
    requestId: string,
    answer: JsonValue,
  ): AgentRuntimeResult<void> {
    try {
      process.write(
        `${JSON.stringify({
          type: "control_response",
          response: { subtype: "success", request_id: requestId, response: answer },
        })}\n`,
      );
      return { ok: true, value: undefined };
    } catch {
      return {
        ok: false,
        error: {
          code: "transport_lost",
          message: "Provider control response was not written",
          retry: "after_reconcile",
        },
      };
    }
  }

  close(): void {
    for (const pending of this.#pending.values()) {
      pending.finish({
        ok: false,
        error: {
          code: "transport_lost",
          message: "Provider control channel closed",
          retry: "after_reconcile",
        },
      });
    }
    this.#pending.clear();
  }
}
