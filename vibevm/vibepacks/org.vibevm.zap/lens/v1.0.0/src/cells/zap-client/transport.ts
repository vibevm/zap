import { z, type ZodType } from "zod";
import { encodeCanonicalJson, parseWireJson } from "./codec.ts";
import { diagnostic } from "./diagnostics.ts";
import {
  EventCursorSchema,
  EventSummarySchema,
  RefusalSchema,
  ResyncSchema,
  machineResponse,
} from "./schemas.ts";
import type {
  EventCursor,
  ZapClientResult,
  ZapCredential,
  ZapExchangeResponse,
  ZapHttpExchange,
  ZapStreamPage,
} from "./types.ts";

/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
export class Transport {
  readonly #endpoint: URL;
  readonly #credential: ZapCredential;
  readonly #exchange: ZapHttpExchange;
  readonly #maximum: number;

  constructor(
    endpoint: URL,
    credential: ZapCredential,
    exchange: ZapHttpExchange,
    maximum: number,
  ) {
    this.#endpoint = endpoint;
    this.#credential = credential;
    this.#exchange = exchange;
    this.#maximum = maximum;
  }

  get<T>(path: string, kind: string, schema: ZodType<T>, signal?: AbortSignal) {
    return this.call(path, undefined, kind, schema, signal);
  }

  post<T>(path: string, body: unknown, kind: string, schema: ZodType<T>, signal?: AbortSignal) {
    return this.call(path, body, kind, schema, signal);
  }

  async postCaptured<T>(
    path: string,
    body: unknown,
    kind: string,
    schema: ZodType<T>,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<{ value: T; canonicalValue: Uint8Array }>> {
    const response = await this.raw(path, body, signal);
    if (!response.ok) return response;
    const contentType = header(response.value.headers, "content-type") ?? "";
    if (!contentType.toLowerCase().startsWith("application/json")) {
      return failure("malformed_response", "ZAP machine response is not application/json");
    }
    try {
      const raw = parseWireJson(response.value.body);
      const loose = z.looseObject({ kind: z.string(), value: z.unknown() }).safeParse(raw);
      if (!loose.success || loose.data.kind !== kind) {
        return loose.success
          ? {
              ok: false,
              error: { kind: "foreign_response", expected: kind, received: loose.data.kind },
            }
          : failure("malformed_response", "ZAP response is not a typed machine response");
      }
      const parsed = schema.safeParse(loose.data.value);
      return parsed.success
        ? success({ value: parsed.data, canonicalValue: encodeCanonicalJson(loose.data.value) })
        : failure("malformed_response", "ZAP response value failed runtime validation");
    } catch {
      return failure("malformed_response", "ZAP response is invalid UTF-8 or strict JSON");
    }
  }

  async raw(
    path: string,
    body: unknown,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapExchangeResponse>> {
    let encoded: Uint8Array;
    try {
      encoded = encodeCanonicalJson(body);
    } catch {
      return failure("configuration", "request cannot be encoded as canonical JSON");
    }
    try {
      const response = await this.#exchange.request({
        method: "POST",
        url: new URL(path, this.#endpoint),
        headers: this.headers(true),
        body: encoded,
        ...(signal === undefined ? {} : { signal }),
      });
      if (response.body.length > this.#maximum) return responseLimit(this.#maximum, response);
      if (response.status < 200 || response.status > 299) return decodeFailure(response);
      return success(response);
    } catch {
      return failure("transport", "ZAP HTTP exchange ended without a response");
    }
  }

  private async call<T>(
    path: string,
    body: unknown,
    kind: string,
    schema: ZodType<T>,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<T>> {
    let encoded: Uint8Array | undefined;
    try {
      encoded = body === undefined ? undefined : encodeCanonicalJson(body);
    } catch {
      return failure("configuration", "request cannot be encoded as canonical JSON");
    }
    let response: ZapExchangeResponse;
    try {
      response = await this.#exchange.request({
        method: encoded === undefined ? "GET" : "POST",
        url: new URL(path, this.#endpoint),
        headers: this.headers(encoded !== undefined),
        ...(encoded === undefined ? {} : { body: encoded }),
        ...(signal === undefined ? {} : { signal }),
      });
    } catch {
      return failure("transport", "ZAP HTTP exchange ended without a response");
    }
    if (response.body.length > this.#maximum) return responseLimit(this.#maximum, response);
    if (response.status < 200 || response.status > 299) return decodeFailure(response);
    const contentType = header(response.headers, "content-type") ?? "";
    if (!contentType.toLowerCase().startsWith("application/json")) {
      return failure("malformed_response", "ZAP machine response is not application/json");
    }
    try {
      const raw = parseWireJson(response.body);
      const envelope = machineResponse(kind, schema).safeParse(raw);
      if (envelope.success) return success(envelope.data.value);
      const received = z.looseObject({ kind: z.string() }).safeParse(raw);
      return received.success
        ? {
            ok: false,
            error: { kind: "foreign_response", expected: kind, received: received.data.kind },
          }
        : failure("malformed_response", "ZAP response is not a typed machine response");
    } catch {
      return failure("malformed_response", "ZAP response is invalid UTF-8 or strict JSON");
    }
  }

  private headers(body: boolean): Readonly<Record<string, string>> {
    return {
      Accept: "application/json",
      Authorization: `Bearer ${this.#credential.bearer}`,
      "X-ZAP-Credential-ID": this.#credential.id,
      ...(body ? { "Content-Type": "application/json" } : {}),
    };
  }
}

function header(headers: Readonly<Record<string, string>>, name: string): string | undefined {
  return Object.entries(headers).find(([key]) => key.toLowerCase() === name)?.[1];
}

export function parseSse(bytes: Uint8Array): ZapStreamPage {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const blocks = text.split("\n\n").filter((value) => value.length > 0);
  const events = [];
  let cursor: EventCursor | undefined;
  for (const block of blocks) {
    const lines = block.split("\n");
    if (
      lines[0] === "event: zap" &&
      lines[1]?.startsWith("id: ") &&
      lines[2]?.startsWith("data: ")
    ) {
      const event = EventSummarySchema.parse(
        parseWireJson(new TextEncoder().encode(lines[2].slice(6))),
      );
      if (event.sequence !== lines[1].slice(4)) throw diagnostic("SSE event ID differs");
      events.push(event);
    } else if (
      lines[0] === "event: cursor" &&
      lines[1]?.startsWith("data: ") &&
      lines.length === 2
    ) {
      if (cursor !== undefined) throw diagnostic("duplicate SSE cursor");
      cursor = EventCursorSchema.parse(parseWireJson(new TextEncoder().encode(lines[1].slice(6))));
    } else {
      throw diagnostic("unknown SSE frame");
    }
  }
  if (cursor === undefined || blocks.at(-1)?.startsWith("event: cursor") !== true) {
    throw diagnostic("SSE cursor is missing or not final");
  }
  return { events, cursor };
}

function decodeFailure(response: ZapExchangeResponse): ZapClientResult<never> {
  try {
    const raw = parseWireJson(response.body);
    const resync = ResyncSchema.safeParse(raw);
    if (resync.success) return { ok: false, error: resync.data };
    const refusal = RefusalSchema.safeParse(raw);
    return refusal.success
      ? {
          ok: false,
          error: { kind: "http_refusal", status: response.status, refusal: refusal.data },
        }
      : failure("malformed_response", "HTTP failure is not a typed ZAP refusal");
  } catch {
    return failure("malformed_response", "HTTP failure is invalid UTF-8 or strict JSON");
  }
}

function responseLimit(maximum: number, response: ZapExchangeResponse): ZapClientResult<never> {
  return {
    ok: false,
    error: { kind: "limit_exceeded", maximum, actual: response.body.length },
  };
}

function success<T>(value: T): ZapClientResult<T> {
  return { ok: true, value };
}

function failure(
  kind: "configuration" | "transport" | "malformed_response",
  message: string,
): ZapClientResult<never> {
  switch (kind) {
    case "configuration":
      return { ok: false, error: { kind: "configuration", message } };
    case "transport":
      return { ok: false, error: { kind: "transport", message } };
    case "malformed_response":
      return { ok: false, error: { kind: "malformed_response", message } };
  }
}
