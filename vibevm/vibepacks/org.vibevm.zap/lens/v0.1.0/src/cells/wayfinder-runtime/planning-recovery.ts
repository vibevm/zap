/** Truthful restart projection for protected planning sources. @scope spec://org.vibevm.zap/lens/PROP-014#algorithm-binding */
import { PrincipalIdSchema } from "../protocol/index.ts";
import type { WorkspacePlanningController } from "../workspace-planning/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";
import type { WayfinderResult } from "./types.ts";

export async function reconcileDetachedPlanningSources(input: {
  readonly store: WorkspaceStore;
  readonly planning: WorkspacePlanningController | null;
  readonly repositories: RuntimeRepositoryWorkspaces | null;
  readonly projectIds: readonly string[];
}): Promise<WayfinderResult<null>> {
  for (const rawProjectId of new Set(input.projectIds)) {
    const projectId = ProjectIdSchema.safeParse(rawProjectId);
    if (!projectId.success) return failure("configured project identity is invalid");
    const access = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.planning-recovery"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.planning-recovery"),
      authorizedProjectIds: [projectId.data],
    });
    const project = input.store.read(access, {
      operation: "project.get.v1",
      projectId: projectId.data,
    });
    if (!project.ok || project.value.operation !== "project.get.v1")
      return failure("registered project context is unavailable during planning recovery");
    for (const context of project.value.detail.contexts) {
      if (
        context.planning.state !== "configured" ||
        input.planning?.attached(projectId.data, context.contextId) === true
      )
        continue;
      if (context.planId !== null && input.repositories !== null) {
        const plan = input.repositories.service.getPlan(context.planId);
        if (plan.ok && plan.value.algorithmBinding.state === "bound") {
          const pending = await input.repositories.service.updateAlgorithmBinding({
            requestId: `planning-recovery.${plan.value.planId}.${plan.value.revision}`,
            principalId: "principal.planning-recovery",
            executionHostId: input.repositories.executionHostId,
            planId: plan.value.planId,
            expectedRevision: plan.value.revision,
            algorithmBinding: { state: "pending" },
          });
          if (!pending.ok) return failure(pending.error.message);
        }
      }
      const detached = input.store.bindExistingContextPlanning({
        projectId: projectId.data,
        contextId: context.contextId,
        expectedRevision: context.revision,
        planning: {
          state: "unavailable",
          reason: "Protected planning source must be reconnected after restart.",
        },
      });
      if (!detached.ok) return failure(detached.error.message);
    }
  }
  return { ok: true, value: null };
}

function failure(message: string): WayfinderResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_config",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-014#algorithm-binding: ${message}; fix surface: reconnect the exact protected planning source`,
    },
  };
}
