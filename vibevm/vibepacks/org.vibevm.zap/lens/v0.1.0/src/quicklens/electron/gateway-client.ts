/**
 * Trusted Electron-main client for the cookie-authenticated Quicklens gateway.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shells
 */
type GatewayFetch = (input: string, init: RequestInit) => Promise<Response>;
type InvalidationReason = "events" | "questions" | "plan" | "reconnect";

export interface ElectronGatewayClient {
  read(): Promise<unknown>;
  answerQuestion(input: unknown): Promise<unknown>;
  proposePlanIntent(input: unknown): Promise<unknown>;
  previewPlan(input: unknown): Promise<unknown>;
  applyPlan(input: unknown): Promise<unknown>;
  reconcilePlan(input: unknown): Promise<unknown>;
  decidePlan(input: unknown): Promise<unknown>;
  subscribe(listener: (reason: InvalidationReason) => void): () => void;
}

export interface ElectronGatewayOptions {
  readonly baseUrl: string;
  readonly pairingToken: string;
  readonly origin: string;
  readonly fetcher: GatewayFetch;
}

export interface ElectronWorkspaceGatewayClient {
  read(input: unknown): Promise<unknown>;
  command(input: unknown): Promise<unknown>;
  events(input: unknown): Promise<unknown>;
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

export function createElectronGatewayClient(
  options: ElectronGatewayOptions,
): ElectronGatewayClient | null {
  const gateway = parseGateway(options.baseUrl);
  if (gateway === null || options.pairingToken.length < 24 || options.pairingToken.length > 512)
    return null;
  let pairingToken: string | undefined = options.pairingToken;
  let sessionCookie: string | undefined;
  let pairing: Promise<{ readonly paired: boolean; readonly value: unknown }> | undefined;
  const listeners = new Set<(reason: InvalidationReason) => void>();
  let invalidationCursor = 0;
  let pollGeneration = 0;

  const fetchGateway = async (
    route: string,
    input: unknown,
    authorization?: string,
  ): Promise<{ readonly response: Response; readonly value: unknown }> => {
    const headers = new Headers({ "Content-Type": "application/json", Origin: options.origin });
    if (authorization !== undefined) headers.set("Authorization", authorization);
    if (sessionCookie !== undefined) headers.set("Cookie", sessionCookie);
    const request: RequestInit = {
      method: "POST",
      headers,
      redirect: "error",
    };
    if (route !== "v1/pair") request.body = JSON.stringify(input);
    const response = await options.fetcher(new URL(route, gateway).href, request);
    return { response, value: await responseValue(response) };
  };

  const pair = async (token: string) => {
    try {
      const result = await fetchGateway("v1/pair", {}, `Bearer ${token}`);
      const cookie = result.response.headers.get("set-cookie")?.split(";", 1)[0];
      const validCookie = cookie?.match(
        /^quicklens_session_[A-Za-z][A-Za-z0-9_-]{2,63}=[A-Za-z0-9_-]{20,}$/,
      )?.[0];
      if (result.response.ok && isPairSuccess(result.value) && validCookie !== undefined) {
        sessionCookie = validCookie;
        pairingToken = undefined;
        return { paired: true, value: result.value } as const;
      }
      return { paired: false, value: result.value } as const;
    } catch {
      return { paired: false, value: unavailable() } as const;
    }
  };

  const request = async (route: string, input: unknown): Promise<unknown> => {
    try {
      if (pairingToken !== undefined) {
        pairing ??= pair(pairingToken);
        const paired = await pairing;
        if (!paired.paired) return paired.value;
      }
      return (await fetchGateway(route, input)).value;
    } catch {
      return unavailable();
    }
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
    const page = invalidationPage(
      await request(routes.invalidations, { after: invalidationCursor }),
    );
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

export function createElectronWorkspaceGatewayClient(
  options: ElectronGatewayOptions,
): ElectronWorkspaceGatewayClient | null {
  const gateway = parseGateway(options.baseUrl);
  if (gateway === null || options.pairingToken.length < 24 || options.pairingToken.length > 512)
    return null;
  const endpoint = gateway;
  let pairingToken: string | undefined = options.pairingToken;
  let sessionCookie: string | undefined;
  let pairing: Promise<boolean> | undefined;

  async function request(route: string, input: unknown): Promise<unknown> {
    try {
      if (pairingToken !== undefined) {
        pairing ??= pairWorkspace(pairingToken);
        if (!(await pairing)) return unavailable();
      }
      const headers = new Headers({ "Content-Type": "application/json", Origin: options.origin });
      if (sessionCookie !== undefined) headers.set("Cookie", sessionCookie);
      const response = await options.fetcher(new URL(route, endpoint).href, {
        method: "POST",
        headers,
        body: JSON.stringify(input),
        redirect: "error",
      });
      return await responseValue(response);
    } catch {
      return unavailable();
    }
  }

  async function pairWorkspace(token: string): Promise<boolean> {
    try {
      const response = await options.fetcher(new URL("v1/pair", endpoint).href, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, Origin: options.origin },
        redirect: "error",
      });
      const value = await responseValue(response);
      if (!response.ok || !isPairSuccess(value)) return false;
      sessionCookie = response.headers.get("set-cookie")?.split(";", 1)[0];
      pairingToken = undefined;
      return true;
    } catch {
      return false;
    }
  }

  return {
    read: (input) => request("v1/workspace/read", input),
    command: (input) => request("v1/workspace/command", input),
    events: (input) => request("v1/workspace/events", input),
  };
}

async function responseValue(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return unavailable();
  }
}

function parseGateway(candidate: string): URL | null {
  try {
    const gateway = new URL(candidate);
    const scopedPath = /^\/quicklens\/[A-Za-z][A-Za-z0-9_-]{2,63}\/?$/.test(gateway.pathname);
    if (
      gateway.protocol === "http:" &&
      ["127.0.0.1", "localhost", "[::1]"].includes(gateway.hostname) &&
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
      recovery: "Start the paired loopback gateway and reopen Quicklens.",
    },
  };
}
