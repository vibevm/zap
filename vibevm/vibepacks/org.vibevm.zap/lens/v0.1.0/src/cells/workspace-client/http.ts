/** @scope spec://org.vibevm.zap/lens/PROP-005#network-boundary */
/** Authenticated loopback HTTP transport for WorkspaceClientPort. */
import { z } from "zod";
import {
  HistoryPageSchema,
  ProductSetupResponseSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceErrorSchema,
  WorkspaceReadResponseSchema,
  type HistoryCursor,
  type HistoryEvent,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceError,
  type WorkspaceEventsRequest,
  type HistoryPage,
  type WorkspaceReadRequest,
  type WorkspaceReadResponse,
  type WorkspaceResult,
  type ProductSetupPort,
  type ProductSetupResponse,
  type ProductSetupResult,
} from "../workspace-model/index.ts";

type Fetcher = (input: string, init: RequestInit) => Promise<Response>;

export interface WorkspaceHttpClientOptions {
  readonly baseUrl: string;
  readonly origin: string;
  readonly pairingToken?: string;
  readonly fetcher?: Fetcher;
  readonly pollIntervalMs?: number;
}

export interface WorkspaceHttpConnection {
  readonly workspace: WorkspaceClientPort;
  readonly product: ProductSetupPort;
}

export function createWorkspaceHttpClient(
  options: WorkspaceHttpClientOptions,
): WorkspaceClientPort | null {
  return createWorkspaceHttpConnection(options)?.workspace ?? null;
}

export function createWorkspaceHttpConnection(
  options: WorkspaceHttpClientOptions,
): WorkspaceHttpConnection | null {
  const gateway = parseWorkspaceGateway(options.baseUrl);
  if (gateway === null || options.origin.length < 1) return null;
  const endpoint = gateway;
  const fetcher = options.fetcher ?? globalThis.fetch.bind(globalThis);
  let pairingToken = options.pairingToken;
  let sessionCookie: string | undefined;
  let pairing: Promise<boolean> | undefined;
  const pollInterval = options.pollIntervalMs ?? 500;

  const request = async <T>(
    route: string,
    input: unknown,
    schema: z.ZodType<T>,
  ): Promise<WorkspaceResult<T>> => {
    try {
      if (pairingToken !== undefined) {
        pairing ??= pair(pairingToken);
        if (!(await pairing)) return unavailable("Workspace gateway pairing failed.");
      }
      const headers = new Headers({ Origin: options.origin, "Content-Type": "application/json" });
      if (sessionCookie !== undefined) headers.set("Cookie", sessionCookie);
      const response = await fetcher(new URL(route, endpoint).href, {
        method: "POST",
        headers,
        body: JSON.stringify(input),
        credentials: "include",
        redirect: "error",
      });
      const raw: unknown = await response.json();
      return decode(raw, schema);
    } catch {
      return unavailable("Workspace gateway request failed.");
    }
  };

  async function pair(token: string): Promise<boolean> {
    try {
      const headers = new Headers({ Authorization: `Bearer ${token}`, Origin: options.origin });
      const response = await fetcher(new URL("v1/pair", endpoint).href, {
        method: "POST",
        headers,
        credentials: "include",
        redirect: "error",
      });
      const raw: unknown = await response.json();
      const paired = z
        .object({ ok: z.literal(true), value: z.object({ paired: z.literal(true) }).strict() })
        .strict()
        .safeParse(raw);
      if (!response.ok || !paired.success) return false;
      const cookie = response.headers.get("set-cookie")?.split(";", 1)[0];
      if (cookie !== undefined) sessionCookie = cookie;
      pairingToken = undefined;
      return true;
    } catch {
      return false;
    }
  }

  const workspace: WorkspaceClientPort = {
    read: (input: WorkspaceReadRequest): Promise<WorkspaceResult<WorkspaceReadResponse>> =>
      request("v1/workspace/read", input, WorkspaceReadResponseSchema),
    command: (input: WorkspaceCommandRequest): Promise<WorkspaceResult<WorkspaceCommandResponse>> =>
      request("v1/workspace/command", input, WorkspaceCommandResponseSchema),
    events: (input: WorkspaceEventsRequest): Promise<WorkspaceResult<HistoryPage>> =>
      request("v1/workspace/events", input, HistoryPageSchema),
    subscribe: (input) => subscribe(input, request, pollInterval),
  };
  const productRequest: ProductSetupPort["request"] = async (input) => {
    try {
      if (pairingToken !== undefined) {
        pairing ??= pair(pairingToken);
        if (!(await pairing))
          return productFailure("unavailable", "Product gateway pairing failed.");
      }
      const headers = new Headers({ Origin: options.origin, "Content-Type": "application/json" });
      if (sessionCookie !== undefined) headers.set("Cookie", sessionCookie);
      const response = await fetcher(new URL("v1/product/request", endpoint).href, {
        method: "POST",
        headers,
        body: JSON.stringify(input),
        credentials: "include",
        redirect: "error",
      });
      const raw: unknown = await response.json();
      const parsed = z
        .union([
          z.object({ ok: z.literal(true), value: ProductSetupResponseSchema }).strict(),
          z
            .object({
              ok: z.literal(false),
              error: z
                .object({
                  code: z.enum(["invalid_input", "not_found", "conflict", "unavailable"]),
                  message: z.string().min(1),
                })
                .strict(),
            })
            .strict(),
        ])
        .safeParse(raw);
      return parsed.success
        ? parsed.data
        : productFailure("unavailable", "Product gateway returned invalid data.");
    } catch {
      return productFailure("unavailable", "Product gateway request failed.");
    }
  };
  const product: ProductSetupPort = {
    request: productRequest,
  };
  return { workspace, product };
}

function productFailure(code: string, message: string): ProductSetupResult<ProductSetupResponse> {
  if (code === "invalid_input" || code === "not_found" || code === "conflict")
    return { ok: false, error: { code, message } };
  return { ok: false, error: { code: "unavailable", message } };
}

async function* subscribe(
  input: Parameters<WorkspaceClientPort["subscribe"]>[0],
  request: <T>(route: string, body: unknown, schema: z.ZodType<T>) => Promise<WorkspaceResult<T>>,
  pollInterval: number,
): AsyncIterable<WorkspaceResult<HistoryEvent>> {
  let cursor: HistoryCursor = input.cursor;
  while (input.signal?.aborted !== true) {
    const page = await request("v1/workspace/events", { cursor, limit: 256 }, HistoryPageSchema);
    if (!page.ok) {
      yield page;
      return;
    }
    for (const event of page.value.events) yield { ok: true, value: event };
    cursor = page.value.next ?? page.value.resume;
    if (page.value.events.length === 0) await wait(pollInterval, input.signal);
  }
}

function decode<T>(value: unknown, schema: z.ZodType<T>): WorkspaceResult<T> {
  const envelope = z.union([
    z.object({ ok: z.literal(true), value: schema }).strict(),
    z.object({ ok: z.literal(false), error: WorkspaceErrorSchema }).strict(),
  ]);
  const parsed = envelope.safeParse(value);
  return parsed.success ? parsed.data : unavailable("Workspace gateway returned invalid data.");
}

function unavailable(message: string): WorkspaceResult<never> {
  const error: WorkspaceError = {
    code: "unavailable",
    message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#transport: ${message}`,
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

function parseWorkspaceGateway(candidate: string): URL | null {
  try {
    const gateway = new URL(candidate);
    const scopedPath = /^\/quicklens\/[A-Za-z][A-Za-z0-9_-]{2,63}\/?$/.test(gateway.pathname);
    if (
      gateway.protocol !== "http:" ||
      !["127.0.0.1", "localhost", "[::1]"].includes(gateway.hostname) ||
      !scopedPath ||
      gateway.username !== "" ||
      gateway.password !== "" ||
      gateway.search !== "" ||
      gateway.hash !== ""
    )
      return null;
    if (!gateway.pathname.endsWith("/")) gateway.pathname += "/";
    return gateway;
  } catch {
    return null;
  }
}

export { parseWorkspaceGateway };
