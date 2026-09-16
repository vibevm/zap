/** Zap notes, deferred instructions and recoverable Trash feature. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
export { openAnnotationStore } from "./store.ts";
export type { AnnotationStore, AnnotationResult, AnnotationErrorCode } from "./types.ts";
export { createAnnotationService } from "./service.ts";
export type {
  AnnotationService,
  AnnotationNotificationPort,
  AnnotationRestoreIntentPort,
} from "./service.ts";
export { createAnnotationWorkAttachmentPort } from "./attachments.ts";
export type { AnnotationTargetResolver } from "./attachments.ts";
export { annotationDeliveryMarkdown } from "./delivery.ts";
