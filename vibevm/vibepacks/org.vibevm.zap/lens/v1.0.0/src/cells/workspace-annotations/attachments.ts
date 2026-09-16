/** Deferred-note attachment adapter for managed work. @scope spec://org.vibevm.zap/lens/PROP-011#deferred-delivery */
import type { WorkAttachmentPort, ManagedWorkResult } from "../managed-work/index.ts";
import type { AnnotationTargetSnapshot, ProjectObjectReference } from "../workspace-model/index.ts";
import type { AnnotationStore } from "./types.ts";
import { annotationDeliveryMarkdown } from "./delivery.ts";

export interface AnnotationTargetResolver {
  resolve(
    target: ProjectObjectReference,
  ): Promise<
    | { readonly state: "present"; readonly snapshot: AnnotationTargetSnapshot }
    | { readonly state: "missing" }
    | { readonly state: "unavailable"; readonly reason: string }
  >;
}

export function createAnnotationWorkAttachmentPort(options: {
  readonly store: AnnotationStore;
  readonly resolver?: AnnotationTargetResolver;
  readonly idFactory?: (kind: string) => string;
  readonly clock?: () => Date;
}): WorkAttachmentPort {
  const idFactory = options.idFactory ?? ((kind) => `${kind}.${crypto.randomUUID()}`);
  const clock = options.clock ?? (() => new Date());
  return {
    async prepareBeforeWork(input) {
      if (input.targets.length === 0)
        return { ok: true, value: { state: "ready", instructions: [] } };
      if (options.resolver === undefined)
        return { ok: true, value: { state: "waiting_for_target", instructions: [] } };
      const notes = options.store.listDeferred(
        input.access,
        input.targets[0]?.projectId ?? "",
        input.targets[0]?.contextId ?? "",
        input.targets,
      );
      if (!notes.ok) return failure(notes.error.message);
      const instructions: Array<{ attachmentId: string; version: string; bodyMarkdown: string }> =
        [];
      for (const note of notes.value) {
        const resolved = await options.resolver.resolve(note.target);
        if (resolved.state !== "present")
          return { ok: true, value: { state: "waiting_for_target", instructions: [] } };
        const deliveryId = idFactory("annotation-delivery");
        const offered = options.store.offerDelivery({
          deliveryId,
          noteId: note.noteId,
          noteVersion: note.currentVersion,
          attemptId: input.attemptId,
          recipientActorId: input.recipientActorId,
          projectId: note.projectId,
          contextId: note.contextId,
          offeredAt: clock().toISOString(),
        });
        if (!offered.ok) return failure(offered.error.message);
        instructions.push({
          attachmentId: offered.value.deliveryId,
          version: offered.value.noteVersion,
          bodyMarkdown: annotationDeliveryMarkdown(note),
        });
      }
      return { ok: true, value: { state: "ready", instructions } };
    },
    acknowledge(input) {
      const acknowledged = options.store.acknowledgeDelivery({
        access: input.access,
        deliveryId: input.attachmentId,
        attemptId: input.attemptId,
        version: input.version,
        messageId: null,
        acknowledgedAt: clock().toISOString(),
      });
      return Promise.resolve(
        acknowledged.ok ? { ok: true, value: null } : failure(acknowledged.error.message),
      );
    },
  };
}

function failure(message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code: "unavailable", message } };
}
