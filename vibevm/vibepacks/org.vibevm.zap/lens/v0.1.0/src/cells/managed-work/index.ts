/** Managed-agent public seam. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
export * from "./contracts.ts";
export * from "./control.ts";
export { createManagedControlRuntime } from "./control-runtime.ts";
export { createManagedProviderControlAdapters } from "./provider-controls.ts";
export {
  createZapMockManagedControlAdapter,
  ZapMockManagedAssignmentSchema,
  ZapMockManagedInputSchema,
  ZapMockManagedSidebandEventSchema,
} from "./mock-control.ts";
export {
  createCodexManagedControlAdapter,
  createNativeManagedProviderControlAdapters,
  createOpenCodeManagedControlAdapter,
} from "./native-provider-controls.ts";
export type {
  ManagedControlProcess,
  ManagedControlProcessFactory,
} from "./native-provider-controls.ts";
export * from "./provider-types.ts";
export * from "./providers.ts";
export * from "./workspace.ts";
export { createManagedAgentBackend } from "./backend.ts";
export * from "./agent-port.ts";
export type {
  ManagedActorBindingPort,
  ManagedParentPort,
  ManagedSelectionPort,
} from "./backend.ts";
export { openManagedWorkStore } from "./store.ts";
export type { ManagedWorkStore } from "./store.ts";
