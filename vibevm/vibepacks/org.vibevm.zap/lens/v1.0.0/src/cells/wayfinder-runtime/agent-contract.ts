/** Public Wayfinder agent foundation contract. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { GatewayAddress } from "../http/index.ts";
import type {
  ManagedActorBindingPort,
  ManagedAgentBackend,
  WorkAttachmentPort,
} from "../managed-work/index.ts";
import type {
  OwnedCoordinatorAgentPort,
  RepositoryWorkspaceFeature,
} from "../workspace-service/index.ts";
import type { createWorkspaceInteractionFeature } from "../workspace-interaction/index.ts";
import type { WayfinderAgentScopeInput, WayfinderEnsuredAgentScope } from "./agent-scope.ts";
import type { CoordinatorMcpLaunch, CoordinatorMcpLaunchInput } from "./coordinator-mcp.ts";
import type { ManagedRepositoryPlanPort } from "./managed-agent.ts";

export interface WayfinderAgentFoundation {
  readonly interactions: ReturnType<typeof createWorkspaceInteractionFeature>;
  readonly ownedCoordinators: OwnedCoordinatorAgentPort;
  readonly managedActors: ManagedActorBindingPort;
  ensureScope(
    input: WayfinderAgentScopeInput,
  ): Promise<WayfinderAgentFoundationResult<WayfinderEnsuredAgentScope>>;
  prepareOwnedCoordinatorLaunch(
    input: CoordinatorMcpLaunchInput,
  ): Promise<WayfinderAgentFoundationResult<CoordinatorMcpLaunch>>;
  bindManagedWork(backend: ManagedAgentBackend): WayfinderAgentFoundationResult<null>;
  bindNativeWork(attachments: WorkAttachmentPort): WayfinderAgentFoundationResult<null>;
  bindRepositoryPlans(port: ManagedRepositoryPlanPort): WayfinderAgentFoundationResult<null>;
  bindRepositoryWork(feature: RepositoryWorkspaceFeature): WayfinderAgentFoundationResult<null>;
  start(): Promise<WayfinderAgentFoundationResult<GatewayAddress>>;
  close(): Promise<void>;
}

export type WayfinderAgentFoundationResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };
