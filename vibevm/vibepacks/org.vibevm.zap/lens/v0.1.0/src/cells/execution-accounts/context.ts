/** Honest context request application. @scope spec://org.vibevm.zap/lens/PROP-015#context */
import { AppliedContextSchema } from "../execution-catalog/index.ts";
import {
  parseContextInput,
  type ContextApplicationPort,
  type ExecutionAccountResult,
} from "./types.ts";

export function createContextApplicationPort(): ContextApplicationPort {
  return {
    apply(input) {
      const parsed = parseContextInput(input);
      if (!parsed.ok) return failure("invalid context request or capability");
      const { request, capability } = parsed;
      if (capability.mode === "configurable") {
        if (request.mode === "inherit")
          return applied({ state: "inherited", tokens: input.inheritedTokens ?? null });
        const tokens = request.mode === "explicit" ? request.tokens : capability.defaultTokens;
        if (tokens === null)
          return applied({ state: "unknown", reason: "provider default context is not observed" });
        if (
          !capability.allowedTokens.includes(tokens) ||
          (capability.documentedMaximumTokens !== null &&
            tokens > capability.documentedMaximumTokens)
        )
          return failure("requested context is not an allowed host configuration");
        return applied({ state: "configured", tokens });
      }
      if (capability.mode === "fixed") {
        if (request.mode === "explicit" && request.tokens !== capability.tokens)
          return failure("fixed context cannot apply the requested override");
        return applied({ state: "fixed", tokens: capability.tokens });
      }
      if (capability.mode === "inherited") {
        if (request.mode === "explicit")
          return failure("inherited context cannot apply an explicit override");
        return applied({ state: "inherited", tokens: input.inheritedTokens ?? null });
      }
      if (capability.mode === "unsupported") {
        if (request.mode === "explicit")
          return failure("context override is unsupported by this launch surface");
        return applied({ state: "unsupported" });
      }
      if (request.mode === "explicit")
        return failure("context capability is unknown; explicit override is refused");
      return applied({ state: "unknown", reason: capability.reason });
    },
  };
}

function applied(input: unknown) {
  return { ok: true as const, value: AppliedContextSchema.parse(input) };
}

function failure(message: string): ExecutionAccountResult<never> {
  return { ok: false, error: { code: "unsupported_override", message } };
}
