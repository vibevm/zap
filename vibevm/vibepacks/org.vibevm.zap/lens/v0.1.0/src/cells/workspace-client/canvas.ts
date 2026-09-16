/** Bounded separately scoped workspace reads for the unified canvas. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import type {
  ProjectId,
  ManagedWorkView,
  WorkContextId,
  WorkspaceClientPort,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { readProjectWorkspace, type ProjectWorkspaceView } from "./index.ts";
import { readRepositoryWorkspace, type RepositoryWorkspaceView } from "./repository.ts";

export interface WorkspaceCanvasSelection {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly fallbackLabel: string;
}

export type WorkspaceCanvasProject =
  | {
      readonly state: "ready";
      readonly selection: WorkspaceCanvasSelection;
      readonly contextId: WorkContextId;
      readonly view: ProjectWorkspaceView;
      readonly revision: string;
      readonly coverage: "complete" | "partial";
      readonly coverageReasons: readonly string[];
      readonly managedWorks: readonly ManagedWorkView[];
      readonly repository:
        | { readonly state: "ready"; readonly value: RepositoryWorkspaceView }
        | { readonly state: "unavailable"; readonly reason: string };
    }
  | {
      readonly state: "unavailable";
      readonly selection: WorkspaceCanvasSelection;
      readonly reason: string;
    };

export interface WorkspaceCanvasInput {
  readonly projects: readonly WorkspaceCanvasProject[];
  readonly completeness: "complete" | "partial";
}

export async function readWorkspaceCanvas(
  port: WorkspaceClientPort,
  selections: readonly WorkspaceCanvasSelection[],
): Promise<WorkspaceResult<WorkspaceCanvasInput>> {
  if (selections.length === 0 || selections.length > 8) {
    return failure("Canvas selection must contain 1 through 8 projects.");
  }
  const keys = selections.map((selection) => `${selection.projectId}\u0000${selection.contextId}`);
  if (new Set(keys).size !== keys.length)
    return failure("Canvas project/context selections repeat.");
  const projects = await Promise.all(
    selections.map(async (selection): Promise<WorkspaceCanvasProject> => {
      const [read, managed, repository] = await Promise.all([
        readProjectWorkspace(port, selection.projectId, selection.contextId),
        port.read({
          operation: "managed-work.list.v1",
          projectId: selection.projectId,
          contextId: selection.contextId,
        }),
        readRepositoryWorkspace(port, selection.projectId, selection.contextId),
      ]);
      if (!read.ok) return { state: "unavailable", selection, reason: read.error.message };
      const coverageReasons = [
        ...projectCoverageReasons(read.value),
        ...(managed.ok && managed.value.operation === "managed-work.list.v1"
          ? []
          : [managed.ok ? "Managed work response did not match." : managed.error.message]),
        ...(repository.ok ? [] : [repository.error.message]),
      ];
      return {
        state: "ready",
        selection,
        contextId: selection.contextId,
        view: read.value,
        revision:
          read.value.snapshot.state === "ready"
            ? read.value.snapshot.snapshot.revision
            : read.value.project.revision,
        coverage: coverageReasons.length === 0 ? "complete" : "partial",
        coverageReasons,
        managedWorks:
          managed.ok && managed.value.operation === "managed-work.list.v1"
            ? managed.value.works
            : [],
        repository: repository.ok
          ? { state: "ready", value: repository.value }
          : { state: "unavailable", reason: repository.error.message },
      };
    }),
  );
  return {
    ok: true,
    value: {
      projects,
      completeness: projects.every(
        (project) => project.state === "ready" && project.coverage === "complete",
      )
        ? "complete"
        : "partial",
    },
  };
}

function projectCoverageReasons(view: ProjectWorkspaceView): readonly string[] {
  const reasons: string[] = [];
  if (view.snapshot.state === "unavailable") reasons.push(view.snapshot.reason);
  if (view.snapshot.state === "ready" && view.snapshot.snapshot.phase !== "ready") {
    reasons.push(
      view.snapshot.snapshot.phaseDetail ?? `Plan snapshot is ${view.snapshot.snapshot.phase}.`,
    );
  }
  if (
    view.snapshot.state === "ready" &&
    view.snapshot.snapshot.navigation?.completeness === "partial"
  ) {
    reasons.push("Plan navigation reports partial coverage.");
  }
  if (view.network.coverage.state === "partial") reasons.push(view.network.coverage.reason);
  return reasons;
}

function failure(message: string): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_input",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-010#unified-canvas: ${message}`,
    },
  };
}
