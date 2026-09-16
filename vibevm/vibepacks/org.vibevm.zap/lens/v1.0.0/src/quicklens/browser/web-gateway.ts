/**
 * Same-origin browser client for the authenticated HTTPS Quicklens profile.
 * @scope spec://org.vibevm.zap/lens/PROP-003#web-sessions
 */
import type { InvalidationReason } from "../../cells/quicklens-model/index.ts";
import type { QuicklensHostBridge } from "./host-bridge.ts";
import {
  createWorkspaceWebClient,
  type WorkspaceWebClient,
} from "../../cells/workspace-client/web.ts";

type WebFetch = (input: string, init?: RequestInit) => Promise<Response>;

export interface QuicklensWebClient extends QuicklensHostBridge {
  readonly workspace: WorkspaceWebClient | null;
  login(password: string): Promise<{ readonly ok: boolean; readonly message: string }>;
  logout(): Promise<void>;
  subscribe(listener: (reason: InvalidationReason) => void): () => void;
  onAuthenticationLost(listener: () => void): () => void;
}

const routes = {
  read: "/api/v1/read",
  answerQuestion: "/api/v1/answer",
  proposePlanIntent: "/api/v1/intent",
  previewPlan: "/api/v1/preview",
  applyPlan: "/api/v1/apply",
  reconcilePlan: "/api/v1/reconcile",
  decidePlan: "/api/v1/decide",
  invalidations: "/api/v1/invalidations",
} as const;

export function createQuicklensWebClient(fetcher: WebFetch = globalThis.fetch): QuicklensWebClient {
  const webOrigin = typeof window === "undefined" ? "http://localhost" : window.location.origin;
  const workspace = createWorkspaceWebClient({
    baseUrl: webOrigin,
    origin: webOrigin,
    fetcher,
  });
  let csrf: string | undefined;
  let cursor = 0;
  let generation = 0;
  const listeners = new Set<(reason: InvalidationReason) => void>();
  const sessionListeners = new Set<() => void>();

  const loseAuthentication = (): void => {
    csrf = undefined;
    workspace?.setCsrfToken("");
    generation += 1;
    cursor = 0;
    for (const listener of sessionListeners) listener();
  };

  const request = async (path: string, input: unknown): Promise<unknown> => {
    if (csrf === undefined) return authenticationRequired();
    try {
      const response = await timedFetch(fetcher, path, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-Quicklens-CSRF": csrf },
        credentials: "include",
        redirect: "error",
        body: JSON.stringify(input),
      });
      const value: unknown = await response.json();
      if (response.status === 401) loseAuthentication();
      return value;
    } catch {
      return unavailable();
    }
  };

  return {
    workspace,
    login: async (password) => {
      try {
        const response = await timedFetch(fetcher, "/api/v1/login", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          credentials: "include",
          redirect: "error",
          body: JSON.stringify({ password }),
        });
        const token = response.headers.get("x-quicklens-csrf");
        const value: unknown = await response.json();
        if (response.ok && token !== null && authenticated(value)) {
          csrf = token;
          workspace?.setCsrfToken(token);
          if (listeners.size > 0) void poll(++generation);
          return { ok: true, message: "Authenticated." };
        }
        return { ok: false, message: "Login was refused." };
      } catch {
        return { ok: false, message: "The HTTPS Quicklens gateway is unavailable." };
      }
    },
    logout: async () => {
      if (csrf !== undefined) await request("/api/v1/logout", {});
      loseAuthentication();
    },
    read: () => request(routes.read, {}),
    answerQuestion: (input) => request(routes.answerQuestion, input),
    proposePlanIntent: (input) => request(routes.proposePlanIntent, input),
    previewPlan: (input) => request(routes.previewPlan, input),
    applyPlan: (input) => request(routes.applyPlan, input),
    reconcilePlan: (input) => request(routes.reconcilePlan, input),
    decidePlan: (input) => request(routes.decidePlan, input),
    subscribe: (listener) => {
      listeners.add(listener);
      if (listeners.size === 1 && csrf !== undefined) void poll(++generation);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) generation += 1;
      };
    },
    onAuthenticationLost: (listener) => {
      sessionListeners.add(listener);
      return () => sessionListeners.delete(listener);
    },
  };

  async function poll(activeGeneration: number): Promise<void> {
    const page = invalidationPage(await request(routes.invalidations, { after: cursor }));
    if (page !== null) {
      cursor = page.next;
      for (const reason of new Set(page.reasons)) {
        for (const listener of listeners) listener(reason);
      }
    }
    if (csrf !== undefined && activeGeneration === generation) {
      setTimeout(() => void poll(activeGeneration), 500);
    }
  }
}

function timedFetch(fetcher: WebFetch, path: string, init: RequestInit): Promise<Response> {
  return fetcher(path, { ...init, signal: AbortSignal.timeout(15_000) });
}

function authenticated(value: unknown): boolean {
  return field(value, "ok") === true && field(field(value, "value"), "authenticated") === true;
}

function invalidationPage(value: unknown) {
  const page = field(value, "value");
  const events = field(page, "events");
  const next = field(page, "next");
  if (field(value, "ok") !== true || !Array.isArray(events) || !Number.isSafeInteger(next))
    return null;
  const reasons: InvalidationReason[] = [];
  for (const event of events) {
    const reason = field(event, "reason");
    if (
      reason !== "events" &&
      reason !== "questions" &&
      reason !== "plan" &&
      reason !== "reconnect"
    )
      return null;
    reasons.push(reason);
  }
  return { next: Number(next), reasons };
}

function field(value: unknown, key: string): unknown {
  return typeof value === "object" && value !== null ? Reflect.get(value, key) : undefined;
}

function authenticationRequired() {
  return {
    ok: false,
    error: {
      code: "forbidden",
      message: "Quicklens web authentication is required.",
      recovery: "Enter the configured web password over the HTTPS Quicklens origin.",
    },
  };
}

function unavailable() {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message: "The HTTPS Quicklens gateway did not return a usable response.",
      recovery: "Check the configured tunnel and retry without changing local agent services.",
    },
  };
}
