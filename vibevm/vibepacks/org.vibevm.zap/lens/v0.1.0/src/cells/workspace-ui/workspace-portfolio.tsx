/** Default map with an optional compact project grid. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import { component$, useSignal, type NoSerialize, type QRL } from "@qwik.dev/core";
import type { ProjectWorkspaceView, WorkspaceClientPort } from "../workspace-client/index.ts";
import type { ProjectDescriptor, ProjectId } from "../workspace-model/index.ts";
import { ProjectBoard } from "./project-navigation.tsx";
import { WorkspaceCanvas } from "./workspace-canvas.tsx";

export const WorkspacePortfolio = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projects: readonly ProjectDescriptor[];
  readonly views: Readonly<Record<string, ProjectWorkspaceView | undefined>>;
  readonly theme: "light" | "dark";
  readonly refreshEpoch: number;
  readonly onOpenProject$: QRL<(projectId: ProjectId) => void>;
  readonly onOpenQuestions$: QRL<(projectId: ProjectId) => void>;
}>((props) => {
  const view = useSignal<"map" | "grid">("map");
  return (
    <section>
      <nav class="workspace-tabs portfolio-view-switch" aria-label="Portfolio view">
        <button
          class={view.value === "map" ? "selected" : ""}
          onClick$={() => (view.value = "map")}
        >
          Map
        </button>
        <button
          class={view.value === "grid" ? "selected" : ""}
          onClick$={() => (view.value = "grid")}
        >
          Grid
        </button>
      </nav>
      {view.value === "map" ? (
        <WorkspaceCanvas
          port={props.port}
          projects={props.projects}
          theme={props.theme}
          refreshEpoch={props.refreshEpoch}
          onOpenProject$={props.onOpenProject$}
          onOpenQuestions$={props.onOpenQuestions$}
        />
      ) : (
        <ProjectBoard
          projects={props.projects}
          views={props.views}
          onOpen$={props.onOpenProject$}
        />
      )}
    </section>
  );
});
