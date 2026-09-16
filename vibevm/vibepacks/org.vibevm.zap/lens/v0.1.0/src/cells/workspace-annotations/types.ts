/** Public annotation persistence seam. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import type {
  AnnotationCommandRequest,
  AnnotationCommandResponse,
  AnnotationDelivery,
  AnnotationNote,
  AnnotationReadRequest,
  AnnotationReadResponse,
  AnnotationSourceObservation,
  AnnotationTrashEntry,
  ProjectObjectReference,
  WorkspaceAccessContext,
} from "../workspace-model/index.ts";

export type AnnotationErrorCode =
  | "invalid_input"
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "conflict"
  | "stale_revision"
  | "idempotency_conflict"
  | "storage_failure"
  | "unavailable"
  | "closed";

export type AnnotationResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: { readonly code: AnnotationErrorCode; readonly message: string };
    };

export interface AnnotationStore {
  read(
    access: WorkspaceAccessContext,
    request: AnnotationReadRequest,
  ): AnnotationResult<AnnotationReadResponse>;
  command(
    access: WorkspaceAccessContext,
    request: AnnotationCommandRequest,
  ): AnnotationResult<AnnotationCommandResponse>;
  observeSource(
    access: WorkspaceAccessContext,
    observation: AnnotationSourceObservation,
  ): AnnotationResult<readonly AnnotationTrashEntry[]>;
  readTrash(
    access: WorkspaceAccessContext,
    trashId: string,
  ): AnnotationResult<AnnotationTrashEntry>;
  listDeferred(
    access: WorkspaceAccessContext,
    projectId: string,
    contextId: string,
    targets: readonly ProjectObjectReference[],
  ): AnnotationResult<readonly AnnotationNote[]>;
  offerDelivery(
    input: Omit<AnnotationDelivery, "state" | "acknowledgedAt" | "resolvedAt" | "messageId">,
  ): AnnotationResult<AnnotationDelivery>;
  acknowledgeDelivery(input: {
    readonly access: WorkspaceAccessContext;
    readonly deliveryId: string;
    readonly attemptId: string;
    readonly version: string;
    readonly messageId: string | null;
    readonly acknowledgedAt: string;
  }): AnnotationResult<AnnotationDelivery>;
  close(): void;
}
