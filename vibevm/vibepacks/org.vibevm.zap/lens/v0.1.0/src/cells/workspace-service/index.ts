/** Shared Lens application service. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
export { InProcessWorkspaceService, createWorkspaceService } from "./service.ts";
export {
  WORKSPACE_SERVICE_ACTIONS,
  OwnedCoordinatorAgentBindingSchema,
  createCoordinatorAdapterRegistry,
  type CoordinatorAdapterRegistration,
  type CoordinatorAdapterRegistry,
  type WorkspaceService,
  type WorkspaceServiceAction,
  type WorkspaceServiceAuthorization,
  type WorkspaceServiceOptions,
  type WorkspaceManagedTerminalPort,
  type OwnedCoordinatorAgentBinding,
  type OwnedCoordinatorAgentPort,
  type CoordinatorRoutingBridge,
} from "./types.ts";
