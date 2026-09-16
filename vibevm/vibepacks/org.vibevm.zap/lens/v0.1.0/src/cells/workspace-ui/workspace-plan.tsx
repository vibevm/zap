/** Exact-context algorithm plan view. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { component$, noSerialize, type NoSerialize, type QRL } from "@qwik.dev/core";
import { QuicklensApp } from "../quicklens-ui/index.tsx";
import {
  createWorkspacePlanningDataSource,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";

export const WorkspacePlan = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly onBack$: QRL<() => void>;
}>((props) => {
  const port = props.port;
  return port === undefined ? (
    <section class="workspace-panel workspace-empty">Plan client unavailable.</section>
  ) : (
    <QuicklensApp
      source={noSerialize(
        createWorkspacePlanningDataSource({
          port,
          projectId: props.projectId,
          contextId: props.contextId,
        }),
      )}
      headerActionLabel="Back to workspace"
      onHeaderAction$={props.onBack$}
    />
  );
});
