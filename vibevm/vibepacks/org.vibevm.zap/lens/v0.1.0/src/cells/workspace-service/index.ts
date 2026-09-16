/** Shared Lens application service. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
export { InProcessWorkspaceService } from "./service.ts";
export { createWorkspaceService } from "./factory.ts";
export { managedWorkView, projectManagedWorkClaim } from "./managed-work.ts";
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
  type ManagedWakePort,
  type ManagedWakeTarget,
  type ManagedWakeEvent,
  type RepositoryWorkspaceFeature,
  type RepositoryWorkspaceReadRequest,
  type RepositoryWorkspaceCommandRequest,
} from "./types.ts";
