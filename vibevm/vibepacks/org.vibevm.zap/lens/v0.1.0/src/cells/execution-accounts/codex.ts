/** Selected-home Codex launch and read-only subscription usage. @scope spec://org.vibevm.zap/lens/PROP-015#subscription */
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  createNodeCodexProcessFactory,
  type CodexProcessFactory,
  type CodexRpcProcess,
} from "../codex-coordinator/index.ts";
import {
  ExecutionCatalogIdSchema,
  UsageObservationSchema,
  type UsageObservation,
} from "../execution-catalog/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import { ReasoningEffortSchema, type ReasoningEffort } from "../model-policy/index.ts";
import {
  type ExecutionAccountIsolationPort,
  type ExecutionAccountResult,
  type ExecutionUsagePort,
  type ExecutionUsageRead,
  type ObservedExecutionAccount,
} from "./types.ts";
import { isolatedExecutionEnvironment } from "./isolation.ts";

type FactoryBuilder = (
  environment: Readonly<Record<string, string | undefined>>,
  proxyPolicy: ProxyPolicy | undefined,
) => CodexProcessFactory;

export interface ObservedCodexModelCapability {
  readonly modelId: string;
  readonly displayName: string;
  readonly supportedEfforts: readonly ReasoningEffort[];
  readonly defaultEffort: ReasoningEffort | null;
}

export function createAccountIsolatedCodexProcessFactory(input: {
  readonly isolation: ExecutionAccountIsolationPort;
  readonly hostId: string;
  readonly ambient?: Readonly<Record<string, string | undefined>>;
  readonly proxyPolicy?: ProxyPolicy;
  readonly createFactory?: FactoryBuilder;
}): CodexProcessFactory {
  const ambient = input.ambient ?? process.env;
  const createFactory =
    input.createFactory ??
    ((environment, proxyPolicy) =>
      createNodeCodexProcessFactory({
        environment,
        ...(proxyPolicy === undefined ? {} : { proxyPolicy }),
      }));
  return {
    async start(profile, cwd) {
      const bindingId = profile.accountBindingId;
      if (bindingId === undefined) {
        return createFactory(ambient, input.proxyPolicy).start(profile, cwd);
      }
      const account = await input.isolation.resolve({
        bindingId,
        hostId: input.hostId,
        agentProduct: "codex",
      });
      if (!account.ok)
        return {
          ok: false,
          error: { kind: "spawn_failed", message: account.error.message },
        };
      return createFactory(
        isolatedExecutionEnvironment(ambient, account.value.environment),
        input.proxyPolicy,
      ).start(profile, cwd);
    },
  };
}

export function createCodexUsagePort(input: {
  readonly isolation: ExecutionAccountIsolationPort;
  readonly hostId: string;
  readonly executablePath: string;
  readonly launchCwd: string;
  readonly requestTimeoutMs?: number;
  readonly ambient?: Readonly<Record<string, string | undefined>>;
  readonly proxyPolicy?: ProxyPolicy;
  readonly createFactory?: FactoryBuilder;
  readonly now?: () => Date;
}): ExecutionUsagePort {
  const factory = createAccountIsolatedCodexProcessFactory(input);
  const now = input.now ?? (() => new Date());
  return {
    async read(request) {
      const connection = ExecutionCatalogIdSchema.safeParse(request.connectionId);
      const binding = ExecutionCatalogIdSchema.safeParse(request.bindingId);
      if (!connection.success || !binding.success)
        return failure("invalid_input", "Codex usage scope is invalid");
      const process = await factory.start(
        {
          executablePath: input.executablePath,
          requestTimeoutMs: input.requestTimeoutMs ?? 30_000,
          accountBindingId: binding.data,
        },
        input.launchCwd,
      );
      if (!process.ok) return failure("unavailable", process.error.message);
      return readUsage(process.value, connection.data, binding.data, request, now());
    },
  };
}

export function createCodexModelCapabilityPort(input: {
  readonly isolation: ExecutionAccountIsolationPort;
  readonly hostId: string;
  readonly executablePath: string;
  readonly launchCwd: string;
  readonly requestTimeoutMs?: number;
  readonly ambient?: Readonly<Record<string, string | undefined>>;
  readonly proxyPolicy?: ProxyPolicy;
  readonly createFactory?: FactoryBuilder;
  readonly now?: () => Date;
}) {
  const factory = createAccountIsolatedCodexProcessFactory(input);
  const now = input.now ?? (() => new Date());
  return {
    async read(bindingId: string): Promise<
      ExecutionAccountResult<{
        readonly bindingId: string;
        readonly models: readonly ObservedCodexModelCapability[];
        readonly observedAt: string;
      }>
    > {
      const binding = ExecutionCatalogIdSchema.safeParse(bindingId);
      if (!binding.success) return failure("invalid_input", "Codex model binding is invalid");
      const process = await factory.start(
        {
          executablePath: input.executablePath,
          requestTimeoutMs: input.requestTimeoutMs ?? 30_000,
          accountBindingId: binding.data,
        },
        input.launchCwd,
      );
      if (!process.ok) return failure("unavailable", process.error.message);
      try {
        const initialized = await initializeProcess(process.value);
        if (!initialized.ok) return initialized;
        const listed = await process.value.request("model/list", {});
        if (!listed.ok) return failure("protocol_error", listed.error.message);
        const parsed = ModelListSchema.safeParse(listed.value);
        if (!parsed.success) return failure("protocol_error", "Codex model list is invalid");
        return {
          ok: true,
          value: {
            bindingId: binding.data,
            models: parsed.data.data.map((model) => ({
              modelId: model.id ?? model.model,
              displayName: model.displayName,
              supportedEfforts: model.supportedReasoningEfforts.map(
                (entry) => entry.reasoningEffort,
              ),
              defaultEffort: model.defaultReasoningEffort ?? null,
            })),
            observedAt: now().toISOString(),
          },
        };
      } finally {
        await process.value.terminate(5_000);
      }
    },
  };
}

const AccountReadSchema = z
  .object({ account: z.record(z.string(), z.unknown()).nullable() })
  .loose();
const WindowSchema = z
  .object({
    usedPercent: z.number().nullable().optional(),
    windowDurationMins: z.number().nonnegative().nullable().optional(),
    resetsAt: z.number().nullable().optional(),
  })
  .loose();
const BucketSchema = z
  .object({
    limitId: z.string().min(1).max(512).optional(),
    limitName: z.string().max(512).nullable().optional(),
    planType: z.string().max(160).nullable().optional(),
    primary: WindowSchema.nullable().optional(),
    secondary: WindowSchema.nullable().optional(),
  })
  .loose();
const RateLimitsSchema = z
  .object({
    rateLimits: BucketSchema.nullable().optional(),
    rateLimitsByLimitId: z.record(z.string(), BucketSchema).nullable().optional(),
  })
  .loose();
const ModelListSchema = z
  .object({
    data: z.array(
      z
        .object({
          id: z.string().min(1).max(256).optional(),
          model: z.string().min(1).max(256),
          displayName: z.string().min(1).max(256),
          supportedReasoningEfforts: z.array(
            z.object({ reasoningEffort: ReasoningEffortSchema }).loose(),
          ),
          defaultReasoningEffort: ReasoningEffortSchema.nullable().optional(),
        })
        .loose()
        .refine((model) => model.id !== undefined || model.model.length > 0),
    ),
  })
  .loose();

async function readUsage(
  process: CodexRpcProcess,
  connectionId: string,
  bindingId: string,
  request: Parameters<ExecutionUsagePort["read"]>[0],
  observedAt: Date,
): Promise<ExecutionAccountResult<ExecutionUsageRead>> {
  try {
    const initialized = await initializeProcess(process);
    if (!initialized.ok) return initialized;
    const accountResult = await process.request("account/read", { refreshToken: false });
    if (!accountResult.ok) return failure("protocol_error", accountResult.error.message);
    const accountParsed = AccountReadSchema.safeParse(accountResult.value);
    if (!accountParsed.success)
      return failure("protocol_error", "Codex account response is invalid");
    const rateResult = await process.request("account/rateLimits/read", {});
    if (!rateResult.ok) return failure("protocol_error", rateResult.error.message);
    const rateParsed = RateLimitsSchema.safeParse(rateResult.value);
    if (!rateParsed.success)
      return failure("protocol_error", "Codex rate-limit response is invalid");
    const account = observedAccount(accountParsed.data.account);
    const buckets = rateBuckets(rateParsed.data);
    const observations = observationsFor(
      connectionId,
      buckets,
      request.bucketApplicability ?? {},
      observedAt,
    );
    return {
      ok: true,
      value: {
        bindingId,
        account,
        observations:
          observations.length > 0 ? observations : [unknownObservation(connectionId, observedAt)],
      },
    };
  } finally {
    await process.terminate(5_000);
  }
}

async function initializeProcess(process: CodexRpcProcess): Promise<ExecutionAccountResult<null>> {
  const initialized = await process.request("initialize", {
    clientInfo: { name: "quicklens", title: "Zap Wayfinder", version: "0.1.0" },
    capabilities: { experimentalApi: true },
  });
  if (!initialized.ok) return failure("protocol_error", initialized.error.message);
  const acknowledged = process.notify("initialized", {});
  return acknowledged.ok
    ? { ok: true, value: null }
    : failure("protocol_error", acknowledged.error.message);
}

function observedAccount(account: Record<string, unknown> | null): ObservedExecutionAccount | null {
  if (account === null) return null;
  const authMode = stringField(account, "type") ?? "unknown";
  const planType = stringField(account, "planType") ?? null;
  const identity =
    stringField(account, "accountId") ??
    stringField(account, "id") ??
    stringField(account, "email");
  return {
    authMode,
    planType,
    identityDigest:
      identity === undefined
        ? null
        : createHash("sha256").update(`${authMode}\u0000${identity}`).digest("hex"),
  };
}

function rateBuckets(input: z.infer<typeof RateLimitsSchema>) {
  const multiple = input.rateLimitsByLimitId;
  if (multiple !== undefined && multiple !== null && Object.keys(multiple).length > 0)
    return Object.entries(multiple).map(([key, value]) => ({ key, value }));
  return input.rateLimits === undefined || input.rateLimits === null
    ? []
    : [{ key: input.rateLimits.limitId ?? "codex", value: input.rateLimits }];
}

function observationsFor(
  connectionId: string,
  buckets: ReturnType<typeof rateBuckets>,
  applicability: Readonly<Record<string, UsageObservation["applicability"]>>,
  observedAt: Date,
): UsageObservation[] {
  return buckets.flatMap(({ key, value }) =>
    (["primary", "secondary"] as const).flatMap((kind) => {
      const window = value[kind];
      if (window === undefined || window === null) return [];
      const used = window.usedPercent;
      const remaining = used === undefined || used === null ? null : clamp(100 - used);
      const duration = window.windowDurationMins;
      const label = (value.limitName ?? value.limitId ?? key).slice(0, 160) || key;
      return [
        UsageObservationSchema.parse({
          observationId: stableId("usage", connectionId, key, kind, observedAt.toISOString()),
          connectionId,
          bucketId: stableId("bucket", key, kind),
          bucketLabel: label,
          applicability: applicability[key] ?? {
            kind: "unknown",
            reason: "Codex bucket applicability was not explicitly configured",
          },
          meterKind: "subscription",
          unit: used === undefined || used === null ? "unknown" : "percent",
          remainingPercent: remaining,
          exactRemainingTokens: null,
          window: {
            kind: `${kind}:${duration === undefined || duration === null ? "unknown" : String(duration)}m`,
            resetsAt: resetTime(window.resetsAt),
          },
          observedAt: observedAt.toISOString(),
          source: "codex-app-server:account/rateLimits/read",
          status: remaining === null ? "unknown" : "observed",
          detail:
            used === undefined || used === null
              ? "Codex returned a quota window without usedPercent"
              : `Codex reported usedPercent=${String(used)}; remaining is clamped 100-usedPercent`,
        }),
      ];
    }),
  );
}

function unknownObservation(connectionId: string, observedAt: Date): UsageObservation {
  return UsageObservationSchema.parse({
    observationId: stableId("usage", connectionId, "unknown", observedAt.toISOString()),
    connectionId,
    bucketId: stableId("bucket", "codex", "unknown"),
    bucketLabel: "Codex subscription",
    applicability: { kind: "unknown", reason: "No Codex rate-limit bucket was returned" },
    meterKind: "subscription",
    unit: "unknown",
    remainingPercent: null,
    exactRemainingTokens: null,
    window: { kind: "unknown", resetsAt: null },
    observedAt: observedAt.toISOString(),
    source: "codex-app-server:account/rateLimits/read",
    status: "unknown",
    detail: "Selected Codex binding returned no observable rate-limit window",
  });
}

function stableId(prefix: string, ...parts: readonly string[]): string {
  return `${prefix}.${createHash("sha256").update(parts.join("\u0000")).digest("hex").slice(0, 24)}`;
}

function resetTime(seconds: number | null | undefined): string | null {
  if (seconds === undefined || seconds === null || seconds < 0) return null;
  const date = new Date(seconds * 1_000);
  return Number.isNaN(date.valueOf()) ? null : date.toISOString();
}

function clamp(value: number): number {
  return Math.min(100, Math.max(0, value));
}

function stringField(value: Record<string, unknown>, field: string): string | undefined {
  const candidate = value[field];
  return typeof candidate === "string" && candidate.length > 0 ? candidate : undefined;
}

function failure(
  code: "invalid_input" | "unavailable" | "protocol_error",
  message: string,
): ExecutionAccountResult<never> {
  return { ok: false, error: { code, message } };
}
