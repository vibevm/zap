/** Bounded selected-account quota refresh for the execution catalog. @scope spec://org.vibevm.zap/lens/PROP-015#subscription */
import { createHash } from "node:crypto";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import {
  createCodexUsagePort,
  type ExecutionAccountIsolationPort,
} from "../execution-accounts/index.ts";
import { UsageObservationSchema, type ExecutionCatalogResult } from "../execution-catalog/index.ts";
import type { ExecutionCatalogUsageRefreshPort } from "../execution-catalog-service/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";

export function createRuntimeExecutionUsage(input: {
  readonly accounts: ExecutionAccountIsolationPort;
  readonly hostId: string;
  readonly codexProfiles: readonly CodexCoordinatorProfile[];
  readonly launchCwd: string;
  readonly proxyPolicy: ProxyPolicy;
  readonly now?: () => Date;
}): ExecutionCatalogUsageRefreshPort {
  const now = input.now ?? (() => new Date());
  const cache = new Map<
    string,
    {
      readonly observedAt: number;
      readonly result: ExecutionCatalogResult<readonly ReturnType<typeof unsupported>[]>;
    }
  >();
  const inFlight = new Map<
    string,
    Promise<ExecutionCatalogResult<readonly ReturnType<typeof unsupported>[]>>
  >();
  return {
    async refresh(request) {
      if (request.connection.agentProduct !== "codex")
        return {
          ok: true,
          value: [unsupported(request.connection.connectionId, now())],
        };
      const profile =
        input.codexProfiles.find(
          (candidate) => candidate.accountBindingId === request.connection.launchBindingId,
        ) ?? input.codexProfiles[0];
      if (profile === undefined)
        return failure("not_found", "Codex launch template is unavailable for quota refresh");
      const key = `${request.connection.connectionId}\u0000${request.connection.launchBindingId}`;
      const currentTime = now().valueOf();
      const cached = cache.get(key);
      if (cached !== undefined && currentTime - cached.observedAt < 300_000) return cached.result;
      const pending = inFlight.get(key);
      if (pending !== undefined) return pending;
      const refresh = readCodexUsage(input, profile, request.connection, now);
      inFlight.set(key, refresh);
      try {
        const result = await refresh;
        cache.set(key, { observedAt: currentTime, result });
        return result;
      } finally {
        inFlight.delete(key);
      }
    },
  };
}

async function readCodexUsage(
  input: Parameters<typeof createRuntimeExecutionUsage>[0],
  profile: CodexCoordinatorProfile,
  connection: { readonly connectionId: string; readonly launchBindingId: string },
  now: () => Date,
) {
  const usage = createCodexUsagePort({
    isolation: input.accounts,
    hostId: input.hostId,
    executablePath: profile.executablePath,
    launchCwd: input.launchCwd,
    requestTimeoutMs: profile.requestTimeoutMs,
    proxyPolicy: profile.proxy ?? input.proxyPolicy,
    now,
  });
  const observed = await usage.read({
    connectionId: connection.connectionId,
    bindingId: connection.launchBindingId,
  });
  return observed.ok
    ? { ok: true as const, value: observed.value.observations }
    : failure("not_found", observed.error.message);
}

function unsupported(connectionId: string, observedAt: Date) {
  return UsageObservationSchema.parse({
    observationId: `usage.${digest(`${connectionId}\u0000${observedAt.toISOString()}`)}`,
    connectionId,
    bucketId: `bucket.${digest(`${connectionId}\u0000unsupported`)}`,
    bucketLabel: "Subscription usage",
    applicability: { kind: "unknown", reason: "provider has no verified usage adapter" },
    meterKind: "unknown",
    unit: "unknown",
    remainingPercent: null,
    exactRemainingTokens: null,
    window: { kind: "unknown", resetsAt: null },
    observedAt: observedAt.toISOString(),
    source: "zap-wayfinder:unsupported-provider-usage",
    status: "unsupported",
    detail: "No verified read-only subscription usage API is configured for this provider",
  });
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}
function failure(code: "not_found", message: string): ExecutionCatalogResult<never> {
  return { ok: false, error: { code, message } };
}
