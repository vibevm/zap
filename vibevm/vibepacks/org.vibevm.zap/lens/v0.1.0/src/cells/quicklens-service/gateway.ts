/**
 * Cookie-authenticated loopback browser gateway for QuicklensDataSource.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shells
 */
import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { z } from "zod";
import {
  ExactDecimalSchema,
  PlanBasisSchema,
  PlanDecisionInputSchema,
  QuicklensRefSchema,
  type QuicklensDataSource,
  type QuicklensError,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import {
  WorkspaceCommandRequestSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceErrorSchema,
  WorkspaceEventsRequestSchema,
  WorkspaceReadRequestSchema,
  WorkspaceReadResponseSchema,
  HistoryPageSchema,
  type WorkspaceClientPort,
  type WorkspaceResult,
} from "../workspace-model/index.ts";

const CSP = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

export interface QuicklensGatewayOptions {
  readonly source: QuicklensDataSource;
  readonly namespace: string;
  readonly pairingToken: string;
  readonly allowedHosts: readonly string[];
  readonly allowedOrigins: readonly string[];
  readonly maximumBodyBytes?: number;
  readonly workspaceSource?: WorkspaceSourceFactory;
  readonly multiSession?: boolean;
  readonly maximumSessions?: number;
  readonly pairingTicketTtlMs?: number;
}

export interface WorkspaceSessionIdentity {
  readonly clientId: string;
  readonly sessionId: string;
}

export type WorkspaceSourceFactory = (
  identity: WorkspaceSessionIdentity,
) => WorkspaceClientPort | Promise<WorkspaceClientPort>;

export interface QuicklensGateway {
  start(input: {
    readonly host: string;
    readonly port: number;
  }): Promise<QuicklensResult<{ host: string; port: number; basePath: string }>>;
  close(): Promise<void>;
  issuePairingTicket?(): QuicklensResult<{ readonly ticket: string; readonly expiresAt: string }>;
}

const AnswerSchema = z
  .object({
    questionRef: QuicklensRefSchema,
    expectedRevision: ExactDecimalSchema,
    answer: z.string().max(16_384),
  })
  .strict();
const IntentSchema = z
  .object({
    text: z.string().min(1).max(16_384),
    basis: PlanBasisSchema,
    targetActorRef: QuicklensRefSchema,
  })
  .strict();
const PreviewSchema = z.object({ intentRef: QuicklensRefSchema, basis: PlanBasisSchema }).strict();
const ApplySchema = z
  .object({
    operationRef: QuicklensRefSchema,
    previewRef: QuicklensRefSchema,
    basis: PlanBasisSchema,
  })
  .strict();
const ReconcileSchema = z
  .object({ operationRef: QuicklensRefSchema, basis: PlanBasisSchema })
  .strict();
const InvalidationsSchema = z.object({ after: z.number().int().min(0) }).strict();

interface GatewaySession {
  readonly hash: Buffer;
  readonly identity: WorkspaceSessionIdentity;
  workspacePort: WorkspaceClientPort | undefined;
}

export function createQuicklensGateway(
  options: QuicklensGatewayOptions,
): QuicklensResult<QuicklensGateway> {
  const maximum = options.maximumBodyBytes ?? 64 * 1024;
  if (
    options.pairingToken.length < 24 ||
    options.pairingToken.length > 512 ||
    !/^[A-Za-z][A-Za-z0-9_-]{2,63}$/.test(options.namespace) ||
    options.allowedHosts.length === 0 ||
    options.allowedOrigins.length === 0 ||
    !Number.isInteger(maximum) ||
    maximum < 1_024 ||
    maximum > 1024 * 1024
  ) {
    return qerror("invalid_data", "Quicklens gateway configuration is invalid");
  }
  const pairingHash = hash(options.pairingToken);
  const basePath = `/quicklens/${options.namespace}`;
  const cookieName = `quicklens_session_${options.namespace}`;
  let paired = false;
  let sessionHash: Buffer | undefined;
  let workspaceIdentity: WorkspaceSessionIdentity | undefined;
  let workspacePort: WorkspaceClientPort | undefined;
  const multiSession = options.multiSession === true;
  const sessions = new Map<string, GatewaySession>();
  const tickets = new Map<string, number>();
  const maximumSessions = options.maximumSessions ?? 16;
  const ticketTtlMs = options.pairingTicketTtlMs ?? 5 * 60_000;
  let invalidationSequence = 0;
  const invalidations: { sequence: number; reason: string }[] = [];
  const unsubscribeInvalidations = options.source.subscribe?.((reason) => {
    invalidationSequence += 1;
    invalidations.push({ sequence: invalidationSequence, reason });
    if (invalidations.length > 100) invalidations.shift();
  });
  const server = createServer((request, response) => {
    void dispatch(request, response).catch(() => {
      send(response, 500, qerror("unavailable", "Quicklens gateway request failed"), request);
    });
  });

  const dispatch = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    const source = validateSource(request, options);
    if (!source.ok) {
      send(response, 403, source, request);
      return;
    }
    if (request.method === "OPTIONS") {
      securityHeaders(response, request);
      response.writeHead(204, {
        "Access-Control-Allow-Credentials": "true",
        "Access-Control-Allow-Headers": "Authorization, Content-Type",
        "Access-Control-Allow-Methods": "POST, OPTIONS",
      });
      response.end();
      return;
    }
    if (request.method !== "POST" || request.url === undefined) {
      send(response, 404, qerror("unsupported", "Quicklens route is unavailable"), request);
      return;
    }
    const absolutePath = new URL(request.url, "http://loopback").pathname;
    if (!absolutePath.startsWith(`${basePath}/v1/`)) {
      send(response, 404, qerror("unsupported", "Quicklens route is unavailable"), request);
      return;
    }
    const path = absolutePath.slice(basePath.length);
    if (path === "/v1/pair") {
      const supplied = bearer(request);
      const suppliedHash = supplied === undefined ? undefined : hash(supplied);
      const ticketExpiry =
        suppliedHash === undefined ? undefined : tickets.get(suppliedHash.toString("hex"));
      const validTicket =
        suppliedHash !== undefined &&
        ticketExpiry !== undefined &&
        ticketExpiry > Date.now() &&
        sessions.size < maximumSessions;
      if (
        supplied === undefined ||
        (multiSession ? !validTicket : paired || !equalHash(pairingHash, hash(supplied)))
      ) {
        send(response, 401, qerror("forbidden", "Quicklens pairing was refused"), request);
        return;
      }
      if (multiSession && suppliedHash !== undefined) tickets.delete(suppliedHash.toString("hex"));
      const session = randomBytes(32).toString("base64url");
      sessionHash = hash(session);
      paired = true;
      workspaceIdentity = {
        clientId: `client.${options.namespace}.${randomBytes(12).toString("hex")}`,
        sessionId: `session.${options.namespace}.${randomBytes(12).toString("hex")}`,
      };
      if (multiSession) {
        sessions.set(session, {
          hash: sessionHash,
          identity: workspaceIdentity,
          workspacePort: undefined,
        });
      }
      response.setHeader(
        "Set-Cookie",
        `${cookieName}=${session}; HttpOnly; SameSite=Strict; Path=${basePath}`,
      );
      send(response, 200, { ok: true, value: { paired: true } }, request);
      return;
    }
    const currentSession = multiSession ? sessionFor(request, cookieName, sessions) : undefined;
    if (
      multiSession
        ? currentSession === undefined
        : !paired ||
          sessionHash === undefined ||
          !authenticatedCookie(request, cookieName, sessionHash)
    ) {
      send(response, 401, qerror("forbidden", "Quicklens UI session is not paired"), request);
      return;
    }
    const body = await readBody(request, maximum);
    if (!body.ok) {
      send(response, 400, body, request);
      return;
    }
    const result =
      path === "/v1/invalidations"
        ? invalidationPage(body.value, invalidations, invalidationSequence)
        : path.startsWith("/v1/workspace/")
          ? await workspaceOperation(
              path,
              body.value,
              options.workspaceSource,
              currentSession?.identity ?? workspaceIdentity,
              currentSession?.workspacePort ?? workspacePort,
              request,
              (port) => {
                if (currentSession !== undefined) currentSession.workspacePort = port;
                else workspacePort = port;
              },
            )
          : await operation(path, body.value, options.source, request);
    send(
      response,
      result.ok ? 200 : result.error.code === "forbidden" ? 403 : 409,
      result,
      request,
    );
  };

  return {
    ok: true,
    value: {
      start: async (input) => {
        if (!loopback(input.host) || input.port < 0 || input.port > 65_535) {
          return qerror("invalid_data", "Quicklens gateway must bind bounded loopback");
        }
        return new Promise((resolve) => {
          const failed = (): void => {
            resolve(qerror("unavailable", "Quicklens gateway bind failed"));
          };
          server.once("error", failed);
          server.listen(input.port, input.host, () => {
            server.off("error", failed);
            const address = server.address();
            resolve(
              address !== null && typeof address !== "string"
                ? { ok: true, value: { ...addressView(address), basePath } }
                : qerror("unavailable", "Quicklens gateway address is unavailable"),
            );
          });
        });
      },
      close: () =>
        new Promise((resolve) => {
          unsubscribeInvalidations?.();
          if (!server.listening) {
            resolve();
            return;
          }
          server.close(() => {
            resolve();
          });
          server.closeAllConnections();
        }),
      ...(multiSession
        ? {
            issuePairingTicket: () => {
              if (tickets.size >= maximumSessions)
                return qerror("unavailable", "Wayfinder pairing capacity is full");
              const ticket = randomBytes(32).toString("base64url");
              const expiresAt = Date.now() + ticketTtlMs;
              tickets.set(hash(ticket).toString("hex"), expiresAt);
              return {
                ok: true as const,
                value: { ticket, expiresAt: new Date(expiresAt).toISOString() },
              };
            },
          }
        : {}),
    },
  };
}

async function operation(
  path: string,
  value: unknown,
  source: QuicklensDataSource,
  request: IncomingMessage,
): Promise<QuicklensResult<unknown>> {
  if (path === "/v1/read") {
    const empty = z.object({}).strict().safeParse(value);
    if (!empty.success) return qerror("invalid_data", "read input is invalid");
    const controller = new AbortController();
    request.once("aborted", () => {
      controller.abort();
    });
    return source.read({ signal: controller.signal });
  }
  if (path === "/v1/answer")
    return parsed(AnswerSchema, value, (input) => source.answerQuestion(input));
  if (path === "/v1/intent")
    return parsed(IntentSchema, value, (input) => source.proposePlanIntent(input));
  if (path === "/v1/preview")
    return parsed(PreviewSchema, value, (input) => source.previewPlan(input));
  if (path === "/v1/apply") return parsed(ApplySchema, value, (input) => source.applyPlan(input));
  if (path === "/v1/reconcile")
    return parsed(ReconcileSchema, value, (input) => source.reconcilePlan(input));
  if (path === "/v1/decide")
    return parsed(PlanDecisionInputSchema, value, (input) => source.decidePlan(input));
  return qerror("unsupported", "Quicklens route is unavailable");
}

async function workspaceOperation(
  path: string,
  value: unknown,
  factory: WorkspaceSourceFactory | undefined,
  identity: WorkspaceSessionIdentity | undefined,
  current: WorkspaceClientPort | undefined,
  request: IncomingMessage,
  setPort: (port: WorkspaceClientPort) => void,
): Promise<QuicklensResult<unknown> | WorkspaceResult<unknown>> {
  if (factory === undefined || identity === undefined) {
    return qerror("unsupported", "Zap Wayfinder workspace transport is not configured");
  }
  let port = current;
  if (port === undefined) {
    try {
      port = await factory(identity);
      setPort(port);
    } catch {
      return qerror("unavailable", "Zap Wayfinder workspace service is unavailable");
    }
  }
  const controller = new AbortController();
  request.once("aborted", () => {
    controller.abort();
  });
  if (path === "/v1/workspace/read") {
    const input = WorkspaceReadRequestSchema.safeParse(value);
    if (!input.success) return qerror("invalid_data", "Workspace read input is invalid");
    const result = await port.read(input.data);
    if (!result.ok) return validWorkspaceError(result);
    const response = WorkspaceReadResponseSchema.safeParse(result.value);
    return response.success ? { ok: true, value: response.data } : workspaceProtocolError();
  }
  if (path === "/v1/workspace/command") {
    const input = WorkspaceCommandRequestSchema.safeParse(value);
    if (!input.success) return qerror("invalid_data", "Workspace command input is invalid");
    const result = await port.command(input.data);
    if (!result.ok) return validWorkspaceError(result);
    const response = WorkspaceCommandResponseSchema.safeParse(result.value);
    return response.success ? { ok: true, value: response.data } : workspaceProtocolError();
  }
  if (path === "/v1/workspace/events") {
    const input = WorkspaceEventsRequestSchema.safeParse(value);
    if (!input.success) return qerror("invalid_data", "Workspace events input is invalid");
    const result = await port.events(input.data);
    if (!result.ok) return validWorkspaceError(result);
    const response = HistoryPageSchema.safeParse(result.value);
    return response.success ? { ok: true, value: response.data } : workspaceProtocolError();
  }
  return qerror("unsupported", "Workspace route is unavailable");
}

function validWorkspaceError(result: { readonly ok: false; readonly error: unknown }) {
  const parsed = WorkspaceErrorSchema.safeParse(result.error);
  if (parsed.success) return { ok: false as const, error: parsed.data };
  return workspaceProtocolError();
}

function workspaceProtocolError(): QuicklensResult<never> {
  return qerror("unavailable", "Zap Wayfinder returned an invalid workspace response");
}

async function parsed<T>(
  schema: z.ZodType<T>,
  value: unknown,
  call: (input: T) => Promise<QuicklensResult<unknown>>,
): Promise<QuicklensResult<unknown>> {
  const input = schema.safeParse(value);
  return input.success
    ? call(input.data)
    : qerror("invalid_data", "Quicklens operation input is invalid");
}

async function readBody(
  request: IncomingMessage,
  maximum: number,
): Promise<QuicklensResult<unknown>> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    if (!Buffer.isBuffer(chunk)) return qerror("invalid_data", "Quicklens request body is invalid");
    size += chunk.length;
    if (size > maximum) return qerror("invalid_data", "Quicklens request body exceeds its bound");
    chunks.push(chunk);
  }
  try {
    const value: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    return { ok: true, value };
  } catch {
    return qerror("invalid_data", "Quicklens request body is not JSON");
  }
}

function validateSource(
  request: IncomingMessage,
  options: QuicklensGatewayOptions,
): QuicklensResult<null> {
  const host = singleHeader(request, "host");
  const origin = singleHeader(request, "origin");
  return host !== undefined &&
    hostAllowed(host, options.allowedHosts) &&
    origin !== undefined &&
    options.allowedOrigins.includes(origin)
    ? { ok: true, value: null }
    : qerror("forbidden", "Quicklens Host or Origin is not allowlisted");
}

function hostAllowed(value: string, allowed: readonly string[]): boolean {
  if (allowed.includes(value)) return true;
  try {
    return allowed.includes(new URL(`http://${value}`).hostname);
  } catch {
    return false;
  }
}

function authenticatedCookie(
  request: IncomingMessage,
  cookieName: string,
  expected: Buffer,
): boolean {
  const cookie = singleHeader(request, "cookie")
    ?.split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${cookieName}=`))
    ?.slice(cookieName.length + 1);
  return cookie !== undefined && equalHash(expected, hash(cookie));
}

function sessionFor(
  request: IncomingMessage,
  cookieName: string,
  sessions: ReadonlyMap<string, GatewaySession>,
): GatewaySession | undefined {
  const cookie = singleHeader(request, "cookie")
    ?.split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${cookieName}=`))
    ?.slice(cookieName.length + 1);
  if (cookie === undefined) return undefined;
  for (const session of sessions.values()) {
    if (equalHash(session.hash, hash(cookie))) return session;
  }
  return undefined;
}

function invalidationPage(
  value: unknown,
  events: readonly { sequence: number; reason: string }[],
  head: number,
): QuicklensResult<unknown> {
  const input = InvalidationsSchema.safeParse(value);
  if (!input.success) return qerror("invalid_data", "invalidation cursor is invalid");
  if (input.data.after > head) {
    return { ok: true, value: { events: [{ sequence: head, reason: "reconnect" }], next: head } };
  }
  const selected = events.filter((event) => event.sequence > input.data.after).slice(0, 100);
  return {
    ok: true,
    value: {
      events: selected,
      next: selected.at(-1)?.sequence ?? Math.min(input.data.after, head),
    },
  };
}

function bearer(request: IncomingMessage): string | undefined {
  const value = singleHeader(request, "authorization");
  return value?.startsWith("Bearer ") ? value.slice(7) : undefined;
}

function singleHeader(request: IncomingMessage, name: string): string | undefined {
  const values: string[] = [];
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    if (request.rawHeaders[index]?.toLowerCase() === name) {
      const value = request.rawHeaders[index + 1];
      if (value !== undefined) values.push(value);
    }
  }
  return values.length === 1 ? values[0] : undefined;
}

function send(
  response: ServerResponse,
  status: number,
  value: unknown,
  request: IncomingMessage,
): void {
  securityHeaders(response, request);
  response.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}

function securityHeaders(response: ServerResponse, request: IncomingMessage): void {
  response.setHeader("Content-Security-Policy", CSP);
  response.setHeader("X-Frame-Options", "DENY");
  response.setHeader("X-Content-Type-Options", "nosniff");
  response.setHeader("Referrer-Policy", "no-referrer");
  const origin = singleHeader(request, "origin");
  if (origin !== undefined) {
    response.setHeader("Access-Control-Allow-Origin", origin);
    response.setHeader("Access-Control-Allow-Credentials", "true");
    response.setHeader("Vary", "Origin");
  }
}

function hash(value: string): Buffer {
  return createHash("sha256").update(value).digest();
}

function equalHash(left: Buffer, right: Buffer): boolean {
  return left.length === right.length && timingSafeEqual(left, right);
}

function loopback(host: string): boolean {
  return host === "127.0.0.1" || host === "localhost" || host === "::1";
}

function addressView(address: AddressInfo): { host: string; port: number } {
  return { host: address.address, port: address.port };
}

function qerror(code: QuicklensError["code"], message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code,
      message,
      recovery: "Pair the trusted local UI session and retry the named Quicklens operation.",
    },
  };
}
