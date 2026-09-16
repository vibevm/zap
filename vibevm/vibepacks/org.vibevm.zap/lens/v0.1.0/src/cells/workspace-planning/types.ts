/** Shared Wayfinder planning contracts. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { AgentTransportPort } from "../transport/index.ts";
import type { QuicklensSourceRuntime } from "../quicklens-service/index.ts";
import type { QuicklensSnapshot } from "../quicklens-model/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceReadResponse,
  WorkspaceResult,
  ProjectId,
  WorkContextId,
} from "../workspace-model/index.ts";
import type { PublicConnection } from "../protocol/index.ts";

export type WorkspacePlanCommand = Extract<
  WorkspaceCommandRequest,
  { operation: `plan.${string}` }
>;
export type AgentPlanningPort = ReturnType<QuicklensSourceRuntime["createAgentPlanning"]>;

export interface WorkspacePlanningSourceObserver {
  observe(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly reason: string;
    readonly snapshot: QuicklensSnapshot;
  }): void;
}

export interface WorkspacePlanningFeature {
  snapshot(
    access: WorkspaceAccessContext,
    projectId: WorkspacePlanCommand["projectId"],
    contextId: WorkspacePlanCommand["contextId"],
  ): Promise<WorkspaceResult<WorkspaceReadResponse>>;
  command(
    access: WorkspaceAccessContext,
    request: WorkspacePlanCommand,
  ): Promise<WorkspaceResult<WorkspaceCommandResponse>>;
  agent(actor: PublicConnection, transport: AgentTransportPort): WorkspaceResult<AgentPlanningPort>;
  close(): void;
}
