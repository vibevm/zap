/** @scope spec://org.vibevm.zap/lens/PROP-001#transport */
import type { IncomingMessage } from "node:http";
import { timingSafeEqual } from "node:crypto";
import type { ZodType } from "zod";
import {
  PrincipalEmitInputSchema,
  EventsInputSchema,
  ScopedListInputSchema,
  type Credential,
  type Result,
  CredentialSchema,
} from "../protocol/index.ts";
import {
  AdapterSessionIdSchema,
  failure,
  type AdapterSessionId,
  type TransportBrokerPort,
} from "../transport/index.ts";
import { singleHeader } from "./server-response.ts";

export function validateRequestSource(
  request: IncomingMessage,
  options: { readonly allowedHosts: readonly string[]; readonly allowedOrigins: readonly string[] },
): Result<null> {
  const host = singleHeader(request, "host");
  if (host === undefined || !hostAllowed(host, options.allowedHosts))
    return failure("unauthorized", "Host header is not allowlisted");
  const origin = singleHeader(request, "origin");
  return origin !== undefined && !options.allowedOrigins.includes(origin)
    ? failure("unauthorized", "Origin header is not allowlisted")
    : { ok: true, value: null };
}

export function bearerCredential(request: IncomingMessage): Result<Credential> {
  const value = singleHeader(request, "authorization");
  const parsed = CredentialSchema.safeParse(value?.startsWith("Bearer ") ? value.slice(7) : "");
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("unauthorized", "Bearer credential is missing or malformed");
}

export function adapterSessionHeader(request: IncomingMessage): Result<AdapterSessionId> {
  const parsed = AdapterSessionIdSchema.safeParse(
    singleHeader(request, "x-codlens-adapter-session"),
  );
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("unauthorized", "adapter session credential is missing or malformed");
}

export function isLoopback(host: string): boolean {
  return host === "127.0.0.1" || host === "localhost" || host === "::1";
}

export async function executePrincipalCommand(
  path: string,
  body: unknown,
  broker: TransportBrokerPort,
  principalToken: Credential,
): Promise<Result<unknown>> {
  const auth = { principalToken };
  if (path === "/v1/actors")
    return parsedCall(ScopedListInputSchema, body, (input) => broker.listActors(auth, input));
  if (path === "/v1/questions")
    return parsedCall(ScopedListInputSchema, body, (input) => broker.listQuestions(auth, input));
  if (path === "/v1/event-page")
    return parsedCall(EventsInputSchema, body, (input) => broker.events(auth, input));
  return parsedCall(PrincipalEmitInputSchema, body, (input) => broker.emitPrincipal(auth, input));
}

export function hostAllowed(value: string, allowed: readonly string[]): boolean {
  if (allowed.includes(value)) return true;
  try {
    return allowed.includes(new URL(`http://${value}`).hostname);
  } catch {
    return false;
  }
}

export function constantEqual(left: string, right: string): boolean {
  const leftBytes = Buffer.from(left);
  const rightBytes = Buffer.from(right);
  return leftBytes.length === rightBytes.length && timingSafeEqual(leftBytes, rightBytes);
}

export function delay(milliseconds: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, milliseconds);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}

export async function readJson(
  request: IncomingMessage,
  maximum: number,
): Promise<Result<unknown>> {
  const chunks: Buffer[] = [];
  let length = 0;
  for await (const chunk of request) {
    if (!Buffer.isBuffer(chunk)) {
      return failure("invalid_input", "HTTP body contains a non-byte chunk");
    }
    length += chunk.length;
    if (length > maximum) {
      return failure("backpressure", "HTTP request body exceeds its configured bound");
    }
    chunks.push(chunk);
  }
  try {
    const value: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    return { ok: true, value };
  } catch {
    return failure("invalid_input", "HTTP request body is not JSON");
  }
}

export async function parsedCall<I, O>(
  schema: ZodType<I>,
  value: unknown,
  call: (input: I) => Result<O> | Promise<Result<O>>,
): Promise<Result<O>> {
  const parsed = parse(schema, value);
  return parsed.ok ? await call(parsed.value) : parsed;
}

export function parse<T>(schema: ZodType<T>, value: unknown): Result<T> {
  const parsed = schema.safeParse(value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure(
        "invalid_input",
        "external JSON failed runtime validation",
        "send the closed lens/1 schema for this route",
      );
}

export function statusOf<T>(result: Result<T>): number {
  if (result.ok) return 200;
  const code = result.error.code;
  if (code === "unauthorized") return 401;
  if (code === "forbidden") return 403;
  if (code === "not_found") return 404;
  if (code === "backpressure") return 429;
  if (code === "invalid_input") return 400;
  if (code === "storage_failure") return 500;
  return 409;
}
