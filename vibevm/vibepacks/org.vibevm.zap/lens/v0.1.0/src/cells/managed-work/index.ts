/** Managed-agent public seam. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
export * from "./contracts.ts";
export * from "./provider-types.ts";
export * from "./providers.ts";
export { createManagedAgentBackend } from "./backend.ts";
export * from "./agent-port.ts";
export type {
  ManagedActorBindingPort,
  ManagedParentPort,
  ManagedSelectionPort,
} from "./backend.ts";
export { openManagedWorkStore } from "./store.ts";
export type { ManagedWorkStore } from "./store.ts";
