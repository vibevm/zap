/** Codex coordinator adapter construction. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { isAbsolute } from "node:path";
import {
  runtimeFailure,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
} from "../agent-runtime/index.ts";
import { CodexAdapter } from "./adapter.ts";
import { createNodeCodexProcessFactory, type CodexProcessFactory } from "./process.ts";
import { CodexCoordinatorProfileSchema, type CodexCoordinatorProfile } from "./profile.ts";

export function createCodexCoordinatorAdapter(options: {
  readonly profiles: readonly CodexCoordinatorProfile[];
  readonly processFactory?: CodexProcessFactory;
}): AgentRuntimeResult<CoordinatorAdapter> {
  const profiles = new Map<string, CodexCoordinatorProfile>();
  for (const candidate of options.profiles) {
    const parsed = CodexCoordinatorProfileSchema.safeParse(candidate);
    if (!parsed.success || !isAbsolute(parsed.data.executablePath)) {
      return runtimeFailure("invalid_input", "Codex coordinator profile is invalid");
    }
    if (profiles.has(parsed.data.profileId)) {
      return runtimeFailure("invalid_input", "Codex coordinator profile ids must be unique");
    }
    profiles.set(parsed.data.profileId, parsed.data);
  }
  if (profiles.size === 0) {
    return runtimeFailure("invalid_input", "At least one profile is required");
  }
  return {
    ok: true,
    value: new CodexAdapter(profiles, options.processFactory ?? createNodeCodexProcessFactory()),
  };
}
