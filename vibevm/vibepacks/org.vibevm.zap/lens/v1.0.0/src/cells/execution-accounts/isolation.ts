/** Trusted host account-home isolation. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import { realpath, stat } from "node:fs/promises";
import { resolve } from "node:path";
import {
  ProtectedExecutionBindingSchema,
  type ExecutionAccountIsolationPort,
  type ExecutionAccountResult,
  type ProtectedExecutionBinding,
  type ResolvedExecutionAccount,
} from "./types.ts";

const ACCOUNT_AUTH_ENV = new Set([
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_AUTH_TOKEN",
  "ANTHROPIC_BASE_URL",
  "CLAUDE_CODE_OAUTH_TOKEN",
  "CODEX_ACCESS_TOKEN",
  "OPENAI_API_KEY",
  "OPENAI_BASE_URL",
  "OPENROUTER_API_KEY",
  "DEEPSEEK_API_KEY",
  "DASHSCOPE_API_KEY",
  "GEMINI_API_KEY",
  "GOOGLE_API_KEY",
  "MISTRAL_API_KEY",
  "MOONSHOT_API_KEY",
  "QWEN_API_KEY",
  "XAI_API_KEY",
  "ZAI_API_KEY",
]);

export function isolatedExecutionEnvironment(
  ambient: Readonly<Record<string, string | undefined>>,
  binding: Readonly<Record<string, string>>,
): Readonly<Record<string, string | undefined>> {
  return {
    ...Object.fromEntries(Object.entries(ambient).filter(([name]) => !ACCOUNT_AUTH_ENV.has(name))),
    ...binding,
  };
}

export function createExecutionAccountIsolation(
  rawBindings: readonly ProtectedExecutionBinding[],
): ExecutionAccountResult<ExecutionAccountIsolationPort> {
  const bindings = new Map<string, ProtectedExecutionBinding>();
  for (const raw of rawBindings) {
    const parsed = ProtectedExecutionBindingSchema.safeParse(raw);
    if (!parsed.success) return failure("invalid_input", "protected execution binding is invalid");
    if (bindings.has(parsed.data.bindingId))
      return failure("invalid_input", "protected execution binding ids must be unique");
    bindings.set(parsed.data.bindingId, parsed.data);
  }
  return {
    ok: true,
    value: {
      list() {
        return [...bindings.values()]
          .sort((left, right) => left.bindingId.localeCompare(right.bindingId))
          .map((binding) => ({
            bindingId: binding.bindingId,
            hostId: binding.hostId,
            displayName: binding.displayName,
            agentProduct: binding.agentProduct,
            enabled: binding.enabled,
            setupGuidance: binding.setupGuidance,
          }));
      },
      async resolve(input) {
        const binding = bindings.get(input.bindingId);
        if (binding === undefined)
          return failure("not_found", "execution binding is not registered");
        if (!binding.enabled) return failure("disabled", "execution binding is disabled");
        if (binding.hostId !== input.hostId)
          return failure("host_mismatch", "execution binding belongs to another host");
        if (binding.agentProduct !== input.agentProduct)
          return failure("product_mismatch", "execution binding belongs to another agent product");
        if (binding.kind === "zap_mock_fixture") {
          return { ok: true, value: resolved(binding, {}, null) };
        }
        if (binding.kind === "environment_reference") {
          return {
            ok: true,
            value: resolved(binding, {}, binding.environmentRef),
          };
        }
        const home = await availableDirectory(binding.homePath);
        if (home === null) return failure("unavailable", "protected account home is unavailable");
        const environment =
          binding.kind === "codex_home" ? { CODEX_HOME: home } : { CLAUDE_CONFIG_DIR: home };
        return { ok: true, value: resolved(binding, environment, null) };
      },
    },
  };
}

function resolved(
  binding: ProtectedExecutionBinding,
  environment: Readonly<Record<string, string>>,
  environmentRef: string | null,
): ResolvedExecutionAccount {
  return {
    bindingId: binding.bindingId,
    hostId: binding.hostId,
    agentProduct: binding.agentProduct,
    environment,
    environmentRef,
    setupGuidance: binding.setupGuidance,
  };
}

async function availableDirectory(path: string): Promise<string | null> {
  try {
    const canonical = await realpath(resolve(path));
    return (await stat(canonical)).isDirectory() ? canonical : null;
  } catch {
    return null;
  }
}

function failure(
  code:
    | "invalid_input"
    | "not_found"
    | "disabled"
    | "host_mismatch"
    | "product_mismatch"
    | "unavailable",
  message: string,
): ExecutionAccountResult<never> {
  return { ok: false, error: { code, message } };
}
