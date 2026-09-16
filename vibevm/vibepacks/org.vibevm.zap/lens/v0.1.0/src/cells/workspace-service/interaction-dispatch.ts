/** Resolve the live coordinator interaction boundary for one context. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { Launch } from "./launch.ts";

export function interactionDispatch(
  launches: Iterable<Launch>,
  store: WorkspaceStore,
  projectId: string,
  contextId: string,
) {
  const launch = [...launches].find(
    (candidate) => candidate.projectId === projectId && candidate.contextId === contextId,
  );
  if (launch === undefined) return null;
  const execution = store.readProjectExecution(launch.projectId, launch.contextId);
  return {
    projectId: launch.projectId,
    contextId: launch.contextId,
    coordinatorSessionId: launch.scope.coordinatorSessionId,
    adapter: launch.adapter,
    processEpoch: launch.processEpoch,
    executionEnabled: execution.ok && execution.value.state === "running",
  };
}
