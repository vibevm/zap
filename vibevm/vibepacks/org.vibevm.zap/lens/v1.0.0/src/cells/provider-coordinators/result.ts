/** Provider boundary result normalization. @scope spec://org.vibevm.zap/lens/PROP-006#root */
import type { AgentRuntimeResult } from "../agent-runtime/index.ts";

type ProviderFailureCode =
  | "busy"
  | "invalid_input"
  | "unsupported"
  | "not_found"
  | "protocol_error"
  | "stale_epoch"
  | "host_refused";

export async function guardedProviderTransportCall<T>(
  call: () => Promise<AgentRuntimeResult<T>>,
): Promise<AgentRuntimeResult<T>> {
  try {
    return await call();
  } catch {
    return providerFailure(
      "host_refused",
      "provider transport failed before a session was established",
    );
  }
}

export function providerFailure(
  code: ProviderFailureCode,
  message: string,
): AgentRuntimeResult<never> {
  return {
    ok: false,
    error: {
      code,
      message,
      retry:
        code === "protocol_error"
          ? "after_reconcile"
          : code === "host_refused"
            ? "after_refresh"
            : "never",
    },
  };
}
