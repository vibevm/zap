/** Workspace service factory. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { InProcessWorkspaceService } from "./service.ts";
import type { WorkspaceService, WorkspaceServiceOptions } from "./types.ts";

export function createWorkspaceService(options: WorkspaceServiceOptions): WorkspaceService {
  return new InProcessWorkspaceService(options);
}
