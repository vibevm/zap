/** Durable source-change projection. @scope spec://org.vibevm.zap/lens/PROP-002#specification-drift */
import { createHash } from "node:crypto";
import { JsonValueSchema } from "../protocol/index.ts";
import type { QuicklensSnapshot } from "../quicklens-model/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";

export function recordSourceChange(input: {
  readonly store: WorkspaceStore;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly reason: string;
  readonly before: QuicklensSnapshot | null;
  readonly after: QuicklensSnapshot;
}): void {
  const beforeBasis = input.before?.plan?.basis ?? null;
  const afterBasis = input.after.plan?.basis ?? null;
  const payload = JsonValueSchema.safeParse({
    reason: input.reason,
    before: { revision: input.before?.revision ?? null, basis: beforeBasis },
    after: { revision: input.after.revision, basis: afterBasis },
  });
  if (!payload.success) return;
  const previousPlanId = input.before?.navigation?.adoptedPlanRef ?? null;
  const proposedPlanId = input.after.navigation?.adoptedPlanRef ?? null;
  input.store.ingestEvent({
    projectId: input.projectId,
    contextId: input.contextId,
    kind: "source.changed",
    source: "lens",
    actorId: null,
    occurrenceAt: input.after.capturedAt,
    sourceEventId: `source-change:${digest(input.projectId, input.contextId, beforeBasis, afterBasis)}`,
    correlationId: proposedPlanId,
    causationId: null,
    planProvenance:
      afterBasis === null
        ? null
        : {
            previousPlanId,
            proposedPlanId,
            sourceBasisRef: afterBasis.sourceBasisRef,
            appliedRevision: null,
          },
    sourceSequence: null,
    payload: payload.data,
  });
}

function digest(projectId: string, contextId: string, before: unknown, after: unknown): string {
  return createHash("sha256")
    .update(JSON.stringify({ projectId, contextId, before, after }))
    .digest("hex");
}
