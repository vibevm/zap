/**
 * Public Codex coordinator adapter seam.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle
 */
export { createCodexCoordinatorAdapter } from "./factory.ts";
export {
  CODEX_COORDINATOR_CAPABILITIES,
  CODEX_LIFECYCLE_CAPABILITIES,
  CodexCoordinatorProfileSchema,
  type CodexCoordinatorProfile,
} from "./profile.ts";
export {
  createNodeCodexProcessFactory,
  codexProcessEnvironment,
  resolveInstalledCodexExecutable,
  CodexProcessErrorSchema,
  CodexProcessProfileSchema,
  type CodexProcessError,
  type CodexProcessFactory,
  type CodexProcessProfile,
  type CodexProcessResult,
  type CodexRpcProcess,
} from "./process.ts";
export * from "./protocol.ts";
