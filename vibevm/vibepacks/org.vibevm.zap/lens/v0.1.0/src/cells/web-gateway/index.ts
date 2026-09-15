/**
 * Authenticated same-origin web profile for Quicklens behind an explicit HTTPS tunnel.
 * @scope spec://org.vibevm.zap/lens/PROP-003#web-sessions
 */
import { randomBytes } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { resolve } from "node:path";
import { z } from "zod";
import {
  ExactDecimalSchema,
  PlanBasisSchema,
  PlanDecisionInputSchema,
  QuicklensRefSchema,
  type InvalidationReason,
  type QuicklensDataSource,
  type QuicklensError,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import type { PasswordAuthenticator } from "../web-auth/index.ts";
import { serveStaticAsset } from "./static.ts";
import { restrictSnapshot } from "./scope.ts";
import {
  cookieValue,
  digest,
  loopback,
  purge,
  safeEqual,
  secureHeaders,
  sendJson,
  singleHeader,
  trustedProxyRequest,
} from "./security.ts";
import {
  WorkspaceCommandRequestSchema,
  WorkspaceEventsRequestSchema,
  WorkspaceReadRequestSchema,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
export const WebOperationSchema = z.enum([
  "read",
  "answer",
  "intent",
  "preview",
  "apply",
  "reconcile",
  "decide",
  "invalidations",
]);
export type WebOperation = z.infer<typeof WebOperationSchema>;
export interface QuicklensWebGatewayOptions {
  readonly source: QuicklensDataSource;
  readonly authenticator: PasswordAuthenticator;
  readonly rendererRoot: string;
  readonly publicOrigin: string;
  readonly proxyProofToken: string;
  readonly role: "viewer" | "operator" | "owner";
  readonly allowedOperations: readonly WebOperation[];
  readonly sessionTtlMilliseconds?: number;
  readonly maximumSessions?: number;
  readonly maximumLoginAttempts?: number;
  readonly loginWindowMilliseconds?: number;
  readonly maximumBodyBytes?: number;
  readonly clock?: () => number;
  readonly workspaceClientFactory?: (input: {
    readonly sessionId: string;
    readonly role: "viewer" | "operator" | "owner";
    readonly operations: ReadonlySet<WebOperation>;
  }) => WorkspaceClientPort | null;
}
export interface QuicklensWebGateway {
  start(input: {
    readonly host: string;
    readonly port: number;
  }): Promise<QuicklensResult<AddressInfo>>;
  close(): Promise<void>;
}
interface SessionRecord {
  readonly csrfHash: string;
  readonly expiresAt: number;
  readonly authVersion: string;
  readonly role: "viewer" | "operator" | "owner";
  readonly operations: ReadonlySet<WebOperation>;
  readonly workspaceClient: WorkspaceClientPort | null;
}
const COOKIE = "__Host-qls";
const LoginSchema = z.object({ password: z.string().min(1).max(1_024) }).strict();
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
/** @implements spec://org.vibevm.zap/lens/PROP-003#channel-separation */
export function createQuicklensWebGateway(
  options: QuicklensWebGatewayOptions,
): QuicklensResult<QuicklensWebGateway> {
  const configured = configuration(options);
  if (!configured.ok) return configured;
  const config = configured.value;
  const sessions = new Map<string, SessionRecord>();
  const loginAttempts: number[] = [];
  const invalidations: Array<{ readonly sequence: number; readonly reason: InvalidationReason }> =
    [];
  let invalidationHead = 0;
  let boundPort: number | undefined;
  const unsubscribe = options.source.subscribe?.((reason) => {
    invalidationHead += 1;
    invalidations.push({ sequence: invalidationHead, reason });
    if (invalidations.length > 100) invalidations.shift();
  });
  const server = createServer((request, response) => {
    void dispatch(request, response).catch(() => {
      sendJson(response, 500, qerror("unavailable", "Quicklens web request failed."));
    });
  });
  const dispatch = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    secureHeaders(response);
    if (!trustedProxyRequest(request, config, boundPort)) {
      sendJson(response, 403, qerror("forbidden", "Trusted HTTPS tunnel proof was refused."));
      return;
    }
    const url = new URL(request.url ?? "/invalid", config.publicOrigin);
    if (url.pathname.startsWith("/api/")) {
      await api(request, response, url.pathname);
      return;
    }
    if (request.method !== "GET" && request.method !== "HEAD") {
      sendJson(response, 404, qerror("unsupported", "Quicklens web route is unavailable."));
      return;
    }
    await serveStaticAsset(request, response, url.pathname, config.rendererRoot);
  };
  const api = async (
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
  ): Promise<void> => {
    if (request.method !== "POST" || singleHeader(request, "origin") !== config.publicOrigin) {
      sendJson(response, 403, qerror("forbidden", "Quicklens web Origin was refused."));
      return;
    }
    if (!(await options.authenticator.refresh())) {
      sendJson(response, 503, qerror("unavailable", "Quicklens password verifier is unavailable."));
      return;
    }
    if (path === "/api/v1/login") {
      await login(request, response);
      return;
    }
    const authenticated = session(request, sessions, config);
    if (!authenticated.ok) {
      sendJson(response, 401, authenticated);
      return;
    }
    if (!csrfMatches(request, authenticated.value)) {
      sendJson(response, 403, qerror("forbidden", "Quicklens CSRF proof was refused."));
      return;
    }
    if (path === "/api/v1/logout") {
      sessions.delete(authenticated.value.idHash);
      response.setHeader(
        "Set-Cookie",
        `${COOKIE}=; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=0`,
      );
      response.setHeader("Clear-Site-Data", '"cache", "cookies", "storage"');
      sendJson(response, 200, { ok: true, value: { loggedOut: true } });
      return;
    }
    if (path.startsWith("/api/v1/workspace/")) {
      await workspaceApi(
        request,
        response,
        path.slice("/api/v1/workspace/".length),
        authenticated.value.record.workspaceClient,
      );
      return;
    }
    const operation = routeOperation(path);
    if (operation === null || !authenticated.value.record.operations.has(operation)) {
      sendJson(
        response,
        403,
        qerror("forbidden", "Quicklens web operation is outside this UI role."),
      );
      return;
    }
    const body = await readBody(request, config.maximumBodyBytes);
    if (!body.ok) {
      sendJson(response, 400, body);
      return;
    }
    const controller = new AbortController();
    request.once("aborted", () => {
      controller.abort();
    });
    response.once("close", () => {
      if (!response.writableEnded) controller.abort();
    });
    const result = await callOperation(
      operation,
      body.value,
      options.source,
      invalidations,
      invalidationHead,
      controller.signal,
      authenticated.value.record.operations,
    );
    sendJson(response, result.ok ? 200 : result.error.code === "forbidden" ? 403 : 409, result);
  };
  const workspaceApi = async (
    request: IncomingMessage,
    response: ServerResponse,
    operation: string,
    client: WorkspaceClientPort | null,
  ): Promise<void> => {
    if (client === null) {
      sendJson(
        response,
        403,
        qerror("forbidden", "Shared workspace access is not configured for this web session."),
      );
      return;
    }
    const body = await readBody(request, config.maximumBodyBytes);
    if (!body.ok) {
      sendJson(response, 400, body);
      return;
    }
    if (operation === "read") {
      const input = WorkspaceReadRequestSchema.safeParse(body.value);
      if (!input.success) {
        sendJson(response, 400, qerror("invalid_data", "Workspace read input is invalid."));
        return;
      }
      sendJson(response, 200, await client.read(input.data));
      return;
    }
    if (operation === "events") {
      const input = WorkspaceEventsRequestSchema.safeParse(body.value);
      if (!input.success) {
        sendJson(response, 400, qerror("invalid_data", "Workspace events input is invalid."));
        return;
      }
      sendJson(response, 200, await client.events(input.data));
      return;
    }
    if (operation === "command") {
      const input = WorkspaceCommandRequestSchema.safeParse(body.value);
      if (!input.success) {
        sendJson(response, 400, qerror("invalid_data", "Workspace command input is invalid."));
        return;
      }
      sendJson(response, 200, await client.command(input.data));
      return;
    }
    sendJson(response, 404, qerror("unsupported", "Workspace web route is unavailable."));
  };
  const login = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    const now = config.clock();
    purge(sessions, now, options.authenticator.version());
    while ((loginAttempts[0] ?? now) <= now - config.loginWindowMilliseconds) loginAttempts.shift();
    if (loginAttempts.length >= config.maximumLoginAttempts) {
      sendJson(response, 429, qerror("forbidden", "Quicklens login is temporarily throttled."));
      return;
    }
    loginAttempts.push(now);
    const body = await readBody(request, config.maximumBodyBytes);
    const credentials = body.ok ? LoginSchema.safeParse(body.value) : null;
    const verification =
      credentials?.success === true
        ? await options.authenticator.verify(credentials.data.password)
        : null;
    if (!(await options.authenticator.refresh())) {
      sendJson(response, 503, qerror("unavailable", "Quicklens password verifier is unavailable."));
      return;
    }
    const accepted =
      verification?.ok === true && verification.version === options.authenticator.version();
    if (!accepted) {
      sendJson(response, 401, qerror("forbidden", "Quicklens login was refused."));
      return;
    }
    const prior = cookieValue(request);
    if (prior !== undefined) sessions.delete(digest(prior));
    if (sessions.size >= config.maximumSessions) {
      sendJson(response, 503, qerror("unavailable", "Quicklens session capacity is exhausted."));
      return;
    }
    const id = randomBytes(32).toString("base64url");
    const csrf = randomBytes(32).toString("base64url");
    const expiresAt = now + config.sessionTtlMilliseconds;
    sessions.set(digest(id), {
      csrfHash: digest(csrf),
      expiresAt,
      authVersion: options.authenticator.version(),
      role: options.role,
      operations: new Set(options.allowedOperations),
      workspaceClient:
        options.workspaceClientFactory?.({
          sessionId: digest(id),
          role: options.role,
          operations: new Set(options.allowedOperations),
        }) ?? null,
    });
    response.setHeader(
      "Set-Cookie",
      `${COOKIE}=${id}; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=${String(Math.floor(config.sessionTtlMilliseconds / 1_000))}`,
    );
    response.setHeader("X-Quicklens-CSRF", csrf);
    sendJson(response, 200, {
      ok: true,
      value: {
        authenticated: true,
        role: options.role,
        expiresAt: new Date(expiresAt).toISOString(),
      },
    });
  };
  return {
    ok: true,
    value: {
      start: async (input) => {
        if (!loopback(input.host) || input.port < 0 || input.port > 65_535) {
          return qerror("invalid_data", "Quicklens web gateway must bind loopback.");
        }
        return new Promise((resolveStart) => {
          const failed = (): void => {
            resolveStart(qerror("unavailable", "Quicklens web bind failed."));
          };
          server.once("error", failed);
          server.listen(input.port, input.host, () => {
            server.off("error", failed);
            const address = server.address();
            if (address !== null && typeof address !== "string") boundPort = address.port;
            resolveStart(
              address !== null && typeof address !== "string"
                ? { ok: true, value: address }
                : qerror("unavailable", "Quicklens web address is unavailable."),
            );
          });
        });
      },
      close: () =>
        new Promise((resolveClose) => {
          unsubscribe?.();
          sessions.clear();
          if (!server.listening) {
            resolveClose();
            return;
          }
          server.close(() => {
            resolveClose();
          });
          server.closeAllConnections();
        }),
    },
  };
}
async function callOperation(
  operation: WebOperation,
  value: unknown,
  source: QuicklensDataSource,
  events: readonly { readonly sequence: number; readonly reason: InvalidationReason }[],
  head: number,
  signal: AbortSignal,
  operations: ReadonlySet<WebOperation>,
): Promise<QuicklensResult<unknown>> {
  if (operation === "read") {
    if (!z.object({}).strict().safeParse(value).success)
      return qerror("invalid_data", "Read input is invalid.");
    const result = await source.read({ signal });
    return result.ok ? { ok: true, value: restrictSnapshot(result.value, operations) } : result;
  }
  if (operation === "answer")
    return parsed(AnswerSchema, value, (input) => source.answerQuestion(input));
  if (operation === "intent")
    return parsed(IntentSchema, value, (input) => source.proposePlanIntent(input));
  if (operation === "preview")
    return parsed(PreviewSchema, value, (input) => source.previewPlan(input));
  if (operation === "apply") return parsed(ApplySchema, value, (input) => source.applyPlan(input));
  if (operation === "reconcile")
    return parsed(ReconcileSchema, value, (input) => source.reconcilePlan(input));
  if (operation === "decide")
    return parsed(PlanDecisionInputSchema, value, (input) => source.decidePlan(input));
  const input = InvalidationsSchema.safeParse(value);
  if (!input.success) return qerror("invalid_data", "Invalidation cursor is invalid.");
  const first = events[0]?.sequence;
  if (input.data.after > head || (first !== undefined && input.data.after < first - 1)) {
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
async function parsed<T>(
  schema: z.ZodType<T>,
  value: unknown,
  call: (input: T) => Promise<QuicklensResult<unknown>>,
) {
  const input = schema.safeParse(value);
  return input.success
    ? call(input.data)
    : qerror("invalid_data", "Quicklens web operation input is invalid.");
}
function routeOperation(path: string): WebOperation | null {
  const name = path.startsWith("/api/v1/") ? path.slice(8) : "";
  return WebOperationSchema.safeParse(name).success ? WebOperationSchema.parse(name) : null;
}
function session(
  request: IncomingMessage,
  sessions: Map<string, SessionRecord>,
  config: Config,
): QuicklensResult<{ readonly idHash: string; readonly record: SessionRecord }> {
  const id = cookieValue(request);
  if (id === undefined) return qerror("forbidden", "Quicklens web authentication is required.");
  const idHash = digest(id);
  const record = sessions.get(idHash);
  if (
    record === undefined ||
    record.expiresAt <= config.clock() ||
    record.authVersion !== config.authenticator.version()
  ) {
    sessions.delete(idHash);
    return qerror("forbidden", "Quicklens web session is invalid or expired.");
  }
  return { ok: true, value: { idHash, record } };
}
function csrfMatches(
  request: IncomingMessage,
  sessionValue: { readonly record: SessionRecord },
): boolean {
  const csrf = singleHeader(request, "x-quicklens-csrf");
  return csrf !== undefined && safeEqual(digest(csrf), sessionValue.record.csrfHash);
}
async function readBody(
  request: IncomingMessage,
  maximum: number,
): Promise<QuicklensResult<unknown>> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    if (!Buffer.isBuffer(chunk)) return qerror("invalid_data", "Quicklens web body is invalid.");
    size += chunk.length;
    if (size > maximum) return qerror("invalid_data", "Quicklens web body exceeds its bound.");
    chunks.push(chunk);
  }
  try {
    const value: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    return { ok: true, value };
  } catch {
    return qerror("invalid_data", "Quicklens web body is not JSON.");
  }
}
interface Config {
  readonly publicOrigin: string;
  readonly publicHost: string;
  readonly rendererRoot: string;
  readonly proxyProofToken: string;
  readonly sessionTtlMilliseconds: number;
  readonly maximumSessions: number;
  readonly maximumLoginAttempts: number;
  readonly loginWindowMilliseconds: number;
  readonly maximumBodyBytes: number;
  readonly clock: () => number;
  readonly authenticator: PasswordAuthenticator;
}
function configuration(options: QuicklensWebGatewayOptions): QuicklensResult<Config> {
  try {
    const origin = new URL(options.publicOrigin);
    const operations = z
      .array(WebOperationSchema)
      .min(1)
      .max(8)
      .safeParse(options.allowedOperations);
    const values = {
      sessionTtlMilliseconds: options.sessionTtlMilliseconds ?? 30 * 60_000,
      maximumSessions: options.maximumSessions ?? 32,
      maximumLoginAttempts: options.maximumLoginAttempts ?? 5,
      loginWindowMilliseconds: options.loginWindowMilliseconds ?? 60_000,
      maximumBodyBytes: options.maximumBodyBytes ?? 64 * 1024,
    };
    const bounded =
      origin.protocol === "https:" &&
      origin.origin === options.publicOrigin &&
      origin.pathname === "/" &&
      options.proxyProofToken.length >= 24 &&
      options.proxyProofToken.length <= 512 &&
      operations.success &&
      operations.data.every((operation) => roleOperations(options.role).has(operation)) &&
      values.sessionTtlMilliseconds >= 60_000 &&
      values.sessionTtlMilliseconds <= 24 * 60 * 60_000 &&
      values.maximumSessions >= 1 &&
      values.maximumSessions <= 1_000 &&
      values.maximumLoginAttempts >= 1 &&
      values.maximumLoginAttempts <= 100 &&
      values.loginWindowMilliseconds >= 1_000 &&
      values.loginWindowMilliseconds <= 60 * 60_000 &&
      values.maximumBodyBytes >= 1_024 &&
      values.maximumBodyBytes <= 1024 * 1024;
    return bounded
      ? {
          ok: true,
          value: {
            ...values,
            publicOrigin: origin.origin,
            publicHost: origin.host,
            rendererRoot: resolve(options.rendererRoot),
            proxyProofToken: options.proxyProofToken,
            clock: options.clock ?? Date.now,
            authenticator: options.authenticator,
          },
        }
      : qerror("invalid_data", "Quicklens web gateway configuration is invalid.");
  } catch {
    return qerror("invalid_data", "Quicklens public origin is invalid.");
  }
}
function roleOperations(role: "viewer" | "operator" | "owner"): ReadonlySet<WebOperation> {
  const view: WebOperation[] = ["read", "invalidations"];
  const operate: WebOperation[] = ["answer", "intent", "preview", "apply", "reconcile"];
  return new Set(
    role === "viewer"
      ? view
      : role === "operator"
        ? [...view, ...operate]
        : [...view, ...operate, "decide"],
  );
}
function qerror(code: QuicklensError["code"], message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code,
      message,
      recovery: "Authenticate through the configured HTTPS Quicklens web origin and retry.",
    },
  };
}
