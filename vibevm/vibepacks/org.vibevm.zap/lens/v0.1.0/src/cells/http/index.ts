/** Loopback HTTP command and SSE transport. @scope spec://org.vibevm.zap/lens/PROP-001#transport */
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { z } from "zod";
import { executeHostHook } from "./host-hook.ts";
import { resumeRetainedSessions } from "./resume.ts";
import {
  constantEqual,
  adapterSessionHeader,
  bearerCredential,
  executePrincipalCommand,
  isLoopback,
  parse,
  parsedCall,
  readJson,
  delay,
  statusOf,
  validateRequestSource,
} from "./wire.ts";
import {
  AckInputSchema,
  AnswerQuestionInputSchema,
  AskInputSchema,
  ConnectInputSchema,
  DelegateInputSchema,
  EmitInputSchema,
  ForwardInboxInputSchema,
  EventsInputSchema,
  InboxInputSchema,
  WaitInboxInputSchema,
  ClientRequestIdSchema,
  CredentialSchema,
  MessageIdSchema,
  HostBindingSchema,
  type BrokerError,
  type BindingAuth,
  type Credential,
  type PrincipalAuth,
  type Result,
} from "../protocol/index.ts";
import {
  AdapterSessionIdSchema,
  AdapterSessions,
  failure,
  type AdapterSessionId,
  type AdapterSessionVault,
  type TransportBrokerPort,
  createRetainedAgentTransport,
} from "../transport/index.ts";
import {
  AgentQuestionInputSchema,
  type AgentQuestionPublisher,
} from "../workspace-interaction/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { ManagedWorkAgentPort, NativeWorkAgentPort } from "../managed-work/index.ts";
import { executeAgentPlanning } from "./agent-planning.ts";
import { executeInboxWait } from "./inbox-wait.ts";
import { executeWorkCommand } from "./work-command.ts";
import {
  addressInfo,
  sendJson,
  sendResult,
  singleHeader,
  writeCors,
  writeSse,
} from "./server-response.ts";

const UnattestedHostSchema = HostBindingSchema.extend({
  provenance: z.enum(["explicit_handle", "unverified"]),
}).strict();
const ConnectBodySchema = ConnectInputSchema.omit({ principalToken: true })
  .extend({ host: UnattestedHostSchema })
  .strict();
const DelegateBodySchema = DelegateInputSchema.extend({
  host: UnattestedHostSchema,
}).strict();

export interface LensHttpGatewayOptions {
  readonly broker: TransportBrokerPort;
  readonly allowedHosts: readonly string[];
  readonly allowedOrigins: readonly string[];
  readonly statusToken: Credential;
  readonly adapterSessionIdFactory: () => string;
  readonly adapterSessionVault?: AdapterSessionVault;
  readonly maximumBodyBytes?: number;
  readonly streamPollMilliseconds?: number;
  readonly agentQuestions?: AgentQuestionPublisher;
  readonly planning?: WorkspacePlanningFeature;
  readonly managedWork?: () => ManagedWorkAgentPort | undefined;
  readonly nativeWork?: () => NativeWorkAgentPort | undefined;
}

export interface GatewayAddress {
  readonly host: string;
  readonly port: number;
}

export interface LensHttpGateway {
  start(address: GatewayAddress): Promise<Result<GatewayAddress>>;
  close(): Promise<Result<null>>;
}
export { createAgentHttpClient, createPrincipalHttpClient } from "./client.ts";
export { createManagedWorkHttpClient } from "./managed-work.ts";
export { createNativeWorkHttpClient } from "./native-work.ts";
export type { AgentHttpClientOptions } from "./client.ts";

/** Creates a credential-redacting, origin-checked HTTP façade. */
export function createLensHttpGateway(options: LensHttpGatewayOptions): LensHttpGateway {
  const sessions = new AdapterSessions(
    options.adapterSessionIdFactory,
    options.adapterSessionVault,
  );
  const maximumBodyBytes = options.maximumBodyBytes ?? 65_536;
  const pollMilliseconds = options.streamPollMilliseconds ?? 250;
  const streams = new Set<AbortController>();
  const server = createServer((request, response) => {
    void dispatch(
      request,
      response,
      options,
      sessions,
      maximumBodyBytes,
      pollMilliseconds,
      streams,
    ).catch(() => {
      if (!response.headersSent) {
        sendResult(
          response,
          failure("storage_failure", "HTTP request failed unexpectedly"),
          500,
          request,
        );
      } else {
        response.destroy();
      }
    });
  });

  return {
    start: async (address) => {
      const resumed = await resumeRetainedSessions(options.broker, sessions);
      if (!resumed.ok) return resumed;
      return new Promise((resolve) => {
        if (!isLoopback(address.host) || address.port < 0 || address.port > 65_535) {
          resolve(
            failure(
              "invalid_input",
              "HTTP gateway bind address is not bounded loopback",
              "bind the gateway to 127.0.0.1, localhost, or ::1",
            ),
          );
          return;
        }
        const onError = (): void => {
          resolve(
            failure(
              "storage_failure",
              "HTTP gateway failed to bind",
              "choose an available loopback port",
            ),
          );
        };
        server.once("error", onError);
        server.listen(address.port, address.host, () => {
          server.off("error", onError);
          const bound = server.address();
          if (bound === null || typeof bound === "string") {
            resolve(failure("storage_failure", "HTTP gateway did not expose a socket address"));
            return;
          }
          resolve({ ok: true, value: addressInfo(bound) });
        });
      });
    },
    close: () =>
      new Promise((resolve) => {
        for (const stream of streams) stream.abort();
        if (!server.listening) {
          resolve({ ok: true, value: null });
          return;
        }
        server.close((error) => {
          resolve(
            error === undefined
              ? { ok: true, value: null }
              : failure(
                  "storage_failure",
                  "HTTP gateway did not close cleanly",
                  "retry shutdown after active streams close",
                ),
          );
        });
        server.closeAllConnections();
      }),
  };
}

async function dispatch(
  request: IncomingMessage,
  response: ServerResponse,
  options: LensHttpGatewayOptions,
  sessions: AdapterSessions,
  maximumBodyBytes: number,
  pollMilliseconds: number,
  streams: Set<AbortController>,
): Promise<void> {
  const checked = validateRequestSource(request, options);
  if (!checked.ok) {
    sendResult(response, checked, 403, request);
    return;
  }
  if (request.method === "OPTIONS") {
    writeCors(response, request);
    response.writeHead(204, {
      "Access-Control-Allow-Headers": "Authorization, Content-Type, X-Codlens-Adapter-Session",
      "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
      "Cache-Control": "no-store",
    });
    response.end();
    return;
  }
  const url = request.url === undefined ? undefined : new URL(request.url, "http://local");
  if (url === undefined) {
    sendResult(response, failure("invalid_input", "request URL is missing"), 400, request);
    return;
  }
  if (request.method === "GET" && url.pathname === "/v1/status") {
    const token = bearerCredential(request);
    const authenticated = token.ok && constantEqual(token.value, options.statusToken);
    if (!authenticated) {
      sendResult(response, failure("unauthorized", "status token is invalid"), 401, request);
      return;
    }
    sendJson(
      response,
      200,
      {
        protocol: "lens/1",
        mcpProtocol: "2025-11-25",
        commands: [
          "connect",
          "context",
          "emit",
          "ask",
          "ask-user-question",
          "inbox",
          "inbox-wait",
          "ack",
          "delegate",
          "finish",
          "forward",
          "answer",
          "actors",
          "questions",
          "notice",
          "events",
          "managed-work",
        ],
        unsupported: ["plan.execute", "generic_mcp.unsolicited_model_wake", "mcp.2026-07-28"],
      },
      request,
    );
    return;
  }
  if (request.method === "GET" && url.pathname === "/v1/events") {
    await streamEvents(request, response, url, options, pollMilliseconds, streams);
    return;
  }
  if (request.method !== "POST") {
    sendResult(response, failure("not_found", "HTTP route is unavailable"), 404, request);
    return;
  }
  const body = await readJson(request, maximumBodyBytes);
  if (!body.ok) {
    sendResult(response, body, body.error.code === "backpressure" ? 413 : 400, request);
    return;
  }

  if (url.pathname === "/v1/connect") {
    const principal = bearerCredential(request);
    const parsed = parse(ConnectBodySchema, body.value);
    if (!principal.ok || !parsed.ok) {
      sendResult(response, principal.ok ? parsed : principal, 401, request);
      return;
    }
    const connected = await options.broker.connect({
      ...parsed.value,
      principalToken: principal.value,
    });
    const retained = connected.ok
      ? sessions.retain(
          principal.value,
          connected.value,
          parsed.value.host,
          parsed.value.replyPolicy,
        )
      : connected;
    sendResult(response, retained, statusOf(retained), request);
    return;
  }

  if (url.pathname.startsWith("/v1/host-hook/")) {
    await handleHostHook(request, response, url.pathname.slice(14), body.value, options, sessions);
    return;
  }

  if (url.pathname === "/v1/answer") {
    const principal = bearerCredential(request);
    const parsed = parse(AnswerQuestionInputSchema, body.value);
    if (!principal.ok || !parsed.ok) {
      sendResult(response, principal.ok ? parsed : principal, 401, request);
      return;
    }
    const answered = await options.broker.answer({ principalToken: principal.value }, parsed.value);
    sendResult(response, answered, statusOf(answered), request);
    return;
  }

  if (["/v1/actors", "/v1/questions", "/v1/notice", "/v1/event-page"].includes(url.pathname)) {
    const principal = bearerCredential(request);
    if (!principal.ok) {
      sendResult(response, principal, 401, request);
      return;
    }
    const result = await executePrincipalCommand(
      url.pathname,
      body.value,
      options.broker,
      principal.value,
    );
    sendResult(response, result, statusOf(result), request);
    return;
  }

  const principal = bearerCredential(request);
  const adapterSession = adapterSessionHeader(request);
  if (!principal.ok || !adapterSession.ok) {
    sendResult(response, principal.ok ? adapterSession : principal, 401, request);
    return;
  }
  const auth = sessions.resolve(adapterSession.value);
  if (!auth.ok) {
    sendResult(response, auth, 401, request);
    return;
  }
  if (!constantEqual(auth.value.principalToken, principal.value)) {
    sendResult(
      response,
      failure("unauthorized", "adapter session does not belong to this principal"),
      401,
      request,
    );
    return;
  }
  if (url.pathname === "/v1/inbox-wait") {
    const input = parse(WaitInboxInputSchema, body.value);
    if (!input.ok) {
      sendResult(response, input, 400, request);
      return;
    }
    const controller = new AbortController();
    response.once("close", () => {
      controller.abort();
    });
    const result = await executeInboxWait(
      options.broker,
      auth.value,
      input.value,
      controller.signal,
    );
    sendResult(response, result, statusOf(result), request);
    return;
  }
  const result = await executeActorCommand(
    url.pathname,
    body.value,
    options,
    sessions,
    adapterSession.value,
    auth.value,
  );
  sendResult(response, result, statusOf(result), request);
}

async function handleHostHook(
  request: IncomingMessage,
  response: ServerResponse,
  host: string,
  input: unknown,
  options: LensHttpGatewayOptions,
  sessions: AdapterSessions,
): Promise<void> {
  const principal = bearerCredential(request);
  if (!principal.ok) {
    sendResult(response, principal, 401, request);
    return;
  }
  const sessionHeader = singleHeader(request, "x-codlens-adapter-session");
  const sessionId =
    sessionHeader === undefined ? undefined : AdapterSessionIdSchema.safeParse(sessionHeader);
  if (sessionId !== undefined && !sessionId.success) {
    sendResult(
      response,
      failure("unauthorized", "adapter session header is malformed"),
      401,
      request,
    );
    return;
  }
  const offered = await executeHostHook({
    broker: options.broker,
    sessions,
    principalToken: principal.value,
    host,
    input,
    ...(sessionId?.success ? { adapterSessionId: sessionId.data } : {}),
  });
  if (!offered.ok) {
    sendResult(response, offered, statusOf(offered), request);
    return;
  }
  if (offered.value === null) {
    writeCors(response, request);
    response.writeHead(204, { "Cache-Control": "no-store" });
    response.end();
    return;
  }
  sendJson(response, 200, offered.value.hostOutput, request);
}

async function executeActorCommand(
  path: string,
  body: unknown,
  options: LensHttpGatewayOptions,
  sessions: AdapterSessions,
  adapterSessionId: AdapterSessionId,
  auth: BindingAuth,
): Promise<Result<unknown>> {
  const work = await executeWorkCommand(path, body, adapterSessionId, options);
  if (work !== null) return work;
  if (path.startsWith("/v1/agent-plan/")) {
    if (options.planning === undefined)
      return failure("unsupported_operation", "Wayfinder planning is not configured");
    const actor = await options.broker.context(auth);
    if (!actor.ok) return actor;
    const port = options.planning.agent(
      actor.value,
      createRetainedAgentTransport({
        broker: options.broker,
        principalToken: auth.principalToken,
        sessions,
      }),
    );
    return port.ok
      ? executeAgentPlanning(
          port.value,
          path.slice("/v1/agent-plan/".length),
          actor.value,
          adapterSessionId,
          body,
        )
      : failure(
          port.error.code === "unavailable" ? "unsupported_operation" : port.error.code,
          port.error.message,
        );
  }
  if (path === "/v1/ask-user-question") {
    if (options.agentQuestions === undefined)
      return failure("unsupported_operation", "Wayfinder rich questions are not configured");
    const input = parse(AgentQuestionInputSchema, body);
    if (!input.ok) return input;
    const actor = await options.broker.context(auth);
    if (!actor.ok) return actor;
    if (!actor.value.actor.capabilities.includes("question:ask"))
      return failure("forbidden", "current actor lacks question authority");
    return options.agentQuestions.publish(actor.value, input.value);
  }
  if (path === "/v1/emit")
    return parsedCall(EmitInputSchema, body, (input) => options.broker.emit(auth, input));
  if (path === "/v1/ask")
    return parsedCall(AskInputSchema, body, (input) => options.broker.ask(auth, input));
  if (path === "/v1/inbox")
    return parsedCall(InboxInputSchema, body, (input) => options.broker.inbox(auth, input));
  if (path === "/v1/plan-intent") {
    const input = parse(z.object({ messageId: MessageIdSchema }).strict(), body);
    return input.ok ? options.broker.planIntent(auth, input.value.messageId) : input;
  }
  if (path === "/v1/ack")
    return parsedCall(AckInputSchema, body, (input) => options.broker.ack(auth, input));
  if (path === "/v1/context") return options.broker.context(auth);
  if (path === "/v1/finish") {
    const input = parse(z.object({ clientRequestId: ClientRequestIdSchema }).strict(), body);
    if (!input.ok) return input;
    const actor = sessions.actor(adapterSessionId);
    return actor.ok
      ? options.broker.expireActor(auth, {
          clientRequestId: input.value.clientRequestId,
          actorId: actor.value.actorId,
        })
      : actor;
  }
  if (path === "/v1/forward") {
    const input = parse(ForwardInboxInputSchema, body);
    if (!input.ok) return input;
    const allowed = sessions.forwardAllowed(
      auth.principalToken,
      input.value.fromActorId,
      input.value.toActorId,
    );
    return allowed.ok ? options.broker.forwardInbox(auth, input.value) : allowed;
  }
  if (path === "/v1/delegate") {
    const input = parse(DelegateBodySchema, body);
    if (!input.ok) return input;
    const delegated = await options.broker.delegate(auth, input.value);
    if (!delegated.ok) return delegated;
    const principal = CredentialSchema.safeParse(auth.principalToken);
    return principal.success
      ? sessions.retain(principal.data, delegated.value, input.value.host, input.value.replyPolicy)
      : failure("unauthorized", "retained principal credential is invalid");
  }
  return failure("not_found", "HTTP actor command route is unavailable");
}

async function streamEvents(
  request: IncomingMessage,
  response: ServerResponse,
  url: URL,
  options: LensHttpGatewayOptions,
  pollMilliseconds: number,
  streams: Set<AbortController>,
): Promise<void> {
  const principal = bearerCredential(request);
  const parsed = parse(EventsInputSchema, {
    workspaceId: url.searchParams.get("workspaceId"),
    conversationId: url.searchParams.get("conversationId"),
    afterSequence: url.searchParams.get("afterSequence") ?? "0",
    limit: Number(url.searchParams.get("limit") ?? "50"),
  });
  if (!principal.ok || !parsed.ok) {
    sendResult(response, principal.ok ? parsed : principal, 401, request);
    return;
  }
  const controller = new AbortController();
  streams.add(controller);
  request.once("close", () => {
    controller.abort();
  });
  let cursor = parsed.value.afterSequence;
  let page = await options.broker.events(
    { principalToken: principal.value },
    { ...parsed.value, afterSequence: cursor },
  );
  if (!page.ok) {
    streams.delete(controller);
    sendResult(response, page, statusOf(page), request);
    return;
  }
  writeCors(response, request);
  response.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-store",
    Connection: "keep-alive",
  });
  response.flushHeaders();
  while (!controller.signal.aborted && !response.destroyed) {
    for (const event of page.value.events) {
      const wrote = await writeSse(
        response,
        `id: ${event.sequence}\nevent: lens\ndata: ${JSON.stringify(event)}\n\n`,
        controller.signal,
      );
      if (!wrote) break;
    }
    cursor = page.value.observationCursor;
    if (!page.value.hasMore) await delay(pollMilliseconds, controller.signal);
    page = await options.broker.events(
      { principalToken: principal.value },
      { ...parsed.value, afterSequence: cursor },
    );
    if (!page.ok) {
      await writeSse(
        response,
        `event: error\ndata: ${JSON.stringify(page.error)}\n\n`,
        controller.signal,
      );
      break;
    }
  }
  streams.delete(controller);
  response.end();
}

export type { BrokerError, PrincipalAuth };
