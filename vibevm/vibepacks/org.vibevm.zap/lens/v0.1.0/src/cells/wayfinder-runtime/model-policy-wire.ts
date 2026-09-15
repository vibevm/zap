/** Runtime trusted model-policy context from configured coordinator routing. @scope spec://org.vibevm.zap/lens/PROP-008#capability-evidence */
import type { TrustedModelContext } from "../model-policy/index.ts";
import type { TrustedModelContextProvider } from "../model-policy-service/index.ts";
import type { ModelPolicyStoreError, ModelPolicyStoreResult } from "../model-policy-store/index.ts";
import type {
  CoordinatorRoutingError,
  CoordinatorRoutingProvider,
} from "../coordinator-routing/index.ts";

export function createRuntimeTrustedContextProvider(
  provider: CoordinatorRoutingProvider | undefined,
): TrustedModelContextProvider {
  return {
    async resolve(input): Promise<ModelPolicyStoreResult<TrustedModelContext>> {
      if (provider === undefined)
        return {
          ok: false,
          error: {
            code: "invalid_input",
            message: "trusted coordinator capability evidence is not configured for this runtime",
          },
        };
      const trusted = await provider.trustedContext(input);
      return trusted.ok
        ? trusted
        : {
            ok: false,
            error: {
              code: mapRoutingErrorCode(trusted.error.code),
              message: trusted.error.message,
            },
          };
    },
  };
}

function mapRoutingErrorCode(code: CoordinatorRoutingError["code"]): ModelPolicyStoreError["code"] {
  switch (code) {
    case "unauthorized":
      return "unauthorized";
    case "forbidden":
      return "forbidden";
    case "not_found":
      return "not_found";
    case "conflict":
      return "conflict";
    case "storage_failure":
      return "storage_failure";
    case "invalid_input":
    case "policy_refused":
    case "unsupported":
      return "invalid_input";
  }
}
