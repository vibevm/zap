/**
 * Browser-only pairing and fixed-route client for the trusted Quicklens gateway.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shells
 */
import type { InvalidationReason } from "../../cells/quicklens-model/index.ts";
import type { QuicklensHostBridge } from "./host-bridge.ts";

type GatewayFetch = (input: string, init: RequestInit) => Promise<Response>;

interface BrowserLocation {
  readonly href: string;
}

interface BrowserHistory {
  replaceState(data: unknown, unused: string, url?: string | URL | null): void;
}

const routes = {
  read: "v1/read",
  answerQuestion: "v1/answer",
  proposePlanIntent: "v1/intent",
  previewPlan: "v1/preview",
  applyPlan: "v1/apply",
  reconcilePlan: "v1/reconcile",
  decidePlan: "v1/decide",
  invalidations: "v1/invalidations",
} as const;

export function createBrowserGatewayBridge(
  location: BrowserLocation,
  history: BrowserHistory,
  fetcher: GatewayFetch = globalThis.fetch,
): QuicklensHostBridge | null {
  const page = new URL(location.href);
  const gateway = parseGateway(page.searchParams.get("gateway"), page);
  if (gateway === null) return null;
  let pairingToken = new URLSearchParams(page.hash.slice(1)).get("pair") ?? undefined;
  let pairing: Promise<{ readonly paired: boolean; readonly value: unknown }> | undefined;
  const listeners = new Set<(reason: InvalidationReason) => void>();
  let invalidationCursor = 0;
  let pollGeneration = 0;

  const request = async (route: string, input: unknown): Promise<unknown> => {
    if (pairingToken !== undefined) {
      pairing ??= pair(fetcher, gateway, pairingToken).then((result) => {
        if (result.paired) {
          pairingToken = undefined;
          history.replaceState(null, "", `${page.pathname}${page.search}`);
        }
        return result;
      });
      const paired = await pairing;
      if (!paired.paired) return paired.value;
    }
    return (await call(fetcher, gateway, route, input)).value;
  };

  return {
    read: () => request(routes.read, {}),
    answerQuestion: (input) => request(routes.answerQuestion, input),
    proposePlanIntent: (input) => request(routes.proposePlanIntent, input),
    previewPlan: (input) => request(routes.previewPlan, input),
    applyPlan: (input) => request(routes.applyPlan, input),
    reconcilePlan: (input) => request(routes.reconcilePlan, input),
    decidePlan: (input) => request(routes.decidePlan, input),
    subscribe: (listener) => {
      listeners.add(listener);
      if (listeners.size === 1) void pollInvalidations(++pollGeneration);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) pollGeneration += 1;
      };
    },
  };

  async function pollInvalidations(generation: number): Promise<void> {
    const value = await request(routes.invalidations, { after: invalidationCursor });
    const page = invalidationPage(value);
    if (page !== null) {
      invalidationCursor = page.next;
      for (const reason of new Set(page.reasons)) {
        for (const listener of listeners) listener(reason);
      }
    }
    if (generation === pollGeneration && listeners.size > 0) {
      setTimeout(() => void pollInvalidations(generation), 500);
    }
  }
}

async function pair(
  fetcher: GatewayFetch,
  gateway: URL,
  token: string,
): Promise<{ readonly paired: boolean; readonly value: unknown }> {
  try {
    const response = await fetcher(new URL("v1/pair", gateway).href, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      credentials: "include",
      redirect: "error",
    });
    const value = await responseValue(response);
    return { paired: response.ok && isPairSuccess(value), value };
  } catch {
    return { paired: false, value: unavailable() };
  }
}

async function call(
  fetcher: GatewayFetch,
  gateway: URL,
  route: string,
  input: unknown,
): Promise<{ readonly status: number; readonly value: unknown }> {
  try {
    const response = await fetcher(new URL(route, gateway).href, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
      credentials: "include",
      redirect: "error",
    });
    return { status: response.status, value: await responseValue(response) };
  } catch {
    return { status: 0, value: unavailable() };
  }
}

async function responseValue(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return unavailable();
  }
}

function parseGateway(candidate: string | null, page: URL): URL | null {
  if (candidate === null) return null;
  try {
    const gateway = new URL(candidate);
    const loopback = ["127.0.0.1", "localhost"].includes(gateway.hostname);
    const scopedPath = /^\/quicklens\/[A-Za-z][A-Za-z0-9_-]{2,63}\/?$/.test(gateway.pathname);
    if (
      gateway.protocol === "http:" &&
      loopback &&
      gateway.hostname === page.hostname &&
      gateway.username === "" &&
      gateway.password === "" &&
      scopedPath &&
      gateway.search === "" &&
      gateway.hash === ""
    ) {
      if (!gateway.pathname.endsWith("/")) gateway.pathname += "/";
      return gateway;
    }
    return null;
  } catch {
    return null;
  }
}

function isPairSuccess(value: unknown): boolean {
  const result = field(value, "value");
  return field(value, "ok") === true && field(result, "paired") === true;
}

function invalidationPage(
  value: unknown,
): { readonly next: number; readonly reasons: readonly InvalidationReason[] } | null {
  const page = field(value, "value");
  const events = field(page, "events");
  const next = field(page, "next");
  if (
    field(value, "ok") !== true ||
    !Array.isArray(events) ||
    !Number.isSafeInteger(next) ||
    Number(next) < 0
  )
    return null;
  const reasons: InvalidationReason[] = [];
  for (const event of events) {
    const sequence = field(event, "sequence");
    const reason = field(event, "reason");
    if (!Number.isSafeInteger(sequence) || !isInvalidationReason(reason)) return null;
    reasons.push(reason);
  }
  return { next: Number(next), reasons };
}

function isInvalidationReason(value: unknown): value is InvalidationReason {
  return value === "events" || value === "questions" || value === "plan" || value === "reconnect";
}

function field(value: unknown, key: string): unknown {
  return typeof value === "object" && value !== null ? Reflect.get(value, key) : undefined;
}

function unavailable() {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message: "The trusted Quicklens gateway did not return a usable response.",
      recovery: "Start the paired loopback gateway and refresh this Quicklens session.",
    },
  };
}
