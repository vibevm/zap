/** Trusted dynamic planning-source attachment. @scope spec://org.vibevm.zap/lens/PROP-014#identity */
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type {
  WorkspacePlanningAttachment,
  WorkspacePlanningController,
} from "../workspace-planning/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";
import { invalidConfig } from "./runtime-result.ts";

export async function attachRuntimePlanningSource(input: {
  readonly planning: WorkspacePlanningController | null;
  readonly repositories: RuntimeRepositoryWorkspaces | null;
  readonly store: WorkspaceStore;
  readonly attachment: WorkspacePlanningAttachment & {
    readonly planId: string;
    readonly principalId: string;
  };
}) {
  if (input.planning === null || input.repositories === null)
    return invalidConfig("planning source attachment is not configured");
  const attachment = input.attachment;
  const plan = input.repositories.service.getPlan(attachment.planId);
  if (
    !plan.ok ||
    plan.value.projectId !== attachment.projectId ||
    plan.value.contextId !== attachment.contextId
  )
    return invalidConfig("planning source attachment is outside the exact plan context");
  const attached = await input.planning.attach(attachment);
  if (!attached.ok) return invalidConfig(attached.error.message);
  if (attached.value.state !== "bound")
    return invalidConfig("planning source did not produce a bound algorithm identity");
  const current = input.repositories.service.getPlan(attachment.planId);
  if (!current.ok) return invalidConfig(current.error.message);
  if (JSON.stringify(current.value.algorithmBinding) !== JSON.stringify(attached.value)) {
    const updated = await input.repositories.service.updateAlgorithmBinding({
      requestId: `planning-attachment.${attachment.planId}.${current.value.revision}`,
      principalId: attachment.principalId,
      executionHostId: input.repositories.executionHostId,
      planId: attachment.planId,
      expectedRevision: current.value.revision,
      algorithmBinding: attached.value,
    });
    if (!updated.ok) return invalidConfig(updated.error.message);
  }
  const projectId = ProjectIdSchema.parse(attachment.projectId);
  const contextId = WorkContextIdSchema.parse(attachment.contextId);
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(attachment.principalId),
    actorId: null,
    clientId: ClientIdSchema.parse("client.planning-source-attachment"),
    authorizedProjectIds: [projectId],
  });
  const context = input.store.read(access, { operation: "context.get.v1", projectId, contextId });
  if (!context.ok || context.value.operation !== "context.get.v1")
    return invalidConfig("attached planning context is unavailable");
  const projected = input.store.bindExistingContextPlanning({
    projectId,
    contextId,
    expectedRevision: context.value.context.revision,
    planning: {
      state: "configured",
      storeId: attached.value.storeId,
      campaignId: attached.value.campaignId,
      baseId: attached.value.baseId,
    },
  });
  return projected.ok
    ? { ok: true as const, value: attached.value }
    : invalidConfig(projected.error.message);
}
