/** Agent-visible exact annotation context. @scope spec://org.vibevm.zap/lens/PROP-011#deferred-delivery */
import type { AnnotationNote } from "../workspace-model/index.ts";

export function annotationDeliveryMarkdown(note: AnnotationNote): string {
  return [
    `Deferred instruction: ${note.title}`,
    "",
    note.bodyMarkdown,
    "",
    `Target: ${note.target.domain} ${note.target.ref}`,
    `Project/context: ${note.projectId} / ${note.contextId}`,
    `Source basis: ${note.sourceBasisRef}`,
    `Note version: ${note.currentVersion}`,
  ].join("\n");
}
