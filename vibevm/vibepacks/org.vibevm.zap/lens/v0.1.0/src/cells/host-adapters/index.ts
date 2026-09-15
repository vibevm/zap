/**
 * Runtime-validated host identity and delivery adapters.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-001#adapters
 * @example
 * const routed = routeSafePointHook("codex", hookInput, bindings, handle);
 * if (routed.ok) buildSafePointOffer(routed.value, persistedMessages);
 */
export { getHostCapabilities } from "./capabilities.ts";
export { dispatchOpenCode } from "./open-code.ts";
export { buildSafePointOffer, routeSafePointHook } from "./safe-point.ts";
export {
  ActorRouteBindingSchema,
  AuthenticatedActorHandleSchema,
  PersistedHostMessageSchema,
} from "./types.ts";
export type {
  ActorRouteBinding,
  AdapterDiagnostic,
  AdapterDiagnosticCode,
  AuthenticatedActorHandle,
  DeliveryObservation,
  DeliveryObservations,
  HookRoute,
  HostCapabilities,
  HostInputPath,
  OpenCodeDispatch,
  OpenCodeSendRequest,
  OpenCodeTransport,
  PersistedHostMessage,
  SafePointDelivery,
  SafePointEvent,
  SafePointHost,
  SafePointOffer,
} from "./types.ts";
