/** Repository target catalog for shared notes and Trash. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { createHash } from "node:crypto";
import { JsonValueSchema } from "../protocol/index.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
import { WorkContextIdSchema, type WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";

export function observeRepositoryAnnotationTargets(input: {
  readonly annotations: AnnotationService | undefined;
  readonly runtime: RuntimeRepositoryWorkspaces;
  readonly access: WorkspaceAccessContext;
  readonly projectId: WorkspaceAccessContext["authorizedProjectIds"][number];
  readonly contextId: string;
}): void {
  if (input.annotations === undefined) return;
  const plans = input.runtime.service.listPlans(input.projectId);
  if (!plans.ok) return;
  const scopedPlans = plans.value.filter((plan) => plan.contextId === input.contextId);
  const targets = scopedPlans.flatMap((plan) => {
    const planTarget = target(input, "plan_workspace", plan.planId, plan.revision, plan);
    const worktrees = input.runtime.service.listWorktrees(plan.planId);
    const integrations = input.runtime.service.listIntegrations(plan.planId);
    return [
      planTarget,
      ...(worktrees.ok
        ? worktrees.value.map((worktree) =>
            target(input, "worktree", worktree.worktreeId, worktree.revision, worktree),
          )
        : []),
      ...(integrations.ok
        ? integrations.value.map((integration) =>
            target(
              input,
              "integration",
              integration.integrationId,
              integration.revision,
              integration,
            ),
          )
        : []),
    ];
  });
  const complete = targets.length <= 100_000;
  const bounded = complete ? targets : targets.slice(0, 100_000);
  const contextId = WorkContextIdSchema.parse(input.contextId);
  input.annotations.observeSource(input.access, {
    projectId: input.projectId,
    contextId,
    state: complete ? "authoritative_full" : "partial",
    basisRef: `repository:${digest(
      bounded.map(({ target: reference, snapshot }) => [reference.ref, snapshot.basisRef]),
    )}`,
    observedAt: new Date().toISOString(),
    presentTargets: bounded.map(({ target: reference }) => reference),
    removedTargets: [],
    snapshots: bounded,
    coveredDomains: ["plan_workspace", "worktree", "integration"],
  });
}

function target(
  input: Parameters<typeof observeRepositoryAnnotationTargets>[0],
  domain: "plan_workspace" | "worktree" | "integration",
  ref: string,
  revision: string,
  value: unknown,
) {
  return {
    target: {
      projectId: input.projectId,
      contextId: WorkContextIdSchema.parse(input.contextId),
      domain,
      ref,
    },
    snapshot: {
      basisRef: revision,
      capturedAt: new Date().toISOString(),
      value: JsonValueSchema.parse(value),
    },
  };
}

function digest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}
