import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import type {
  CodexProcessFactory,
  CodexProcessProfile,
  CodexProcessResult,
  CodexRpcProcess,
} from "../codex-coordinator/index.ts";
import type { JsonValue } from "../protocol/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import {
  createCodexUsagePort,
  createCodexModelCapabilityPort,
  createExecutionAccountIsolation,
  ProtectedExecutionBindingSchema,
} from "./index.ts";

test("Codex quota reads use the selected account home and preserve distinct buckets", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-codex-usage-"));
  try {
    const homeA = join(root, "a");
    const homeB = join(root, "b");
    await Promise.all([mkdir(homeA), mkdir(homeB)]);
    const isolation = createExecutionAccountIsolation([
      binding("binding.codex.a", homeA),
      binding("binding.codex.b", homeB),
    ]);
    assert.equal(isolation.ok, true);
    if (!isolation.ok) return;
    const environments: Readonly<Record<string, string | undefined>>[] = [];
    const calls: string[] = [];
    const usage = createCodexUsagePort({
      isolation: isolation.value,
      hostId: "host.local.test",
      executablePath: join(root, "codex.exe"),
      launchCwd: root,
      ambient: {
        PATH: "synthetic-path",
        OPENAI_API_KEY: "synthetic-ambient-must-not-cross",
        CODEX_ACCESS_TOKEN: "synthetic-ambient-token-must-not-cross",
      },
      now: () => new Date("2026-09-16T12:00:00.000Z"),
      createFactory(environment) {
        environments.push(environment);
        return scriptedFactory(environment["CODEX_HOME"] ?? "", calls);
      },
    });
    const a = await usage.read({
      connectionId: "connection.codex.a",
      bindingId: "binding.codex.a",
      bucketApplicability: { codex: { kind: "account" } },
    });
    const b = await usage.read({
      connectionId: "connection.codex.b",
      bindingId: "binding.codex.b",
      bucketApplicability: { codex: { kind: "account" } },
    });
    assert.equal(a.ok, true);
    assert.equal(b.ok, true);
    if (!a.ok || !b.ok) return;
    assert.equal(environments[0]?.["CODEX_HOME"], homeA);
    assert.equal(environments[1]?.["CODEX_HOME"], homeB);
    assert.equal(environments[0]?.["OPENAI_API_KEY"], undefined);
    assert.equal(environments[0]?.["CODEX_ACCESS_TOKEN"], undefined);
    assert.equal(environments[0]?.["PATH"], "synthetic-path");
    assert.notEqual(a.value.account?.identityDigest, b.value.account?.identityDigest);
    assert.equal(a.value.observations.length, 3);
    assert.equal(a.value.observations[0]?.remainingPercent, 0);
    assert.equal(a.value.observations[0]?.meterKind, "subscription");
    assert.equal(a.value.observations[1]?.remainingPercent, 70);
    assert.equal(a.value.observations[1]?.applicability.kind, "unknown");
    assert.equal(a.value.observations[2]?.remainingPercent, null);
    assert.equal(b.value.observations[0]?.remainingPercent, 80);
    assert.equal(calls.includes("account/usage/read"), false);
    assert.deepEqual(calls, [
      "initialize",
      "notify:initialized",
      "account/read",
      "account/rateLimits/read",
      "initialize",
      "notify:initialized",
      "account/read",
      "account/rateLimits/read",
    ]);
    const capabilityCalls: string[] = [];
    const capabilities = createCodexModelCapabilityPort({
      isolation: isolation.value,
      hostId: "host.local.test",
      executablePath: join(root, "codex.exe"),
      launchCwd: root,
      now: () => new Date("2026-09-16T12:00:00.000Z"),
      createFactory(environment) {
        return scriptedFactory(environment["CODEX_HOME"] ?? "", capabilityCalls);
      },
    });
    const observed = await capabilities.read("binding.codex.a");
    assert.equal(observed.ok, true);
    if (!observed.ok) return;
    assert.deepEqual(observed.value.models[0], {
      modelId: "gpt-5.6-sol",
      displayName: "GPT-5.6-Sol",
      supportedEfforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "low",
    });
    assert.deepEqual(capabilityCalls, ["initialize", "notify:initialized", "model/list"]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function binding(bindingId: string, homePath: string) {
  return ProtectedExecutionBindingSchema.parse({
    bindingId,
    hostId: ExecutionHostIdSchema.parse("host.local.test"),
    displayName: bindingId,
    enabled: true,
    setupGuidance: "Run Codex sign-in with this protected CODEX_HOME before enabling it.",
    kind: "codex_home" as const,
    agentProduct: "codex" as const,
    homePath,
  });
}

function scriptedFactory(home: string, calls: string[]): CodexProcessFactory {
  return {
    start(profile: CodexProcessProfile) {
      void profile;
      return Promise.resolve({ ok: true, value: new ScriptedCodexProcess(home, calls) });
    },
  };
}

class ScriptedCodexProcess implements CodexRpcProcess {
  readonly epoch = "epoch.synthetic";
  readonly #home: string;
  readonly #calls: string[];

  constructor(home: string, calls: string[]) {
    this.#home = home;
    this.#calls = calls;
  }

  request(method: string): Promise<CodexProcessResult<JsonValue>> {
    this.#calls.push(method);
    const a = this.#home.endsWith("a");
    if (method === "initialize") return Promise.resolve({ ok: true, value: {} });
    if (method === "account/read") {
      return Promise.resolve({
        ok: true,
        value: {
          account: {
            type: "chatgpt",
            email: a ? "alpha@example.test" : "beta@example.test",
            planType: "pro",
          },
          requiresOpenaiAuth: true,
        },
      });
    }
    if (method === "account/rateLimits/read") {
      return Promise.resolve({
        ok: true,
        value: {
          rateLimitsByLimitId: a
            ? {
                codex: {
                  limitId: "codex",
                  primary: { usedPercent: 110, windowDurationMins: 300, resetsAt: 1_800_000_000 },
                },
                codex_other: {
                  limitId: "codex_other",
                  limitName: "Other Codex",
                  primary: { usedPercent: 30, windowDurationMins: 60, resetsAt: 1_800_000_100 },
                  secondary: { windowDurationMins: 10_080, resetsAt: 1_800_000_200 },
                },
              }
            : {
                codex: {
                  limitId: "codex",
                  primary: { usedPercent: 20, windowDurationMins: 300, resetsAt: 1_800_000_000 },
                },
              },
        },
      });
    }
    if (method === "model/list")
      return Promise.resolve({
        ok: true,
        value: {
          data: [
            {
              id: "gpt-5.6-sol",
              model: "gpt-5.6-sol",
              displayName: "GPT-5.6-Sol",
              supportedReasoningEfforts: ["low", "medium", "high", "xhigh", "max", "ultra"].map(
                (reasoningEffort) => ({ reasoningEffort }),
              ),
              defaultReasoningEffort: "low",
            },
          ],
        },
      });
    return Promise.resolve({
      ok: false,
      error: { kind: "rpc", message: "unexpected synthetic method" },
    });
  }

  notify(method: string): CodexProcessResult<void> {
    this.#calls.push(`notify:${method}`);
    return { ok: true, value: undefined };
  }
  respond(): CodexProcessResult<void> {
    return { ok: true, value: undefined };
  }
  subscribe(): () => void {
    return () => undefined;
  }
  onExit(): () => void {
    return () => undefined;
  }
  terminate(): Promise<CodexProcessResult<{ code: number | null }>> {
    return Promise.resolve({ ok: true, value: { code: 0 } });
  }
  close(): void {}
}
