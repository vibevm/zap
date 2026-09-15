/** Browser-safe same-origin WorkspaceClientPort over the password web profile. @scope spec://org.vibevm.zap/lens/PROP-003#web-sessions */
import { z } from "zod";
import {
  HistoryPageSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceErrorSchema,
  WorkspaceReadResponseSchema,
  type HistoryEvent,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
  type WorkspaceError,
  type WorkspaceEventsRequest,
  type WorkspaceReadRequest,
  type WorkspaceResult,
  type WorkspaceSubscribeRequest,
} from "../workspace-model/index.ts";

type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;
export interface WorkspaceWebClientOptions {
  readonly baseUrl: string;
  readonly origin?: string;
  readonly fetcher?: Fetcher;
  readonly csrfToken?: string;
}
export interface WorkspaceWebClient extends WorkspaceClientPort {
  login(password: string): Promise<WorkspaceResult<{ readonly authenticated: true }>>;
  setCsrfToken(token: string): void;
}

export function createWorkspaceWebClient(
  options: WorkspaceWebClientOptions,
): WorkspaceWebClient | null {
  const base = parseBase(options.baseUrl);
  if (base === null) return null;
  const fetcher = options.fetcher ?? globalThis.fetch.bind(globalThis);
  let csrf = options.csrfToken;
  let unauthorized = false;
  const request = async <T>(
    route: string,
    body: unknown,
    schema: z.ZodType<T>,
  ): Promise<WorkspaceResult<T>> => {
    if (unauthorized) return unavailable("Workspace web session is unauthorized; polling stopped.");
    try {
      const headers = new Headers({
        "Content-Type": "application/json",
        Origin: options.origin ?? base.origin,
      });
      if (csrf !== undefined) headers.set("X-Quicklens-CSRF", csrf);
      const response = await fetcher(new URL(route, base.href).href, {
        method: "POST",
        headers,
        credentials: "include",
        redirect: "error",
        body: JSON.stringify(body),
      });
      const nextCsrf = response.headers.get("x-quicklens-csrf");
      if (nextCsrf !== null) csrf = nextCsrf;
      if (response.status === 401) {
        unauthorized = true;
        return unavailable("Workspace web session is unauthorized; polling stopped.");
      }
      const raw: unknown = await response.json();
      return decode(raw, schema);
    } catch {
      return unavailable("Workspace web request failed.");
    }
  };
  const client: WorkspaceWebClient = {
    login: async (password) => {
      const result = await request(
        "api/v1/login",
        { password },
        z.object({ authenticated: z.literal(true) }).strict(),
      );
      return result;
    },
    setCsrfToken: (token) => {
      csrf = token;
      unauthorized = false;
    },
    read: (input: WorkspaceReadRequest) =>
      request("api/v1/workspace/read", input, WorkspaceReadResponseSchema),
    command: (input: WorkspaceCommandRequest) =>
      request("api/v1/workspace/command", input, WorkspaceCommandResponseSchema),
    events: (input: WorkspaceEventsRequest) =>
      request("api/v1/workspace/events", input, HistoryPageSchema),
    subscribe: (input: WorkspaceSubscribeRequest) => subscribe(input, request, () => unauthorized),
  };
  return client;
}

async function* subscribe(
  input: WorkspaceSubscribeRequest,
  request: <T>(route: string, body: unknown, schema: z.ZodType<T>) => Promise<WorkspaceResult<T>>,
  isUnauthorized: () => boolean,
): AsyncIterable<WorkspaceResult<HistoryEvent>> {
  let cursor = input.cursor;
  while (input.signal?.aborted !== true && !isUnauthorized()) {
    const page = await request(
      "api/v1/workspace/events",
      { cursor, limit: 256 },
      HistoryPageSchema,
    );
    if (!page.ok) {
      yield page;
      return;
    }
    for (const event of page.value.events) yield { ok: true, value: event };
    cursor = page.value.next ?? page.value.resume;
    if (page.value.events.length === 0) await wait(500, input.signal);
  }
}

function decode<T>(raw: unknown, schema: z.ZodType<T>): WorkspaceResult<T> {
  const envelope = z.union([
    z.object({ ok: z.literal(true), value: schema }).strict(),
    z.object({ ok: z.literal(false), error: WorkspaceErrorSchema }).strict(),
  ]);
  const parsed = envelope.safeParse(raw);
  return parsed.success ? parsed.data : unavailable("Workspace web response was invalid.");
}
function unavailable(message: string): WorkspaceResult<never> {
  const error: WorkspaceError = {
    code: "unavailable",
    message: `violates REQ spec://org.vibevm.zap/lens/PROP-003#web-sessions: ${message}`,
  };
  return { ok: false, error };
}
function wait(milliseconds: number, signal: AbortSignal | undefined): Promise<void> {
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
function parseBase(raw: string): URL | null {
  try {
    const url = new URL(raw);
    if (url.protocol !== "https:" && url.protocol !== "http:") return null;
    if (url.username || url.password || url.search || url.hash) return null;
    if (!url.pathname.endsWith("/")) url.pathname += "/";
    return url;
  } catch {
    return null;
  }
}
